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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use charter_core::engine::Size;
use charter_core::harness::Harness;
use charter_core::reopen::{Chat, Record, Reopened};

use charter_core::harness::StateHooks;

use crate::sessions::{Opening, Reporting, Sessions};

/// One chat the app has open, as the UI and the quit warning see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    pub session: u32,
    pub name: String,
    pub cwd: Option<PathBuf>,
    pub harness: Option<Harness>,
    /// The harness profile it started on, and the persona it adopted — what the sidebar
    /// names a chat by, beside its harness.
    pub profile: Option<String>,
    pub persona: Option<String>,
    /// Whether it is the chat in front. At a launch this is the one that was in front when
    /// the app was quit, so the window comes back looking as it was left.
    pub in_front: bool,
    /// How it came to be open. A chat the operator just started is `Fresh`, the same as one
    /// that could not be resumed — the difference is only interesting at a relaunch, which
    /// is where the UI says it.
    pub how: Reopened,
}

/// The most chats one record may start at a launch. The product's scale is fifty (the
/// spec's limits table); this is only a backstop against a record nobody meant.
const MOST_AT_ONCE: usize = 200;

/// Where the record goes whenever what is open changes.
///
/// It is a callback rather than a file so that this module keeps knowing nothing about the
/// plane — and so a test can see exactly when a write would happen.
pub type Recorder = Box<dyn Fn(&Record) + Send + Sync>;

/// Told as each chat starts, BEFORE its program does: its number, the harness it runs and the
/// conversation charter chose for it. Everything the board needs to judge a report about it.
pub type Starting = Box<dyn Fn(u32, Option<Harness>, Option<String>) + Send + Sync>;

/// One chat the app has open: what it was started as, how it came back, and the harness it
/// actually runs.
///
/// The harness is KEPT rather than asked of the chat again, because asking means asking its
/// program's NAME, and a profile's command is commonly a wrapper. The board is told this
/// same value at the start, so the sidebar and the board cannot disagree about what a chat
/// is running — which they did: the board learned the profile's declared kind while the
/// sidebar read `claude-stand-in` and said "no harness".
#[derive(Debug)]
struct Running {
    chat: Chat,
    how: Reopened,
    harness: Option<Harness>,
}

/// Every chat the app has open, and which of them is in front.
pub struct Chats {
    sessions: Sessions,
    /// Told as each chat starts, before its program does.
    starting: Mutex<Option<Starting>>,
    /// Told when a chat that was announced turned out not to start.
    #[allow(clippy::type_complexity)]
    never_started: Mutex<Option<Box<dyn Fn(u32) + Send + Sync>>>,
    /// The `charter` binary a hook runs, when the app knows where its own is.
    binary: Option<PathBuf>,
    open: Mutex<HashMap<u32, Running>>,
    front: Mutex<Option<u32>>,
    /// Chats a launch could not start, and why. They are kept because the record has to
    /// keep them: a workspace directory that has moved, or a harness mid-reinstall, must
    /// not silently delete the chat on the next write.
    would_not_start: Mutex<Vec<(Chat, String)>>,
    record_it: Recorder,
    /// Held across building a record and handing it over, so two changes at once cannot
    /// write themselves out of order and leave the older one on disk.
    writing: Mutex<()>,
    /// Set while a record is being put back, so reading one does not write it again once
    /// for every chat in it — fifty chats would be fifty writes of the same file, at the
    /// one moment the app is being measured for cold start.
    putting_back: AtomicBool,
}

impl Chats {
    /// Chats whose record is written by `record_it` every time what is open changes.
    ///
    /// Quitting writes it too, but only a graceful quit reaches that: an app that is killed,
    /// or crashes, runs no exit handler. Writing as it goes means such an app comes back on
    /// the chats it had rather than on none.
    pub fn recorded_by(record_it: Recorder) -> Self {
        Self::recorded_by_reporting_to(record_it, None)
    }

    /// The same, with sessions that report what their harness does to `reporting`'s socket.
    pub fn recorded_by_reporting_to(record_it: Recorder, reporting: Option<Reporting>) -> Self {
        Self {
            sessions: Sessions::reporting_to(reporting),
            starting: Mutex::new(None),
            never_started: Mutex::new(None),
            binary: None,
            open: Mutex::new(HashMap::new()),
            front: Mutex::new(None),
            would_not_start: Mutex::new(Vec::new()),
            record_it,
            writing: Mutex::new(()),
            putting_back: AtomicBool::new(false),
        }
    }

