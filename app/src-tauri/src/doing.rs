//! What each working chat is doing, told to the window as one small value per chat (#1493).
//!
//! The core sorts what a tool hook says and keeps the latest per chat
//! (`purlis_core::doing`); this holds that for one project, feeds it the two facts it asks of
//! the board, and tells the window `chat-doing` when a chat's line changes, at most a few
//! times a second per chat. **In memory only**: nothing here writes it anywhere, the event
//! log is not told, and it goes with the chat.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use purlis_core::doing::{Kind, Said, Tell, Told, Tracker};
use purlis_core::state::{Board, State};

use crate::planes::PlaneId;

/// The event the window is sent when what a chat is doing changes.
pub const EVENT: &str = "chat-doing";

/// What one chat is doing, as the window says it in one line: a kind from the fixed list
/// (`purlis_core::doing::Kind`) and at most one short name the core has passed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Doing {
    /// The kind's word: `thinking`, `command`, `editing`, `reading`, `searching`, `fetching`,
    /// `helper`, `dispatching`, `asking`, `reporting`, `tool`.
    pub kind: String,
    /// A file's base name (ASCII letters, digits and a few marks, short) or a program from the
    /// core's fixed list, where the kind has one. Shown as text.
    pub name: Option<String>,
    /// How many files it has read in a row, for `reading`; 0 otherwise.
    pub count: u32,
    /// Whether a hook said its tool came back: the window then says it in the past ("ran
    /// cargo"), until the next tool heard.
    pub over: bool,
}

/// What `chat-doing` carries, and what `chat_doings` answers a list of: one chat's line, or
/// that it has none. It travels in memory only.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ChatDoing {
    pub plane: PlaneId,
    pub session: u32,
    /// Which telling this is. Tellings reach the window in any order, and for one chat the
    /// higher number is the later word.
    pub sequence: u32,
    /// What it is doing, or null: its turn ended, it is asking the person, or it is gone.
    pub doing: Option<Doing>,
}

/// Told each change of a chat's line.
pub type Teller = Arc<dyn Fn(ChatDoing) + Send + Sync + 'static>;

/// What one project's chats are doing.
pub struct Doings {
    plane: PlaneId,
    tracker: Mutex<Tracker>,
    /// A slot filled after the fact, as the hooks' other tellers are.
    teller: Mutex<Option<Teller>>,
}

impl Doings {
    /// Nothing known of any chat of `plane`.
    pub fn new(plane: PlaneId) -> Arc<Self> {
        Arc::new(Self {
            plane,
            tracker: Mutex::new(Tracker::default()),
            teller: Mutex::new(None),
        })
    }

    /// Who is told each change from now on.
    pub fn tell_to(&self, teller: Teller) {
        *self.teller.lock().unwrap_or_else(PoisonError::into_inner) = Some(teller);
    }

    /// The board took a report of chat `chat`: a turn that began starts its line, and a chat
    /// that is no longer running loses it.
    ///
    /// **The board is held while the tracker is told**, here and in [`Self::heard`], so the
    /// tracker always ends on what the board says now, whichever of two hooks' threads comes
    /// second. The window is told after both are let go.
    pub fn reported(self: &Arc<Self>, board: &Mutex<Board>, chat: u32) {
        let tell = {
            let board = held(board);
            let running = board.state(chat) == State::Running;
            self.tracker()
                .reported(chat, board.turns(chat), running, Instant::now())
        };
        self.act(chat, tell);
    }

    /// A tool hook of chat `chat` said `said`. Nothing for a chat the board does not have
    /// running: one whose hooks have begun no turn the app heard.
    pub fn heard(self: &Arc<Self>, board: &Mutex<Board>, chat: u32, said: Said) {
        let tell = {
            let board = held(board);
            let running = board.state(chat) == State::Running;
            self.tracker().heard(chat, said, running, Instant::now())
        };
        self.act(chat, tell);
    }

