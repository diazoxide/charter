//! What the app asks of whatever runs its sessions and hears their hooks: the session host.
//!
//! Today the host is in the app's own process: [`crate::sessions::Sessions`] runs the
//! sessions, and [`crate::hooks::Hooks`] holds the board their hooks report to. ADR 0068
//! (accepted 2026-09-30) moves both into `charterd`, behind a socket. These two traits are that
//! seam, drawn first in place with nothing moved and nothing changed. The app reaches a session
//! only through [`SessionHost`], and a chat's hook state only through [`ChatBoard`].
//!
//! - [`SessionHost`] starts a session, writes to it, resizes it, opens and closes views of it,
//!   ends it, and tells whoever asked when its program exits. It also deals the numbers chats
//!   are keyed on (charter-app#90, and ADR 0066 for the identity that follows).
//! - [`ChatBoard`] is what the board says about each chat, and the three moves the window
//!   makes on it: a chat closed, a request ignored, a report handed back.
//!
//! They are two traits because they are two things with different owners today. The window
//! reaches the board through the plane, and the chats layer does not reach it at all.
//!
//! **What else sits on the host's side.** Auto-save and `planewatch` are started when a plane
//! is held and stopped when it is let go of. Auto-save hears a chat end through
//! [`SessionHost::when_one_ends`], and nothing in the app calls either of them outside that
//! lifecycle, so neither needs a trait here. What a session is told to report to (the hook
//! socket and the chats' tokens, [`crate::sessions::Reporting`]) goes to the host when the
//! host is made.

use std::sync::Arc;

use purlis_core::engine::Size;
use purlis_core::session::Exit;
use purlis_core::state::State;

use crate::hooks::Moved;

/// What to run in a new session. No program is the operator's shell.
#[derive(Debug, Clone)]
pub struct Opening {
    pub program: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub size: Size,
    /// Set in the program's environment, on top of what the app itself was started with.
    pub env: Vec<(String, String)>,
    /// Extra variable names kept out of this chat, beyond the `OP_*` prefix: every identity
    /// source and target a vault of this plane declares (`registry::identity_vars`), so a vault
    /// bound to a non-`OP_` variable (`--token-env PROD_1P_TOKEN`) leaks it to no chat either
    /// (#271 review, U6). Removed from both the set env and the inherited one.
    pub env_strip: Vec<String>,
    /// The harness the chat runs, whose own declared variables it is also started with
    /// ([`purlis_core::harness::Harness::env_passed`]). None is a shell.
    pub harness: Option<purlis_core::harness::Harness>,
    /// The operator's own additions to what a chat is started with, from the chat's plane
    /// (`[chat_env] pass`, [`purlis_core::chatenv::read`]).
    pub env_pass: Vec<String>,
    /// Whether this is a shell the operator opened from the window, which the kill switch lets
    /// through: looking at what the agents did is a human act (OV-1, ADR 0071). Every other
    /// start — every chat, every relaunch, every shell a record puts back — is refused while
    /// agents are stopped.
    pub operator_shell: bool,
    /// charter's git hooks, for a chat that runs a harness (SQ-16, ADR 0074): the chat's git is
    /// armed with them after every other variable is settled, so a `GIT_CONFIG_COUNT` the
    /// operator passes or a profile sets keeps its pairs. None for a shell.
    pub git_hooks: Option<purlis_core::githooks::GitHooks>,
}

/// Where a view's text goes. It is called on the view's own thread, one batch at a time.
pub type Sink = Box<dyn FnMut(String) + Send>;

/// Told how each session ended, as it ends, with the session's id. An `Arc` so each session
/// can hold it for as long as its program runs.
pub type Ends = Arc<dyn Fn(u32, Exit) + Send + Sync>;

/// A view the UI has open, and the size the screen it opened on was drawn for.
#[derive(Debug, Clone, Copy)]
pub struct Watching {
    pub view: u32,
    pub size: Size,
}

