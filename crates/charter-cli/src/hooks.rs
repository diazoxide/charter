//! `charter hook <word>` for every word other than the app's reporting events: the tool hooks,
//! the persona briefing at session start, and the registry that lists them all.
//!
//! The decisions are [`charter_core::toolhooks`], [`charter_core::briefing`] and
//! [`charter_core::personagate`]; this file reads the process — the payload on stdin, the
//! environment, the directory — hands it over, and prints what comes back. It is the only
//! place any of them meets stdout, which is why the one rule that matters is kept here:
//!
//! **exit 2 only for a denial that could not be printed** ([`crate::guard::deny`]). Every
//! other outcome of every handler is exit 0, with one line of JSON or nothing.

use std::path::PathBuf;
use std::process::ExitCode;

use charter_core::toolhooks::{self, Answer, Hook};

/// The plane as a hook sees it — `config.ROOT` and `config.HAS_CONTROL_PLANE`.
///
/// `$CHARTER_ROOT`, then the marker walk up from the PROCESS's directory, then the directory
/// itself. `in_plane` is the MARKER at that root and not a successful resolve: `$CHARTER_ROOT`
/// is taken on trust, and a session pinned at a directory that is not a plane must not have
/// the plane-gated hooks act there (charter#852; `guard.rs` says how the differential found it).
pub struct Where {
    pub root: PathBuf,
    pub in_plane: bool,
    pub cwd: PathBuf,
}

impl Where {
    pub fn here() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        let root = charter_core::plane::resolve(&cwd).unwrap_or_else(|_| cwd.clone());
        let in_plane = root.join(charter_core::plane::MANIFEST).is_file();
        Self {
            root,
            in_plane,
            cwd,
        }
    }
}

