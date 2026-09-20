//! How a hook process reaches the app: one line on a unix socket the app owns.
//!
//! A hook runs inside the chat's own process tree, so everything it needs is already in its
//! environment — the socket to write to and the chat it belongs to. It looks nothing up, reads
//! no plane and opens no file, which is what keeps it inside the spec's 50 ms.
//!
//! **It can never break a turn.** Every failure here is silent and fast: no app listening, a
//! socket that has gone, a payload that will not parse. A harness whose turn fails because
//! charter wanted to draw a spinner is worse than a spinner that is wrong.

use std::io;

use crate::state::{Detail, Ending, Event, Started};

// The channel is a unix socket. Windows has `AF_UNIX` but Rust's standard library does not
// expose it, and a socket file's `0600` — which is what keeps anything else on the machine
// out of the channel — has no mode bit to set there either. So Windows gets a second answer,
// and until it is written that answer is a REFUSAL and not a quiet no-op: `bind` returns
// `Unsupported`, `send` returns `Unsupported`, and an app that cannot open its hook channel
// says so instead of running with a channel nothing is listening on (charter-app#95).
//
// This used to be a `compile_error!`, which stopped the whole crate at expansion and so hid
// every OTHER thing Windows has to say about the core. What is below says the same thing at
// the same volume and lets the rest of the build be measured (M4).

/// The socket a hook writes to, in the environment of every session the app starts.
pub const SOCKET_ENV: &str = "CHARTER_HOOK_SOCKET";

/// Which chat the hook is running inside, in the same environment.
///
/// The app's own number for the chat, set at the `exec`, exactly as the Python charter sets
/// `$CHARTER_SESSION_ID` (`charter/hooks.py:_chat_id`). A hook already knows which chat it is
/// in; nothing has to be worked out from a payload.
pub const CHAT_ENV: &str = "CHARTER_CHAT";

/// Where Claude Code puts the conversation a hook is running in.
///
/// ADR 0024, C7, measured again on claude 2.1.276: it always equals the payload's
/// `session_id`. It is read only as a fallback, and it is what keeps the check against a
/// nested harness standing when a payload cannot be read — a pipe can be slow, an
/// environment cannot. A harness that sets no such variable simply has no fallback.
pub const CLAUDE_CONVERSATION_ENV: &str = "CLAUDE_CODE_SESSION_ID";

/// Where Claude Code puts the pid of the harness a hook is running under.
///
/// It is what tells `/clear` from a nested harness, and the Python charter's
/// `_record_harness_report` turns on it: a new conversation id from the SAME pid is `/clear`
/// (ADR 0024, C6) and the chat follows it; a new id from a DIFFERENT pid is a `claude` run
/// inside the chat (C5) and is ignored. Without it the two are the same event.
pub const CLAUDE_PID_ENV: &str = "CLAUDE_PID";

/// Everything a session must NOT inherit from whatever started charter.
///
/// charter may itself be launched from inside a harness session — an operator running the app
/// from a chat, or a scenario test — and then every chat it starts inherits that harness's
/// identity. A hook would report the LAUNCHER's conversation and process, so the chat's own
/// report would look like somebody else's: either refused as a nested harness, or, worse,
/// taken as one chat's when it is another's.
///
/// A scenario test found this: a fake harness with no identity of its own reported the pid of
/// the Claude Code session that had started the app.
///
/// Only the harness the app starts may set these, and it sets them for its own hooks.
pub const NOT_INHERITED: &[&str] = &[CLAUDE_CONVERSATION_ENV, CLAUDE_PID_ENV, "CLAUDECODE"];

/// Which conversation a report is of, and how well that is known.
///
/// **"I could not tell" and "someone is lying" are different evidence, and merging them was a
/// defect.** An earlier version answered `Option<String>` for both, so the chat's own harness
/// having a slow payload looked exactly like a nested one contradicting its environment — and
/// the honest harness's report was dropped, leaving the chat `running` for ever. A review
/// found it.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conversation {
    /// The payload and the environment agree, or only one of them exists.
    Named(String),
    /// They disagree. Something is reporting a conversation that is not its own — a harness
    /// nested in the chat's shell is the case ADR 0024's C5 names.
    Contradicted,
    /// A payload that parsed and named no conversation charter recognises.
    ///
    /// **A harness speaking another dialect.** opencode puts it under `sessionID`, not
    /// `session_id` (ADR 0024, O1), and charter has not measured every harness there is. In a
    /// chat that has already adopted one, a report in a dialect charter cannot read is by
    /// definition not that chat's own — a review reproduced a nested `opencode` marking the
    /// outer Claude chat as waiting, mid-turn, because it inherits `CLAUDE_PID` and sets
    /// neither variable of its own.
    Foreign,
    /// Nothing could be read at all: no JSON, because charter's own deadline passed or the
    /// pipe was cut. Not evidence of anything, and never treated as such.
    #[default]
    Unknown,
}

/// What one hook tells the app.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Report {
    /// The app's number for the chat the hook ran in.
    pub chat: u32,
    /// The event the harness fired.
    pub event: Event,
    /// Which conversation the harness says this is.
    ///
    /// The app checks it against the conversation it started the chat under, so a harness
    /// nested INSIDE the chat cannot move the chat's state (ADR 0024, C5).
    #[serde(default)]
    pub conversation: Conversation,
    /// The pid of the harness that fired this, where the harness names one.
    ///
    /// The whole of what tells `/clear` (C6) from a nested harness (C5): both report an id
    /// the chat has not seen, and only the pid says which happened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// What the harness said about this event beyond its name: what a `SessionStart` was for,
    /// and what a `SessionEnd` was for. Each is meaningless on the other's event and ignored
    /// there.
    #[serde(default)]
    pub detail: Detail,
}