    /// Chat `chat`'s own purlis command reached the app: a dispatch, a question for the chat
    /// that dispatched it, or its report. Nothing of the command is kept.
    pub fn asked(self: &Arc<Self>, board: &Mutex<Board>, chat: u32, kind: Kind) {
        let tell = {
            let board = held(board);
            let running = board.state(chat) == State::Running;
            self.tracker().asked(chat, kind, running, Instant::now())
        };
        self.act(chat, tell);
    }

    /// Chat `chat` is closed: what was held of it is dropped, and the window is told it has
    /// no line, so a chat that later wears the same number never starts with this one's.
    pub fn closed(&self, chat: u32) {
        let gone = self.tracker().closed(chat);
        if let Some(told) = gone {
            self.tell(chat, told);
        }
    }

    /// The line of every chat `board` has running, for a window that has just opened.
    pub fn now(&self, board: &Mutex<Board>) -> Vec<ChatDoing> {
        let board = held(board);
        self.tracker()
            .all()
            .into_iter()
            .filter(|(chat, _)| board.state(*chat) == State::Running)
            .map(|(chat, told)| self.said(chat, told))
            .collect()
    }

    fn tracker(&self) -> MutexGuard<'_, Tracker> {
        self.tracker.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn said(&self, chat: u32, told: Told) -> ChatDoing {
        ChatDoing {
            plane: self.plane.clone(),
            session: chat,
            sequence: told.sequence,
            doing: told.doing.map(|doing| Doing {
                kind: doing.kind.word().to_owned(),
                name: doing.name,
                count: doing.count,
                over: doing.over,
            }),
        }
    }

    fn tell(&self, chat: u32, told: Told) {
        // Taken out of the lock before it runs, as an answer is.
        let teller = self
            .teller
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(teller) = teller {
            teller(self.said(chat, told));
        }
    }

    /// Tells the window now, or once the chat's share of tellings allows: one short-lived
    /// thread per chat at most, since the tracker puts off one telling at a time.
    fn act(self: &Arc<Self>, chat: u32, tell: Tell) {
        match tell {
            Tell::Now(told) => self.tell(chat, told),
            Tell::Later(wait) => {
                let doings = Arc::downgrade(self);
                let spawned = std::thread::Builder::new()
                    .name("purlis-chat-doing".to_owned())
                    .spawn(move || {
                        std::thread::sleep(wait);
                        let Some(doings) = doings.upgrade() else {
                            return;
                        };
                        let due = doings.tracker().due(chat, Instant::now());
                        if let Some(told) = due {
                            doings.tell(chat, told);
                        }
                    });
                if spawned.is_err() {
                    // No thread to wait on: say it now rather than never.
                    let due = self.tracker().due(chat, Instant::now());
                    if let Some(told) = due {
                        self.tell(chat, told);
                    }
                }
            }
            Tell::Nothing => {}
        }
    }
}

/// The board, whether or not a thread panicked while holding it (`hooks::held_board`).
fn held(board: &Mutex<Board>) -> MutexGuard<'_, Board> {
    crate::hooks::held_board(board)
}

