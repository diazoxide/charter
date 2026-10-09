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
//! longer. One that has not ended, whose end cannot be read, or that still owes its asking
//! chat a report (marked when the report reached no chat, which can be long after the end), is
//! collected a month after it was last written. Either way not while the chat that asked or the chat that worked is one
//! the reopen record brings back, by its id and not its number, which another launch deals
//! again ([`crate::dispatchrecord::aged_from`]). A write of one that was cut short leaves its
//! temporary file beside it, holding the same brief: it is collected by the same rule.
//!
//! **A trace that records a secret handed out is kept** (V71). Those events
//! ([`crate::secrets::cmd::HANDED_OUT`]) are the only record of which credential went where
//! until AU-5 writes them into the audit chain; once it does, they follow the 30-day rule too.
//!
//! **The rest of the per-session stores** (#1004) are collected by the same month and the same
//! live rule, a returning chat's number or conversation keeping its own: the commit gate's
//! cooldowns (`commit-gate/<sid>`), the first-edit-in-a-clone markers
//! (`ws-edit-nudge/<sid>-<ws>`), the saved-record lines a workspace's chat left for a `Stop`
//! that never ran (`workspaces/<ws>/<state>/sessions/<chat>.saved`; the plane root's are in
//! `sessions/`), and a session's ephemeral memory (`persona-state/ephemeral/<session>/`),
//! removed whole only once every file and folder in it is a month old. Losing a cooldown or a
//! nudge marker costs a returning session one more question or one more reminder, never its
//! work. `chat-turns/` needs no rule: nothing writes it any more, and nothing reads it.
//!
//! **Only what this names, and only this plane's.** Plain files directly in those
//! directories, whose names are ones charter writes there. The directories are
//! `<plane>/.charter/…` — never `$CHARTER_HOME`, which can be one directory several planes
//! share, and which the pointer writers (`wscmd::select`, `active`) do not read either. Each
//! directory is opened from the plane one component at a time without following a link, and
//! every file is aged, read and removed through that handle, so a link swapped in on the way
//! after the sweep started cannot point it anywhere else. A file is looked at again after it
//! is read and kept when it was written to or replaced meanwhile. The tool gate's `.tools` and `.gate`
//! are [`crate::personagate::sweep_ceilings`]'s; the hook spool, the event log and every
//! other store under `.charter/` are left alone.
//!
//! **`purlis workspace use` collects through here too** ([`on_select`], #1025): the month-old
//! markers in `sessions/` by the same live rule, and the per-terminal pointers in
//! `terminals/`, which no reopen record names, by their age alone.

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
    /// What a chat's harness said its session cost ([`crate::usage::spend_dir`], #1457).
    pub spend: usize,
    /// The commit gate's per-session cooldowns, `commit-gate/<sid>` (#1004).
    pub gates: usize,
    /// The first-edit-in-a-clone markers, `ws-edit-nudge/<sid>-<ws>` (#1004).
    pub nudges: usize,
    /// Saved-record lines no `Stop` passed on, `workspaces/<ws>/<state>/sessions/<chat>.saved`
    /// (#1004). The plane root's own are in `sessions/` and counted there.
    pub saved: usize,
    /// Sessions' ephemeral memory, one `persona-state/ephemeral/<session>/` each (#1004).
    pub ephemeral: usize,
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
        sessions: sweep_sessions(plane, now, live),
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
        // A chat's figure, by its id: kept while the chat comes back, however old.
        spend: collect(
            plane,
            &[
                crate::names::state_name(plane),
                "app",
                crate::usage::SPEND_DIR_NAME,
            ],
            now,
            |name| {
                name.strip_suffix(".json").is_some_and(|chat| {
                    !chat.is_empty() && !chats.iter().any(|live| live.id.as_deref() == Some(chat))
                })
            },
            |_, written| Some(written),
        ),
        // A cooldown is one session's, named by its id: kept while that session comes back.
        gates: collect(
            plane,
            &[crate::names::state_name(plane), "commit-gate"],
            now,
            |name| a_session_key(name) && !live.iter().any(|sid| sid == name),
            |_, written| Some(written),
        ),
        // `<sid>-<ws>`: kept for any live session the name can begin with.
        nudges: collect(
            plane,
            &[crate::names::state_name(plane), "ws-edit-nudge"],
            now,
            |name| {
                a_session_key(name)
                    && !live.iter().any(|sid| {
                        name.strip_prefix(sid.as_str())
                            .is_some_and(|rest| rest.starts_with('-'))
                    })
            },
            |_, written| Some(written),
        ),
        saved: sweep_saved(plane, now, live),
        ephemeral: sweep_ephemeral(plane, now, live),
    }
}