impl Report {
    /// The report a hook makes, read from its environment and its payload.
    ///
    /// `payload` is the harness's JSON on stdin. It is read for one field and never required:
    /// the EVENT comes from argv, which charter wrote itself, so a payload that will not parse
    /// still reports the event — it just cannot say which conversation fired it.
    pub fn read(event: Event, payload: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        // No chat means this harness was not started by the app — the operator's own
        // `claude` in a terminal, with hooks pointed here. There is no chat to move, and
        // inventing one would move somebody else's.
        let chat: u32 = env(CHAT_ENV)?.parse().ok()?;
        let read = serde_json::from_str::<serde_json::Value>(payload).ok();
        let field = |name: &str| {
            read.as_ref()
                .and_then(|payload| payload.get(name))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        let said = field("session_id");
        let source = field("source");
        // `/clear` fires `SessionEnd(reason=clear)` and then `SessionStart(source=clear)`
        // from the same process — measured on claude 2.1.276. Without this the first of the
        // two would end the chat and the second could never be heard.
        let reason = field("reason");
        Some(Self {
            chat,
            event,
            conversation: conversation(said, read.is_some(), env),
            pid: env(CLAUDE_PID_ENV)
                .and_then(|pid| pid.parse().ok())
                .filter(|pid| *pid > 0),
            detail: Detail {
                started: Started::of(source.as_deref()),
                ending: Ending::of(reason.as_deref()),
            },
        })
    }
}

/// Which conversation this report is of, and how well that is known.
///
/// **The payload and the environment must AGREE, and this used to be an `or`.** That was
/// backwards, and a review proved it: the environment holds the OUTER chat's id — a harness
/// nested in the chat's own shell inherits it — so falling back to it when a payload is slow
/// does not lose the id, it substitutes the wrong one, and the nested harness's report is
/// then taken as the outer chat's. The very case ADR 0024's C5 exists to refuse.
///
/// The Python charter has always required the two to be equal (`charter/hooks.py`), and ADR
/// 0024's consequences say so outright: "A Claude Code report counts only when
/// `CLAUDE_CODE_SESSION_ID` equals its payload's id."
///
/// A harness that sets no such variable — Codex — has nothing to disagree with, so its
/// payload stands alone.
fn conversation(
    said: Option<String>,
    parsed: bool,
    env: &dyn Fn(&str) -> Option<String>,
) -> Conversation {
    // A payload charter READ but could not find a conversation in is a harness speaking a
    // dialect charter has not measured — not the same thing as a payload it could not read at
    // all, and the difference is what keeps a nested one out.
    if said.is_none() && parsed {
        return Conversation::Foreign;
    }
    match (said, env(CLAUDE_CONVERSATION_ENV)) {
        // Claude Code, and the two agree: this is its own hook (ADR 0024, C7).
        (Some(said), Some(here)) if said == here => Conversation::Named(said),
        // Claude Code, and they disagree. Something is reporting a conversation that is not
        // the one its own process is in.
        (Some(_), Some(_)) => Conversation::Contradicted,
        // The environment names one and the payload could not be read at all. That is not a
        // contradiction — it is charter failing to read, and the report must not be punished
        // for it. The pid is what identifies the harness in this case.
        (None, Some(_)) => Conversation::Unknown,
        // No such variable at all: a harness that is not Claude Code. Its payload is all
        // there is, and it is not contradicted by anything.
        (Some(said), None) => Conversation::Named(said),
        (None, None) => Conversation::Unknown,
    }
}

/// Sends one report to the socket at `path`. Answers whether the app took it.
///
/// One connection is one report, written and closed. Nothing is waited for: the app has the
/// line, and a hook that waited for an answer would be spending a harness's turn on a
/// spinner.
#[cfg(unix)]
pub fn send(path: &std::path::Path, report: &Report) -> io::Result<()> {
    use std::io::Write;

    // A path that is not there, and a socket file whose app has gone, both refuse at once
    // — `ENOENT` and `ECONNREFUSED`. Neither can hang, so no timeout is armed for them.
    let mut socket = std::os::unix::net::UnixStream::connect(path)?;
    let mut line = serde_json::to_vec(report).map_err(io::Error::other)?;
    line.push(b'\n');
    socket.write_all(&line)?;
    socket.flush()
}

/// There is no channel to send on where charter has no unix socket.
///
/// The hook's own contract already covers this — every failure here is silent and fast, and
/// `charter hook` drops the error on the floor — so a Windows hook costs its turn nothing.
/// What it does NOT do is pretend: the error names the platform, so a `doctor` that asks
/// gets an answer rather than a success that moved nothing.
#[cfg(not(unix))]
pub fn send(_path: &std::path::Path, _report: &Report) -> io::Result<()> {
    Err(no_channel())
}

/// The one refusal both halves give, so the two cannot drift into two different stories.
#[cfg(not(unix))]
fn no_channel() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "charter's hook channel is a unix socket with a 0600 mode on it, and neither has an \
         answer on this platform yet (charter-app#95)",
    )
}