/// The instant a hook acts at: `--now` (a LOCAL naive stamp, as everywhere in this binary),
/// else the clock.
pub fn instant(now: Option<&str>) -> chrono::DateTime<chrono::Utc> {
    now.and_then(|text| text.parse::<chrono::NaiveDateTime>().ok())
        .and_then(|naive| chrono::TimeZone::from_local_datetime(&chrono::Local, &naive).single())
        .map_or_else(chrono::Utc::now, |local| local.with_timezone(&chrono::Utc))
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Run `handler` over the process's payload, plane and clock.
pub fn with_hook<T>(payload: &str, now: Option<&str>, handler: impl FnOnce(&Hook) -> T) -> T {
    let here = Where::here();
    let data: serde_json::Value = serde_json::from_str(payload).unwrap_or(serde_json::Value::Null);
    let host = charter_core::dispatch::host();
    let hook = Hook {
        root: &here.root,
        in_plane: here.in_plane,
        payload: &data,
        env: &env,
        cwd: &here.cwd,
        now: instant(now),
        host: &host,
    };
    handler(&hook)
}

/// The tool hook `name`, if it is one this file answers, run and printed.
pub fn tool(name: &str, now: Option<&str>) -> Option<ExitCode> {
    let handler: fn(&Hook) -> Answer = match name {
        "pretooluse-read" => toolhooks::pretooluse_read,
        "pretooluse-edit" => toolhooks::pretooluse_edit,
        "pretooluse-dispatch" => toolhooks::pretooluse_dispatch,
        "posttooluse" => toolhooks::posttooluse,
        "posttooluse-skill" => toolhooks::posttooluse_skill,
        "posttooluse-dispatch" => toolhooks::posttooluse_dispatch,
        "posttooluse-message" => toolhooks::posttooluse_message,
        _ => return None,
    };
    let answer = with_hook(&crate::payload(), now, handler);
    Some(answered(&answer))
}

/// Print an [`Answer`]: nothing, a line, or a denial with its exit-2 fallback.
pub fn answered(answer: &Answer) -> ExitCode {
    match answer {
        Answer::Nothing => ExitCode::SUCCESS,
        Answer::Say(line) => {
            say(line);
            ExitCode::SUCCESS
        }
        Answer::Deny(verdict) => crate::guard::deny(verdict),
    }
}

/// One line on stdout. A failure is dropped: what failed to print is context or an allow, and
/// neither is worth the exit status that would turn it into a block.
pub fn say(line: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// `sessionstart`'s own work, before the app is told anything: the piece announcement, this
/// session's heartbeat, the persona tool gate's ceiling, and the briefing — printed as
/// `additionalContext`. Silent outside a plane, where there is no persona and no workspace.
pub fn sessionstart(payload: &str, now: Option<&str>) {
    with_hook(payload, now, |hook| {
        if !hook.in_plane {
            return;
        }
        // BEFORE this session's own heartbeat, which would otherwise replace the holder's mark
        // and hide the collision the announcement exists to show.
        let piece = charter_core::briefing::piece_announcement(hook.root, hook.payload, hook.now);
        hook.touch_piece();
        // Freeze every persona's tools before the session has had a turn in which to rewrite
        // one (charter#432). Keyed on the payload's id, as every later ask is.
        let explicit = hook.payload.get("session_id").and_then(|v| v.as_str());
        if let Some(sid) = charter_core::hookstate::session(explicit, &env) {
            charter_core::personagate::snapshot(hook.root, &sid);
        }
        let ask = charter_core::briefing::Ask {
            root: hook.root,
            cwd: hook.cwd,
            payload: hook.payload,
            env: &env,
            now: hook.now,
        };
        let mut parts = charter_core::briefing::parts(&ask, piece);
        let workspace = charter_core::briefing::workspace_of(&ask);
        // Each extension that is on here and adds a section, quoted as data under its name —
        // and each that hears it told a chat started — within one bounded wait
        // (charter-app#343). None of it can hold the start past `Bounds::SESSION_START`, and a
        // machine with no config directory has no extension to ask.
        if let Some(config) = charter_core::machine::config_root_if_there() {
            use charter_core::extension::briefing::{self, Asked, Bounds};
            // `BuiltIn::none()`, as `charter statusline`: the binary does not know where an app
            // bundle is, so a built-in extension neither briefs nor hears a chat start
            // (ADR 0041's amendment for charter-app#343).
            let briefed = briefing::at_session_start(
                &config,
                &charter_core::extension::BuiltIn::none(),
                &charter_core::extension::project::Choices::read_in(hook.root, Some(&workspace)),
                &Asked {
                    workspace: workspace.clone(),
                    persona: env(charter_core::active::PERSONA_ENV).filter(|it| !it.is_empty()),
                },
                Bounds::SESSION_START,
            );
            parts.extend(briefed.parts);
            for note in briefed.notes {
                eprintln!("charter: {note}");
            }
        }
        // Reports kept for this workspace because the chat that asked for them has closed
        // (charter-app#259): the next chat to START here is the one that learns them — never a
        // chat already running that compacted or cleared, which would take them as a side
        // effect of its own housekeeping.
        let starting = hook
            .payload
            .get("source")
            .and_then(|v| v.as_str())
            .is_none_or(|source| source == "startup");
        let kept = if starting {
            charter_core::handback::take(
                hook.root,
                charter_core::handback::For::Workspace(&workspace),
            )
        } else {
            Vec::new()
        };
        if let Some(reports) = charter_core::handback::context(&kept, true) {
            parts.push(reports);
        }
        if let Some(line) = charter_core::briefing::emitted(&parts) {
            say(&line);
        }
    });
}

/// `userpromptsubmit`'s own work: the heartbeat, and any report a chat this one handed work to
/// has sent back (charter-app#259), handed to the turn that is starting as
/// `additionalContext` — quoted as data, never typed into the chat. The rest of the Python
/// handler — the roster, the commitment gate, the "control plane updated" note — is not
/// ported (see the Hook help).
pub fn userpromptsubmit(payload: &str, now: Option<&str>) {
    with_hook(payload, now, |hook| {
        hook.touch_piece();
        if !hook.in_plane {
            return;
        }
        // The app's number for this chat, which is what a report is left under. A chat the app
        // did not start has none, and nothing was left for it.
        let Some(chat) = env(charter_core::hookwire::CHAT_ENV).and_then(|id| id.parse().ok())
        else {
            return;
        };
        let reports =
            charter_core::handback::take(hook.root, charter_core::handback::For::Chat(chat));
        if let Some(text) = charter_core::handback::context(&reports, false) {
            say(&charter_core::handback::emitted("UserPromptSubmit", &text));
        }
    });
}

/// `charter hook --list`.
pub fn list(json: bool) -> ExitCode {
    if json {
        say(&charter_core::hookreg::json());
    } else {
        print!("{}", charter_core::hookreg::table());
    }
    ExitCode::SUCCESS
}
