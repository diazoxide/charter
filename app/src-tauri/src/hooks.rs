//! The app's side of the hook channel: what a chat is doing, and which one needs you.
//!
//! A hook writes one line on a socket this owns (`charter_core::hookwire`), which moves a
//! chat on the board and, if a reader would see a difference, tells the window. There is no
//! polling anywhere: the listener blocks on `accept`, and the window is pushed to.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use charter_core::hookwire::{Answer, Ask, Listener, NOTHING_ANSWERS, Reading, Report};
use charter_core::session::Exit;
use charter_core::state::{Board, State};

use crate::planes::{PlaneId, Teller};

/// The event the window listens for. One chat, its state, whether it is asking for you.
///
/// **The plane travels with it, and that is not decoration.** Every plane numbers its chats
/// from one, so a window holding two of them would be told "session 3 is waiting" twice about
/// two different chats. The pair is the identity; one half of it is a guess.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Moved {
    pub plane: PlaneId,
    pub session: u32,
    pub state: String,
    pub needs_you: bool,
    /// Every chat asking for you, so the queue is never assembled from a series of events
    /// the window might have missed one of.
    pub queue: Vec<u32>,
    /// When this chat last moved, as a count of moves on its plane's board — bigger is
    /// more recent. `charter_core::state::Board::moved_at` is the whole definition.
    ///
    /// **The window cannot work this out for itself, which is why it rides an event that
    /// already fires.** Charter ADR 0039 sorts the chat strip's overflow menu by last
    /// activity, and nothing in the window knows when a chat did anything: the strip is
    /// `tabs.order`, which is an opening order, and the needs-you queue is oldest-first,
    /// which is a different fact. An order computed in the window would also restart at
    /// every launch and disagree between two windows on one plane, where this one is the
    /// board's and the board is the plane's.
    ///
    /// A count rather than a clock, and a `u32` rather than a `u64`: both are argued where
    /// the field is produced, and the second is not negotiable here — `specta` refuses to
    /// export a `u64` and the app panics at startup in a debug build when one is reached
    /// for.
    pub moved_at: u32,
    /// The chats that have reported back to this one and not been read yet, by the name the
    /// operator sees them under, oldest first (charter-app#259). Each is a needs-you item that
    /// says `<child> reported back` rather than only this chat's name. Empty for nearly every
    /// chat, and emptied by this chat's next prompt, which is the turn the reports are handed.
    pub reports: Vec<String>,
    /// Which snapshot of the board this is — bigger was taken later (charter-app#248).
    ///
    /// **What lets the window put its events back in order.** Every `Moved` is built under the
    /// board's lock, but it is SENT after the lock is let go, on whichever thread built it: a
    /// hook's report on the socket's thread, a close on the command's. So a report taken just
    /// before a close can reach the window just after it, and the window, which keeps the last
    /// queue it was told, would put the closed chat back. The window drops any snapshot older
    /// than the one it holds (`chatState.ts`), which it can do only because this is numbered
    /// in the order the board was read. [`sequence`] is the whole definition.
    pub sequence: u32,
}

/// The board, the socket, and the thread reading it — one plane's whole side of the channel.
pub struct Hooks {
    /// The plane this listens for, stamped onto every event it sends.
    plane: PlaneId,
    /// Shared with the thread reading the socket: one board, so what the window is told and
    /// what the window can ask for can never disagree.
    board: Arc<Mutex<Board>>,
    /// The reading, while it is going on. Held in a lock rather than owned outright so that
    /// closing a plane can release the socket THEN, rather than whenever the last handle to
    /// the plane happens to be dropped — a project the operator closed has to stop listening
    /// while they are still looking at the app.
    reading: Mutex<Option<Reading>>,
    socket: Option<PathBuf>,
    /// Who answers an ask on this socket, once there is someone to (charter-app#204).
    ///
    /// A slot filled after the fact, because the answer needs the plane's chats and the
    /// chats are built after the socket: a session's environment carries the socket's path,
    /// so the socket has to exist first. Until it is filled every ask is answered with a
    /// refusal, never with a silence an asker would have to wait out.
    answering: Arc<Mutex<Option<Answering>>>,
    /// Told when a chat's own harness says the operator prompted it (charter-app#259): a
    /// handed-off chat that has sent its report owes another once it is given more to do.
    /// A slot filled after the fact, for [`Self::answering`]'s reason.
    prompted: Arc<Mutex<Option<Prompted>>>,
}

