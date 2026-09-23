//! `charter hook pretooluse` — the Bash guard, answered rather than blocked.
//!
//! **This file is the switch M3.1 was built in front of.** Until it existed, `main.rs`'s
//! `is_a_tool_hook` answered every word in the `pretooluse`/`posttooluse` namespace with exit 2
//! — *block* — because refusing a tool call is the only safe thing a program that has checked
//! nothing can do. The eight arms are ported and assembled ([`charter_core::toolgate`]), and so
//! now is the rest of the tool-hook namespace (`hooks.rs`); `is_a_tool_hook` is left guarding
//! only words charter has not invented yet.
//!
//! # What this does
//!
//! `charter/hooks.py:pretooluse` is its bookkeeping, eight refusals, and one ALLOW, in that
//! order, and so is this:
//!
//! 1. **Bookkeeping** ([`charter_core::toolhooks::pretooluse_bookkeeping`]), in a plane only:
//!    the heartbeat of the piece the command runs in, and the record-memory cadence reset a
//!    `charter … remember` earns. The Python's guard sighting, turn marker and trace row are
//!    not kept — nothing in charter-app reads them.
//! 2. **The refusals** ([`charter_core::toolgate::verdict`]): one JSON object on stdout, or
//!    nothing.
//! 3. **The persona tool gate** ([`charter_core::personagate`]), in a plane only and only when
//!    nothing refused: `allow`, so the harness does not prompt, when the active persona's
//!    `tools:` declares the program. It never denies; the worst it can do is leave the prompt.
//!
//! # Why a denial is exit 0
//!
//! The verdict IS the JSON. A `PreToolUse` hook refuses by printing
//! `permissionDecision: "deny"` and exiting cleanly; exit 2 is the *fallback* for when that
//! print could not be delivered, and every other non-zero status is a non-blocking error the
//! harness logs while the tool call proceeds. So the one direction this must not fail in is a
//! decided denial that never reached stdout, which is charter#438 and is why the write is
//! checked, the stream is flushed inside the check, and a failure there exits 2 with the reason
//! on stderr. A write is not a delivery: `print` to a pipe buffers, and the first version of
//! that fix upstream returned 0 on a real broken pipe.
//!
//! # Where the plane comes from
//!
//! The PROCESS's directory, not the payload's. That is what the Python does — `charter.config`
//! resolves once at import, from `$CHARTER_ROOT` and the interpreter's own cwd — and the two
//! are genuinely different questions: the plane is where charter's files are, and the payload's
//! `cwd` is where the command being judged would run. Both are passed, each to the arms that
//! ask for it.

use std::io::Write;
use std::process::ExitCode;

use charter_core::handoffguard::Caller;
use charter_core::toolgate::{self, Call, Plane};

/// What a harness reads as "block" — and, here, only as the fallback for an undelivered
/// denial. `hooks.py:DENY_EXIT`.
const DENY_EXIT: u8 = 2;

/// `$CHARTER_HARNESS`: which harness this session is running under, as charter's own word for
/// it. A7 reads it to decide whether an `agent_id` on the payload means a sub-agent.
const HARNESS_ENV: &str = "CHARTER_HARNESS";

/// The plane's facts, owned, because [`Plane`] borrows them.
struct Found {
    root: String,
    forges: Vec<charter_core::forge::Forge>,
}

