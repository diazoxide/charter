//! `charter hook pretooluse` — the Bash guard, answered rather than blocked.
//!
//! **This file is the switch M3.1 was built in front of.** Until it existed, `main.rs`'s
//! `is_a_tool_hook` answered every word in the `pretooluse`/`posttooluse` namespace with exit 2
//! — *block* — because refusing a tool call is the only safe thing a program that has checked
//! nothing can do. The eight arms are ported and assembled ([`purlis_core::toolgate`]), and so
//! now is the rest of the tool-hook namespace (`hooks.rs`); `is_a_tool_hook` is left guarding
//! only words charter has not invented yet.
//!
//! # What this does
//!
//! `charter/hooks.py:pretooluse` is its bookkeeping, eight refusals, and one ALLOW, in that
//! order, and so is this:
//!
//! 1. **Bookkeeping** ([`purlis_core::toolhooks::pretooluse_bookkeeping`]), in a plane only:
//!    the heartbeat of the piece the command runs in, and the record-memory cadence reset a
//!    `charter … remember` earns. The Python's guard sighting, turn marker and trace row are
//!    not kept — nothing in charter-app reads them.
//! 2. **The refusals** ([`purlis_core::toolgate::verdict`]): one JSON object on stdout, or
//!    nothing.
//! 3. **The persona tool gate** ([`purlis_core::personagate`]), in a plane only and only when
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
use std::time::{Duration, Instant};

use purlis_core::handoffguard::Caller;
use purlis_core::toolgate::{self, Call, Launched, Plane};
use purlis_core::toolhooks::Answer;

/// What a harness reads as "block" — and, here, only as the fallback for an undelivered
/// denial. `hooks.py:DENY_EXIT`.
const DENY_EXIT: u8 = 2;

/// `$CHARTER_HARNESS`: which harness this session is running under, as charter's own word for
/// it. A7 reads it to decide whether an `agent_id` on the payload means a sub-agent.
const HARNESS_ENV: &str = "PURLIS_HARNESS";

/// The plane's facts, owned, because [`Plane`] borrows them.
struct Found {
    root: String,
    forges: Vec<purlis_core::forge::Forge>,
}

