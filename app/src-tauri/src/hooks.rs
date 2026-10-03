//! The app's side of the hook channel: what a chat is doing, and which one needs you.
//!
//! A hook writes one line on a socket this owns (`charter_core::hookwire`), which moves a
//! chat on the board and, if a reader would see a difference, tells the window. There is no
//! polling anywhere: the listener blocks on `accept`, and the window is pushed to.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use charter_core::hookwire::{
    Answer, Ask, ChatTokens, Hearing, Listener, NOTHING_ANSWERS, Reading, Report, SessionSaved,
    StartedByHand,
};
use charter_core::session::Exit;
use charter_core::state::{Board, State};

use crate::host::{ChatBoard, Glance};
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
    /// When this chat last moved, as a count of moves on every plane's board in this process
    /// — bigger is more recent, within a plane and across planes.
    /// `charter_core::state::Board::moved_at` is the whole definition.
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
    /// The commits of this chat charter's `pre-commit` refused and the operator has not seen,
    /// each as the one masked line its item says, oldest first (SQ-16). Emptied by the chat's
    /// next prompt, or by Ignore.
    pub refusals: Vec<String>,
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

/// The event the window is sent when a harness was started by hand in a shell tab (ADR 0062).
pub const BY_HAND: &str = "harness-by-hand";

/// A harness the operator started by hand in a shell tab, as the window draws its banner.
///
/// **Nothing about the chat moves.** It is not a state and not a needs-you item: the tab says
/// what happened and offers to open that harness as a chat, and the operator's click is what
/// does anything.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ByHand {
    pub plane: PlaneId,
    /// The shell tab's chat.
    pub session: u32,
    /// The harness, by the word the plane calls it — a profile's `kind`.
    pub harness: String,
    /// Where the shell was standing when it started it, which is where a chat opened in its
    /// place starts.
    pub cwd: Option<String>,
}

/// Told when a harness is started by hand in a shell tab of any plane. The event carries its
/// plane, as [`Moved`] does.
pub type ByHandTeller = Arc<dyn Fn(ByHand) + Send + Sync + 'static>;

/// What the window is told about `notice`, heard on `plane`'s socket — or nothing, for a
/// harness this app does not start. A word charter has no chat for would put a button on the
/// tab that could only be refused.
fn by_hand(plane: &PlaneId, notice: StartedByHand) -> Option<ByHand> {
    let harness = charter_core::harness::Harness::of_kind(&notice.started_by_hand)?;
    Some(ByHand {
        plane: plane.clone(),
        session: notice.chat,
        harness: harness.name().to_owned(),
        cwd: notice.cwd.map(|cwd| cwd.display().to_string()),
    })
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
    /// The chats' tokens the socket checks every line against, issued as each chat starts
    /// (`crate::sessions::Reporting`).
    tokens: Option<Arc<ChatTokens>>,
    /// Who answers an ask on this socket, once there is someone to (charter-app#204).
    ///
    /// A slot filled after the fact, because the answer needs the plane's chats and the
    /// chats are built after the socket: a session's environment carries the socket's path,
    /// so the socket has to exist first. Until it is filled every ask is answered with a
    /// refusal, never with a silence an asker would have to wait out.
    answering: Arc<Mutex<Option<Answering>>>,
    /// Told each report the board took, after the window has been told what it moved — a
    /// slot filled after the fact, for `answering`'s reason: what listens needs the plane's
    /// chats, which are built after the socket. A curation chat's typed prompt waits on it
    /// (`crate::curation::Typed`).
    heard: Arc<Mutex<Option<Heard>>>,
    /// Told each time the board moves a chat onto another conversation — a slot filled after
    /// the fact, for `answering`'s reason. The chats' record listens, so the conversation a
    /// relaunch resumes is the one the chat is in now (Q10).
    following: Arc<Mutex<Option<Following>>>,
    /// The host's event log, once the app has one (FD-9) — a slot filled after the fact, so
    /// a project is opened the same way with or without it. Every line the channel hears from
    /// a chat's hooks is recorded there, whether or not it moved the board.
    events: Arc<Mutex<Option<Events>>>,
    /// Told EVERY report on this socket once the board has had it, whether it moved the board or
    /// not — a slot filled after the fact, for `answering`'s reason. A smart close queued for a
    /// turn's end waits on it (`crate::smartclose`): the `Stop` that ends a turn in which the
    /// chat asked a question moves nothing a reader sees, and is still the end of the turn.
    all_reports: Arc<Mutex<Option<Heard>>>,
    /// Told each session record a chat's `charter session record` says it saved (ADR 0064) — a
    /// slot filled after the fact, for `answering`'s reason.
    saved: Arc<Mutex<Option<SavedHeard>>>,
}