/// A name a per-session store's writer makes from a session id: non-empty, every character one
/// [`crate::hookstate::safe`] keeps, and neither a dotfile nor `.`/`..`.
fn a_session_key(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && crate::hookstate::safe(name) == name
}

/// The month-old saved-record lines in each workspace's own `sessions/`
/// ([`crate::sessionrecord::relay::marker`]), but a live chat's. A line is passed on only
/// within minutes of being left, so an aged one is never sent; the live rule is kept anyway,
/// so the number a returning chat keeps keeps its files, as it does in the plane's `sessions/`.
fn sweep_saved(plane: &Path, now: SystemTime, live: &[String]) -> usize {
    let Some(workspaces) = held::Dir::open(plane, &["workspaces"]) else {
        return 0;
    };
    workspaces
        .names()
        .into_iter()
        .filter(|ws| crate::contain::workspace_name_ok(ws))
        .map(|ws| {
            let state = crate::names::state_name(&plane.join("workspaces").join(&ws));
            collect(
                plane,
                &["workspaces", ws.as_str(), state, "sessions"],
                now,
                |name| {
                    name.strip_suffix(".saved").is_some_and(|chat| {
                        !chat.is_empty() && chat.bytes().all(|b| b.is_ascii_digit())
                    }) && !of_a_live_session(name, live)
                },
                |_, written| Some(written),
            )
        })
        .sum()
}

/// The ephemeral memory of every session that is not live and whose every file and folder was
/// last written a month or more ago, removed whole; how many sessions went.
///
/// **Only the shape charter writes** ([`crate::recall::ephemeral_dir`]):
/// `<session>/<persona>/<file>`. A session folder holding anything else — a link, a folder
/// deeper down, a name charter never makes, something that cannot be read — is kept whole.
/// A file is removed only while it is the one judged, and a folder only once it is empty, so
/// a write landing during the sweep keeps what it wrote.
fn sweep_ephemeral(plane: &Path, now: SystemTime, live: &[String]) -> usize {
    let Some(store) = held::Dir::open(
        plane,
        &[
            crate::names::state_name(plane),
            "persona-state",
            "ephemeral",
        ],
    ) else {
        return 0;
    };
    let mut gone = 0;
    for session in store.names() {
        if !a_session_key(&session) || live.contains(&session) {
            continue;
        }
        let Some(dir) = store.dir(&session) else {
            continue;
        };
        let Some(held) = ephemeral_of(&dir, now) else {
            continue;
        };
        let mut whole = true;
        for (_, persona, files) in &held {
            for (name, found) in files {
                whole &= unchanged(persona, name, found) && persona.remove(name);
            }
        }
        for (name, _, _) in &held {
            whole &= dir.remove_dir(name);
        }
        if whole && store.remove_dir(&session) {
            gone += 1;
        }
    }
    gone
}

/// One persona's ephemeral folder in a session: its name, the folder held, and each file in it
/// with what it was found as.
type Held = (String, held::Dir, Vec<(String, std::fs::Metadata)>);

/// A session's ephemeral folders and their files, when every one is charter's shape and
/// month-old; `None` when anything says to keep it.
fn ephemeral_of(session: &held::Dir, now: SystemTime) -> Option<Vec<Held>> {
    let month_old = |found: &std::fs::Metadata| found.modified().is_ok_and(|at| aged(at, now));
    if !session.metadata().is_some_and(|found| month_old(&found)) {
        return None;
    }
    let mut held = Vec::new();
    for persona in session.names() {
        if !a_session_key(&persona) {
            return None;
        }
        let dir = session.dir(&persona)?;
        if !dir.metadata().is_some_and(|found| month_old(&found)) {
            return None;
        }
        let mut files = Vec::new();
        for name in dir.names() {
            let found = dir
                .file(&name)?
                .metadata()
                .ok()
                .filter(|found| found.is_file() && month_old(found))?;
            files.push((name, found));
        }
        held.push((persona, dir, files));
    }
    Some(held)
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
    let Some(record) = returning(plane) else {
        return Swept {
            reports: sweep_reports(plane, now),
            ..Swept::default()
        };
    };
    sweep_keeping(
        plane,
        now,
        &live_of(&record),
        &crate::dispatchrecord::Live::of(&record),
    )
}