/// `charter hook pretooluse` — refuse this tool call, or get out of the way.
///
/// `payload` is the harness's JSON on stdin. A payload that will not parse is `{}`, exactly as
/// `_read_stdin` makes it: every arm is then asked about the empty command and none fires. That
/// is deliberate and is not a fail-open — a guard with no command to judge has nothing to
/// refuse, and refusing anyway would block every tool call on a harness whose payload charter
/// does not understand.
pub fn pretooluse(payload: &str, now: Option<&str>) -> crate::hooks::Answered {
    // Bookkeeping first, as the Python does: a refusal below still means the session was here.
    crate::hooks::with_hook(payload, now, purlis_core::toolhooks::pretooluse_bookkeeping);
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
    let harness = purlis_core::envvar::var(HARNESS_ENV);

    // `config.ROOT`: `$CHARTER_ROOT`, then the marker walk up from the process's directory,
    // then — when nothing is found — the directory itself, which is what Python falls back to
    // and what makes `STATE_DIR` exist outside a plane.
    let here = std::env::current_dir().unwrap_or_default();
    let root = purlis_core::plane::resolve(&here).unwrap_or(here);
    // `config.STATE_DIR`, which exists with or without a plane: the leak guard is ungated and
    // still needs a `.charter/` to refuse a walk into. `$CHARTER_HOME` overrides it, here as
    // everywhere else.
    let state_dir = purlis_core::plane::state_dir(&root);
    // **`_in_a_plane()` is the MARKER, not a successful resolve** — `config.HAS_CONTROL_PLANE`
    // is `(ROOT / "charter.toml").is_file()` and nothing else. The difference is real and the
    // differential found it: `$CHARTER_ROOT` is taken on trust by `plane::resolve`, so a
    // session pinned at a directory that is not a plane resolved to it happily, and the five
    // gated arms then denied in a directory with no control plane — which is charter#852
    // exactly, reintroduced one level down.
    let found = purlis_core::names::has_manifest(&root).then(|| Found {
        root: root.display().to_string(),
        forges: purlis_core::forge::known_ordered(&root),
    });
    // Where Claude Code loaded its settings: the session's start folder, which a `cd` in the
    // shell does not move (RN-7).
    let session_dir = std::env::var("CLAUDE_PROJECT_DIR").unwrap_or_default();
    // What the app told this chat at its start: whether it was given a sandbox, and the folder
    // it was started in (#1345). A harness the app did not start has neither.
    let chat_dir = std::env::var(purlis_core::sandboxblock::CHAT_DIR_ENV).ok();
    let launched = Launched {
        sandboxed: purlis_core::sandbox::chat_is_sandboxed(),
        chat_dir: chat_dir.as_deref(),
    };
    let plane = found.as_ref().map(|found| Plane {
        root: &found.root,
        forges: &found.forges,
        session_dir: &session_dir,
        launched,
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
        return crate::hooks::Answered::of(Answer::Deny(verdict));
    }
    // Nothing refused. A dispatch or a handoff that shares its call with another command is
    // asked about (D-1444-14), before the persona tool gate could allow the call whole.
    if let Some(asked) = toolgate::asks(&call, plane.as_ref()) {
        return crate::hooks::Answered::of(Answer::Say(asked.emitted()));
    }
    // Then the persona tool gate is asked. Its answer is an allow or nothing;
    // one it could not print is simply the ordinary prompt. It reads the command too, so a
    // reading of it nested too deep to read whole is the refusal the guards give (#1355).
    let (allowed, too_deep) = purlis_core::shellseg::too_deep_within(|| {
        crate::hooks::with_hook(payload, now, purlis_core::toolhooks::persona_allow)
    });
    if too_deep {
        return crate::hooks::Answered::of(Answer::Deny(toolgate::too_deep()));
    }
    crate::hooks::Answered::of(allowed.map_or(Answer::Nothing, Answer::Say))
}

/// The stack the guards run on: far more than any of them needs.
///
/// A stack overflow is not a panic: it aborts the process before any hook can answer, and a
/// guard that dies without answering is an allow. Each guard bounds how deep it reads (the
/// floor's `MAX_DEPTH`), and this is the backstop beneath them all. Only the pages a guard
/// touches are ever committed, so the reservation costs nothing a person could notice.
const GUARD_STACK: usize = 256 * 1024 * 1024;

/// The time every `PreToolUse` hook decides within, from the moment it starts: reading the
/// payload, then every guard and gate it runs (#1355).
///
/// A harness gives a hook a timeout of its own, and whether the tool call runs when it passes is
/// the harness's choice: Codex and Claude Code run it (see `docs/hooks.md` → When a hook runs
/// out of time). So purlis never lets a harness time a guard out. It decides inside this budget,
/// and a decision not made in time is a refusal. Two seconds, well inside the smallest
/// `PreToolUse` timeout purlis registers (`the_budget_is_well_inside_every_harness_timeout`),
/// and far more than an ordinary command takes. The caps in front of the guards
/// ([`purlis_core::guardcaps`]) bound how slow a command inside them can be, and purlis-core's
/// `the_whole_verdict_at_the_largest_and_deepest_command_stays_well_inside_the_budget` holds
/// the slowest shapes known under this; the budget is the backstop for one nobody has found.
///
/// It starts in `hook`, not when the process does: the time to start the process is outside it
/// (`docs/hooks.md` → When a hook runs out of time).
pub(crate) const BUDGET: Duration = Duration::from_secs(2);

/// What sets the budget in milliseconds instead, in a debug build, for the tests that reach it.
#[cfg(debug_assertions)]
const BUDGET_ON_PURPOSE_ENV: &str = "PURLIS_TEST_GUARD_DEADLINE_MS";

/// Set in a debug build, the judging thread stalls before it judges: the tests' way to reach
/// the budget on every hook word without a command crafted for each.
#[cfg(debug_assertions)]
const STALLS_ON_PURPOSE_ENV: &str = "PURLIS_TEST_GUARD_STALLS";

/// The panic payload that says the hook ran out of its budget, rather than that it crashed.
const UNANSWERED: &str = "the guard did not answer in time";

/// One hook's budget, started when the hook is.
pub(crate) struct Budget {
    ends: Instant,
}

impl Budget {
    /// The budget, from now.
    pub(crate) fn start() -> Self {
        Self {
            ends: Instant::now() + budget(),
        }
    }

    /// What is left of it.
    pub(crate) fn left(&self) -> Duration {
        self.ends.saturating_duration_since(Instant::now())
    }
}

fn budget() -> Duration {
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var(BUDGET_ON_PURPOSE_ENV)
        .ok()
        .and_then(|ms| ms.parse().ok())
    {
        return Duration::from_millis(ms);
    }
    BUDGET
}

/// The hook ran out of its budget: refused, by the panic hook ([`refuse_on_a_crash`]), which
/// ends the process with the sentence that says so.
pub(crate) fn out_of_time() -> ! {
    std::panic::panic_any(UNANSWERED)
}

/// Runs `judge` on a thread with [`GUARD_STACK`] of stack, and waits for its answer for what is
/// left of `budget`. A panic on that thread, a thread that cannot start, and an answer that does
/// not come in time are all the panic hook's refusal ([`refuse_on_a_crash`]), which ends the
/// process.
pub(crate) fn decided_within<T: Send>(budget: &Budget, judge: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        let (answer, answered) = std::sync::mpsc::channel();
        let started = std::thread::Builder::new()
            .name("guard".into())
            .stack_size(GUARD_STACK)
            .spawn_scoped(scope, move || {
                #[cfg(debug_assertions)]
                if std::env::var_os(STALLS_ON_PURPOSE_ENV).is_some() {
                    std::thread::sleep(Duration::from_secs(3600));
                }
                let _ = answer.send(judge());
            });
        if let Err(why) = started {
            panic!("the guard's thread did not start: {why}");
        }
        match answered.recv_timeout(budget.left()) {
            Ok(verdict) => verdict,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => out_of_time(),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("the guard's thread ended without answering")
            }
        }
    })
}