/// Told the number of a chat the operator has just prompted.
pub type Prompted = Arc<dyn Fn(u32) + Send + Sync + 'static>;

/// What answers an ask, told which connection it came on.
pub type Answering = Arc<dyn Fn(u64, Ask) -> Answer + Send + Sync + 'static>;

/// Where this app listens, and where containment of that path begins.
///
/// The two travel together because `Listener::bind` needs both: a link anywhere between them
/// is refused, and the root is named rather than worked out — a walk from the root of the
/// filesystem would refuse every path on macOS, where `/tmp` and `/var` are themselves links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Where {
    /// A directory the caller already trusts. Nothing above it is checked.
    pub within: PathBuf,
    pub socket: PathBuf,
}

/// Where this app listens for its sessions' hooks.
///
/// Beside the record M1.7 already writes (`.charter/app/`) when there is a plane, so
/// everything the app keeps for a plane is in one place and an operator looking for it finds
/// it. `Listener::bind` makes that directory 0700.
///
/// **There is always somewhere.** The channel belongs to the APP, not to the plane: a chat
/// started outside a plane is still a chat, and it would be a strange rule that a harness can
/// report what it is doing only when there happens to be a `charter.toml` above it. An
/// earlier version answered `None` here, which quietly made every chat in such a run
/// `unknown` — including every chat in the scenario tests, which is how it was found.
///
/// The fallback is keyed by the plane (or by nothing) so two apps do not land on one socket,
/// and by the user so two accounts on one machine do not either. A path is handed to each
/// session in its environment, so nothing ever has to guess it.
pub fn socket_for(plane: Option<&Path>) -> Where {
    if let Some(plane) = plane {
        let beside_the_record = plane.join(".charter").join("app").join("hooks.sock");
        // macOS allows 104 bytes for a unix socket path including the terminator
        // (`sys/un.h`), Linux 108; the smaller is the one to hold to, since a plane is
        // portable. A plane nested deeper than that is not a failure, just not somewhere the
        // socket can live.
        if beside_the_record.as_os_str().len() <= LONGEST_SOCKET_PATH {
            // The plane is the root: it is the operator's own tree, and `.charter/app` below
            // it is charter's own directory (charter-app#28 rules the same for the record
            // that already lives there).
            return Where {
                within: plane.to_path_buf(),
                socket: beside_the_record,
            };
        }
    }
    let within = private_dir();
    let socket = within
        .join(format!("charter-{}-{:016x}", whoami(), keyed_on(plane)))
        .join("hooks.sock");
    Where { within, socket }
}

/// Where a socket goes when it cannot go beside the plane.
///
/// `$XDG_RUNTIME_DIR` first: on Linux it is the standard per-user 0700 directory for exactly
/// this, and the temp directory there is `/tmp`, which everyone can write to. macOS has no
/// such variable and its `TMPDIR` is already a per-user 0700 directory.
///
/// Either way `Listener::bind` creates the socket's own directory with the mode set as it is
/// made and refuses one it does not own, so this chooses a good neighbourhood rather than
/// being the thing that keeps anyone out.
fn private_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute() && dir.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// The longest a unix socket path may be, on the stricter of the two platforms charter builds
/// for. One byte is left for the terminator.
const LONGEST_SOCKET_PATH: usize = 103;