/// The socket the app listens on for hook reports.
#[cfg(unix)]
pub struct Listener {
    socket: std::os::unix::net::UnixListener,
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl Listener {
    /// Binds a fresh socket at `socket`, replacing one left behind by a process that is gone.
    ///
    /// `within` is where containment begins: every component from it down to the socket must
    /// not be a link (`contain::no_link_on_the_way`, shared with the record `reopen` keeps in
    /// the same directory). It is named by the caller and never worked out here — a walk from
    /// the root of the filesystem would refuse every path on macOS, where `/tmp` and `/var`
    /// are themselves links.
    ///
    /// **This is the one caller of that walk that cannot close the window after it** (charter
    /// ADR 0028). `reopen` opens the record through `contain::open_no_link`, which puts
    /// `O_NOFOLLOW` on the open so the kernel answers the last component at the instant it is
    /// opened; `bind` takes a path and there is no portable `bindat`, so the walk here, the
    /// `remove_file` of a stale socket below, the `bind` itself and the `set_permissions`
    /// after it are four path calls with three windows between them. The socket carries
    /// nothing secret and executes nothing it is handed, which is why that is accepted and
    /// written down rather than worked around; the fix is the same `openat`-beneath-a-
    /// descriptor rewrite ADR 0028 puts at M3.
    pub fn bind(within: &std::path::Path, socket: &std::path::Path) -> io::Result<Self> {
        // The same walk `reopen` uses for the record in this very directory
        // (charter-app#28), and the same one, not a second copy of it: two containment gates
        // drift, which is the failure this repo has found five times.
        crate::contain::no_link_on_the_way(within, socket)?;
        let path = socket;
        if let Some(parent) = path.parent() {
            private_directory(parent)?;
        }
        // `bind` refuses an address already in use, and a socket file outlives the process
        // that made it — so an app that was killed would stop the next one from listening at
        // all. Removing it first is the standard answer, and the single-instance plugin is
        // what makes it safe: there is no second live app whose socket this could be.
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        let socket = std::os::unix::net::UnixListener::bind(path)?;
        // Whoever can write here can move a chat's state and raise a notification. Nothing
        // secret travels over it and nothing it carries is executed, so this is not a
        // secret's lock — it is the difference between "the operator" and "anything running
        // on the machine", and it is one call.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            socket,
            path: path.to_path_buf(),
        })
    }

    /// The path a hook writes to.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Hands every report to `each`, on a thread of its own, until the listener is dropped.
    ///
    /// One connection is one report. A caller that cannot keep up does not block a harness:
    /// the hook has already written its line and gone.
    pub fn each(self, each: Box<dyn Fn(Report) + Send + Sync + 'static>) -> Reading {
        let stopping = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let path = self.path.clone();
        let stopped = std::sync::Arc::clone(&stopping);
        let each: std::sync::Arc<dyn Fn(Report) + Send + Sync> = std::sync::Arc::from(each);
        let reading = std::thread::spawn(move || {
            for connection in self.socket.incoming() {
                if stopped.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                // One bad connection is one lost report, never the end of the channel: an
                // app that stopped listening because something wrote nonsense would leave
                // every chat frozen at the state it last had, and never say so. The two
                // reachable ways in — a line that will not read, and one that will not parse
                // — have a test; `accept` failing does not, because nothing a test can do
                // makes it fail. It is written the same way for the same reason.
                let Ok(connection) = connection else { continue };
                // **A thread per connection, and this used to be one thread for all of
                // them.** An independent review reproduced the consequence: anything that
                // connects and does not write — `nc -U` on the socket, a hook stopped in a
                // debugger, a child that inherited the descriptor — held the single reader
                // inside `read_line` forever. Every other session's report queued behind it,
                // the sidebar went quiet with nothing to say why, and quitting deadlocked in
                // this thread's `join`.
                //
                // A report is a few dozen bytes and the thread lives for as long as one
                // takes to arrive, so this is not fifty threads; it is however many hooks
                // are mid-write, which is almost always none.
                let each = std::sync::Arc::clone(&each);
                let started = std::thread::Builder::new()
                    .name("charter-hook-report".into())
                    .spawn(move || {
                        if let Some(report) = read_one(connection) {
                            each(report);
                        }
                    });
                // A thread that will not start costs this one report. Refusing the rest of
                // the channel over it would cost every report after it too.
                let _ = started;
            }
        });
        Reading {
            stopping,
            path,
            reading: Some(reading),
        }
    }
}

/// Makes `directory`, and makes sure nobody else on the machine may enter it.
///
/// **The directory, not only the socket, and the mode is set as it is CREATED.** Between a
/// `bind` and a `chmod` there is a window where the socket sits at whatever the umask
/// allowed, and the socket's own path is predictable — on Linux the fallback lives in `/tmp`,
/// which everyone can write to. A local user who got there first would own the channel: they
/// could read every report and inject their own, moving chats and raising notifications.
///
/// A review found the earlier version, which created the directory at the umask and then
/// DISCARDED the result of tightening it. Every step here is checked, and a path that is
/// already something else — a symlink, a file, a directory somebody else owns — is refused
/// rather than used.
#[cfg(unix)]
fn private_directory(directory: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    if let Some(above) = directory.parent() {
        std::fs::create_dir_all(above)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(directory) {
        Ok(()) => return Ok(()),
        // Ours from a previous run, or somebody else's. The checks below decide which.
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err),
    }
    // `symlink_metadata`, so a symlink pointing at a directory is not mistaken for one:
    // `set_permissions` would follow it and tighten whatever it aims at instead.
    let found = std::fs::symlink_metadata(directory)?;
    if !found.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} is not a directory", directory.display()),
        ));
    }
    // Checked, not discarded: this fails for a directory charter does not own, which is
    // exactly the case worth refusing to start the channel over.
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

/// How long one connection has to say its piece.
///
/// A hook writes its line and closes at once; this is only a bound on something that does
/// not. Short, because the thread holding it is doing nothing else, and generous next to the
/// 1.8 ms the whole hook call was measured at.
///
/// `cfg(unix)` because its one reader is: off unix there is no connection to bound.
#[cfg(unix)]
const A_REPORT_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(2);