/// From here on, a panic anywhere in this process is a refusal: exit 2, one line on stderr.
///
/// **A crashed guard is otherwise an allow** (#349). A panic exits 101, or under the release
/// profile's `panic = "abort"` ends in SIGABRT, and a harness reads every non-zero status but
/// 2 as a non-blocking error and runs the tool. `catch_unwind` cannot help where there is no
/// unwinding, but a panic hook runs before the abort does, so the hook answers and exits
/// itself — the abort is never reached. It covers every thread, which is right: under abort a
/// panic on any thread ends the process, and it must end it as a refusal.
///
/// Exit 2 is what Claude Code and Codex both read as "block", handing stderr to the model; they
/// ignore stdout on it, so no JSON is printed. The panic's own message is not said: it can
/// carry what the guard was judging, and the model is owed only why the call did not run.
///
/// Only for a `PreToolUse` word. A crash on a reporting hook must never exit 2, which on `Stop`
/// would keep a session from ending.
pub(crate) fn refuse_on_a_crash(tell_the_host: fn(&str, bool)) {
    // The word this process answers, for the host's event log: the call is refused, and one
    // event says so (FD-9).
    // `args_os`, read before anything is installed: `args` panics on a word that is not
    // UTF-8, and nothing on the way to this hook may panic.
    let word = std::env::args_os()
        .nth(2)
        .map(|word| word.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map(|at| format!(" (at {}:{})", at.file(), at.line()))
            .unwrap_or_default();
        let mut err = std::io::stderr().lock();
        let unanswered = info.payload().downcast_ref::<&str>() == Some(&UNANSWERED);
        if unanswered {
            let _ = writeln!(
                err,
                "purlis guard: this tool call is refused because purlis could not check it in \
                 time: the command is too long or too deeply nested, so split it into smaller \
                 commands."
            );
        } else {
            let _ = writeln!(
                err,
                "purlis guard: this tool call is refused because the guard crashed{at} before \
                 it could answer, and a guard that could not answer does not allow."
            );
        }
        let _ = err.flush();
        drop(err);
        tell_the_host(&word, unanswered);
        std::process::exit(i32::from(DENY_EXIT));
    }));
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

#[cfg(test)]
mod budget_tests {
    use super::*;

    /// The budget, and the bounded wait to tell the app after it, end well before any harness
    /// would time a `PreToolUse` hook out: the hook always decides, and is never decided for.
    #[test]
    fn the_budget_is_well_inside_every_harness_timeout() {
        for hook in purlis_core::hookreg::HANDLERS
            .iter()
            .filter(|hook| hook.event == "PreToolUse")
        {
            let harness = Duration::from_secs(u64::from(hook.timeout));
            assert!(
                BUDGET * 2 <= harness,
                "{}: {BUDGET:?} of {harness:?}",
                hook.name
            );
        }
    }
}