    /// Chats nothing records — what the tests use when the record is not what they are about.
    pub fn new() -> Self {
        Self::recorded_by(Box::new(|_| {}))
    }

    /// Calls `tell` as each chat starts, BEFORE its program does, with everything the board
    /// needs in order to judge a report about it.
    pub fn when_one_starts(&self, tell: Starting) {
        *lock(&self.starting) = Some(tell);
    }

    /// Calls `tell` when a chat that was announced never started after all.
    ///
    /// The announcement has to come before the program, so a program that then fails to start
    /// leaves the board holding a chat that does not exist. Nothing is misattributed — ids are
    /// never reused — but it is one entry per failed start for the life of the app.
    pub fn when_one_does_not_start(&self, tell: Box<dyn Fn(u32) + Send + Sync>) {
        *lock(&self.never_started) = Some(tell);
    }

    /// The sessions underneath, for everything that is about a terminal and not about a chat.
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// Where the `charter` binary a hook runs is, when the app knows.
    ///
    /// Its own executable, because the app and the binary ship together: the one on `PATH`
    /// may be an older install, or the Python charter, and a hook pointed at either would be
    /// answering a different program's idea of these events.
    pub fn arming_with(&mut self, binary: Option<PathBuf>) {
        self.binary = binary;
    }

    /// The arguments that arm this harness's state hooks on this session alone, if any.
    fn state_hook_args(&self, harness: Option<Harness>) -> Vec<String> {
        let (Some(harness), Some(binary)) = (harness, self.binary.as_ref()) else {
            return Vec::new();
        };
        match harness.state_hooks(binary) {
            StateHooks::ThisSessionOnly { args, .. } => args,
            // Nothing is added to the command line, and nothing of the operator's is written
            // behind their back. The chat shows `unknown`.
            StateHooks::None => Vec::new(),
        }
    }

    /// Starts a chat the core has already worked out the launch for — a chat on a profile.
    ///
    /// The harness, the arguments and the environment all come from `ready`, which resolved
    /// them from the profile's DECLARED kind. Nothing here asks the program's name what it
    /// is: a profile's command is commonly a wrapper, and the answer would be `None`.
    pub fn start_ready(
        &self,
        chat: &Chat,
        ready: &charter_core::start::Ready,
        size: Size,
    ) -> Result<u32, String> {
        self.open_it(
            chat,
            ready.program.clone(),
            ready.args.clone(),
            ready.env.clone(),
            ready.harness,
            ready.session.as_ref().map(ToString::to_string),
            ready.how.clone(),
            size,
        )
    }

    /// [`Self::put_back`] against a plane the test does not care about — every chat in
    /// these records is a shell, which is resolved from the record alone.
    #[cfg(test)]
    fn put_back_here(&self, record: &Record, size: Size) -> Vec<Open> {
        self.put_back(record, std::path::Path::new("/nonexistent-plane"), size)
    }

    /// Starts one chat out of the record, on its own profile where it had one.
    ///
    /// **The profile is looked up again**, never taken from the record: an edit to it takes
    /// effect at this launch rather than a stale copy running, and a profile that is gone
    /// means this chat is skipped BY NAME — another profile may be another account, where
    /// this chat's resume id does not exist and where its workspace's code was never meant
    /// to go. It stays in the record, so declaring the profile again brings it back.
    fn start_recorded(
        &self,
        chat: &Chat,
        root: &std::path::Path,
        size: Size,
    ) -> Result<u32, String> {
        let Some(profile) = chat.profile.clone() else {
            return self.start(chat, size);
        };
        let ready = charter_core::start::ready(
            &charter_core::start::Start {
                profile: Some(profile),
                persona: chat.persona.clone(),
                name: chat.name.clone(),
                cwd: chat.cwd.clone(),
                resume: chat.resume.clone(),
                // The chat's own footer choice, brought back with it. It rides on the
                // environment, which is rebuilt at every start, so a relaunch that did not
                // carry it would silently blank a footer the operator had turned on.
                show_footer: chat.show_footer,
            },
            root,
        )?;
        self.start_ready(chat, &ready, size)
    }

    /// Starts a chat, and remembers what it was started as.
    ///
    /// For a chat that is NOT on a profile — the operator's shell — where what runs is
    /// decided from the record alone.
    pub fn start(&self, chat: &Chat, size: Size) -> Result<u32, String> {
        let launch = chat.launch();
        self.open_it(
            chat,
            launch.program,
            launch.args,
            Vec::new(),
            chat.harness(),
            launch.session.as_ref().map(ToString::to_string),
            launch.how,
            size,
        )
    }

