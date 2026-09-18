//! The chats the app has open: the sessions, plus what each one was started as.
//!
//! `Sessions` knows how to run a program in a terminal and nothing about why. This knows
//! why: which harness a session is, what conversation it is under, and which one is in
//! front — everything a quit has to write down and a launch has to put back.
//!
//! It is a layer of its own so that the record is written from what the app itself did, and
//! not from anything a harness said. Nothing here reads a session's output.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use charter_core::engine::Size;
use charter_core::harness::Harness;
use charter_core::reopen::{Chat, Record, Reopened};

use crate::sessions::{Opening, Sessions};

/// One chat the app has open, as the UI and the quit warning see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    pub session: u32,
    pub name: String,
    pub cwd: Option<PathBuf>,
    pub harness: Option<Harness>,
    /// Whether it is the chat in front. At a launch this is the one that was in front when
    /// the app was quit, so the window comes back looking as it was left.
    pub in_front: bool,
    /// How it came to be open. A chat the operator just started is `Fresh`, the same as one
    /// that could not be resumed — the difference is only interesting at a relaunch, which
    /// is where the UI says it.
    pub how: Reopened,
}

/// Every chat the app has open, and which of them is in front.
pub struct Chats {
    sessions: Sessions,
    open: Mutex<HashMap<u32, (Chat, Reopened)>>,
    front: Mutex<Option<u32>>,
}

impl Chats {
    pub fn new() -> Self {
        Self {
            sessions: Sessions::new(),
            open: Mutex::new(HashMap::new()),
            front: Mutex::new(None),
        }
    }

    /// The sessions underneath, for everything that is about a terminal and not about a chat.
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// Starts a chat, and remembers what it was started as.
    pub fn start(&self, chat: &Chat, size: Size) -> Result<u32, String> {
        let launch = chat.launch();
        let session = self.sessions.open(&Opening {
            program: Some(launch.program),
            args: launch.args,
            cwd: chat.cwd.as_ref().map(|cwd| cwd.display().to_string()),
            size,
        })?;
        // Under the id it was actually given, not the one it was recorded with: a chat
        // started fresh is under an id the app just chose, and that is what has to be
        // written down for the next launch to resume it.
        let under = Chat {
            resume: launch.session,
            ..chat.clone()
        };
        lock(&self.open).insert(session, (under, launch.how));
        Ok(session)
    }

    /// Ends a chat. It is no longer one a quit would record.
    pub fn close(&self, session: u32) -> Result<(), String> {
        lock(&self.open).remove(&session);
        let mut front = lock(&self.front);
        if *front == Some(session) {
            *front = None;
        }
        self.sessions.close(session)
    }

    /// Says which chat is in front, so the record knows which one to bring back in front.
    pub fn bring_to_front(&self, session: Option<u32>) {
        *lock(&self.front) = session;
    }

    /// What is open, in the order the sessions were opened.
    pub fn open_now(&self) -> Vec<Open> {
        let open = lock(&self.open);
        let front = *lock(&self.front);
        // In the order the sessions were opened, which is the order their ids were handed
        // out, so the window comes back with its tabs the way they were left.
        self.sessions
            .running()
            .into_iter()
            .filter_map(|session| {
                let (chat, how) = open.get(&session)?;
                Some(Open {
                    session,
                    name: chat.name.clone(),
                    cwd: chat.cwd.clone(),
                    harness: chat.harness(),
                    in_front: front == Some(session),
                    how: how.clone(),
                })
            })
            .collect()
    }

    /// What was open, to write down.
    pub fn record(&self) -> Record {
        let open = lock(&self.open);
        let front = *lock(&self.front);
        Record {
            chats: self
                .sessions
                .running()
                .into_iter()
                .filter_map(|session| {
                    let (chat, _) = open.get(&session)?;
                    Some(Chat {
                        active: front == Some(session),
                        ..chat.clone()
                    })
                })
                .collect(),
        }
    }