/// What is told a session record was saved.
pub type SavedHeard = Arc<dyn Fn(SessionSaved) + Send + Sync + 'static>;

/// What is told each report the board took.
pub type Heard = Arc<dyn Fn(&Report) + Send + Sync + 'static>;

/// What is told that chat `session` is now in conversation `id`: the id its own harness
/// reported, that the board adopted or followed — and, where the move began a run (`/clear`,
/// ADR 0066), that run's id, which is the chat's current run from now on.
pub type Following = Arc<dyn Fn(u32, &str, Option<&str>) + Send + Sync + 'static>;

/// What answers an ask, told which connection it came on.
pub type Answering = Arc<dyn Fn(u64, Ask) -> Answer + Send + Sync + 'static>;

/// The host's event log, shared by every project this process holds: one writer per device.
pub type Events = Arc<Mutex<charter_core::eventlog::Recorder>>;

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
            tokens: None,
            answering: Arc::new(Mutex::new(None)),
            heard: Arc::new(Mutex::new(None)),
            following: Arc::new(Mutex::new(None)),
            all_reports: Arc::new(Mutex::new(None)),
            saved: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(None)),
        }
    }

    /// Listens on `socket`, handing each change to `moved`.
    ///
    /// A socket that cannot be opened is not worth refusing to start over: the app comes up
    /// with every chat `unknown` and says so on stderr, which is a working app with one
    /// feature missing rather than no app at all.
    pub fn listening_on(
        plane: PlaneId,
        at: &Where,
        moved: Teller,
        told_by_hand: ByHandTeller,
    ) -> std::io::Result<Self> {
        let listener = Listener::bind(&at.within, &at.socket)?;
        let socket = listener.path().to_path_buf();
        let tokens = listener.tokens();
        let board = Arc::new(Mutex::new(Board::new()));
        let answering: Arc<Mutex<Option<Answering>>> = Arc::new(Mutex::new(None));
        let heard: Arc<Mutex<Option<Heard>>> = Arc::new(Mutex::new(None));
        let following: Arc<Mutex<Option<Following>>> = Arc::new(Mutex::new(None));
        let all_reports: Arc<Mutex<Option<Heard>>> = Arc::new(Mutex::new(None));
        let saved: Arc<Mutex<Option<SavedHeard>>> = Arc::new(Mutex::new(None));
        let events: Arc<Mutex<Option<Events>>> = Arc::new(Mutex::new(None));
        let reading = listener.hear(Hearing {
            each: {
                let board = Arc::clone(&board);
                let plane = plane.clone();
                let heard = Arc::clone(&heard);
                let following = Arc::clone(&following);
                let all_reports = Arc::clone(&all_reports);
                let moved = Arc::clone(&moved);
                let events = Arc::clone(&events);
                Box::new(move |report| {
                    let applied = apply(&board, &plane, &report);
                    let followed = applied.followed();
                    // The run a `/clear` begins is the host's, minted here, so the record
                    // names it whether or not this machine keeps an event log (ADR 0066).
                    let begun = (followed == charter_core::eventlog::Followed::Moved)
                        .then(charter_core::reopen::mint);
                    let recorded = record(&events, |log| {
                        log.report_with(plane.root(), &report, followed, begun.as_deref())
                    });
                    // Before the window is told, so the record already names the conversation
                    // by the time anything the move prompts could ask for it. Whether or not a
                    // reader sees a difference: after `/clear` the chat may be in the state it
                    // was in, under a conversation it was not.
                    if let Some(id) = applied.followed {
                        let listener = following
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone();
                        if let Some(listener) = listener {
                            listener(report.chat, &id, begun.as_deref());
                        }
                    }
                    if let Some(what) = applied.moved {
                        moved(what);
                        // Only a report that moved the board: one for a chat it does not have,
                        // or one ADR 0024's rules refused, is heard by nobody. A chat's first
                        // start always moves it (`unknown` to `waiting`). Taken out of the
                        // lock before it runs, as an answer is.
                        let listener = heard.lock().unwrap_or_else(PoisonError::into_inner).clone();
                        if let Some(listener) = listener {
                            listener(&report);
                        }
                    }
                    let listener = all_reports
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    if let Some(listener) = listener {
                        listener(&report);
                    }
                    recorded
                })
            },
            answer: {
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
            noticed: {
                let plane = plane.clone();
                Box::new(move |notice| {
                    if let Some(told) = by_hand(&plane, notice) {
                        told_by_hand(told);
                    }
                })
            },
            saved: {
                let saved = Arc::clone(&saved);
                Box::new(move |record| {
                    let listener = saved.lock().unwrap_or_else(PoisonError::into_inner).clone();
                    match listener {
                        Some(listener) => listener(record),
                        None => tracing::warn!(
                            "charter: chat {} saved a session record before this project could \
                             hear it, so nothing was closed",
                            record.chat
                        ),
                    }
                })
            },
            refused: {
                // A refused commit (SQ-16): an event in the log, and a needs-you item on the
                // chat, built under the same hold as the change, as every other move is. Taken
                // only once the event is durable (FD-30).
                let board = Arc::clone(&board);
                let plane = plane.clone();
                let events = Arc::clone(&events);
                Box::new(move |refused| {
                    let recorded = record(&events, |log| log.refused(plane.root(), refused.chat));
                    let what = {
                        let mut guard = held_board(&board);
                        guard
                            .commit_refused(refused.chat, &refused.commit_refused)
                            .then(|| seen_by(&guard, &plane, refused.chat))
                    };
                    if let Some(what) = what {
                        moved(what);
                    }
                    recorded
                })
            },
            tool: {
                let events = Arc::clone(&events);
                let plane = plane.clone();
                Box::new(move |call| {
                    record(&events, |log| {
                        log.tool(plane.root(), &call, std::time::Instant::now())
                    })
                })
            },
        });
        Ok(Self {
            plane,
            board,
            reading: Mutex::new(Some(reading)),
            socket: Some(socket),
            tokens: Some(tokens),
            answering,
            heard,
            following,
            all_reports,
            saved,
            events,
        })
    }

    /// Records every hook call this project's channel hears into `events` from now on (FD-9).
    pub fn record_into(&self, events: Events) {
        *self.events.lock().unwrap_or_else(PoisonError::into_inner) = Some(events);
    }

    /// Drains this project's hook spool into the event log (FD-30, ADR 0068 §6): the lines its
    /// chats' hooks spooled while no host took them, each checked, and every gap and rejected
    /// line recorded as such.
    ///
    /// **Before any chat starts**, as a project is opened: a chat the reopen record names is
    /// told to the log first, so a line it spooled is recorded under the run it ran in. With
    /// no event log, or no channel, nothing is drained and the spool waits for a host that has
    /// both.
    pub fn drain_spool(&self) {
        let Some(socket) = &self.socket else { return };
        let Some(events) = self
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        else {
            return;
        };
        let root = self.plane.root();
        let mut log = events.lock().unwrap_or_else(PoisonError::into_inner);
        if charter_core::reopen::path(root).is_file()
            && let Ok(record) = charter_core::reopen::read_or_refusal(root)
        {
            for chat in record.chats {
                if let (Some(number), Some(id), Some(run)) =
                    (chat.number, chat.identity.id, chat.identity.run)
                {
                    log.knows(
                        root,
                        number,
                        charter_core::eventlog::RunOf {
                            chat: &id,
                            run: &run,
                        },
                    );
                }
            }
        }
        let durable = log.durable();
        let drained = charter_core::hookwire::spool::drain(
            &charter_core::hookwire::spool::dir_for(socket),
            &mut |item| {
                let event = log.spooled(root, item)?;
                durable.through(event.seq)
            },
        );
        if let Err(why) = drained {
            tracing::warn!(
                "charter: the hook spool was not drained ({why}); it is drained at the next start"
            );
        }
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

    /// Who is told each report the board takes from now on.
    pub fn when_heard(&self, heard: Heard) {
        *self.heard.lock().unwrap_or_else(PoisonError::into_inner) = Some(heard);
    }

    /// Who is told every report on this socket from now on, after the board has had it, whether
    /// or not it moved anything.
    pub fn when_reported(&self, heard: Heard) {
        *self
            .all_reports
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(heard);
    }

    /// Who is told, from now on, each session record a chat says it saved (ADR 0064).
    pub fn when_saved(&self, saved: SavedHeard) {
        *self.saved.lock().unwrap_or_else(PoisonError::into_inner) = Some(saved);
    }

    /// Who is told, from now on, each time a chat's own harness moves it onto another
    /// conversation.
    ///
    /// **Only what the board took.** It is told the conversation [`Board::conversation`]
    /// answers after a report, and only when that changed — so a report ADR 0024's rules
    /// refused (a nested harness's, [`Conversation::Contradicted`], a
    /// [`Conversation::Foreign`] after adoption) is told to nobody, because it moved nothing.
    ///
    /// [`Conversation::Contradicted`]: charter_core::hookwire::Conversation::Contradicted
    /// [`Conversation::Foreign`]: charter_core::hookwire::Conversation::Foreign
    pub fn when_it_follows(&self, following: Following) {
        *self
            .following
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(following);
    }

    /// Where this listens. Only a test asks: a chat is told through [`Hooks::reporting`].
    #[cfg(test)]
    pub fn socket(&self) -> Option<&Path> {
        self.socket.as_deref()
    }

    /// What a chat started in this plane is told to report to: the socket and the tokens it
    /// checks, or nothing when this is not listening.
    pub fn reporting(&self) -> Option<crate::sessions::Reporting> {
        Some(crate::sessions::Reporting {
            socket: self.socket.clone()?,
            tokens: Arc::clone(self.tokens.as_ref()?),
        })
    }

    /// A fresh token for chat `chat`, as the app issues one when the chat starts: what a test
    /// standing in for that chat's hook sends with.
    #[cfg(test)]
    pub fn token_for(&self, chat: u32) -> charter_core::hookwire::ChatToken {
        self.tokens
            .as_ref()
            .expect("the plane is listening")
            .issue(chat)
            .expect("a token")
    }

    pub fn board(&self) -> MutexGuard<'_, Board> {
        held_board(&self.board)
    }

    /// The board itself, for a caller that has to outlive this handle — the exit reporting,
    /// which is armed before the app manages anything.
    pub fn shared_board(&self) -> Arc<Mutex<Board>> {
        Arc::clone(&self.board)
    }
}

