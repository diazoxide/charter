//! How long charter keeps the per-session state it writes into a plane's `.charter/` (SC-7).
//!
//! Three stores grew for as long as a plane was used, because nothing that writes them ever
//! removes them: the per-session markers in `.charter/sessions/` (230 on one plane), the
//! session traces in `.charter/persona-state/trace/`, and the report drafts the Python charter
//! left in `.charter/reports/`. Each is Clone state and transient (ADR 0069), so a month after
//! a file was last written it is collected — when the app opens the plane ([`on_open`]),
//! before any of its chats starts.
//!
//! **What a live chat needs is kept, however old.** A chat the plane's reopen record brings
//! back reads its workspace pointer, its lock and its trace under the number it keeps across a
//! launch, and its usage ring under the conversation it resumes; every file under either key
//! stays. A file a running session still writes has a fresh modification time, which is the
//! age this reads, so it stays too.
//!
//! **Only what this names.** Plain files directly in those three directories, whose names are
//! ones charter writes there, never through a link. The tool gate's `.tools` and `.gate` are
//! [`crate::personagate::sweep_ceilings`]'s; the hook spool, the event log, `terminals/` and
//! every other store under `.charter/` are left alone.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long a per-session file is kept after it was last written.
pub const KEEP_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// What one sweep removed, per store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Swept {
    pub sessions: usize,
    pub traces: usize,
    pub reports: usize,
}

/// Remove the per-session files of `plane` older than [`KEEP_FOR`] at `now`, except those of
/// a session in `live`.
pub fn sweep(plane: &Path, now: SystemTime, live: &[String]) -> Swept {
    let state = crate::hookstate::State::of(plane);
    Swept {
        sessions: collect(state.trust(), &state.sessions(), now, |name| {
            a_marker(name) && !of_a_live_session(name, live)
        }),
        traces: collect(plane, &trace_dir(plane), now, |name| {
            name.strip_suffix(".jsonl")
                .is_some_and(|sid| !sid.is_empty() && crate::hookstate::safe(sid) == sid)
                && !of_a_live_session(name, live)
        }),
        reports: sweep_reports(&state, now),
    }
}

/// [`sweep`] as the app runs it when it opens a plane: `live` is every chat the plane's
/// reopen record ([`crate::reopen`]) will bring back — its number, which `$CHARTER_SESSION_ID`
/// carries, and the conversation it resumes, which the usage ring is keyed on. Run before any
/// chat of the plane has started, so nothing else is live in this app.
///
/// **A record that cannot be read keeps every session's files.** It is the only list of the
/// chats that come back; without it, a chat's month-old pointer cannot be told from a gone
/// one's, so only the report drafts, which no chat reads, are collected. So does a record
/// written before a chat kept its number ([`crate::reopen::Chat::number`]): its chats are
/// dealt numbers again at the launch, and which pointer becomes whose is not known yet.
pub fn on_open(plane: &Path, now: SystemTime) -> Swept {
    match crate::reopen::read_or_refusal(plane) {
        Ok(record) if record.chats.iter().all(|chat| chat.number.is_some()) => {
            let live: Vec<String> = record
                .chats
                .iter()
                .flat_map(|chat| {
                    let number = chat.number.map(|n| n.to_string());
                    let resume = chat.resume.as_ref().map(|id| id.as_str().to_owned());
                    number.into_iter().chain(resume)
                })
                .collect();
            sweep(plane, now, &live)
        }
        _ => Swept {
            reports: sweep_reports(&crate::hookstate::State::of(plane), now),
            ..Swept::default()
        },
    }
}

/// The report drafts, which are no session's.
fn sweep_reports(state: &crate::hookstate::State, now: SystemTime) -> usize {
    collect(
        state.trust(),
        &state.dir().join("reports"),
        now,
        a_report_draft,
    )
}

/// Where the session traces are: [`crate::trace::file`]'s directory, which is below the plane
/// whatever `$CHARTER_HOME` says.
fn trace_dir(plane: &Path) -> PathBuf {
    let file = crate::trace::file(plane, crate::trace::NO_SESSION);
    file.parent().map(Path::to_path_buf).unwrap_or(file)
}

/// Remove one file, never through a link on the way from `trust`; whether it went.
fn remove(trust: &Path, path: &Path) -> bool {
    crate::contain::no_link_on_the_way(trust, path).is_ok() && std::fs::remove_file(path).is_ok()
}

/// A report draft the Python charter wrote: `<id>.json`, the id 16 lowercase hex digits
/// (`charter/report.py`'s fingerprint).
fn a_report_draft(name: &str) -> bool {
    name.strip_suffix(".json").is_some_and(|id| {
        id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// A per-session marker in `sessions/` this sweep collects: a name charter makes
/// (`<sid>.<ending>`, every character one [`crate::hookstate::safe`] keeps, never a dotfile),
/// and not the tool gate's ceiling or its marker, which
/// [`crate::personagate::sweep_ceilings`] collects sooner and in the order a live session
/// needs.
fn a_marker(name: &str) -> bool {
    let Some((sid, ending)) = name.split_once('.') else {
        return false;
    };
    !sid.is_empty()
        && crate::hookstate::safe(name) == name
        && !matches!(name.rsplit_once('.'), Some((_, "tools" | "gate")))
        && !ending.is_empty()
}

/// Whether `name` is one of a live session's files: `<sid>.` and anything, for a `sid` in
/// `live`. A prefix and not a parse, because a session id may itself hold a dot (`ide.2`)
/// and an ask marker holds three; a name two sessions could both claim is kept.
fn of_a_live_session(name: &str, live: &[String]) -> bool {
    live.iter().any(|sid| {
        name.strip_prefix(sid.as_str())
            .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// Remove every plain file directly in `dir` that `ours` claims and that was last written
/// [`KEEP_FOR`] or more before `now`; how many went.
fn collect(trust: &Path, dir: &Path, now: SystemTime, ours: impl Fn(&str) -> bool) -> usize {
    if crate::contain::no_link_on_the_way(trust, dir).is_err() {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut gone = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !ours(&name) || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|found| found.modified())
            .is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age >= KEEP_FOR));
        if stale && remove(trust, &entry.path()) {
            gone += 1;
        }
    }
    gone
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;