/// The most one report may be.
///
/// A report is a chat number, an event word, a uuid and a pid — well under 200 bytes. The cap
/// is what stops a client that writes without ever sending a newline from growing a `String`
/// in the app's memory until there is none left: the deadline above bounds how LONG one may
/// write, and two seconds of writing is gigabytes.
///
/// `cfg(unix)` for the same reason as the deadline above it.
#[cfg(unix)]
const A_REPORT_IS_AT_MOST: u64 = 64 * 1024;

/// Reads one report from one connection, or nothing.
#[cfg(unix)]
fn read_one(connection: std::os::unix::net::UnixStream) -> Option<Report> {
    use std::io::{BufRead, Read};

    // Both directions: a client that connects and neither writes nor closes must not hold
    // this thread past the deadline.
    let _ = connection.set_read_timeout(Some(A_REPORT_TAKES_AT_MOST));
    let mut line = String::new();
    std::io::BufReader::new(connection.take(A_REPORT_IS_AT_MOST))
        .read_line(&mut line)
        .ok()?;
    serde_json::from_str::<Report>(&line).ok()
}

/// A listener being read on its own thread. Dropping it stops the reading.
#[cfg(unix)]
pub struct Reading {
    stopping: std::sync::Arc<std::sync::atomic::AtomicBool>,
    path: std::path::PathBuf,
    reading: Option<std::thread::JoinHandle<()>>,
}

#[cfg(unix)]
impl Drop for Reading {
    fn drop(&mut self) {
        self.stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
        // `accept` is blocked in the thread and nothing else will wake it, so the flag alone
        // would leave it there for the life of the process. One connection of our own is
        // what it is waiting for.
        let _ = std::os::unix::net::UnixStream::connect(&self.path);
        if let Some(reading) = self.reading.take() {
            let _ = reading.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The channel the app could not open, where there is no unix socket to open.
///
/// **An empty enum and not a struct with a stub in it.** Nothing can construct one, so
/// [`Listener::path`] and [`Listener::each`] below are not "unimplemented" — they are
/// unreachable, and the compiler is the one saying so. The only way in is [`Listener::bind`],
/// and it refuses. An app that takes that refusal for what it is cannot end up holding a
/// channel that quietly carries nothing.
#[cfg(not(unix))]
pub enum Listener {}

#[cfg(not(unix))]
impl Listener {
    /// Refuses, with the reason. See the note at the top of this module.
    pub fn bind(_within: &std::path::Path, _socket: &std::path::Path) -> io::Result<Self> {
        Err(no_channel())
    }

    /// Unreachable: no `Listener` is ever constructed on this platform.
    pub fn path(&self) -> &std::path::Path {
        match *self {}
    }

    /// Unreachable, for the same reason.
    pub fn each(self, _each: Box<dyn Fn(Report) + Send + Sync + 'static>) -> Reading {
        match self {}
    }
}

/// [`Reading`]'s counterpart on a platform with no channel, and empty for the same reason.
#[cfg(not(unix))]
pub enum Reading {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    // Named here rather than at the top of the module: the module's own code needs neither
    // of these off unix, and an import that is only right on one platform belongs with the
    // code that is only compiled there.
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::mpsc;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |want| {
            pairs
                .iter()
                .find(|(k, _)| k == want)
                .map(|(_, v)| v.clone())
        }
    }

    const CLAUDE_STOP: &str = r#"{"session_id":"11111111-2222-4333-8444-555555555555",
        "transcript_path":"/tmp/t.jsonl","cwd":"/tmp","hook_event_name":"Stop",
        "stop_hook_active":false,"last_assistant_message":"pong"}"#;

    #[test]
    fn a_hook_reports_the_chat_its_environment_names() {
        // Measured on claude 2.1.276: the payload carries `session_id`, and it is the same
        // value as `$CLAUDE_CODE_SESSION_ID` (ADR 0024, C7).
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);

        let report = Report::read(Event::Stop, CLAUDE_STOP, &env).expect("a report");

        assert_eq!(report.chat, 7);
        assert_eq!(report.event, Event::Stop);
        assert_eq!(
            report.conversation,
            Conversation::Named("11111111-2222-4333-8444-555555555555".to_owned())
        );
    }

    #[test]
    fn a_hook_outside_a_chat_the_app_started_reports_nothing() {
        // No `CHARTER_CHAT` means this harness was not started by the app — the operator's
        // own `claude` in a terminal, with the plugin's hooks pointed here. There is no chat
        // to move, and inventing one would move somebody else's.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock")]);

        assert_eq!(Report::read(Event::Stop, CLAUDE_STOP, &env), None);
    }