/// The in-process host's board. Each call is documented once, on [`ChatBoard`].
impl ChatBoard for Hooks {
    fn glance(&self, session: u32) -> Glance {
        let board = self.board();
        Glance {
            state: board.state(session),
            asking: board.asking(session),
            turns: board.turns(session),
        }
    }

    fn conversation(&self, session: u32) -> Option<String> {
        self.board().conversation(session).map(str::to_owned)
    }

    fn now(&self, session: u32) -> Moved {
        now(&self.board, self.plane.clone(), session)
    }

    fn closed(&self, session: u32) -> Moved {
        let mut board = self.board();
        board.closed(session);
        seen_by(&board, &self.plane, session)
    }

    fn ignored(&self, session: u32) -> Moved {
        let mut board = self.board();
        board.ignored(session);
        seen_by(&board, &self.plane, session)
    }

    fn reported_back(&self, session: u32, from: &str) -> Option<Moved> {
        let mut board = self.board();
        board
            .reported_back(session, from)
            .then(|| seen_by(&board, &self.plane, session))
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
        refusals: board.refusals(session),
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
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            Some(n.saturating_add(1))
        })
        .unwrap_or_else(|n| n);
    was.saturating_add(1)
}

/// What one report did.
struct Applied {
    /// What a reader would now see differently, or nothing when no reader would.
    moved: Option<Moved>,
    /// The conversation the chat is in now, where the report moved it onto one: the first id
    /// a Codex or opencode chat names, or the one a Claude Code chat's own process moved to on
    /// `/clear` (C6). Nothing for a report that left the chat where it was.
    followed: Option<String>,
    /// The conversation the chat was in before the report, where it was in one: with
    /// `followed`, what tells a harness naming its conversation from `/clear` (ADR 0066).
    was: Option<String>,
}