/// Whether a session's program is ready to be typed at, as the terminal and the clock say it:
/// never what the program wrote. It is what a curation prompt waits on (ADR 0061).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    /// Whether the terminal is still in the kernel's line editing rather than handing keys to
    /// the program (`purlis_core::session::Session::edits_lines`), or none where the platform
    /// cannot say.
    pub edits_lines: Option<bool>,
    /// How long the program has written nothing (`purlis_core::session::Session::quiet_for`):
    /// when bytes last arrived, never what they were.
    pub quiet_for: std::time::Duration,
}

/// Runs the app's sessions, and is the only way anything above it reaches one.
///
/// Every call that names a session that is not running is refused with a sentence, never a
/// panic.
pub trait SessionHost: Send + Sync {
    /// Starts a session, and answers with the id it is called by from now on.
    ///
    /// `announce` is called with that id BEFORE the program starts. A harness fires
    /// `SessionStart` at its own exec, and anything that learned the chat's number afterwards
    /// would miss it — for a chat that is then idle, waiting for a first prompt, no second
    /// event ever comes and it reads `unknown` for the rest of the run. A review found that
    /// on the relaunch path, where a whole record's worth of chats start at once.
    ///
    /// `wanted` is the number this chat ALREADY answers to, where it has one. A relaunch
    /// hands the number the record kept for each chat, so the chat comes back on its own
    /// `.charter/sessions/<sid>.workspace` and `<sid>.lock` rather than on whichever the
    /// order of the record would have dealt it (charter-app#90). `None` is a chat that has
    /// no number yet, which is every chat the operator starts.
    ///
    /// A number a session is **already running under** is refused and a fresh one taken
    /// instead: a record that names one twice — hand-edited, or two records concatenated —
    /// would otherwise give two live chats one key, which is this same defect pointed the
    /// other way. Whichever number is chosen, the counter is raised past it, so nothing
    /// dealt later can land on it either.
    fn open(
        &self,
        wanted: Option<u32>,
        opening: &Opening,
        announce: &dyn Fn(u32),
    ) -> Result<u32, String>;

    /// Sends what a pane typed to the program: bytes, written to its pty as they are. Text is
    /// its UTF-8; a mouse report in the default encoding is bytes that are not text at all
    /// (charter#493).
    fn input(&self, id: u32, bytes: &[u8]) -> Result<(), String>;

    /// Gives a session's terminal, and the screen its views are drawn from, a new size. The
    /// program hears of it from its terminal, as it would in any other.
    fn resize(&self, id: u32, size: Size) -> Result<(), String>;

    /// Opens a view of a session: `sink` is sent the screen as it already is and then the
    /// session's output, as text, until the view is closed. The answer says which view that
    /// is, and the size its screen was drawn for.
    fn watch(&self, id: u32, sink: Sink) -> Result<Watching, String>;

    /// Closes a view. Its session keeps running, with its terminal, for the next pane.
    fn unwatch(&self, id: u32, view: u32) -> Result<(), String>;

    /// Ends a session and everything it started. Its views end with it, and so does its token.
    fn close(&self, id: u32) -> Result<(), String>;

    /// Ends every session, and does not return until their programs are gone. This is what
    /// quitting calls: the process is about to end, and a thread would not be waited for.
    fn end_all(&self);

    /// Interrupts and ends every session's program, and everything each started, all at once,
    /// and does not return until they are gone. The sessions stay, with their last screens, as
    /// they do when a program ends on its own: this is the kill switch (OV-1), not a close.
    /// Answers how many it asked.
    fn stop_every_program(&self) -> usize;

    /// Refuses every start but the operator's own shell while `switch` is thrown (OV-1, ADR
    /// 0071). Every way a chat starts — the operator, a relaunch, a handoff, a curation
    /// action — comes through [`Self::open`], so that is the one place the refusal has to be.
    fn stopped_by(&mut self, switch: Arc<crate::killswitch::KillSwitch>);

    /// The operating system's id for a session's program, while it runs, where the host can
    /// say. The record keeps it, so `commit-msg` can tell a commit the chat's harness made from
    /// one made in a program that only inherited the chat's environment (V82, #1018); the tests
    /// ask it to see a stopped program is gone.
    fn process_id(&self, _id: u32) -> Option<u32> {
        None
    }

