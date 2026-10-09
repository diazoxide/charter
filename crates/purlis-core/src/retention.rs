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
//! **A dispatch record is session data too** (#1452). `.charter/app/dispatches/<id>.json` holds
//! a brief and a report, and is collected a month after its dispatch ended, as the record says
//! (#1556): a later write of it, such as forgetting what its two chats said, does not keep it
//! longer. One that has not ended, or whose end cannot be read, is collected a month after it
//! was last written. Either way not while the chat that asked or the chat that worked is one
//! the reopen record brings back, by its id and not its number, which another launch deals
//! again ([`crate::dispatchrecord::aged_from`]). A write of one that was cut short leaves its
//! temporary file beside it, holding the same brief: it is collected by the same rule.
//!
//! **A trace that records a secret handed out is kept** (V71). Those events
//! ([`crate::secrets::cmd::HANDED_OUT`]) are the only record of which credential went where
//! until AU-5 writes them into the audit chain; once it does, they follow the 30-day rule too.
//!
//! **Only what this names, and only this plane's.** Plain files directly in those
//! directories, whose names are ones charter writes there. The directories are
//! `<plane>/.charter/…` — never `$CHARTER_HOME`, which can be one directory several planes
//! share, and which the pointer writers (`wscmd::select`, `active`) do not read either. Each
//! directory is opened from the plane one component at a time without following a link, and
//! every file is aged, read and removed through that handle, so a link swapped in on the way
//! after the sweep started cannot point it anywhere else. The tool gate's `.tools` and `.gate`
//! are [`crate::personagate::sweep_ceilings`]'s; the hook spool, the event log, `terminals/`
//! and every other store under `.charter/` are left alone.

use std::path::Path;
use std::time::{Duration, SystemTime};

/// How long a per-session file is kept after it was last written.
pub const KEEP_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The largest trace read to look for a secret hand-out. A larger one is kept unread.
const MOST_TRACE_BYTES: u64 = 64 * 1024 * 1024;

/// What one sweep removed, per store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Swept {
    pub sessions: usize,
    pub traces: usize,
    pub reports: usize,
    /// Dispatch records ([`crate::dispatchrecord`], #1452).
    pub dispatches: usize,
}

/// Remove the per-session files of `plane` older than [`KEEP_FOR`] at `now`, except those of
/// a session in `live` and a trace that records a secret handed out.
pub fn sweep(plane: &Path, now: SystemTime, live: &[String]) -> Swept {
    sweep_keeping(plane, now, live, &[])
}

/// [`sweep`], also keeping the dispatch records of the chats in `chats`: the ones a live chat
/// asked for or worked on, matched by the chat's ULID ([`crate::dispatchrecord::same_chat`]).
/// A chat's number is dealt again in another launch, so a number alone keeps only a record
/// that names its chat by nothing else.
pub fn sweep_keeping(
    plane: &Path,
    now: SystemTime,
    live: &[String],
    chats: &[crate::dispatchrecord::Live],
) -> Swept {
    Swept {
        sessions: collect(
            plane,
            &[crate::names::state_name(plane), "sessions"],
            now,
            |name| a_marker(name) && !of_a_live_session(name, live),
            |_, written| Some(written),
        ),
        traces: collect(
            plane,
            &[crate::names::state_name(plane), "persona-state", "trace"],
            now,
            |name| {
                name.strip_suffix(".jsonl")
                    .is_some_and(|sid| !sid.is_empty() && crate::hookstate::safe(sid) == sid)
                    && !of_a_live_session(name, live)
            },
            |file, written| (!hands_out_a_secret(file)).then_some(written),
        ),
        reports: sweep_reports(plane, now),
        // Read however lately written: a record says when its dispatch ended (#1556).
        dispatches: collect_by(
            plane,
            &[
                crate::names::state_name(plane),
                "app",
                crate::dispatchrecord::DIR_NAME,
            ],
            now,
            // A record, or what a record's write left behind when it was cut short: a
            // temporary file holding a brief, which nothing else ever removes.
            |name| {
                crate::dispatchrecord::a_record(name)
                    || crate::dispatchrecord::a_record_s_temp(name)
            },
            true,
            |file, written| crate::dispatchrecord::aged_from(file, written, now, chats),
        ),
    }
}