    /// The one place a session is opened and a chat is remembered.
    ///
    /// Everything that differs between a profile chat and a shell chat is decided by the
    /// caller and arrives here as arguments — above all the HARNESS, which for a profile
    /// comes from its declared kind and must not be asked of the program's name.
    #[allow(clippy::too_many_arguments)]
    fn open_it(
        &self,
        chat: &Chat,
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        harness: Option<Harness>,
        conversation: Option<String>,
        how: charter_core::reopen::Reopened,
        size: Size,
    ) -> Result<u32, String> {
        // Charter's own words first, and the state hooks before even those: a harness reads
        // its settings before it reads anything else on the line, and a chat's own recorded
        // arguments may end in a positional prompt that nothing may come after.
        let mut all = self.state_hook_args(harness);
        all.extend(args);
        // What the announcement below said, so a start that fails can take it back.
        let announced = std::sync::atomic::AtomicU32::new(0);
        let session = self
            .sessions
            .open(
                &Opening {
                    program: Some(program),
                    args: all,
                    cwd: chat.cwd.as_ref().map(|cwd| cwd.display().to_string()),
                    size,
                    env,
                },
                &|session| {
                    announced.store(session, std::sync::atomic::Ordering::SeqCst);
                    if let Some(starting) = lock(&self.starting).as_ref() {
                        starting(session, harness, conversation.clone());
                    }
                },
            )
            .inspect_err(|_| {
                // The chat was announced and then did not start. Take it back, or the board
                // holds one entry per failed start for the life of the app.
                let announced = announced.load(std::sync::atomic::Ordering::SeqCst);
                if announced > 0
                    && let Some(gone) = lock(&self.never_started).as_ref()
                {
                    gone(announced);
                }
            })?;
        // Under the id it was actually given, not the one it was recorded with: a chat
        // started fresh is under an id the app just chose, and that is what has to be
        // written down for the next launch to resume it.
        let under = Chat {
            resume: conversation
                .as_deref()
                .and_then(|id| charter_core::harness::SessionId::new(id).ok()),
            ..chat.clone()
        };
        lock(&self.open).insert(
            session,
            Running {
                chat: under,
                how,
                harness,
            },
        );
        self.write_it_down();
        Ok(session)
    }

    /// Ends a chat. It is no longer one a quit would record.
    pub fn close(&self, session: u32) -> Result<(), String> {
        lock(&self.open).remove(&session);
        let mut front = lock(&self.front);
        if *front == Some(session) {
            *front = None;
        }
        drop(front);
        let closed = self.sessions.close(session);
        self.write_it_down();
        closed
    }

    /// Which chat is in front, or none.
    pub fn front(&self) -> Option<u32> {
        *lock(&self.front)
    }