impl Applied {
    /// What the board did with the chat's conversation, as the event log reads a new run.
    fn followed(&self) -> charter_core::eventlog::Followed {
        use charter_core::eventlog::Followed;
        match (&self.followed, &self.was) {
            (None, _) => Followed::No,
            (Some(_), None) => Followed::FirstNamed,
            (Some(_), Some(_)) => Followed::Moved,
        }
    }
}

/// Applies one report, answering with what a reader would now see differently and which
/// conversation, if a new one, the chat is now in.
///
/// The answer is built under the SAME hold as the change. Reports arrive on a thread each, so
/// dropping the lock in between let two of them interleave — mutate, mutate, read, read — and
/// the window could then be sent the older of the two snapshots last and keep it until the
/// next event. A review found it.
///
/// **Which conversation is the board's answer, read before and after**, and not the report's:
/// [`Board::reported`] is the one place that decides whether a report is the chat's own
/// harness speaking (ADR 0024), and a second reading of the report here would be a second
/// answer to that question.
fn apply(board: &Mutex<Board>, plane: &PlaneId, report: &Report) -> Applied {
    let mut guard = held_board(board);
    let was = guard.conversation(report.chat).map(str::to_owned);
    let moved = guard
        .reported(report)
        .then(|| seen_by(&guard, plane, report.chat));
    let now = guard.conversation(report.chat);
    let followed = now
        .filter(|now| was.as_deref() != Some(*now))
        .map(str::to_owned);
    Applied {
        moved,
        followed,
        was,
    }
}