    /// Calls `tell` as each session's program ends, with the id and how it ended.
    ///
    /// No hook reports a program dying, and none can — the process is gone. This is the
    /// operating system telling the app, not charter reading a screen (ADR 0018).
    fn when_one_ends(&self, tell: Ends);

    /// The sessions that are running, in the order they were opened.
    fn running(&self) -> Vec<u32>;

    /// Whether a session's program is ready to be typed at. Refused for a session that is gone.
    fn readiness(&self, id: u32) -> Result<Readiness, String>;

    /// A number no chat in this plane has had, taken now for a chat about to be started with
    /// it — for a caller that has to NAME the chat by its number before it starts: a handed-off
    /// chat with no task name is `<persona> <N>` (charter-app#258). Passed to [`Self::open`] as
    /// the number wanted, it is the one the chat gets, because nothing else can be dealt it.
    fn deal(&self) -> u32;

    /// The highest number this plane has dealt, for the record to keep.
    fn dealt(&self) -> u32;

    /// Says `dealt` numbers have already gone out in this plane, so none of them is dealt
    /// again — charter-app#90.
    ///
    /// A launch calls this with what the record kept, BEFORE putting any chat back. The
    /// numbers above what is in the record are the ones that matter: a chat that was closed
    /// before the quit is not in the record at all, and its `.charter/sessions/<n>.workspace`
    /// and `<n>.lock` are still on disk for the 30 days `wscmd::select`'s prune leaves them.
    fn already_dealt(&self, dealt: u32);

    /// The socket the sessions' hooks report on, where this host listens on one: a sandbox
    /// charter wraps a chat in lets the chat reach it ([`purlis_core::sandbox::At`]).
    fn reports_to(&self) -> Option<std::path::PathBuf> {
        None
    }
}

/// One chat as the board has it, read under one hold so the three agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glance {
    pub state: State,
    /// Whether it is asking for you.
    pub asking: bool,
    /// How many turns it has taken this run.
    pub turns: u32,
    /// The prompt it is stopped on in its terminal, where it is (#1691).
    pub prompt: Option<purlis_core::harness::model::Prompt>,
}

impl Glance {
    /// Whether it is stopped, now, on the prompt it asked mid-turn (#1601): it asked, and it
    /// has not been answered or got past it since, which puts it back to running.
    /// `purlis_core::state::Chat::waits_on_its_prompt`, read off one glance.
    pub fn waits_on_its_prompt(&self) -> bool {
        self.asking && self.state == State::Waiting
    }
}

/// What the host's board says about each chat, as its hooks reported it, and the moves the
/// window makes on it. Nothing here reads a harness's output (ADR 0018).
///
/// Each move answers the [`Moved`] the window must now be told, built under the same hold as
/// the change, so the queue it carries is the board's as of that change.
pub trait ChatBoard: Send + Sync {
    /// Chat `session` as the board has it now.
    fn glance(&self, session: u32) -> Glance;

    /// The conversation chat `session`'s harness is in, where the board knows it.
    fn conversation(&self, session: u32) -> Option<String>;

    /// What the window is told when something other than a hook moves a chat: a chat opening,
    /// or a program that has died. A chat closing is [`Self::closed`].
    fn now(&self, session: u32) -> Moved;

    /// Takes a chat off the board — closed, not merely ended.
    fn closed(&self, session: u32) -> Moved;

    /// Drops a chat's request without answering it — the operator's Ignore (charter-app#248).
    /// Answered whether or not the board changed: a window that asked to ignore a chat believes
    /// it is asking, and the board's answer is the one to leave it with either way.
    fn ignored(&self, session: u32) -> Moved;

    /// A chat `session` handed work to, shown as `from`, has reported back to it
    /// (charter-app#259). Nothing when no reader would see a difference.
    fn reported_back(&self, session: u32, from: &str) -> Option<Moved>;

    /// The operator stopped `from`, a chat `session` started (#1448): its row says so, and it
    /// is no needs-you item. Nothing when no reader would see a difference.
    fn stopped_below(&self, session: u32, from: &str) -> Option<Moved>;