/// Something short and stable that differs between users on one machine.
///
/// `TMPDIR` is already per-user on macOS and is not on Linux, so this is what keeps two
/// accounts apart there. It is not a secret and is not relied on to be one — the directory's
/// 0700 is what keeps others out.
fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .filter(|user| {
            user.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .unwrap_or_else(|| "charter".to_owned())
}

/// A short, stable name for a plane — or for having none.
fn keyed_on(plane: Option<&Path>) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let bytes = plane.map_or(b"no plane".as_slice(), |plane| {
        plane.as_os_str().as_encoded_bytes()
    });
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl Hooks {
    /// Nothing listening: every chat is `unknown`, which is what the spec says a harness with
    /// no state hook shows. The app runs perfectly well like this.
    pub fn deaf(plane: PlaneId) -> Self {
        Self {
            plane,
            board: Arc::new(Mutex::new(Board::new())),
            reading: Mutex::new(None),
            socket: None,
            answering: Arc::new(Mutex::new(None)),
            prompted: Arc::new(Mutex::new(None)),
        }
    }

    /// Listens on `socket`, handing each change to `moved`.
    ///
    /// A socket that cannot be opened is not worth refusing to start over: the app comes up
    /// with every chat `unknown` and says so on stderr, which is a working app with one
    /// feature missing rather than no app at all.
    pub fn listening_on(plane: PlaneId, at: &Where, moved: Teller) -> std::io::Result<Self> {
        let listener = Listener::bind(&at.within, &at.socket)?;
        let socket = listener.path().to_path_buf();
        let board = Arc::new(Mutex::new(Board::new()));
        let answering: Arc<Mutex<Option<Answering>>> = Arc::new(Mutex::new(None));
        let prompted: Arc<Mutex<Option<Prompted>>> = Arc::new(Mutex::new(None));
        let reading = listener.each_answering(
            {
                let board = Arc::clone(&board);
                let plane = plane.clone();
                let prompted = Arc::clone(&prompted);
                Box::new(move |report| {
                    if let Some(what) = apply(&board, &plane, &report) {
                        // A prompt the board took is the chat's own harness saying the operator
                        // gave it a turn — never a nested one, which the board refused.
                        if report.event == charter_core::state::Event::UserPromptSubmit
                            && what.state == "running"
                        {
                            let told = prompted
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .clone();
                            if let Some(told) = told {
                                told(report.chat);
                            }
                        }
                        moved(what);
                    }
                })
            },
            {
                let answering = Arc::clone(&answering);
                Box::new(move |connection, ask| {
                    // Taken out of the lock before it runs: an open starts a program, and a
                    // program that dies at once reaches back into this plane.
                    let answer = answering
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    match answer {
                        Some(answer) => answer(connection, ask),
                        None => Answer::No {
                            why: NOTHING_ANSWERS.to_owned(),
                        },
                    }
                })
            },
        );
        Ok(Self {
            plane,
            board,
            reading: Mutex::new(Some(reading)),
            socket: Some(socket),
            answering,
            prompted,
        })
    }

    /// Stops listening and releases the socket. The plane on disk is untouched.
    ///
    /// Dropping the reader is what unlinks the socket file and joins the thread, and this is
    /// where a closed plane does it — so a plane that is opened again binds a socket of its
    /// own rather than inheriting a live one's path.
    pub fn stop(&self) {
        drop(
            self.reading
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
    }

    /// Whether this is still listening. Only a test asks.
    #[cfg(test)]
    pub fn listening(&self) -> bool {
        self.reading
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// Who answers asks on this socket from now on.
    pub fn answer_with(&self, answering: Answering) {
        *self
            .answering
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(answering);
    }

    /// Who is told when a chat's own harness says the operator prompted it.
    pub fn when_prompted(&self, prompted: Prompted) {
        *self.prompted.lock().unwrap_or_else(PoisonError::into_inner) = Some(prompted);
    }

    /// A chat `session` handed work to, shown as `from`, has reported back to it
    /// (charter-app#259): it is a needs-you item now. Answers what the window must be told, or
    /// nothing when no reader would see a difference; built under the same hold as the change,
    /// for [`apply`]'s reason.
    pub fn reported_back(&self, session: u32, from: &str) -> Option<Moved> {
        let mut board = self.board();
        board
            .reported_back(session, from)
            .then(|| seen_by(&board, &self.plane, session))
    }

    pub fn socket(&self) -> Option<&Path> {
        self.socket.as_deref()
    }

    pub fn board(&self) -> MutexGuard<'_, Board> {
        held_board(&self.board)
    }

    /// The board itself, for a caller that has to outlive this handle — the exit reporting,
    /// which is armed before the app manages anything.
    pub fn shared_board(&self) -> Arc<Mutex<Board>> {
        Arc::clone(&self.board)
    }

    /// Takes a chat off the board — closed, not merely ended — and answers with what the
    /// window must now be told: a queue without it.
    ///
    /// Answered under the same hold as the removal, for the reason [`apply`] gives: the queue
    /// it carries is the board's as of the removal, never one read a moment before it.
    pub fn closed(&self, session: u32) -> Moved {
        let mut board = self.board();
        board.closed(session);
        seen_by(&board, &self.plane, session)
    }

    /// Drops a chat's request without answering it — the operator's Ignore — and answers
    /// with what the window must now be told: a queue without it (charter-app#248).
    ///
    /// Answered whether or not the board changed, and under the same hold as the change, for
    /// the reason [`Hooks::closed`] gives: a window that asked to ignore a chat believes it is
    /// asking, and the board's answer is the one to leave it with either way.
    pub fn ignored(&self, session: u32) -> Moved {
        let mut board = self.board();
        board.ignored(session);
        seen_by(&board, &self.plane, session)
    }

    /// What the window is told when something other than a hook moves a chat: a chat opening,
    /// or a program that has died. A chat closing is [`Hooks::closed`].
    pub fn now(&self, session: u32) -> Moved {
        now(&self.board, self.plane.clone(), session)
    }
}

/// What a reader sees for this chat right now.
pub fn now(board: &Mutex<Board>, plane: PlaneId, session: u32) -> Moved {
    seen_by(&held_board(board), &plane, session)
}

/// The same, for a caller that is already holding the board.
///
/// **Only ever called with the board held**, which is what makes [`sequence`] an order over
/// the board's states: the number is taken inside the same hold as the read.
fn seen_by(board: &Board, plane: &PlaneId, session: u32) -> Moved {
    let queue = board.needs_you();
    Moved {
        sequence: sequence(),
        plane: plane.clone(),
        session,
        state: word(board.state(session)),
        needs_you: queue.contains(&session),
        queue,
        moved_at: board.moved_at(session),
        reports: board.reports(session),
    }
}

/// The next snapshot's number. See [`Moved::sequence`].
///
/// **The process's and not one board's**, which is stronger than the window needs and costs
/// nothing: a project closed and opened again in one run gets a new board, and a count that
/// began again at one would have a window still holding the old board's numbers drop every
/// snapshot of the new one. Taken while a board is held, so for any one board the numbers
/// run in the order its states were read.
///
/// Saturating, and a `u32` for the reason [`Board::moved_at`] gives (`specta` cannot carry a
/// `u64`). At four billion snapshots every later one is numbered the same, and the window
/// keeps a snapshot numbered the same as the one it holds — so the app falls back to taking
/// events in the order they land, which is what it did before there was a number, rather than
/// stopping listening.
fn sequence() -> u32 {
    static TAKEN: AtomicU32 = AtomicU32::new(0);
    let was = TAKEN
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            Some(n.saturating_add(1))
        })
        .unwrap_or_else(|n| n);
    was.saturating_add(1)
}