/// What `purlis workspace use` collects when it writes a pointer (#1025): the month-old
/// per-session markers in `sessions/`, by [`on_open`]'s rule of which chats are live, and the
/// month-old per-terminal pointers in `terminals/`. How many went.
///
/// One rule with the sweep on open, through the same held directory: a chat the reopen
/// record brings back keeps its month-old `<n>.workspace` and `<n>.lock`, a record that cannot
/// say which chats come back keeps every session marker, the tool gate's `.tools` and `.gate`
/// stay [`crate::personagate::sweep_ceilings`]'s, and nothing is followed through a link.
///
/// A per-terminal pointer is keyed by the pane, which no reopen record names, so it is
/// collected by its age alone, as it always was.
pub fn on_select(plane: &Path, now: SystemTime) -> usize {
    let sessions =
        returning(plane).map_or(0, |record| sweep_sessions(plane, now, &live_of(&record)));
    let terminals = collect(
        plane,
        &[crate::names::state_name(plane), "terminals"],
        now,
        a_marker,
        |_, written| Some(written),
    );
    sessions + terminals
}

/// The reopen record that says which chats come back, or `None` when it cannot: one that does
/// not parse, is of another version, cannot be read, or was written before a chat kept its
/// number. No record at all is a project no app has quit in, where no chat comes back.
fn returning(plane: &Path) -> Option<crate::reopen::Record> {
    match crate::reopen::read_strictly(plane) {
        Ok(None) => Some(crate::reopen::Record::default()),
        Ok(Some(record)) if record.chats.iter().all(|chat| chat.number.is_some()) => Some(record),
        _ => None,
    }
}

/// The keys a returning chat's session files are under: its number, which
/// `$CHARTER_SESSION_ID` carries, and the conversation it resumes.
fn live_of(record: &crate::reopen::Record) -> Vec<String> {
    record
        .chats
        .iter()
        .flat_map(|chat| {
            let number = chat.number.map(|n| n.to_string());
            let resume = chat.resume.as_ref().map(|id| id.as_str().to_owned());
            number.into_iter().chain(resume)
        })
        .collect()
}

/// The month-old per-session markers in `sessions/`, but a live session's.
fn sweep_sessions(plane: &Path, now: SystemTime, live: &[String]) -> usize {
    collect(
        plane,
        &[crate::names::state_name(plane), "sessions"],
        now,
        |name| a_marker(name) && !of_a_live_session(name, live),
        |_, written| Some(written),
    )
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
///
/// **Read in bounded pieces** ([`SCAN_CHUNK`]), never whole (#1027): a trace near the cap held
/// 64 MiB in memory for the length of its scan. Each piece is searched together with the tail
/// of the one before, as long as the longest needle less one byte, so a needle cut by a piece's
/// edge is still found.
fn hands_out_a_secret(file: &mut std::fs::File) -> bool {
    // Past the cap by its own size: kept without a byte read. One that grows past it while it
    // is read is caught by the count below.
    if file
        .metadata()
        .is_ok_and(|found| found.len() > MOST_TRACE_BYTES)
    {
        return true;
    }
    scan_for_hand_outs(file, SCAN_CHUNK)
}

/// How much of a trace [`hands_out_a_secret`] holds at once.
const SCAN_CHUNK: usize = 64 * 1024;

/// [`hands_out_a_secret`] over any reader, `chunk` bytes at a time.
///
/// Every fail-safe of the whole read is kept: a read error, a byte sequence that is not UTF-8
/// (a sequence cut by a piece's edge is completed by the next piece first, and one still open
/// at the end is not text), and more than [`MOST_TRACE_BYTES`] all answer "kept".
fn scan_for_hand_outs(mut from: impl std::io::Read, chunk: usize) -> bool {
    let needles: Vec<Vec<u8>> = crate::secrets::cmd::HANDED_OUT
        .iter()
        .map(|event| format!("\"{event}\"").into_bytes())
        .collect();
    let overlap = needles.iter().map(Vec::len).max().unwrap_or(1) - 1;
    let mut piece = vec![0_u8; chunk.max(1)];
    // The tail of what was searched last, carried so a needle across the edge is whole.
    let mut window: Vec<u8> = Vec::with_capacity(overlap + piece.len());
    // A UTF-8 sequence the last piece ended inside, at most three bytes.
    let mut open: Vec<u8> = Vec::new();
    let mut total: u64 = 0;
    loop {
        let n = match from.read(&mut piece) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return true,
        };
        total += n as u64;
        if total > MOST_TRACE_BYTES {
            return true;
        }
        let read = &piece[..n];
        open.extend_from_slice(read);
        match std::str::from_utf8(&open) {
            Ok(_) => open.clear(),
            Err(e) if e.error_len().is_none() => {
                open.drain(..e.valid_up_to());
            }
            Err(_) => return true,
        }
        window.extend_from_slice(read);
        if needles
            .iter()
            .any(|needle| memchr::memmem::find(&window, needle).is_some())
        {
            return true;
        }
        let keep = window.len().min(overlap);
        window.drain(..window.len() - keep);
    }
    // A sequence still open at the end is not text.
    !open.is_empty()
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
        let Some((found, written)) = file
            .metadata()
            .ok()
            .filter(std::fs::Metadata::is_file)
            .and_then(|found| found.modified().ok().map(|written| (found, written)))
        else {
            continue;
        };
        if !own_time && !aged(written, now) {
            continue;
        }
        let stale = from(&mut file, written).is_some_and(|at| aged(at, now));
        // Looked at again after the read, which for a trace can be long (#1027): a file
        // written to meanwhile, or another renamed over the name, is not the one judged stale.
        if stale && unchanged(&store, &name, &found) && store.remove(&name) {
            gone += 1;
        }
    }
    gone
}