    /// The app found that chat `session` needs the person, for `need` (#1448): a needs-you
    /// item on it. Nothing when no reader would see a difference.
    fn needs(&self, session: u32, need: purlis_core::state::Need) -> Option<Moved>;

    /// Chat `session`'s report reached the chat that asked for it (#1448): the turn it is in
    /// ends without a needs-you item. Nothing a reader sees changes now.
    fn reported_to_its_asker(&self, session: u32);

    /// What chat `session` waited on is over, and nothing will prompt it (#1491): the end of
    /// turn that was held for its tasks, or for its asker's answer, is the needs-you item now.
    /// Nothing when no reader would see a difference.
    fn rested(&self, session: u32) -> Option<Moved>;

    /// A task chat `session` asked for failed, ended without a report, or did not start
    /// (#1491): a needs-you item on it. Nothing when no reader would see a difference.
    fn task_failed(&self, session: u32, failed: purlis_core::state::FailedTask) -> Option<Moved>;

    /// The person looked at failure `id` of chat `session`, or cleared its row (#1491): that
    /// one item goes. Nothing when no reader would see a difference.
    fn failure_cleared(&self, session: u32, id: &str) -> Option<Moved>;
    /// The person answered chat `session`'s prompt in the window (HP-6): its turn goes on
    /// (`state::Chat::answered`). Nothing when no reader would see a difference.
    fn answered(&self, session: u32) -> Option<Moved>;
}

/// A session host that runs nothing, for a test of what sits above the seam.
#[cfg(test)]
pub(crate) mod pretend {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
    use std::time::Duration;

    use purlis_core::engine::Size;
    use purlis_core::session::Exit;

    use super::{Ends, Opening, Readiness, SessionHost, Sink, Watching};

    /// Deals numbers, remembers what it was asked to run, and ends a session when a test says
    /// its program ended. A clone is the same host, so a test keeps one after handing one over.
    #[derive(Clone, Default)]
    pub struct Pretend {
        seen: Arc<Seen>,
    }

    #[derive(Default)]
    struct Seen {
        /// The switch that stops every agent, where one was handed over: a start is refused
        /// while it is thrown, as the real host refuses it.
        switch: Mutex<Option<Arc<crate::killswitch::KillSwitch>>>,
        asked: Mutex<Vec<(u32, String)>>,
        openings: Mutex<Vec<Opening>>,
        socket: Mutex<Option<std::path::PathBuf>>,
        running: Mutex<Vec<u32>>,
        typed: Mutex<Vec<(u32, Vec<u8>)>>,
        dealt: AtomicU32,
        ends: Mutex<Option<Ends>>,
        /// Why it starts nothing, while a test has it refuse ([`Pretend::refuses`]).
        refusing: Mutex<Option<String>>,
        /// The numbers it starts nothing under, and why ([`Pretend::refuses_number`]).
        refusing_numbers: Mutex<Vec<(u32, String)>>,
    }

    impl Pretend {
        /// Every session it was asked to start, by number and program.
        pub fn asked(&self) -> Vec<(u32, String)> {
            lock(&self.seen.asked).clone()
        }

        /// Everything it was asked to start, whole, in the order it was asked.
        pub fn openings(&self) -> Vec<Opening> {
            lock(&self.seen.openings).clone()
        }

        /// Refuses every start from now with `why`, as a host with no pseudo-terminal to give
        /// does; `None` starts them again.
        pub fn refuses(&self, why: Option<&str>) {
            *lock(&self.seen.refusing) = why.map(str::to_owned);
        }

        /// Refuses a start asked for under `number` with `why`, and starts every other: what
        /// a launch meets when one chat's program will not start and the rest do.
        pub fn refuses_number(&self, number: u32, why: &str) {
            lock(&self.seen.refusing_numbers).push((number, why.to_owned()));
        }

        /// Starts under `number` again.
        pub fn starts_number(&self, number: u32) {
            lock(&self.seen.refusing_numbers).retain(|(one, _)| *one != number);
        }