    #[test]
    fn a_chat_number_that_is_not_one_reports_nothing() {
        for bad in ["", "seven", "-1", "4294967296", " 7"] {
            let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, bad)]);
            assert_eq!(
                Report::read(Event::Stop, CLAUDE_STOP, &env),
                None,
                "{bad:?} was taken as a chat"
            );
        }
    }

    #[test]
    fn a_payload_that_will_not_parse_still_reports_the_event() {
        // The event came from argv, which charter wrote into the harness's own settings. A
        // truncated or empty payload costs the conversation and nothing else — losing the
        // EVENT would leave a chat marked as working forever.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);

        for payload in ["", "{", "not json at all"] {
            let report = Report::read(Event::Stop, payload, &env)
                .unwrap_or_else(|| panic!("{payload:?} reported nothing"));
            assert_eq!(report.chat, 7);
            assert_eq!(report.event, Event::Stop);
            assert_eq!(report.conversation, Conversation::Unknown);
        }
    }

    #[test]
    fn a_payload_that_parses_but_names_no_conversation_is_a_dialect_charter_cannot_read() {
        // Different from a payload charter could not read at ALL, and the difference is what
        // keeps a nested harness out: opencode names its conversation under `sessionID`
        // (ADR 0024, O1), inherits `CLAUDE_PID` and sets neither variable of its own. A
        // review reproduced one marking the outer Claude chat as waiting, mid-turn.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);

        for payload in [
            "null",
            "[]",
            r#"{"session_id":42}"#,
            r#"{"sessionID":"11111111-2222-4333-8444-555555555555"}"#,
        ] {
            let report = Report::read(Event::Stop, payload, &env)
                .unwrap_or_else(|| panic!("{payload:?} reported nothing"));
            assert_eq!(
                report.conversation,
                Conversation::Foreign,
                "{payload:?} was read as charter's own dialect"
            );
        }
    }

    #[test]
    fn a_conversation_counts_only_when_the_payload_and_the_environment_agree() {
        // ADR 0024, C7: every Claude Code hook's `CLAUDE_CODE_SESSION_ID` equals its
        // payload's `session_id`. The Python charter requires the two to be equal and so
        // does this.
        let env = env_of(&[
            (SOCKET_ENV, "/tmp/s.sock"),
            (CHAT_ENV, "7"),
            (
                CLAUDE_CONVERSATION_ENV,
                "11111111-2222-4333-8444-555555555555",
            ),
            (CLAUDE_PID_ENV, "4242"),
        ]);

        let report = Report::read(Event::Stop, CLAUDE_STOP, &env).expect("a report");

        assert_eq!(
            report.conversation,
            Conversation::Named("11111111-2222-4333-8444-555555555555".to_owned())
        );
        assert_eq!(report.pid, Some(4242));
    }

    #[test]
    fn a_lost_payload_names_no_conversation_rather_than_the_one_in_the_environment() {
        // **This is a defect an independent review found, kept as a test so it cannot come
        // back.** The environment holds the OUTER chat's id, because a harness nested in the
        // chat's shell inherits it. Falling back to it when a payload is slow does not lose
        // the id — it substitutes the wrong one, and the nested harness's report is then
        // taken as the outer chat's. Exactly what ADR 0024's C5 exists to refuse.
        let env = env_of(&[
            (SOCKET_ENV, "/tmp/s.sock"),
            (CHAT_ENV, "7"),
            (CLAUDE_CONVERSATION_ENV, "the-outer-chat"),
        ]);

        let report = Report::read(Event::Stop, "", &env).expect("a report");

        assert_eq!(
            report.conversation,
            Conversation::Unknown,
            "the outer chat's id was borrowed"
        );
    }

    #[test]
    fn a_payload_that_disagrees_with_the_environment_names_no_conversation() {
        // A nested harness whose payload charter can read: it says its own id, and the
        // environment says the outer one. Neither is this chat's report.
        let env = env_of(&[
            (SOCKET_ENV, "/tmp/s.sock"),
            (CHAT_ENV, "7"),
            (CLAUDE_CONVERSATION_ENV, "the-outer-chat"),
        ]);

        let report = Report::read(Event::Stop, CLAUDE_STOP, &env).expect("a report");

        // Contradicted, not merely unknown: charter read both and they disagree. That is
        // different evidence from "I could not tell", and the board treats them differently.
        assert_eq!(report.conversation, Conversation::Contradicted);
    }

    #[test]
    fn a_conversation_that_merely_starts_the_same_way_is_a_different_conversation() {
        // A surviving mutant: `said == here` weakened to `said.starts_with(&here)` passed
        // every test, because nothing compared two ids that were nearly the same. This one
        // line is what the whole check against a nested harness rests on.
        let env = env_of(&[
            (SOCKET_ENV, "/tmp/s.sock"),
            (CHAT_ENV, "7"),
            (
                CLAUDE_CONVERSATION_ENV,
                "11111111-2222-4333-8444-555555555555",
            ),
        ]);
        let longer = r#"{"session_id":"11111111-2222-4333-8444-555555555555-nested"}"#;

        assert_eq!(
            Report::read(Event::Stop, longer, &env)
                .expect("a report")
                .conversation,
            Conversation::Contradicted
        );
    }

    #[test]
    fn a_harness_that_names_no_conversation_in_its_environment_is_taken_at_its_payload() {
        // Codex sets no `CLAUDE_CODE_SESSION_ID`, so its payload is contradicted by nothing.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);

        let report = Report::read(Event::Stop, CLAUDE_STOP, &env).expect("a report");

        assert_eq!(
            report.conversation,
            Conversation::Named("11111111-2222-4333-8444-555555555555".to_owned())
        );
        assert_eq!(report.pid, None);
    }

    #[test]
    fn the_variables_a_session_must_not_inherit_are_the_ones_that_name_a_harness() {
        // Whatever the app inherited, a chat must start without these: they say which
        // conversation and which process a hook belongs to, and only the harness the app
        // starts may answer that.
        assert!(NOT_INHERITED.contains(&CLAUDE_CONVERSATION_ENV));
        assert!(NOT_INHERITED.contains(&CLAUDE_PID_ENV));
        // charter's own two are set per session, after these are removed, so they are not
        // here — removing them would remove what the app just put in.
        assert!(!NOT_INHERITED.contains(&SOCKET_ENV));
        assert!(!NOT_INHERITED.contains(&CHAT_ENV));
    }

    #[test]
    fn a_pid_that_is_not_one_is_no_pid() {
        // `charter/hooks.py` treats a missing or non-numeric `CLAUDE_PID` as "no report" for
        // adoption. A zero is not a pid either.
        for bad in ["", "0", "-1", "nine", "12x"] {
            let env = env_of(&[
                (SOCKET_ENV, "/tmp/s.sock"),
                (CHAT_ENV, "7"),
                (CLAUDE_PID_ENV, bad),
            ]);
            assert_eq!(
                Report::read(Event::Stop, CLAUDE_STOP, &env)
                    .expect("a report")
                    .pid,
                None,
                "{bad:?} was taken as a pid"
            );
        }
    }

    #[test]
    fn a_report_reaches_the_app_that_is_listening() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

        let sent = Report {
            chat: 7,
            event: Event::Notification,
            conversation: Conversation::Named("abc".to_owned()),
            pid: Some(99),
            detail: Detail::default(),
        };
        send(&path, &sent).expect("the app took it");

        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(sent));
    }

    #[test]
    fn every_report_arrives_even_when_many_hooks_fire_at_once() {
        // Fifty live sessions is the product's scale, and a turn ending in each of them is
        // fifty hook processes at the same moment.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report.chat);
        }));

        let path = Arc::new(path);
        let senders: Vec<_> = (0..50u32)
            .map(|chat| {
                let path = Arc::clone(&path);
                std::thread::spawn(move || {
                    send(
                        &path,
                        &Report {
                            chat,
                            event: Event::Stop,
                            conversation: Conversation::Unknown,
                            pid: None,
                            detail: Detail::default(),
                        },
                    )
                })
            })
            .collect();
        for sender in senders {
            sender.join().expect("the thread").expect("it was taken");
        }

        let mut seen: Vec<u32> = (0..50)
            .map(|_| {
                rx.recv_timeout(std::time::Duration::from_secs(10))
                    .expect("a report")
            })
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn one_connection_that_says_nothing_useful_does_not_silence_the_channel() {
        // An app that stopped listening because something wrote nonsense at it would leave
        // every chat frozen at the state it last had — and never say so. Anything at all can
        // reach this socket: a stale hook from an older charter, a port scanner, a person
        // with `nc`.
        use std::io::Write;

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

        // Connected and closed without a word; a line that is not JSON; JSON that is not a
        // report; and a report missing the one field that names a chat.
        drop(std::os::unix::net::UnixStream::connect(&path).expect("a connection"));
        for rubbish in [
            "not json at all\n",
            "{}\n",
            r#"{"event":"stop"}"#,
            r#"{"chat":"seven","event":"stop"}"#,
        ] {
            let mut socket = std::os::unix::net::UnixStream::connect(&path).expect("a connection");
            socket.write_all(rubbish.as_bytes()).expect("it is written");
        }

        let good = Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            detail: Detail::default(),
        };
        send(&path, &good).expect("the app took it");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(good),
            "the channel went quiet after something wrote rubbish at it"
        );
    }

    #[test]
    fn a_client_that_connects_and_says_nothing_does_not_freeze_the_channel() {
        // **A defect an independent review reproduced, kept as a test.** The listener used
        // to read every connection on one thread, so anything that connected and did not
        // write — `nc -U` on the socket, a hook stopped in a debugger, a child that
        // inherited the descriptor — held it inside `read_line` forever. Every other
        // session's report queued behind it, and the sidebar went quiet with nothing to say
        // why.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let (tx, rx) = mpsc::channel();
        let _reading = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .each(Box::new(move |report| {
                let _ = tx.send(report);
            }));

        // Connected, never written to, and held open for the rest of the test.
        let silent = std::os::unix::net::UnixStream::connect(&path).expect("a connection");
        silent
            .set_read_timeout(Some(A_REPORT_TAKES_AT_MOST * 4))
            .expect("a deadline of our own");

        let good = Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            detail: Detail::default(),
        };
        send(&path, &good).expect("the app took it");

        // Well inside the deadline the silent connection will eventually hit: the point is
        // that the good report does not WAIT for it. One reader for every connection would
        // still get there, two seconds later, with fifty sessions' reports behind it.
        assert_eq!(
            rx.recv_timeout(A_REPORT_TAKES_AT_MOST / 4),
            Ok(good),
            "a report queued behind a client that never spoke"
        );

        // And the silent one is let go rather than held forever: the app closes its end when
        // its deadline passes. Without that, every such connection costs a thread for as long
        // as the app runs.
        //
        // End of file OR an error, and the distinction that matters is WHICH error: a clean
        // close reads `Ok(0)` on macOS and can read `ECONNRESET` on Linux, and CI found that
        // difference. `WouldBlock` is the one answer that means the app did NOT let go — it
        // is this client's own deadline firing, four times longer than the app's.
        let mut nothing = [0u8; 1];
        let let_go = (&silent).read(&mut nothing);
        assert!(
            !matches!(&let_go, Err(err) if err.kind() == io::ErrorKind::WouldBlock),
            "the app never let go of a client that said nothing"
        );
    }

    #[test]
    fn a_client_that_connects_and_says_nothing_does_not_hold_up_shutting_down() {
        // The same defect's other half: `Reading::drop` joins the reading thread, and a
        // thread stuck in `read_line` never returns — so quitting the app hung. Quitting is
        // the one thing that must always finish.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let reading = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .each(Box::new(|_| {}));
        let _silent = std::os::unix::net::UnixStream::connect(&path).expect("a connection");

        let (done, waited) = mpsc::channel();
        std::thread::spawn(move || {
            drop(reading);
            let _ = done.send(());
        });

        assert_eq!(
            waited.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(()),
            "shutting down waited on a client that never spoke"
        );
    }

    #[test]
    fn a_client_that_writes_without_ever_ending_a_line_is_cut_off() {
        // Nothing stops a client writing bytes and never sending a newline. Without a cap
        // the app grows a `String` for it until there is no memory left.
        use std::io::Write;

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let (tx, rx) = mpsc::channel();
        let _reading = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .each(Box::new(move |report| {
                let _ = tx.send(report);
            }));

        let mut flood = std::os::unix::net::UnixStream::connect(&path).expect("a connection");
        // Set before anything is written, while the connection is certainly healthy.
        flood
            .set_read_timeout(Some(A_REPORT_TAKES_AT_MOST * 4))
            .expect("a deadline of our own");
        flood
            .set_write_timeout(Some(A_REPORT_TAKES_AT_MOST * 4))
            .expect("a deadline of our own");
        // Comfortably past the cap, with not one newline in it. The app stops reading at the
        // cap and closes, so this either errors or the read below sees end of file — what it
        // must never do is keep taking bytes.
        for _ in 0..40 {
            if flood.write_all(&vec![b'x'; 8 * 1024]).is_err() {
                break;
            }
        }
        // **Inside the deadline, not merely eventually.** Without the cap the app keeps
        // reading until the read deadline passes, so an assertion with no clock in it passes
        // either way — a review pointed that out. The cap is observable by WHEN the app lets
        // go, not by memory.
        let began = std::time::Instant::now();
        let mut nothing = [0u8; 1];
        let let_go = (&flood).read(&mut nothing);
        assert!(
            !matches!(&let_go, Err(err) if err.kind() == io::ErrorKind::WouldBlock),
            "the app is still reading a line that will never end"
        );
        assert!(
            began.elapsed() < A_REPORT_TAKES_AT_MOST / 4,
            "the app read for {:?}, so it was the deadline that stopped it and not the cap",
            began.elapsed()
        );

        let good = Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            detail: Detail::default(),
        };
        send(&path, &good).expect("the app took it");
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(good));
    }

    #[test]
    fn the_socket_sits_in_a_directory_nobody_else_may_enter() {
        // The socket's own mode is set after `bind`, and between the two it is at whatever
        // the umask allowed. A directory nobody else may enter closes that window, and is
        // what actually holds under a permissive umask.
        let dir = tempfile::tempdir().expect("a directory");
        let inside = dir.path().join("app");
        let path = inside.join("hooks.sock");

        let _listener = Listener::bind(dir.path(), &path).expect("a socket");

        let mode = std::fs::metadata(&inside)
            .expect("the directory is there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "the socket's directory is {mode:o}");
    }

    #[test]
    fn a_socket_will_not_be_bound_inside_a_directory_somebody_else_could_have_made() {
        // The fallback path is predictable and, on Linux, lives in `/tmp`. Somebody who gets
        // there first must not end up owning the channel — they could read every report and
        // inject their own. A directory that is not a directory is the shape of that attempt
        // charter can always detect; one owned by another account fails the chmod below it.
        let dir = tempfile::tempdir().expect("a directory");
        let squatted = dir.path().join("app");
        std::fs::write(&squatted, b"not a directory").expect("something else is there first");

        let refused = Listener::bind(dir.path(), &squatted.join("hooks.sock"));

        assert!(
            refused.is_err(),
            "charter bound a socket under something it does not own"
        );
    }

    #[test]
    fn a_socket_reached_through_a_link_higher_up_is_refused() {
        // **The gate sat one level shallower than the write.** `private_directory` checked
        // the directory it was about to make and nothing above it, so `.charter` being a link
        // redirected where charter created a directory and bound a socket — outside the
        // plane, somewhere chosen by whoever wrote the link. `app` itself being a link was
        // caught; the component above it was not.
        //
        // charter-app#28 ruled this exact directory may not be reached through a link, for
        // the record it already keeps there. The socket is charter's own file in charter's
        // own directory: a link anywhere on the way to it has no honest use.
        let plane = tempfile::tempdir().expect("a plane");
        let elsewhere = tempfile::tempdir().expect("somewhere outside it");
        std::os::unix::fs::symlink(elsewhere.path(), plane.path().join(".charter"))
            .expect("a link in the way");

        let refused = Listener::bind(
            plane.path(),
            &plane.path().join(".charter").join("app").join("hooks.sock"),
        );

        assert!(refused.is_err(), "charter bound a socket through a link");
        assert!(
            !elsewhere.path().join("app").exists(),
            "it made a directory outside the plane on the way"
        );
    }

    #[test]
    fn a_component_charter_cannot_even_look_at_is_refused_rather_than_walked_past() {
        // A surviving mutant: taking ANY error as "not there yet" passed everything. It is
        // only `NotFound` that means charter is about to make it — a component it cannot
        // stat is one it knows nothing about, and walking past it would be assuming the rest
        // of the path is what it looks like.
        let plane = tempfile::tempdir().expect("a plane");
        let shut = plane.path().join(".charter");
        std::fs::create_dir(&shut).expect("a directory");
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000))
            .expect("shut to everyone");

        let refused = Listener::bind(plane.path(), &shut.join("app").join("hooks.sock"));

        // Put it back before any assertion, so a failure does not leave an unreadable
        // directory behind for the tempdir to trip over.
        let _ = std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o700));
        assert!(
            refused.is_err(),
            "charter walked past a component it could not look at"
        );
    }

    #[test]
    fn a_socket_under_a_root_with_nothing_linked_on_the_way_is_bound() {
        // The containment starts at a root the caller names and walks DOWN. It cannot start
        // at `/`: on macOS `/tmp` and `/var` are themselves links, so a walk from the root of
        // the filesystem refuses every path on the machine. That is the shape of resolver
        // this repo has got subtly wrong five times, so there is no resolver here — only a
        // walk of the components below a root the caller already trusts.
        let plane = tempfile::tempdir().expect("a plane");

        let listener = Listener::bind(
            plane.path(),
            &plane.path().join(".charter").join("app").join("hooks.sock"),
        )
        .expect("an ordinary plane binds");

        assert!(listener.path().exists());
    }

    #[test]
    fn a_symlink_standing_in_for_the_socket_directory_is_refused() {
        // A surviving mutant: `symlink_metadata` weakened to `metadata` passed everything,
        // because nothing put a symlink there. It is the precise attack the guard exists to
        // stop — on the predictable path under a shared temp directory, a link left by
        // somebody else would be followed, and `set_permissions` would tighten whatever it
        // aims at while charter listened inside it.
        let dir = tempfile::tempdir().expect("a directory");
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).expect("somewhere to point at");
        let link = dir.path().join("app");
        std::os::unix::fs::symlink(&elsewhere, &link).expect("a link in the way");

        let refused = Listener::bind(dir.path(), &link.join("hooks.sock"));

        assert!(
            refused.is_err(),
            "charter listened inside a symlink it did not make"
        );
        // And it did not tighten what the link pointed at on the way past.
        let mode = std::fs::metadata(&elsewhere)
            .expect("still there")
            .permissions()
            .mode();
        assert_ne!(
            mode & 0o777,
            0o700,
            "it followed the link and chmod'd the target"
        );
    }

    #[test]
    fn a_socket_directory_left_at_the_umask_by_an_older_charter_is_tightened() {
        use std::os::unix::fs::DirBuilderExt;

        let dir = tempfile::tempdir().expect("a directory");
        let loose = dir.path().join("app");
        std::fs::DirBuilder::new()
            .mode(0o755)
            .create(&loose)
            .expect("a directory an older charter might have left");

        let listener = Listener::bind(dir.path(), &loose.join("hooks.sock")).expect("a socket");

        let mode = std::fs::metadata(loose)
            .expect("it is there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "the directory is still {mode:o}");
        assert!(listener.path().exists());
    }

    #[test]
    fn a_socket_whose_directory_does_not_exist_yet_is_still_bound() {
        // The app's own path is `<plane>/.charter/app/hooks.sock`, and `.charter/app` may
        // not exist on a plane the app has never run in. A surviving mutant is why this is
        // here: every other test binds inside a directory that already exists.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("never").join("made").join("hooks.sock");

        let listener = Listener::bind(dir.path(), &path).expect("the directory is made");

        assert!(listener.path().exists());
    }

    #[test]
    fn a_socket_nothing_is_listening_on_refuses_rather_than_hangs() {
        // The app is not running, or has quit. The hook must find out at once and get out of
        // the harness's way.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("nobody.sock");

        let began = std::time::Instant::now();
        let sent = send(
            &path,
            &Report {
                chat: 1,
                event: Event::Stop,
                conversation: Conversation::Unknown,
                pid: None,
                detail: Detail::default(),
            },
        );

        assert!(sent.is_err(), "a socket with no app behind it was taken");
        assert!(
            began.elapsed() < std::time::Duration::from_millis(50),
            "it took {:?} to find out nothing was listening",
            began.elapsed()
        );
    }

    #[test]
    fn a_report_is_written_as_one_line_because_that_is_what_the_reader_reads() {
        // The reader takes a line at a time. A surviving mutant is why this is pinned: with
        // the newline dropped, every test still passed — the reader's `read_line` returns at
        // end of file too, so nothing noticed until two reports shared a connection.
        let report = Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            detail: Detail::default(),
        };
        let mut wire = serde_json::to_vec(&report).expect("it serialises");
        wire.push(b'\n');

        assert_eq!(wire.last(), Some(&b'\n'));
        assert_eq!(wire.iter().filter(|byte| **byte == b'\n').count(), 1);
    }

    #[test]
    fn the_socket_is_taken_away_when_the_app_stops_listening() {
        // A surviving mutant: nothing asserted the socket file was cleaned up. One left
        // behind makes the next `bind` unlink a path it does not own.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let reading = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .each(Box::new(|_| {}));
        assert!(path.exists());

        drop(reading);

        assert!(
            !path.exists(),
            "the socket outlived the app listening on it"
        );
    }

    #[test]
    fn nobody_else_on_the_machine_can_write_to_the_socket() {
        // Anything that reaches this socket moves a chat's state. Nothing secret goes over
        // it and nothing it says is executed, so the worst another local process could do is
        // make the sidebar lie and raise a notification — but a socket left at whatever the
        // umask happened to be is not something to leave to the umask.
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");

        let listener = Listener::bind(dir.path(), &path).expect("a socket");

        let mode = std::fs::metadata(listener.path())
            .expect("the socket is there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "the socket is {mode:o}, so another account on this machine can move a chat"
        );
    }

    #[test]
    fn a_socket_left_behind_by_a_process_that_is_gone_is_replaced() {
        // The app was killed, or crashed. A stale socket file must not stop the next launch
        // from listening — and `bind` refuses an address already in use.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        drop(Listener::bind(dir.path(), &path).expect("the first socket"));
        assert!(
            path.exists(),
            "the file is what makes this the interesting case"
        );

        let listener = Listener::bind(dir.path(), &path).expect("the socket is taken over");

        assert_eq!(listener.path(), path);
    }
}