    /// Puts a record back: one session per chat it holds, resumed where it can be.
    ///
    /// A chat whose program cannot be started is left out and the rest still open — a
    /// relaunch that failed whole because one harness had been uninstalled would be worse
    /// than one that came back short.
    pub fn put_back(&self, record: &Record, size: Size) -> Vec<Open> {
        let mut front = None;
        let opened: Vec<u32> = record
            .chats
            .iter()
            .filter_map(|chat| {
                let session = self.start(chat, size).ok()?;
                if chat.active {
                    front = Some(session);
                }
                Some(session)
            })
            .collect();
        self.bring_to_front(front);
        let open = self.open_now();
        open.into_iter()
            .filter(|one| opened.contains(&one.session))
            .collect()
    }

    /// How many chats are remembered, which is not the same as how many are running: this
    /// is what a chat that closed has to stop costing. Only the tests ask.
    #[cfg(test)]
    pub fn remembered(&self) -> usize {
        lock(&self.open).len()
    }

    /// Ends every chat, and does not return until their programs are gone.
    pub fn end_all(&self) {
        self.sessions.end_all();
        lock(&self.open).clear();
        *lock(&self.front) = None;
    }
}

impl Default for Chats {
    fn default() -> Self {
        Self::new()
    }
}

fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use charter_core::harness::SessionId;
    use charter_core::reopen::Fresh;

    use super::*;

    const SIZE: Size = Size {
        columns: 80,
        rows: 24,
    };
    const ID: &str = "11111111-2222-4333-8444-555555555555";

    /// A directory holding a program called `claude` that prints the arguments it was given
    /// and then waits, so a test can see what the app actually put on its command line.
    fn a_claude(dir: &std::path::Path) -> String {
        let claude = dir.join("claude");
        std::fs::write(
            &claude,
            "#!/bin/sh\nprintf 'argv:'\nfor word in \"$@\"; do printf ' %s' \"$word\"; done\nprintf '\\n'\nsleep 600\n",
        )
        .expect("the stand-in claude is written");
        std::fs::set_permissions(&claude, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("it is made runnable");
        claude.display().to_string()
    }

    fn chat(program: &str, name: &str, resume: Option<&str>) -> Chat {
        Chat {
            program: program.to_owned(),
            args: vec![],
            cwd: None,
            name: name.to_owned(),
            resume: resume.map(|id| SessionId::new(id).expect("a valid id in a test")),
            active: false,
        }
    }

    /// Everything a session has printed, once it has printed `text`.
    fn until_printed(chats: &Chats, session: u32, text: &str) -> String {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        let seen = Arc::new(Mutex::new(String::new()));
        let collect = {
            let seen = Arc::clone(&seen);
            move |more: String| lock(&seen).push_str(&more)
        };
        chats
            .sessions()
            .watch(session, Box::new(collect))
            .expect("the view opens");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let so_far = lock(&seen).clone();
            if so_far.contains(text) {
                return so_far;
            }
            assert!(
                Instant::now() < deadline,
                "{text:?} never arrived, only {so_far:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_chat_that_was_started_is_one_the_quit_would_record() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();

        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .expect("the chat starts");

        let record = chats.record();
        assert_eq!(record.chats.len(), 1);
        assert_eq!(record.chats[0].name, "ide.7");
        assert_eq!(chats.open_now()[0].session, session);
        assert_eq!(chats.open_now()[0].harness, Some(Harness::ClaudeCode));
    }

    #[test]
    fn the_record_holds_the_conversation_id_the_app_chose_for_a_new_claude_chat() {
        // The whole point of the record: a chat started fresh today is resumable tomorrow.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        let recorded = chats.record().chats[0].resume.clone();

        assert!(
            recorded.is_some(),
            "the chat was recorded with no conversation"
        );
    }

    #[test]
    fn the_id_in_the_record_is_the_one_the_harness_was_actually_given() {
        // Recording an id the harness never saw would give a resume that always failed.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        let printed = until_printed(&chats, session, "argv:");

        let recorded = chats.record().chats[0]
            .resume
            .clone()
            .expect("the chat has a conversation");
        assert!(
            printed.contains(recorded.as_str()),
            "the record says {recorded}, but claude was given {printed:?}"
        );
    }

    #[test]
    fn a_chat_that_closed_is_not_in_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let going = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.8", None), SIZE)
            .unwrap();

        chats.close(going).expect("it closes");

        let names: Vec<String> = chats
            .record()
            .chats
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(names, vec!["ide.8"]);
    }

    #[test]
    fn the_chat_in_front_is_the_one_the_record_marks_active() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();
        let front = chats
            .start(&chat(&a_claude(dir.path()), "ide.8", None), SIZE)
            .unwrap();

        chats.bring_to_front(Some(front));

        let active: Vec<String> = chats
            .record()
            .chats
            .iter()
            .filter(|c| c.active)
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(active, vec!["ide.8"]);
    }

    #[test]
    fn a_chat_that_closed_costs_nothing_to_remember() {
        // An app left running all day closes chats all day. Each one that stayed remembered
        // would be a little more memory that never comes back — invisible without this.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let going = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.close(going).expect("it closes");

        assert_eq!(chats.remembered(), 0);
    }

    #[test]
    fn the_chat_that_was_in_front_comes_back_in_front() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();
        let was_in_front = Chat {
            active: true,
            ..chat(&claude, "ide.8", None)
        };

        chats.put_back(
            &Record {
                chats: vec![chat(&claude, "ide.7", None), was_in_front],
            },
            SIZE,
        );

        let active: Vec<String> = chats
            .record()
            .chats
            .iter()
            .filter(|c| c.active)
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(active, vec!["ide.8"]);
    }

    #[test]
    fn a_record_is_put_back_as_one_session_for_each_chat_it_holds() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back(
            &Record {
                chats: vec![
                    chat(&claude, "ide.7", Some(ID)),
                    chat(&claude, "ide.8", None),
                ],
            },
            SIZE,
        );

        assert_eq!(open.len(), 2);
        assert_eq!(chats.sessions().running().len(), 2);
        assert_eq!(open[0].name, "ide.7");
        assert_eq!(open[1].name, "ide.8");
    }

    #[test]
    fn a_chat_put_back_with_a_conversation_is_resumed_by_it() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();

        let open = chats.put_back(
            &Record {
                chats: vec![chat(&a_claude(dir.path()), "ide.7", Some(ID))],
            },
            SIZE,
        );

        assert_eq!(open[0].how, Reopened::Resumed(SessionId::new(ID).unwrap()));
        let printed = until_printed(&chats, open[0].session, "argv:");
        assert!(
            printed.contains(&format!("--resume {ID} --name ide.7")),
            "claude was not asked to resume: {printed:?}"
        );
    }

    #[test]
    fn the_chat_that_was_in_front_is_the_one_the_window_is_told_to_show() {
        // The record holds which chat was in front; without this the window would put every
        // chat back and then show whichever one it happened to draw last.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back(
            &Record {
                chats: vec![
                    chat(&claude, "ide.7", None),
                    Chat {
                        active: true,
                        ..chat(&claude, "ide.8", None)
                    },
                ],
            },
            SIZE,
        );

        let in_front: Vec<&str> = open
            .iter()
            .filter(|one| one.in_front)
            .map(|one| one.name.as_str())
            .collect();
        assert_eq!(in_front, vec!["ide.8"]);
    }

    #[test]
    fn a_chat_put_back_with_no_conversation_says_so_and_starts_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();

        let open = chats.put_back(
            &Record {
                chats: vec![chat(&a_claude(dir.path()), "ide.7", None)],
            },
            SIZE,
        );

        assert_eq!(open[0].how, Reopened::Fresh(Fresh::NoConversationRecorded));
    }

    #[test]
    fn a_chat_whose_program_has_gone_is_left_out_and_the_others_still_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back(
            &Record {
                chats: vec![
                    chat("/definitely/not/a/program", "ide.7", Some(ID)),
                    chat(&claude, "ide.8", None),
                ],
            },
            SIZE,
        );

        assert_eq!(open.len(), 1);
        assert_eq!(open[0].name, "ide.8");
    }

    #[test]
    fn a_chat_put_back_is_one_the_next_quit_records_again() {
        // A relaunch that lost the record would resume once and never again.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        chats.put_back(
            &Record {
                chats: vec![chat(&a_claude(dir.path()), "ide.7", Some(ID))],
            },
            SIZE,
        );

        let again = chats.record();

        assert_eq!(again.chats.len(), 1);
        assert_eq!(again.chats[0].resume, Some(SessionId::new(ID).unwrap()));
    }

    #[test]
    fn ending_every_chat_leaves_nothing_to_record() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.end_all();

        assert_eq!(chats.record(), Record::default());
        assert_eq!(chats.sessions().running(), Vec::<u32>::new());
    }
}