/// Writes one event into the host's log, when there is one, and answers once it is durable:
/// the hook is told its line is taken only on `Ok`, and answers its harness after that (ADR
/// 0075 §7, FD-30). The `fsync` is outside the log's lock, so hooks recorded at once share one.
///
/// A write or a sync that fails is said in the app's log and is the error: the hook is not
/// told its line was taken, so it spools the line and the next open records it. With no event
/// log there is nothing to wait for, and that is `Ok`.
fn record(
    events: &Mutex<Option<Events>>,
    write: impl FnOnce(
        &mut charter_core::eventlog::Recorder,
    ) -> std::io::Result<charter_core::eventlog::Event>,
) -> std::io::Result<()> {
    let Some(held) = events
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
    else {
        return Ok(());
    };
    let (event, durable) = {
        let mut log = held.lock().unwrap_or_else(PoisonError::into_inner);
        (write(&mut log), log.durable())
    };
    event
        .and_then(|event| durable.through(event.seq))
        .inspect_err(|why| {
            tracing::warn!("charter: a hook call was not recorded durably ({why})");
        })
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
        assert!(
            apply(&hooks.board, &hooks.plane, &stop(session))
                .moved
                .is_some()
        );
        hooks
    }

    fn stop(session: u32) -> Report {
        Report {
            chat: session,
            event: charter_core::state::Event::Stop,
            conversation: charter_core::hookwire::Conversation::Unknown,
            pid: None,
            agent: None,
            detail: charter_core::state::Detail::default(),
        }
    }

    #[test]
    fn a_report_taken_before_a_close_is_numbered_before_it() {
        // charter-app#248, the race #256 left open: a report's `Moved` is sent on the thread
        // that read it, after the board is let go, so it can reach the window AFTER a close
        // that came later. The number is what lets the window tell which is newer.
        let hooks = asking(7);
        let late = apply(&hooks.board, &hooks.plane, &stop(7)).moved;
        hooks.board().ignored(7);
        let report = apply(&hooks.board, &hooks.plane, &stop(7))
            .moved
            .expect("a new request");

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

    #[test]
    fn a_harness_started_by_hand_in_a_shell_tab_is_told_to_the_window_with_its_plane() {
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let hooks = Hooks::listening_on(
            plane.clone(),
            &at,
            Arc::new(|_| panic!("a harness started by hand moves no chat")),
            Arc::new(move |told| tx.lock().unwrap().send(told).unwrap()),
        )
        .expect("listening");

        charter_core::hookwire::tell(
            hooks.socket().expect("a socket"),
            Some(&hooks.token_for(3)),
            &StartedByHand {
                chat: 3,
                started_by_hand: "codex".to_owned(),
                cwd: Some(PathBuf::from("/work/alpha")),
            },
        )
        .expect("told");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(ByHand {
                plane,
                session: 3,
                harness: "codex".to_owned(),
                cwd: Some("/work/alpha".to_owned()),
            })
        );
    }

    #[test]
    fn every_hook_call_the_channel_hears_is_one_event_in_the_hosts_log() {
        use charter_core::eventlog::{self, ArgsKey, Log, Recorder};
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let logs = dir.path().join("events");
        hooks.record_into(Arc::new(Mutex::new(Recorder::new(
            Log::open(&logs, "DEVICE").expect("a log"),
            ArgsKey::open(&logs).expect("a key"),
        ))));
        hooks.board().opened(3, Some(Harness::ClaudeCode), None);
        let socket = hooks.socket().expect("a socket");
        let token = hooks.token_for(3);

        charter_core::hookwire::send(
            socket,
            Some(&token),
            &Report {
                chat: 3,
                event: charter_core::state::Event::UserPromptSubmit,
                conversation: Default::default(),
                pid: None,
                agent: None,
                detail: Default::default(),
            },
        )
        .expect("sent");
        // Each line is its own connection; the second is sent once the first is in the log.
        let logged = |at_least: usize| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let kinds: Vec<String> = eventlog::read(&logs)
                    .expect("the log reads")
                    .into_iter()
                    .map(|event| event.kind)
                    .collect();
                if kinds.len() >= at_least || std::time::Instant::now() > until {
                    break kinds;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        assert_eq!(logged(2).len(), 2, "the report is in the log");
        charter_core::hookwire::deliver_tool(
            socket,
            Some(&token),
            &charter_core::hookwire::ToolCall {
                chat: 3,
                tool_hook: "pretooluse".to_owned(),
                tool: Some("Bash".to_owned()),
                call: Some("toolu_1".to_owned()),
                args: Some(eventlog::args_hash(&serde_json::json!({"command": "ls"}))),
                decision: charter_core::hookwire::Decision::None,
                rule: None,
                hook_ms: 1,
                agent: None,
                at_ms: 0,
            },
        )
        .expect("told");

        let kinds = logged(3);
        assert_eq!(
            kinds,
            vec!["run.started", "hook.userpromptsubmit", "hook.pretooluse"],
            "one event per hook call, after the run it is under"
        );
    }

    #[test]
    fn a_refused_commit_puts_the_chat_in_the_queue_saying_why() {
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let hooks = Hooks::listening_on(
            plane,
            &at,
            Arc::new(move |moved| tx.lock().unwrap().send(moved).unwrap()),
            Arc::new(|_| panic!("a refused commit is not a harness started by hand")),
        )
        .expect("listening");
        hooks.board().opened(3, Some(Harness::ClaudeCode), None);
        let said = "commit refused in app: a.py:2  an email address  ad**";

        charter_core::hookwire::deliver_refused(
            hooks.socket().expect("a socket"),
            Some(&hooks.token_for(3)),
            &charter_core::hookwire::CommitRefused {
                chat: 3,
                commit_refused: said.to_owned(),
            },
        )
        .expect("told");

        let moved = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the window is told");
        assert_eq!(moved.session, 3);
        assert!(moved.needs_you);
        assert_eq!(moved.queue, vec![3]);
        assert_eq!(moved.refusals, vec![said.to_owned()]);
    }

    #[test]
    fn a_word_that_is_no_harness_charter_starts_puts_nothing_on_the_tab() {
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");

        let told = by_hand(
            &plane,
            StartedByHand {
                chat: 3,
                started_by_hand: "vim".to_owned(),
                cwd: None,
            },
        );

        assert_eq!(told, None);
    }

    use charter_core::harness::Harness;
    use charter_core::hookwire::Conversation;

    /// A board holding chat 7 running `harness`, under `conversation` where charter chose one.
    fn running(harness: Harness, conversation: Option<&str>) -> Hooks {
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks = Hooks::deaf(plane);
        hooks
            .board()
            .opened(7, Some(harness), conversation.map(str::to_owned));
        hooks
    }

    /// The conversation chat 7 moved onto when `conversation` was reported from `pid`, if any.
    fn followed_after(
        hooks: &Hooks,
        conversation: Conversation,
        pid: Option<u32>,
    ) -> Option<String> {
        let report = Report {
            agent: None,
            chat: 7,
            event: charter_core::state::Event::UserPromptSubmit,
            conversation,
            pid,
            detail: charter_core::state::Detail::default(),
        };
        apply(&hooks.board, &hooks.plane, &report).followed
    }

    fn named(id: &str) -> Conversation {
        Conversation::Named(id.to_owned())
    }

    #[test]
    fn a_codex_chat_follows_the_first_conversation_its_harness_names_and_no_other() {
        let hooks = running(Harness::Codex, None);

        assert_eq!(
            followed_after(&hooks, named("aaa"), None).as_deref(),
            Some("aaa")
        );
        assert_eq!(
            followed_after(&hooks, named("aaa"), None),
            None,
            "no rewrite"
        );
        assert_eq!(
            followed_after(&hooks, named("bbb"), None),
            None,
            "a later id from a pid-less harness is a nested run, never followed"
        );
    }

    #[test]
    fn an_opencode_chat_follows_the_first_conversation_its_plugin_names() {
        let hooks = running(Harness::Opencode, None);

        assert_eq!(
            followed_after(&hooks, named("ses_abc"), None).as_deref(),
            Some("ses_abc")
        );
    }

    #[test]
    fn a_report_that_is_not_the_chat_s_own_harness_moves_no_conversation() {
        // ADR 0024 C5: a harness nested in the chat's shell must not be able to rewrite what
        // the chat resumes at the next launch.
        let codex = running(Harness::Codex, None);
        assert_eq!(
            followed_after(&codex, Conversation::Contradicted, None),
            None
        );
        assert_eq!(
            followed_after(&codex, named("ccc"), Some(99)),
            None,
            "a claude inside a codex chat"
        );
        assert_eq!(
            followed_after(&codex, named("aaa"), None).as_deref(),
            Some("aaa")
        );
        assert_eq!(followed_after(&codex, Conversation::Foreign, None), None);

        let claude = running(Harness::ClaudeCode, Some("chosen"));
        assert_eq!(followed_after(&claude, named("chosen"), Some(10)), None);
        assert_eq!(
            followed_after(&claude, named("nested"), Some(11)),
            None,
            "a claude started inside a claude chat"
        );
        assert_eq!(
            followed_after(&claude, Conversation::Contradicted, Some(10)),
            None
        );
    }

    #[test]
    fn a_claude_chat_follows_its_own_process_onto_the_conversation_a_clear_starts() {
        let hooks = running(Harness::ClaudeCode, Some("chosen"));
        assert_eq!(
            followed_after(&hooks, named("chosen"), Some(10)),
            None,
            "the id charter chose is already recorded; adopting it rewrites nothing"
        );

        assert_eq!(
            followed_after(&hooks, named("cleared"), Some(10)).as_deref(),
            Some("cleared")
        );
    }

    #[test]
    fn a_line_spooled_while_no_host_listened_is_recorded_when_the_project_is_opened_again() {
        use charter_core::eventlog::{self, ArgsKey, Log, Recorder};
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join(".charter/app/hooks.sock"),
        };
        let plane: PlaneId =
            serde_json::from_value(serde_json::json!(dir.path())).expect("a plane id");
        let call = charter_core::hookwire::ToolCall {
            chat: 7,
            tool_hook: "posttooluse".to_owned(),
            tool: Some("Read".to_owned()),
            call: Some("toolu_1".to_owned()),
            args: None,
            decision: charter_core::hookwire::Decision::None,
            rule: None,
            hook_ms: 1,
            agent: None,
            at_ms: 0,
        };
        // The app issued chat 7 its token and quit; the chat's hook spooled its call.
        {
            let gone = Hooks::listening_on(plane.clone(), &at, Arc::new(|_| {}), Arc::new(|_| {}))
                .expect("listening");
            let token = gone.token_for(7);
            gone.stop();
            let delivered = charter_core::hookwire::deliver_tool(&at.socket, Some(&token), &call)
                .expect("spooled");
            assert_eq!(delivered, charter_core::hookwire::Delivered::Spooled(1));
        }

        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let logs = dir.path().join("events");
        hooks.record_into(Arc::new(Mutex::new(Recorder::new(
            Log::open(&logs, "DEVICE").expect("a log"),
            ArgsKey::open(&logs).expect("a key"),
        ))));
        hooks.drain_spool();

        let kinds: Vec<String> = eventlog::read(&logs)
            .expect("the log reads")
            .into_iter()
            .map(|event| event.kind)
            .collect();
        assert_eq!(kinds, ["hook.posttooluse", "hook.spool.drained"]);
    }

    #[test]
    fn the_run_a_clear_begins_is_told_with_the_conversation_it_moved_to() {
        // ADR 0066's `clear`: the event log begins the run, and the record has to hold it as
        // the chat's current run, so it is told beside the conversation.
        use charter_core::eventlog::{self, ArgsKey, Log, Recorder};
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let logs = dir.path().join("events");
        hooks.record_into(Arc::new(Mutex::new(Recorder::new(
            Log::open(&logs, "DEVICE").expect("a log"),
            ArgsKey::open(&logs).expect("a key"),
        ))));
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        hooks.when_it_follows(Arc::new(move |chat, id, run| {
            let _ = tx
                .lock()
                .unwrap()
                .send((chat, id.to_owned(), run.map(str::to_owned)));
        }));
        hooks
            .board()
            .opened(7, Some(Harness::ClaudeCode), Some("chosen".to_owned()));
        let socket = hooks.socket().expect("a socket");
        let token = hooks.token_for(7);
        let say = |conversation: &str| {
            charter_core::hookwire::send(
                socket,
                Some(&token),
                &Report {
                    chat: 7,
                    event: charter_core::state::Event::UserPromptSubmit,
                    conversation: named(conversation),
                    pid: Some(10),
                    agent: None,
                    detail: Default::default(),
                },
            )
            .expect("sent");
        };

        say("chosen");
        say("cleared");

        let (chat, id, run) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the move is told");
        assert_eq!((chat, id.as_str()), (7, "cleared"));
        let cleared: Vec<_> = eventlog::read(&logs)
            .expect("the log reads")
            .into_iter()
            .filter(|event| event.kind == "run.started" && event.body["cause"] == "clear")
            .map(|event| event.run)
            .collect();
        assert_eq!(cleared, vec![run], "the run the log began for the clear");
    }

    #[test]
    fn a_clear_begins_a_run_the_record_is_told_even_with_no_event_log() {
        // #856 review F5: the run is the host's, so `reopen.json` moves with it either way.
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        hooks.when_it_follows(Arc::new(move |_, id, run| {
            let _ = tx
                .lock()
                .unwrap()
                .send((id.to_owned(), run.map(str::to_owned)));
        }));
        hooks
            .board()
            .opened(7, Some(Harness::ClaudeCode), Some("chosen".to_owned()));
        let token = hooks.token_for(7);
        for conversation in ["chosen", "cleared"] {
            charter_core::hookwire::send(
                hooks.socket().expect("a socket"),
                Some(&token),
                &Report {
                    chat: 7,
                    event: charter_core::state::Event::UserPromptSubmit,
                    conversation: named(conversation),
                    pid: Some(10),
                    agent: None,
                    detail: Default::default(),
                },
            )
            .expect("sent");
        }

        let (id, run) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the move is told");
        assert_eq!(id, "cleared");
        assert!(
            run.as_deref()
                .and_then(charter_core::reopen::a_ulid)
                .is_some(),
            "{run:?}"
        );
    }
}