/// Applies one report, answering with what a reader would now see differently.
///
/// The answer is built under the SAME hold as the change. Reports arrive on a thread each, so
/// dropping the lock in between let two of them interleave — mutate, mutate, read, read — and
/// the window could then be sent the older of the two snapshots last and keep it until the
/// next event. A review found it.
fn apply(board: &Mutex<Board>, plane: &PlaneId, report: &Report) -> Option<Moved> {
    let mut guard = held_board(board);
    guard
        .reported(report)
        .then(|| seen_by(&guard, plane, report.chat))
}

/// The board, whether or not a thread panicked while holding it.
///
/// A poisoned board is one whose last change may not have finished; the state it holds is
/// still the best answer there is, and refusing to draw anything at all would be worse.
pub fn held_board(board: &Mutex<Board>) -> MutexGuard<'_, Board> {
    board
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The word the window draws, which is the word the spec uses.
pub fn word(state: State) -> String {
    match state {
        State::Unknown => "unknown",
        State::Running => "running",
        State::Waiting => "waiting",
        State::Done => "done",
        State::Failed => "failed",
    }
    .to_owned()
}

/// What an exit says about a chat: the code, or none for a program killed by a signal.
pub fn code_of(exit: &Exit) -> Option<i32> {
    match exit {
        Exit::Code(code) => i32::try_from(*code).ok(),
        // A signal leaves no code behind, and it is not a clean end.
        Exit::Signal(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A board holding chat `session`, which has stopped and is asking for you.
    fn asking(session: u32) -> Hooks {
        // Spelled the way a window hands one back; only the registry mints one for real.
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks = Hooks::deaf(plane);
        hooks.board().opened(session, None, None);
        assert!(apply(&hooks.board, &hooks.plane, &stop(session)).is_some());
        hooks
    }

    fn stop(session: u32) -> Report {
        Report {
            chat: session,
            event: charter_core::state::Event::Stop,
            conversation: charter_core::hookwire::Conversation::Unknown,
            pid: None,
            detail: charter_core::state::Detail::default(),
        }
    }

    #[test]
    fn a_report_taken_before_a_close_is_numbered_before_it() {
        // charter-app#248, the race #256 left open: a report's `Moved` is sent on the thread
        // that read it, after the board is let go, so it can reach the window AFTER a close
        // that came later. The number is what lets the window tell which is newer.
        let hooks = asking(7);
        let late = apply(&hooks.board, &hooks.plane, &stop(7));
        hooks.board().ignored(7);
        let report = apply(&hooks.board, &hooks.plane, &stop(7)).expect("a new request");

        let close = hooks.closed(7);

        assert!(
            late.is_none(),
            "a stop on a chat already asking moved nothing"
        );
        assert_eq!(report.queue, vec![7]);
        assert!(close.queue.is_empty());
        assert!(
            report.sequence < close.sequence,
            "the close ({}) is not numbered after the report it follows ({})",
            close.sequence,
            report.sequence
        );
    }

    #[test]
    fn every_snapshot_is_numbered_after_the_one_before_it() {
        let hooks = asking(7);

        let first = hooks.now(7);
        let second = hooks.now(7);

        assert!(first.sequence < second.sequence);
    }

    #[test]
    fn ignoring_a_chat_tells_a_queue_without_it_and_the_chat_still_waiting() {
        let hooks = asking(7);
        let asked = hooks.now(7);

        let ignored = hooks.ignored(7);

        assert!(ignored.queue.is_empty());
        assert!(!ignored.needs_you);
        assert_eq!(ignored.state, "waiting");
        assert!(ignored.sequence > asked.sequence);
    }

    #[test]
    fn the_socket_sits_beside_the_record_the_app_already_writes() {
        // `.charter/app/` is where M1.7 put `reopen.json`. One place for what the app keeps.
        assert_eq!(
            socket_for(Some(Path::new("/Users/o/plane"))).socket,
            PathBuf::from("/Users/o/plane/.charter/app/hooks.sock")
        );
    }

    #[test]
    fn a_run_outside_any_plane_still_has_somewhere_to_listen() {
        // The channel is the APP's, not the plane's. A chat started outside a plane is still
        // a chat, and answering `unknown` for it because there is no `charter.toml` above it
        // would be a strange rule — it is also how this was found: it made every chat in the
        // scenario tests unknown.
        let socket = socket_for(None).socket;

        assert!(socket.starts_with(private_dir()));
        assert!(socket.as_os_str().len() <= LONGEST_SOCKET_PATH);
    }

    #[test]
    fn a_plane_too_deep_for_a_unix_socket_path_falls_back_instead_of_failing() {
        // macOS allows 104 bytes for the whole path. A plane checked out under a long CI
        // path, or a deeply nested workspace, would otherwise get no event channel at all —
        // and the failure would look like hooks being broken rather than a path being long.
        let deep = PathBuf::from("/Users/operator").join("a".repeat(120));

        let socket = socket_for(Some(&deep)).socket;

        assert!(
            socket.as_os_str().len() <= LONGEST_SOCKET_PATH,
            "{} is {} bytes",
            socket.display(),
            socket.as_os_str().len()
        );
        assert!(socket.starts_with(private_dir()));
    }

    #[test]
    fn the_fallback_prefers_the_runtime_directory_where_the_platform_has_one() {
        // On Linux the temp directory is `/tmp`, which everyone can write to; the standard
        // per-user place for a socket is `$XDG_RUNTIME_DIR`. macOS names no such variable and
        // its own temp directory is already per-user.
        match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(runtime) => assert!(socket_for(None).socket.starts_with(runtime)),
            None => assert!(socket_for(None).socket.starts_with(std::env::temp_dir())),
        }
    }

    #[test]
    fn two_deep_planes_do_not_share_one_socket() {
        let one = PathBuf::from("/Users/operator").join("a".repeat(120));
        let two = PathBuf::from("/Users/operator").join("b".repeat(120));

        assert_ne!(socket_for(Some(&one)).socket, socket_for(Some(&two)).socket);
    }

    #[test]
    fn a_plane_and_no_plane_do_not_share_one_socket() {
        let deep = PathBuf::from("/Users/operator").join("a".repeat(120));

        assert_ne!(socket_for(Some(&deep)).socket, socket_for(None).socket);
    }
}