/// [`sweep`] as the app runs it when it opens a plane: `live` is every chat the plane's
/// reopen record ([`crate::reopen`]) will bring back — its number, which `$CHARTER_SESSION_ID`
/// carries, and the conversation it resumes, which the usage ring is keyed on. Run before any
/// chat of the plane has started, so nothing else is live in this app.
///
/// **Only a record this charter can read says which chats are live.** No record at all is a
/// plane no app has quit in, and every session file is swept by age. A record that parsed at
/// this [`crate::reopen::VERSION`] with every chat numbered gives the live list. Anything else
/// — a record that does not parse, one of another version, one that cannot be read, one
/// written before a chat kept its number ([`crate::reopen::Chat::number`]) — cannot tell a
/// returning chat's month-old pointer from a gone one's, so every session file and trace is
/// kept and only the report drafts, which no chat reads, are collected.
///
/// **What this does not see.** A chat outside the app — a harness started in a terminal — is
/// in no reopen record, so its files go when it has written nothing for 30 days, and one that
/// resumes after that starts without its pointers. So does a chat brought back later from a
/// record (Resume) rather than reopened at this launch. And a `.charter/` copied or synced
/// from another device carries that device's ages and that device's chats.
pub fn on_open(plane: &Path, now: SystemTime) -> Swept {
    let record = match crate::reopen::read_strictly(plane) {
        Ok(None) => crate::reopen::Record::default(),
        Ok(Some(record)) if record.chats.iter().all(|chat| chat.number.is_some()) => record,
        _ => {
            return Swept {
                reports: sweep_reports(plane, now),
                ..Swept::default()
            };
        }
    };
    let live: Vec<String> = record
        .chats
        .iter()
        .flat_map(|chat| {
            let number = chat.number.map(|n| n.to_string());
            let resume = chat.resume.as_ref().map(|id| id.as_str().to_owned());
            number.into_iter().chain(resume)
        })
        .collect();
    sweep_keeping(plane, now, &live, &crate::dispatchrecord::Live::of(&record))
}

/// The Python charter's report drafts, which are no session's. This plane's only, as every
/// store here: a `$CHARTER_HOME/reports` the Python charter may have shared between planes is
/// not one plane's to collect.
fn sweep_reports(plane: &Path, now: SystemTime) -> usize {
    collect(
        plane,
        &[crate::names::state_name(plane), "reports"],
        now,
        a_report_draft,
        |_, written| Some(written),
    )
}