    /// Says which chat is in front, so the record knows which one to bring back in front.
    pub fn bring_to_front(&self, session: Option<u32>) {
        let changed = std::mem::replace(&mut *lock(&self.front), session) != session;
        if changed {
            self.write_it_down();
        }
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
                let running = open.get(&session)?;
                let chat = &running.chat;
                Some(Open {
                    session,
                    name: chat.name.clone(),
                    cwd: chat.cwd.clone(),
                    // The harness this chat was STARTED as, not one inferred from its
                    // program's name — a profile's command is commonly a wrapper, and the
                    // sidebar used to answer "no harness" for one while the board knew the
                    // kind. One idea of what is running, or the two drift.
                    harness: running.harness,
                    profile: chat.profile.clone(),
                    persona: chat.persona.clone(),
                    in_front: front == Some(session),
                    how: running.how.clone(),
                })
            })
            .collect()
    }

    /// What was open, to write down.
    pub fn record(&self) -> Record {
        let open = lock(&self.open);
        let front = *lock(&self.front);
        // The ones that could not be started come first, in the order they were recorded,
        // so they keep their place and are tried again at the next launch.
        let mut chats: Vec<Chat> = lock(&self.would_not_start)
            .iter()
            .map(|(chat, _)| chat.clone())
            .collect();
        chats.extend(self.sessions.running().into_iter().filter_map(|session| {
            let chat = &open.get(&session)?.chat;
            Some(Chat {
                active: front == Some(session),
                ..chat.clone()
            })
        }));
        Record { chats }
    }

    /// Puts a record back: one session per chat it holds, resumed where it can be.
    ///
    /// A chat whose program cannot be started is left out and the rest still open — a
    /// relaunch that failed whole because one harness had been uninstalled would be worse
    /// than one that came back short.
    pub fn put_back(&self, record: &Record, root: &std::path::Path, size: Size) -> Vec<Open> {
        self.putting_back.store(true, Ordering::SeqCst);
        // Every chat here starts a program, synchronously, before there is a window. A
        // record with thousands in it — a runaway, or a file nobody meant — would give an
        // app that hangs on launch with no way to intervene. The cap is far above the
        // fifty the product is for, so it never meets an operator; it is only ever a
        // backstop. What it leaves out stays recorded, like anything else that did not
        // start.
        let (starting, too_many) = record.chats.split_at(record.chats.len().min(MOST_AT_ONCE));
        for chat in too_many {
            lock(&self.would_not_start).push((
                chat.clone(),
                format!("more than {MOST_AT_ONCE} chats were recorded"),
            ));
        }
        let mut front = None;
        let mut opened: Vec<u32> = Vec::new();
        for chat in starting {
            match self.start_recorded(chat, root, size) {
                Ok(session) => {
                    if chat.active {
                        front = Some(session);
                    }
                    opened.push(session);
                }
                // Kept, not dropped: the next record has to hold it too, or a directory
                // that has moved deletes the chat for good.
                Err(why) => lock(&self.would_not_start).push((chat.clone(), why)),
            }
        }
        self.bring_to_front(front);
        // Nothing is written here. What is on disk is the record that was just read, which
        // is still true — and writing what came back would be writing the chats that did
        // not, out of it.
        self.putting_back.store(false, Ordering::SeqCst);
        let open = self.open_now();
        open.into_iter()
            .filter(|one| opened.contains(&one.session))
            .collect()
    }

    /// Hands the record as it now is to whoever writes it.
    fn write_it_down(&self) {
        if self.putting_back.load(Ordering::SeqCst) {
            return;
        }
        // The record is built and handed over under one lock, so that two changes landing
        // together cannot write themselves out of order and leave the older one on disk.
        let _writing = lock(&self.writing);
        (self.record_it)(&self.record());
    }

    /// The chats a launch could not start, by name and reason. The window says so.
    pub fn would_not_start(&self) -> Vec<(String, String)> {
        lock(&self.would_not_start)
            .iter()
            .map(|(chat, why)| (chat.name.clone(), why.clone()))
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
        lock(&self.would_not_start).clear();
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

    #[test]
    fn a_chat_is_announced_before_its_program_starts() {
        // A harness fires `SessionStart` at its own exec, so anything that learned the chat's
        // number afterwards would miss it — and for a chat that is then idle, waiting for a
        // first prompt, no second event ever comes. That is every chat of a relaunch.
        //
        // The order is the whole point: the announcement must land before the program can
        // have run at all.
        use std::sync::mpsc;

        let chats = Chats::new();
        let (tx, rx) = mpsc::channel();
        chats.when_one_starts(Box::new(move |session, harness, conversation| {
            let _ = tx.send((session, harness, conversation));
        }));

        let session = chats
            .start(
                &Chat {
                    // A program that prints and stops at once: by the time `start` returns it
                    // may already be gone, so an announcement made afterwards could be too
                    // late even in this test.
                    program: "/bin/echo".to_owned(),
                    args: vec!["hello".to_owned()],
                    cwd: None,
                    name: "ide.7".to_owned(),
                    resume: None,
                    active: false,
                    profile: None,
                    persona: None,
                    show_footer: false,
                },
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .expect("the chat starts");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok((session, None, None)),
            "the board was not told about the chat"
        );
    }

    #[test]
    fn a_chat_is_announced_before_its_program_could_have_run_a_single_byte() {
        // **The ORDER is the fix, and a surviving mutant proved the first test does not check
        // it**: moving the announcement after the spawn still delivers it, so the defect
        // could come back silently. Checking what the app had bookkept was no better — that
        // happens after the spawn either way.
        //
        // So the announcement WAITS, briefly, for something only a running program could
        // make. A program that has not been started cannot make it however long we wait; one
        // that has makes it in milliseconds. The wait is what turns an ordering into
        // something a test can see.
        use std::sync::{Arc, Mutex};

        let dir = tempfile::tempdir().expect("a directory");
        let mark = dir.path().join("the-program-ran");
        let seen = Arc::new(Mutex::new(None));

        let chats = Chats::new();
        chats.when_one_starts({
            let mark = mark.clone();
            let seen = Arc::clone(&seen);
            Box::new(move |_, _, _| {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(750);
                while std::time::Instant::now() < deadline && !mark.exists() {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                *seen.lock().expect("not poisoned") = Some(mark.exists());
            })
        });

        chats
            .start(
                &Chat {
                    program: "/bin/sh".to_owned(),
                    args: vec![
                        "-c".to_owned(),
                        format!("touch {}; sleep 30", mark.display()),
                    ],
                    cwd: None,
                    name: "ide.7".to_owned(),
                    resume: None,
                    active: false,
                    profile: None,
                    persona: None,
                    show_footer: false,
                },
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .expect("the chat starts");

        assert_eq!(
            *seen.lock().expect("not poisoned"),
            Some(false),
            "the program had already run when the board was told about its chat"
        );
    }

    #[test]
    fn a_chat_that_was_announced_and_then_did_not_start_is_taken_back() {
        // The announcement must come before the program, so it can be about a chat that never
        // happens. Without taking it back, the board holds one entry per failed start for the
        // life of the app.
        use std::sync::{Arc, Mutex};

        let chats = Chats::new();
        let announced = Arc::new(Mutex::new(Vec::new()));
        let taken_back = Arc::new(Mutex::new(Vec::new()));
        chats.when_one_starts({
            let announced = Arc::clone(&announced);
            Box::new(move |session, _, _| announced.lock().expect("not poisoned").push(session))
        });
        chats.when_one_does_not_start({
            let taken_back = Arc::clone(&taken_back);
            Box::new(move |session| taken_back.lock().expect("not poisoned").push(session))
        });

        let refused = chats.start(
            &Chat {
                program: "/no/such/program/anywhere".to_owned(),
                args: Vec::new(),
                cwd: None,
                name: "ide.7".to_owned(),
                resume: None,
                active: false,
                profile: None,
                persona: None,
                show_footer: false,
            },
            Size {
                columns: 80,
                rows: 24,
            },
        );

        assert!(refused.is_err(), "a program that is not there started");
        let announced = announced.lock().expect("not poisoned").clone();
        assert_eq!(announced.len(), 1, "it was never announced");
        assert_eq!(*taken_back.lock().expect("not poisoned"), announced);
    }

    #[test]
    fn a_chat_is_announced_with_the_harness_and_conversation_it_was_started_under() {
        // What the board needs in order to judge a report: which rulebook, and which
        // conversation charter chose. A `claude` nested in the chat's shell reports a
        // different one, and that is the whole of what keeps it out (ADR 0024, C5).
        use std::sync::mpsc;

        let chats = Chats::new();
        let (tx, rx) = mpsc::channel();
        chats.when_one_starts(Box::new(move |session, harness, conversation| {
            let _ = tx.send((session, harness, conversation));
        }));

        // `/bin/echo` named `claude` is what `Harness::of_command` reads, and it is the file
        // name that decides — so this is a Claude Code chat as far as the app is concerned.
        // Copied through `stand_in::copy_of`, not `fs::copy`: this chat runs it the moment it
        // is written, and a program this process copied through its own descriptor can lose
        // to `ETXTBSY` (charter-app#81).
        let dir = tempfile::tempdir().expect("a directory");
        let claude = stand_in::copy_of(std::path::Path::new("/bin/echo"), dir.path(), "claude");

        chats
            .start(
                &Chat {
                    program: claude.display().to_string(),
                    args: Vec::new(),
                    cwd: None,
                    name: "ide.7".to_owned(),
                    resume: None,
                    active: false,
                    profile: None,
                    persona: None,
                    show_footer: false,
                },
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .expect("the chat starts");

        let (_, harness, conversation) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the board was told");
        assert_eq!(harness, Some(Harness::ClaudeCode));
        // Charter chooses Claude Code's id at the start, so the board has it before the
        // harness has said anything.
        assert!(
            conversation.is_some(),
            "the chosen conversation was not passed on"
        );
    }
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
        // Through `stand_in::program`, which holds both halves of this. The rename is what
        // charter-app#39 needed: the stand-in ends in `sleep 600`, so an earlier chat still
        // has it open for execution, and writing a running program is ETXTBSY — measured at
        // 2 failures in 5 runs, and because this binary runs first, `cargo test` stopped and
        // every later test binary was SKIPPED. The write from a child is what charter-app#81
        // needed, in the other direction: a chat runs this the moment it is written.
        stand_in::program(
            dir,
            "claude",
            "#!/bin/sh\nprintf 'argv:'\nfor word in \"$@\"; do printf ' %s' \"$word\"; done\nprintf '\\n'\nsleep 600\n",
        )
        .display()
        .to_string()
    }

    fn chat(program: &str, name: &str, resume: Option<&str>) -> Chat {
        Chat {
            program: program.to_owned(),
            args: vec![],
            cwd: None,
            name: name.to_owned(),
            resume: resume.map(|id| SessionId::new(id).expect("a valid id in a test")),
            active: false,
            profile: None,
            persona: None,
            show_footer: false,
        }
    }

    /// Everything a session has printed, once it has printed `text`.
    /// Everything a session has printed, once `text` is among it — as a READER would see
    /// it, not as the terminal encoded it.
    ///
    /// Two things had to be got right here, and the second cost a CI round. **Wait for what
    /// you are about to assert**: the stand-in `claude` prints `argv:` as a write of its own
    /// and its arguments as later ones, so waiting for `argv:` returned before a single
    /// argument had arrived. And **match against the text, not the encoding**: what a view
    /// emits is a terminal's output — erase-to-end-of-line, carriage returns, a line wrapped
    /// at the pane's width — so a long argv is `--resume` then an escape then the rest, and
    /// no substring of the command line is present as contiguous bytes. Both spellings can
    /// report a failure that has not happened and miss one that has.
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
            let plain = as_a_reader_sees(&so_far);
            if plain.contains(text) {
                return plain;
            }
            assert!(
                Instant::now() < deadline,
                "{text:?} never arrived, only {plain:?} (raw: {so_far:?})"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Terminal output as the words on the screen: escape sequences dropped, and the breaks
    /// a terminal inserts — a wrap, a carriage return — read as the single space that was
    /// between the words before it laid them out.
    fn as_a_reader_sees(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                // CSI and the rest of the sequence: parameters, then one final byte.
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() || c == '~' {
                            break;
                        }
                    }
                }
                continue;
            }
            out.push(if c == '\r' || c == '\n' { ' ' } else { c });
        }
        // A wrap becomes one space, and so does a run of them, so a command line reads the
        // way it was written however the pane laid it out.
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Chats that write every record they make into `wrote`, newest last.
    fn recorded() -> (Chats, std::sync::Arc<Mutex<Vec<Record>>>) {
        let wrote = std::sync::Arc::new(Mutex::new(Vec::new()));
        let keep = std::sync::Arc::clone(&wrote);
        let chats = Chats::recorded_by(Box::new(move |record| lock(&keep).push(record.clone())));
        (chats, wrote)
    }

    #[test]
    fn opening_a_chat_writes_the_record_without_waiting_for_a_quit() {
        // An app that is killed, or crashes, runs no exit handler. Everything open would be
        // lost if the record were only written on the way out.
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();

        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(last.chats.len(), 1);
        assert_eq!(last.chats[0].name, "ide.7");
    }

    #[test]
    fn closing_a_chat_writes_the_record_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();
        let going = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.close(going).expect("it closes");

        assert_eq!(lock(&wrote).last().expect("a record").chats, vec![]);
    }

    #[test]
    fn bringing_another_chat_to_the_front_writes_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        chats.start(&chat(&claude, "ide.7", None), SIZE).unwrap();
        let front = chats.start(&chat(&claude, "ide.8", None), SIZE).unwrap();

        chats.bring_to_front(Some(front));

        let last = lock(&wrote).last().cloned().expect("a record");
        let active: Vec<&str> = last
            .chats
            .iter()
            .filter(|c| c.active)
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(active, vec!["ide.8"]);
    }

    #[test]
    fn bringing_the_same_chat_to_the_front_again_writes_nothing() {
        // Every click on the tab already in front would otherwise be a write.
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();
        let only = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();
        chats.bring_to_front(Some(only));
        let so_far = lock(&wrote).len();

        chats.bring_to_front(Some(only));

        assert_eq!(lock(&wrote).len(), so_far);
    }

    #[test]
    fn putting_a_record_back_does_not_write_the_record() {
        // Fifty chats coming back must not be fifty writes at the one moment cold start is
        // measured — and not one write either: what is on disk is the record just read,
        // which is still true, and rewriting it would write out the chats that did not
        // come back.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();

        chats.put_back_here(
            &Record {
                chats: (0..5)
                    .map(|n| chat(&claude, &format!("ide.{n}"), None))
                    .collect(),
            },
            SIZE,
        );

        let written = lock(&wrote).clone();
        assert_eq!(
            written.len(),
            0,
            "putting a record back wrote it {} times; what is on disk is the record that \
             was just read, which is still true — and rewriting it would write out the \
             chats that did not come back",
            written.len()
        );
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

        // The id charter chose is known before the harness has finished printing it, so the
        // wait is for that exact id rather than for the line it will appear on.
        let recorded = chats.record().chats[0]
            .resume
            .clone()
            .expect("the chat has a conversation");
        let printed = until_printed(&chats, session, recorded.as_str());

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
            profile: None,
            persona: None,
            ..chat(&claude, "ide.8", None)
        };

        chats.put_back_here(
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

        let open = chats.put_back_here(
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

        let open = chats.put_back_here(
            &Record {
                chats: vec![chat(&a_claude(dir.path()), "ide.7", Some(ID))],
            },
            SIZE,
        );

        assert_eq!(open[0].how, Reopened::Resumed(SessionId::new(ID).unwrap()));
        let want = format!("--resume {ID} --name ide.7");
        let printed = until_printed(&chats, open[0].session, &want);
        assert!(
            printed.contains(&want),
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

        let open = chats.put_back_here(
            &Record {
                chats: vec![
                    chat(&claude, "ide.7", None),
                    Chat {
                        active: true,
                        profile: None,
                        persona: None,
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

        let open = chats.put_back_here(
            &Record {
                chats: vec![chat(&a_claude(dir.path()), "ide.7", None)],
            },
            SIZE,
        );

        assert_eq!(open[0].how, Reopened::Fresh(Fresh::NoConversationRecorded));
    }

    #[test]
    fn a_chat_that_could_not_be_started_stays_in_the_record_for_the_next_launch() {
        // Otherwise a workspace directory that is moved, or a harness that is being
        // reinstalled, silently deletes the chat: it fails to start once, the record is
        // written without it, and by the launch after that there is no trace it existed.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();

        chats.put_back_here(
            &Record {
                chats: vec![
                    chat("/definitely/not/a/program", "ide.7", Some(ID)),
                    chat(&claude, "ide.8", None),
                ],
            },
            SIZE,
        );

        let names: Vec<String> = chats
            .record()
            .chats
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(names, vec!["ide.7", "ide.8"]);
        assert!(lock(&wrote).is_empty(), "putting a record back rewrote it");
    }

    #[test]
    fn a_record_cannot_ask_a_launch_to_start_an_unbounded_number_of_programs() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back_here(
            &Record {
                chats: (0..MOST_AT_ONCE + 3)
                    .map(|n| chat(&claude, &format!("ide.{n}"), None))
                    .collect(),
            },
            SIZE,
        );

        assert_eq!(open.len(), MOST_AT_ONCE);
        // And the ones it would not start are kept, not thrown away.
        assert_eq!(chats.would_not_start().len(), 3);
        chats.end_all();
    }

    #[test]
    fn a_chat_that_could_not_be_started_is_named_with_the_reason() {
        // The operator is told, rather than finding a tab quietly missing. Nothing here
        // starts, so there is no stand-in harness to put anywhere.
        let (chats, _) = recorded();

        chats.put_back_here(
            &Record {
                chats: vec![chat("/definitely/not/a/program", "ide.7", Some(ID))],
            },
            SIZE,
        );

        let trouble = chats.would_not_start();
        assert_eq!(trouble.len(), 1);
        assert_eq!(trouble[0].0, "ide.7");
        assert!(!trouble[0].1.is_empty(), "no reason was kept");
    }

    #[test]
    fn a_chat_that_could_not_be_started_is_still_recorded_after_a_later_change() {
        // The record is written again as soon as anything changes; the chat that could not
        // start has to survive that write too, not just the launch.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        chats.put_back_here(
            &Record {
                chats: vec![chat("/definitely/not/a/program", "ide.7", Some(ID))],
            },
            SIZE,
        );

        chats.start(&chat(&claude, "ide.9", None), SIZE).unwrap();

        let last = lock(&wrote).last().cloned().expect("a record was written");
        let names: Vec<String> = last.chats.iter().map(|c| c.name.clone()).collect();
        assert_eq!(names, vec!["ide.7", "ide.9"]);
    }

    #[test]
    fn a_chat_whose_program_has_gone_is_left_out_and_the_others_still_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back_here(
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
        chats.put_back_here(
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
    #[test]
    fn the_record_keeps_the_profile_persona_and_footer_a_chat_was_started_on() {
        // `record()` rebuilds each chat with `..chat.clone()`, so these ride along — which
        // means nothing says so when they stop. A struct literal that names one field and
        // spreads the rest is exactly where a later edit drops one silently, and an edit
        // that added `profile: None` beside the spread would do it: the record would still
        // be written, still be read, and every chat would come back as a shell.
        //
        // The footer choice (charter ADR 0029) rides the same spread and fails the same way:
        // it would be dropped at the quit and the chat would come back blanked.
        let chats = Chats::new();
        let chat = Chat {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 30".to_owned()],
            cwd: None,
            name: "ide.7".to_owned(),
            resume: None,
            active: false,
            profile: Some("claude-work".to_owned()),
            persona: Some("steward".to_owned()),
            show_footer: true,
        };

        let session = chats
            .start(
                &chat,
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .unwrap();
        let record = chats.record();

        assert_eq!(record.chats.len(), 1);
        assert_eq!(record.chats[0].profile.as_deref(), Some("claude-work"));
        assert_eq!(record.chats[0].persona.as_deref(), Some("steward"));
        assert!(record.chats[0].show_footer);
        let _ = chats.close(session);
    }
    #[test]
    fn the_sidebar_is_told_the_harness_a_chat_was_started_as_not_one_read_off_its_program() {
        // A profile's command is commonly a WRAPPER (ADR 0022), and `Harness::of_command`
        // answers `None` for one — the same answer it gives a shell, deliberately. The board
        // is told the profile's declared kind at the start; the sidebar used to ask the
        // program's name instead and say "no harness" for the very same chat. A scenario
        // test caught the disagreement; this is what keeps them one answer.
        let chats = Chats::new();
        let chat = Chat {
            program: "/bin/sh".to_owned(),
            args: Vec::new(),
            cwd: None,
            name: "ide.7".to_owned(),
            resume: None,
            active: false,
            profile: Some("claude-work".to_owned()),
            persona: None,
            show_footer: false,
        };
        assert_eq!(
            chat.harness(),
            None,
            "the premise: the program is not a harness"
        );
        let ready = charter_core::start::Ready {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 30".to_owned()],
            env: Vec::new(),
            cwd: None,
            harness: Some(Harness::ClaudeCode),
            session: None,
            how: charter_core::reopen::Reopened::Fresh(
                charter_core::reopen::Fresh::NoConversationRecorded,
            ),
            wired: String::new(),
        };

        let session = chats
            .start_ready(
                &chat,
                &ready,
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .unwrap();

        let open = chats.open_now();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].harness, Some(Harness::ClaudeCode));
        assert_eq!(open[0].profile.as_deref(), Some("claude-work"));
        let _ = chats.close(session);
    }
    #[test]
    fn the_board_is_told_the_harness_the_profile_declared_before_the_program_starts() {
        // The board judges every report against the harness it was told at the start, and a
        // chat on a wrapper profile would otherwise be told `None` — the narrowest rule
        // there is — while the sidebar said Claude Code. One announcement, one answer.
        //
        // Before the program starts, because a harness fires `SessionStart` at its own exec
        // and a board that learned the chat's number afterwards would miss it.
        let told = std::sync::Arc::new(Mutex::new(
            Vec::<(u32, Option<Harness>, Option<String>)>::new(),
        ));
        let chats = Chats::new();
        {
            let told = std::sync::Arc::clone(&told);
            chats.when_one_starts(Box::new(move |session, harness, conversation| {
                lock(&told).push((session, harness, conversation));
            }));
        }
        let chat = Chat {
            program: "/bin/sh".to_owned(),
            args: Vec::new(),
            cwd: None,
            name: "ide.7".to_owned(),
            resume: None,
            active: false,
            profile: Some("claude-work".to_owned()),
            persona: Some("steward".to_owned()),
            show_footer: false,
        };
        assert_eq!(
            chat.harness(),
            None,
            "the premise: the program is not a harness"
        );
        let ready = charter_core::start::Ready {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 30".to_owned()],
            env: Vec::new(),
            cwd: None,
            harness: Some(Harness::ClaudeCode),
            session: charter_core::harness::SessionId::new(ID).ok(),
            how: charter_core::reopen::Reopened::Fresh(
                charter_core::reopen::Fresh::NoConversationRecorded,
            ),
            wired: String::new(),
        };

        let session = chats.start_ready(&chat, &ready, SIZE).unwrap();

        let told = lock(&told).clone();
        assert_eq!(
            told,
            vec![(session, Some(Harness::ClaudeCode), Some(ID.to_owned()))],
            "the board was told something other than the profile's declared kind"
        );
        let _ = chats.close(session);
    }
}