/// `charter hook pretooluse` — refuse this tool call, or get out of the way.
///
/// `payload` is the harness's JSON on stdin. A payload that will not parse is `{}`, exactly as
/// `_read_stdin` makes it: every arm is then asked about the empty command and none fires. That
/// is deliberate and is not a fail-open — a guard with no command to judge has nothing to
/// refuse, and refusing anyway would block every tool call on a harness whose payload charter
/// does not understand.
pub fn pretooluse(payload: &str, now: Option<&str>) -> ExitCode {
    // Bookkeeping first, as the Python does: a refusal below still means the session was here.
    crate::hooks::with_hook(
        payload,
        now,
        charter_core::toolhooks::pretooluse_bookkeeping,
    );
    let data: serde_json::Value = serde_json::from_str(payload).unwrap_or(serde_json::Value::Null);
    fn text(at: &serde_json::Value) -> String {
        at.as_str().unwrap_or_default().to_string()
    }

    // Python's `ti.get("command", "") or ""` and `data.get("cwd") or ""`: a null, a number or an
    // absent field are all the empty string, and the arms are still asked.
    let command = text(&data["tool_input"]["command"]);
    let cwd = text(&data["cwd"]);
    let agent_id = data["agent_id"].as_str().map(str::to_owned);
    let permission_mode = data["permission_mode"].as_str().map(str::to_owned);
    let harness = std::env::var(HARNESS_ENV).ok();

    // `config.ROOT`: `$CHARTER_ROOT`, then the marker walk up from the process's directory,
    // then — when nothing is found — the directory itself, which is what Python falls back to
    // and what makes `STATE_DIR` exist outside a plane.
    let here = std::env::current_dir().unwrap_or_default();
    let root = charter_core::plane::resolve(&here).unwrap_or(here);
    // `config.STATE_DIR`, which exists with or without a plane: the leak guard is ungated and
    // still needs a `.charter/` to refuse a walk into. `$CHARTER_HOME` overrides it, here as
    // everywhere else.
    let state_dir = charter_core::plane::state_dir(&root);
    // **`_in_a_plane()` is the MARKER, not a successful resolve** — `config.HAS_CONTROL_PLANE`
    // is `(ROOT / "charter.toml").is_file()` and nothing else. The difference is real and the
    // differential found it: `$CHARTER_ROOT` is taken on trust by `plane::resolve`, so a
    // session pinned at a directory that is not a plane resolved to it happily, and the five
    // gated arms then denied in a directory with no control plane — which is charter#852
    // exactly, reintroduced one level down.
    let found = root
        .join(charter_core::plane::MANIFEST)
        .is_file()
        .then(|| Found {
            root: root.display().to_string(),
            forges: charter_core::forge::known_ordered(&root),
        });
    let plane = found.as_ref().map(|found| Plane {
        root: &found.root,
        forges: &found.forges,
    });
    let call = Call {
        command: &command,
        cwd: &cwd,
        state_dir: &state_dir,
        caller: Caller {
            agent_id: agent_id.as_deref(),
            harness: harness.as_deref(),
            permission_mode: permission_mode.as_deref(),
        },
    };
    if let Some(verdict) = toolgate::verdict(&call, plane.as_ref()) {
        return deny(&verdict);
    }
    // Nothing refused, so the persona tool gate is asked. Its answer is an allow or nothing;
    // one it could not print is simply the ordinary prompt.
    if let Some(allow) =
        crate::hooks::with_hook(payload, now, charter_core::toolhooks::persona_allow)
    {
        crate::hooks::say(&allow);
    }
    ExitCode::SUCCESS
}

/// Print the verdict, or fall back to the one status a harness reads as "refused".
///
/// `hooks.py:_deny`. The flush is inside the check on purpose: a `print` to a pipe lands in a
/// userspace buffer and returns cleanly, so without it a broken stdout is discovered at exit,
/// too late for any exit status to mean anything.
pub(crate) fn deny(verdict: &toolgate::Verdict) -> ExitCode {
    let line = verdict.emitted();
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{line}").is_ok() && out.flush().is_ok() {
        return ExitCode::SUCCESS;
    }
    // stderr is what exit 2 hands the model, so the reason still arrives. Best effort: the exit
    // status is the guard, and a second failed stream cannot be allowed to change it.
    let _ = writeln!(std::io::stderr(), "{}", verdict.said());
    let _ = std::io::stderr().flush();
    ExitCode::from(DENY_EXIT)
}