/// Whether a trace records a secret handed out: one of [`crate::secrets::cmd::HANDED_OUT`] as
/// a JSON string anywhere in it. Over-keeps a trace that merely quotes one; a trace that cannot
/// be read whole as text, or is larger than [`MOST_TRACE_BYTES`], counts as one.
fn hands_out_a_secret(file: &mut std::fs::File) -> bool {
    use std::io::Read;
    let mut text = String::new();
    let read = file
        .take(MOST_TRACE_BYTES + 1)
        .read_to_string(&mut text)
        .is_ok_and(|n| n as u64 <= MOST_TRACE_BYTES);
    !read
        || crate::secrets::cmd::HANDED_OUT
            .iter()
            .any(|event| text.contains(&format!("\"{event}\"")))
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

/// Remove every month-old plain file directly in `plane/<steps…>` whose name `ours` takes; how
/// many went. `from` answers when a file is aged from, given the file and when it was last
/// written, or `None` for one it keeps however old.
///
/// A file last written within the month is not read, but in a store whose files say their own
/// time (`own_time`): there a file rewritten since can still be a month past it.
fn collect(
    plane: &Path,
    steps: &[&str],
    now: SystemTime,
    ours: impl Fn(&str) -> bool,
    from: impl Fn(&mut std::fs::File, SystemTime) -> Option<SystemTime>,
) -> usize {
    collect_by(plane, steps, now, ours, false, from)
}

/// [`collect`], reading every file of a store whose files say their own time
/// (`own_time`), however lately each was written.
fn collect_by(
    plane: &Path,
    steps: &[&str],
    now: SystemTime,
    ours: impl Fn(&str) -> bool,
    own_time: bool,
    from: impl Fn(&mut std::fs::File, SystemTime) -> Option<SystemTime>,
) -> usize {
    let Some(store) = held::Dir::open(plane, steps) else {
        return 0;
    };
    let mut gone = 0;
    for name in store.names().into_iter().filter(|name| ours(name)) {
        let Some(mut file) = store.file(&name) else {
            continue;
        };
        let Some(written) = file
            .metadata()
            .ok()
            .filter(std::fs::Metadata::is_file)
            .and_then(|found| found.modified().ok())
        else {
            continue;
        };
        if !own_time && !aged(written, now) {
            continue;
        }
        let stale = from(&mut file, written).is_some_and(|at| aged(at, now));
        if stale && store.remove(&name) {
            gone += 1;
        }
    }
    gone
}

/// Whether `at` is [`KEEP_FOR`] or more before `now`.
fn aged(at: SystemTime, now: SystemTime) -> bool {
    now.duration_since(at).is_ok_and(|age| age >= KEEP_FOR)
}

/// A store's directory, held for the length of one sweep.
mod held {
    use std::path::{Path, PathBuf};

    /// On unix, a descriptor opened from the plane one component at a time with `O_NOFOLLOW`,
    /// and every file aged, read and unlinked relative to it: a component swapped for a link
    /// after the walk cannot move a removal out of the directory that was opened.
    #[cfg(unix)]
    pub(super) struct Dir {
        fd: std::os::fd::OwnedFd,
        path: PathBuf,
    }

    #[cfg(unix)]
    impl Dir {
        /// `plane/<steps…>`, or `None` when a step is missing, not a directory, or a link.
        pub(super) fn open(plane: &Path, steps: &[&str]) -> Option<Self> {
            use rustix::fs::{Mode, OFlags};
            let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
            let mut fd = rustix::fs::open(plane, flags, Mode::empty()).ok()?;
            for step in steps {
                fd =
                    rustix::fs::openat(&fd, *step, flags | OFlags::NOFOLLOW, Mode::empty()).ok()?;
            }
            let path = steps
                .iter()
                .fold(plane.to_path_buf(), |at, step| at.join(step));
            Some(Self { fd, path })
        }

        /// The names in the directory. Read by path: a name is only a candidate, and every
        /// act on it below goes through the descriptor.
        pub(super) fn names(&self) -> Vec<String> {
            names(&self.path)
        }

        /// `name` in this directory, opened read-only without following a link and without
        /// blocking on a FIFO.
        pub(super) fn file(&self, name: &str) -> Option<std::fs::File> {
            use rustix::fs::{Mode, OFlags};
            let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
            rustix::fs::openat(&self.fd, name, flags, Mode::empty())
                .ok()
                .map(std::fs::File::from)
        }

        /// Unlink `name` from this directory; whether it went.
        pub(super) fn remove(&self, name: &str) -> bool {
            rustix::fs::unlinkat(&self.fd, name, rustix::fs::AtFlags::empty()).is_ok()
        }
    }

    /// Elsewhere, the path, checked for a link on the way from the plane before each act.
    #[cfg(not(unix))]
    pub(super) struct Dir {
        plane: PathBuf,
        path: PathBuf,
    }

    #[cfg(not(unix))]
    impl Dir {
        pub(super) fn open(plane: &Path, steps: &[&str]) -> Option<Self> {
            let path = steps
                .iter()
                .fold(plane.to_path_buf(), |at, step| at.join(step));
            crate::contain::no_link_on_the_way(plane, &path).ok()?;
            path.is_dir().then(|| Self {
                plane: plane.to_path_buf(),
                path,
            })
        }

        pub(super) fn names(&self) -> Vec<String> {
            names(&self.path)
        }

        pub(super) fn file(&self, name: &str) -> Option<std::fs::File> {
            crate::contain::open_no_link(&self.plane, &self.path.join(name)).ok()
        }

        pub(super) fn remove(&self, name: &str) -> bool {
            let path = self.path.join(name);
            crate::contain::no_link_on_the_way(&self.plane, &path).is_ok()
                && std::fs::remove_file(path).is_ok()
        }
    }

    fn names(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;