/// Whether `name` in `store` is still the file `before` describes: the same file, neither
/// written to nor replaced since. What is left is the moment between this look and the unlink.
fn unchanged(store: &held::Dir, name: &str, before: &std::fs::Metadata) -> bool {
    store
        .file(name)
        .and_then(|file| file.metadata().ok())
        .is_some_and(|now| same_file_unwritten(before, &now))
}

#[cfg(unix)]
fn same_file_unwritten(before: &std::fs::Metadata, now: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    before.dev() == now.dev()
        && before.ino() == now.ino()
        && before.size() == now.size()
        && before.mtime() == now.mtime()
        && before.mtime_nsec() == now.mtime_nsec()
}

#[cfg(not(unix))]
fn same_file_unwritten(before: &std::fs::Metadata, now: &std::fs::Metadata) -> bool {
    before.len() == now.len() && before.modified().ok() == now.modified().ok()
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

        /// `name` in this directory, opened read-only without following a link, without
        /// blocking on a FIFO, and never as a controlling terminal.
        pub(super) fn file(&self, name: &str) -> Option<std::fs::File> {
            use rustix::fs::{Mode, OFlags};
            let flags = OFlags::RDONLY
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::NOCTTY
                | OFlags::CLOEXEC;
            rustix::fs::openat(&self.fd, name, flags, Mode::empty())
                .ok()
                .map(std::fs::File::from)
        }

        /// Unlink `name` from this directory; whether it went.
        pub(super) fn remove(&self, name: &str) -> bool {
            rustix::fs::unlinkat(&self.fd, name, rustix::fs::AtFlags::empty()).is_ok()
        }

        /// The directory `name` in this one, held the same way: `None` when it is missing, not
        /// a directory, or a link.
        pub(super) fn dir(&self, name: &str) -> Option<Self> {
            use rustix::fs::{Mode, OFlags};
            let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
            let fd = rustix::fs::openat(&self.fd, name, flags, Mode::empty()).ok()?;
            Some(Self {
                fd,
                path: self.path.join(name),
            })
        }

        /// What this directory is now, read through its descriptor.
        pub(super) fn metadata(&self) -> Option<std::fs::Metadata> {
            let fd = self.fd.try_clone().ok()?;
            std::fs::File::from(fd).metadata().ok()
        }

        /// Remove the empty directory `name` from this one; whether it went.
        pub(super) fn remove_dir(&self, name: &str) -> bool {
            rustix::fs::unlinkat(&self.fd, name, rustix::fs::AtFlags::REMOVEDIR).is_ok()
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

        pub(super) fn dir(&self, name: &str) -> Option<Self> {
            let path = self.path.join(name);
            crate::contain::no_link_on_the_way(&self.plane, &path).ok()?;
            path.is_dir().then(|| Self {
                plane: self.plane.clone(),
                path,
            })
        }

        pub(super) fn metadata(&self) -> Option<std::fs::Metadata> {
            crate::contain::no_link_on_the_way(&self.plane, &self.path).ok()?;
            std::fs::symlink_metadata(&self.path).ok()
        }

        pub(super) fn remove_dir(&self, name: &str) -> bool {
            let path = self.path.join(name);
            crate::contain::no_link_on_the_way(&self.plane, &path).is_ok()
                && std::fs::remove_dir(path).is_ok()
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