        /// Everything written to session `id`'s terminal, in order: what a test reads to know
        /// which keys reached a chat, and that none did.
        pub fn typed(&self, id: u32) -> Vec<Vec<u8>> {
            lock(&self.seen.typed)
                .iter()
                .filter(|(to, _)| *to == id)
                .map(|(_, bytes)| bytes.clone())
                .collect()
        }

        /// Says its sessions report on `socket`.
        pub fn reporting_on(&self, socket: std::path::PathBuf) {
            *lock(&self.seen.socket) = Some(socket);
        }

        /// Session `id`'s program ends with `exit`, and whoever asked is told.
        pub fn program_ends(&self, id: u32, exit: Exit) {
            let tell = lock(&self.seen.ends).clone();
            if let Some(tell) = tell {
                tell(id, exit);
            }
        }

        fn here(&self, id: u32) -> Result<(), String> {
            if lock(&self.seen.running).contains(&id) {
                Ok(())
            } else {
                Err(format!("session {id} is not running"))
            }
        }
    }

    impl SessionHost for Pretend {
        fn open(
            &self,
            wanted: Option<u32>,
            opening: &Opening,
            announce: &dyn Fn(u32),
        ) -> Result<u32, String> {
            if let Some(why) = lock(&self.seen.refusing).clone() {
                return Err(why);
            }
            if let Some((_, why)) = lock(&self.seen.refusing_numbers)
                .iter()
                .find(|(number, _)| wanted == Some(*number))
            {
                return Err(why.clone());
            }
            let stopped = lock(&self.seen.switch)
                .as_ref()
                .is_some_and(|switch| switch.is_stopped());
            if !opening.operator_shell && stopped {
                return Err(crate::sessions::STOPPED.to_owned());
            }
            let id = wanted.unwrap_or_else(|| self.deal());
            self.seen.dealt.fetch_max(id, Ordering::SeqCst);
            announce(id);
            let program = opening.program.clone().unwrap_or_default();
            lock(&self.seen.asked).push((id, program));
            lock(&self.seen.openings).push(opening.clone());
            lock(&self.seen.running).push(id);
            Ok(id)
        }
        fn input(&self, id: u32, bytes: &[u8]) -> Result<(), String> {
            self.here(id)?;
            lock(&self.seen.typed).push((id, bytes.to_vec()));
            Ok(())
        }
        fn resize(&self, id: u32, _size: Size) -> Result<(), String> {
            self.here(id)
        }
        fn watch(&self, id: u32, _sink: Sink) -> Result<Watching, String> {
            self.here(id)?;
            Ok(Watching {
                view: 1,
                size: Size {
                    columns: 80,
                    rows: 24,
                },
            })
        }
        fn unwatch(&self, id: u32, _view: u32) -> Result<(), String> {
            self.here(id)
        }
        fn close(&self, id: u32) -> Result<(), String> {
            self.here(id)?;
            lock(&self.seen.running).retain(|one| *one != id);
            Ok(())
        }
        fn end_all(&self) {
            lock(&self.seen.running).clear();
        }
        fn stop_every_program(&self) -> usize {
            lock(&self.seen.running).len()
        }
        fn stopped_by(&mut self, switch: Arc<crate::killswitch::KillSwitch>) {
            *lock(&self.seen.switch) = Some(switch);
        }
        fn when_one_ends(&self, tell: Ends) {
            *lock(&self.seen.ends) = Some(tell);
        }
        fn running(&self) -> Vec<u32> {
            lock(&self.seen.running).clone()
        }
        fn readiness(&self, id: u32) -> Result<Readiness, String> {
            self.here(id).map(|()| Readiness {
                edits_lines: None,
                quiet_for: Duration::ZERO,
            })
        }
        fn deal(&self) -> u32 {
            self.seen.dealt.fetch_add(1, Ordering::SeqCst) + 1
        }
        fn dealt(&self) -> u32 {
            self.seen.dealt.load(Ordering::SeqCst)
        }
        fn already_dealt(&self, dealt: u32) {
            self.seen.dealt.fetch_max(dealt, Ordering::SeqCst);
        }
        fn reports_to(&self) -> Option<std::path::PathBuf> {
            lock(&self.seen.socket).clone()
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