/// Which line chat `ask.chat`'s own purlis command gives it, where it has one: nothing of the
/// ask but its kind is read.
pub fn of_ask(ask: &purlis_core::hookwire::Ask) -> Option<(u32, Kind)> {
    use purlis_core::dispatched::What;
    use purlis_core::hookwire::Ask;
    match ask {
        Ask::Dispatch(dispatch) => Some((dispatch.chat, Kind::Dispatching)),
        Ask::Report(report) => Some((report.chat, Kind::Reporting)),
        Ask::Task(asked) if matches!(asked.what, What::Question { .. }) => {
            Some((asked.chat, Kind::Asking))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purlis_core::hookwire::{Conversation, Report};
    use purlis_core::state::{Detail, Event};
    use std::sync::mpsc;
    use std::time::Duration;

    fn plane() -> PlaneId {
        PlaneId::for_tests(std::path::Path::new("/planes/p"))
    }

    fn report(chat: u32, event: Event) -> Report {
        Report {
            chat,
            event,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail::default(),
        }
    }

    /// A board with chat 4 open, the project's doings, and what the window is told.
    fn listening() -> (Mutex<Board>, Arc<Doings>, mpsc::Receiver<ChatDoing>) {
        let mut board = Board::new();
        board.opened(4, None, None);
        let doings = Doings::new(plane());
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        doings.tell_to(Arc::new(move |told| {
            let _ = tx.lock().unwrap().send(told);
        }));
        (Mutex::new(board), doings, rx)
    }

    fn hears(board: &Mutex<Board>, doings: &Arc<Doings>, chat: u32, event: Event) {
        held(board).reported(&report(chat, event));
        doings.reported(board, chat);
    }

    fn command(name: &str) -> Said {
        Said::Began {
            kind: Kind::Command,
            name: Some(name.to_owned()),
        }
    }

    fn next(rx: &mpsc::Receiver<ChatDoing>) -> ChatDoing {
        rx.recv_timeout(Duration::from_secs(5))
            .expect("the window is told")
    }

    fn kind_of(told: &ChatDoing) -> Option<&str> {
        told.doing.as_ref().map(|doing| doing.kind.as_str())
    }

    #[test]
    fn the_window_is_told_a_chat_s_line_as_it_works_and_that_it_is_gone_when_the_turn_ends() {
        let (board, doings, rx) = listening();
        hears(&board, &doings, 4, Event::SessionStart);
        assert!(
            rx.try_recv().is_err(),
            "a chat waiting for its first prompt has no line"
        );

        hears(&board, &doings, 4, Event::UserPromptSubmit);
        let first = next(&rx);
        assert_eq!(first.plane, plane());
        assert_eq!(first.session, 4);
        assert_eq!(kind_of(&first), Some("thinking"));

        std::thread::sleep(purlis_core::doing::AT_MOST_EVERY);
        doings.heard(&board, 4, command("cargo"));
        let second = next(&rx);
        assert_eq!(
            second.doing,
            Some(Doing {
                kind: "command".to_owned(),
                name: Some("cargo".to_owned()),
                count: 0,
                over: false
            })
        );
        assert!(second.sequence > first.sequence);
        assert_eq!(doings.now(&board).len(), 1);

        // The command comes back: the same line, in the past, and nothing else is invented.
        std::thread::sleep(purlis_core::doing::AT_MOST_EVERY);
        doings.heard(
            &board,
            4,
            Said::Ended {
                kind: Some(Kind::Command),
            },
        );
        let back = next(&rx);
        assert_eq!(
            back.doing,
            Some(Doing {
                kind: "command".to_owned(),
                name: Some("cargo".to_owned()),
                count: 0,
                over: true
            })
        );

        hears(&board, &doings, 4, Event::Stop);
        let last = next(&rx);
        assert_eq!(last.doing, None);
        assert!(last.sequence > back.sequence);
        assert!(
            doings.now(&board).is_empty(),
            "nothing is held once the turn has ended"
        );
    }

    #[test]
    fn a_chat_whose_hooks_began_no_turn_has_no_line_whatever_its_tools_say() {
        let (board, doings, rx) = listening();
        // A harness that sends no events, or one whose hooks are not trusted: the board has
        // heard nothing, so nothing a line says of the chat is believed.
        doings.heard(&board, 4, command("cargo"));
        doings.asked(&board, 4, Kind::Dispatching);
        // And a chat the project does not have.
        doings.heard(&board, 99, command("cargo"));
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        assert!(doings.now(&board).is_empty());
    }

    #[test]
    fn a_question_to_the_person_takes_the_line_away_until_a_turn_runs_again() {
        let (board, doings, rx) = listening();
        hears(&board, &doings, 4, Event::UserPromptSubmit);
        assert_eq!(kind_of(&next(&rx)), Some("thinking"));
        hears(&board, &doings, 4, Event::Notification);
        assert_eq!(next(&rx).doing, None);
        // What a tool says while the chat waits on the person is not its line.
        doings.heard(&board, 4, command("cargo"));
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn a_fast_tool_loop_reaches_the_window_a_few_times_a_second_and_ends_on_its_last_word() {
        let (board, doings, rx) = listening();
        hears(&board, &doings, 4, Event::UserPromptSubmit);
        let began = Instant::now();
        for call in 0..2000 {
            doings.heard(
                &board,
                4,
                Said::Began {
                    kind: Kind::Editing,
                    name: Some(format!("f{call}.rs")),
                },
            );
        }
        let took = began.elapsed();
        // Everything put off has been said once this has passed.
        std::thread::sleep(purlis_core::doing::AT_MOST_EVERY * 2);
        let told: Vec<ChatDoing> = rx.try_iter().collect();
        let most = 2 + took.as_millis() / purlis_core::doing::AT_MOST_EVERY.as_millis();
        assert!(
            told.len() as u128 <= most + 1,
            "{} tellings for 2000 tool calls in {took:?}",
            told.len()
        );
        let last = told.last().expect("told at least once");
        assert_eq!(
            last.doing.as_ref().and_then(|doing| doing.name.as_deref()),
            Some("f1999.rs"),
            "the window ends on the last thing said"
        );
    }

    #[test]
    fn a_closed_chat_is_forgotten_and_the_window_is_told_its_line_is_gone() {
        let (board, doings, rx) = listening();
        hears(&board, &doings, 4, Event::UserPromptSubmit);
        let said = next(&rx);
        doings.closed(4);
        let gone = next(&rx);
        assert_eq!((gone.session, &gone.doing), (4, &None));
        assert!(gone.sequence > said.sequence);
        assert!(doings.now(&board).is_empty());
        // A chat nothing was held of says nothing as it closes.
        doings.closed(9);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    }

    #[test]
    fn a_line_written_by_hand_cannot_say_the_chat_made_a_report() {
        let (board, doings, rx) = listening();
        hears(&board, &doings, 4, Event::UserPromptSubmit);
        next(&rx);
        std::thread::sleep(purlis_core::doing::AT_MOST_EVERY);
        doings.heard(
            &board,
            4,
            Said::Began {
                kind: Kind::Reporting,
                name: Some("cargo".to_owned()),
            },
        );
        let forged = next(&rx);
        assert_eq!(
            forged.doing,
            Some(Doing {
                kind: "tool".to_owned(),
                name: None,
                count: 0,
                over: false
            })
        );
        // The app's own word for a report that really reached it is the only way to it.
        std::thread::sleep(purlis_core::doing::AT_MOST_EVERY);
        doings.asked(&board, 4, Kind::Reporting);
        assert_eq!(kind_of(&next(&rx)), Some("reporting"));
    }

    #[test]
    fn a_chat_s_own_purlis_command_is_said_by_its_kind_alone() {
        use purlis_core::dispatched::{Asked, What};
        use purlis_core::hookwire::Ask;
        let task = |what| Ask::Task(Box::new(Asked { chat: 4, what }));
        assert_eq!(
            of_ask(&task(What::Question {
                text: "CANARY".to_owned()
            })),
            Some((4, Kind::Asking))
        );
        // Asking after its own tasks is not something its row says.
        assert_eq!(of_ask(&task(What::List)), None);
        assert_eq!(
            of_ask(&task(What::Wait {
                of: 5,
                within_secs: 60
            })),
            None
        );
        assert_eq!(of_ask(&Ask::Ticket { chat: 4 }), None);
        assert_eq!(of_ask(&Ask::Vaults { chat: 4 }), None);
    }
}
