//! The sessions the app is running, and the views the UI has open on them.
//!
//! The core owns each session's terminal, whether or not a pane is showing it. A pane that
//! comes on screen opens a view, which is sent the screen as it is and then the session's
//! output; a pane that goes away closes its view, and the session keeps running.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use charter_core::engine::{AlacrittyEngine, Size};
use charter_core::session::{Attachment, Session, Spec};

/// How many lines of history each session keeps. The pane showing it is told, so its own
/// terminal keeps the same.
pub const SCROLLBACK: u32 = 5_000;

/// What a pane is sent once its view has closed, so a screen that will never change again
/// does not look like one that might. A pane whose own view closed writes it and is gone.
const ENDED: &str = "\r\n\x1b[2m— the program has ended —\x1b[0m\r\n";

/// What to run in a new session. No program is the operator's shell.
#[derive(Debug, Clone)]
pub struct Opening {
    pub program: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub size: Size,
}

/// Where a view's text goes. It is called on the view's own thread, one batch at a time.
pub type Sink = Box<dyn FnMut(String) + Send>;

/// A view the UI has open, and the size the screen it opened on was drawn for.
#[derive(Debug, Clone, Copy)]
pub struct Watching {
    pub view: u32,
    pub size: Size,
}

/// Every session the app is running, by the id the UI calls it by.
pub struct Sessions {
    running: Mutex<HashMap<u32, Running>>,
    opened: AtomicU32,
}

struct Running {
    session: Session,
    views: HashMap<u32, Watcher>,
    watched: u32,
}

/// A view of a session that a thread of its own is reading. Dropping this closes the view in
/// the core at once, which ends that thread: its queue ends with the view.
type Watcher = Attachment;

impl Sessions {
    pub fn new() -> Self {
        Self {
            running: Mutex::new(HashMap::new()),
            opened: AtomicU32::new(0),
        }
    }

    /// Starts a session, and answers with the id it is called by from now on.
    pub fn open(&self, opening: &Opening) -> Result<u32, String> {
        let program = opening.program.clone().unwrap_or_else(shell);
        let mut spec = Spec::new(program, opening.size).args(&opening.args);
        spec.cwd = opening.cwd.as_ref().map(Into::into);
        let engine = AlacrittyEngine::new(opening.size, SCROLLBACK as usize);
        let session = Session::spawn(spec, Box::new(engine)).map_err(|err| err.to_string())?;

        let id = self.opened.fetch_add(1, Ordering::Relaxed) + 1;
        lock(&self.running).insert(
            id,
            Running {
                session,
                views: HashMap::new(),
                watched: 0,
            },
        );
        Ok(id)
    }

    /// Ends a session and everything it started. Its views end with it.
    pub fn close(&self, id: u32) -> Result<(), String> {
        lock(&self.running)
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| gone(id))
    }

    /// Sends what a pane typed to the program.
    pub fn input(&self, id: u32, text: &str) -> Result<(), String> {
        self.with(id, |running| {
            running
                .session
                .write(text.as_bytes())
                .map_err(|err| err.to_string())
        })
    }

    pub fn resize(&self, id: u32, size: Size) -> Result<(), String> {
        self.with(id, |running| {
            running.session.resize(size).map_err(|err| err.to_string())
        })
    }

    /// Opens a view of a session: `sink` is sent the screen as it already is and then the
    /// session's output, as text, until the view is closed. The answer says which view that
    /// is, and the size its screen was drawn for.
    pub fn watch(&self, id: u32, mut sink: Sink) -> Result<Watching, String> {
        self.with(id, |running| {
            let (open, size, output) = running.session.attach().into_parts();
            // A thread of its own reads the view, so nothing the UI does runs on the
            // session's reading thread, and a slow pane holds up only itself. The queue ends
            // when the view is closed or the program ends, and the thread ends with it.
            std::thread::Builder::new()
                .name("charter-view".into())
                .spawn(move || {
                    let mut text = Utf8Stream::default();
                    for bytes in output {
                        sink(text.push(&bytes));
                    }
                    sink(ENDED.to_owned());
                })
                .map_err(|err| format!("cannot read the session: {err}"))?;
            running.watched += 1;
            let id = running.watched;
            running.views.insert(id, open);
            Ok(Watching { view: id, size })
        })
    }

    /// Closes a view. Its session keeps running, with its terminal, for the next pane.
    pub fn unwatch(&self, id: u32, view: u32) -> Result<(), String> {
        self.with(id, |running| {
            running
                .views
                .remove(&view)
                .map(|_| ())
                .ok_or_else(|| format!("session {id} has no view {view}"))
        })
    }

    /// Ends every session, and does not return until their programs are gone. This is what
    /// quitting calls: the process is about to end, and a thread would not be waited for.
    pub fn end_all(&self) {
        let ending: Vec<Running> = lock(&self.running).drain().map(|(_, one)| one).collect();
        // All at once: fifty programs each given the same moment to leave on their own.
        let ends: Vec<_> = ending
            .into_iter()
            .filter_map(|one| {
                std::thread::Builder::new()
                    .name("charter-ending".into())
                    .spawn(move || one.session.end())
                    .ok()
            })
            .collect();
        for end in ends {
            let _ = end.join();
        }
    }

    /// The operating system's id for a session's program, while it runs. Only the tests ask;
    /// the UI has no use for it yet.
    #[cfg(test)]
    pub fn process_id(&self, id: u32) -> Option<u32> {
        self.with(id, |running| Ok(running.session.process_id()))
            .ok()
            .flatten()
    }

    /// The sessions that are running, in the order they were opened.
    pub fn running(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = lock(&self.running).keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    fn with<T>(
        &self,
        id: u32,
        act: impl FnOnce(&mut Running) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut running = lock(&self.running);
        act(running.get_mut(&id).ok_or_else(|| gone(id))?)
    }
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new()
    }
}

fn gone(id: u32) -> String {
    format!("session {id} is not running")
}

/// The operator's shell, or a plain one where the environment does not name it.
fn shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned())
}

/// Bytes turned into text, holding back the start of a character split across two reads for
/// the read that finishes it. Bytes that are no character at all become `U+FFFD`, which is
/// what a terminal shows for them.
///
/// This is not byte-for-byte: a program that writes eight-bit control codes (`0x80`–`0x9f`,
/// the seven-bit escape sequences in one byte) has them shown as replacements by the pane,
/// where the core's own terminal reads them as the controls they are. No harness is known to
/// write them; the alternative is sending every chunk as an array of numbers over IPC.
#[derive(Default)]
struct Utf8Stream {
    waiting: Vec<u8>,
}

impl Utf8Stream {
    fn push(&mut self, bytes: &[u8]) -> String {
        let mut rest = std::mem::take(&mut self.waiting);
        rest.extend_from_slice(bytes);
        let mut text = String::new();
        let mut from = 0;
        loop {
            match std::str::from_utf8(&rest[from..]) {
                Ok(whole) => {
                    text.push_str(whole);
                    return text;
                }
                Err(err) => {
                    let good = from + err.valid_up_to();
                    text.push_str(std::str::from_utf8(&rest[from..good]).unwrap_or_default());
                    match err.error_len() {
                        // No character at all: one replacement, as a terminal shows it.
                        Some(wrong) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            from = good + wrong;
                        }
                        // The start of one: it waits for the read that finishes it.
                        None => {
                            self.waiting = rest[good..].to_vec();
                            return text;
                        }
                    }
                }
            }
        }
    }
}

/// A poisoned lock only means another thread panicked while holding it; what is inside is
/// still the sessions that are running.
fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;

    const SIZE: Size = Size {
        columns: 40,
        rows: 10,
    };
    const PATIENCE: Duration = Duration::from_secs(10);

    fn opening(script: &str) -> Opening {
        Opening {
            program: Some("/bin/sh".to_owned()),
            args: vec!["-c".to_owned(), script.to_owned()],
            cwd: None,
            size: SIZE,
        }
    }

    /// The text a view of `id` is sent, from now on.
    fn watching(sessions: &Sessions, id: u32) -> (u32, Arc<Mutex<String>>) {
        let seen = Arc::new(Mutex::new(String::new()));
        let collect = {
            let seen = Arc::clone(&seen);
            move |text: String| lock(&seen).push_str(&text)
        };
        let watching = sessions
            .watch(id, Box::new(collect))
            .expect("the view opens");
        (watching.view, seen)
    }

    fn until_seen(seen: &Mutex<String>, text: &str) {
        let deadline = Instant::now() + PATIENCE;
        while !lock(seen).contains(text) {
            assert!(
                Instant::now() < deadline,
                "{text:?} never arrived, only {:?}",
                lock(seen)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_view_is_sent_what_the_session_prints() {
        let sessions = Sessions::new();
        let id = sessions
            .open(&opening("printf 'output for the pane'; sleep 600"))
            .expect("the session opens");

        let (_view, seen) = watching(&sessions, id);

        until_seen(&seen, "output for the pane");
    }

    #[test]
    fn what_the_pane_types_reaches_the_program() {
        let sessions = Sessions::new();
        let id = sessions
            .open(&opening(
                "read line; printf 'you typed %s' \"$line\"; sleep 600",
            ))
            .expect("the session opens");
        let (_view, seen) = watching(&sessions, id);

        sessions.input(id, "hello\r").expect("the input is taken");

        until_seen(&seen, "you typed hello");
    }

    #[test]
    fn a_closed_view_is_sent_no_more_output() {
        let sessions = Sessions::new();
        let id = sessions
            .open(&opening(
                "read _; printf 'after the pane went away'; sleep 600",
            ))
            .expect("the session opens");
        let (view, seen) = watching(&sessions, id);

        sessions.unwatch(id, view).expect("the view closes");
        sessions.input(id, "\r").unwrap();
        std::thread::sleep(Duration::from_millis(200));

        assert!(
            !lock(&seen).contains("after the pane went away"),
            "the closed view was sent more output: {:?}",
            lock(&seen)
        );
    }

    #[test]
    fn a_pane_is_told_when_the_program_it_is_showing_has_ended() {
        let sessions = Sessions::new();
        let id = sessions
            .open(&opening("printf 'the last thing it printed'"))
            .expect("the session opens");

        let (_view, seen) = watching(&sessions, id);

        until_seen(&seen, "the last thing it printed");
        until_seen(&seen, "the program has ended");
    }

    #[test]
    fn ending_every_session_ends_their_programs_before_it_returns() {
        let sessions = Sessions::new();
        let programs: Vec<u32> = (0..5)
            .map(|_| {
                let id = sessions
                    .open(&opening(
                        "trap '' HUP; echo guarded; while :; do sleep 600; done",
                    ))
                    .expect("the session opens");
                let (_view, seen) = watching(&sessions, id);
                until_seen(&seen, "guarded");
                sessions
                    .process_id(id)
                    .expect("a running program has a pid")
            })
            .collect();

        sessions.end_all();

        for pid in programs {
            assert!(!alive(pid), "process {pid} outlived the app");
        }
        assert_eq!(sessions.running(), Vec::<u32>::new());
    }

    /// Whether a process is still there, as the operating system sees it.
    fn alive(pid: u32) -> bool {
        std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .map(|out| {
                out.status.success()
                    && !String::from_utf8_lossy(&out.stdout)
                        .trim_start()
                        .starts_with('Z')
            })
            .unwrap_or(false)
    }

    #[test]
    fn a_resize_reaches_the_program() {
        let sessions = Sessions::new();
        let id = sessions
            .open(&opening("read _; stty size; sleep 600"))
            .expect("the session opens");
        let (_view, seen) = watching(&sessions, id);

        sessions
            .resize(
                id,
                Size {
                    columns: 100,
                    rows: 30,
                },
            )
            .expect("the resize is taken");
        sessions.input(id, "\r").unwrap();

        until_seen(&seen, "30 100");
    }

    #[test]
    fn a_view_is_told_the_size_the_screen_it_opened_on_was_drawn_for() {
        let sessions = Sessions::new();
        let id = sessions.open(&opening("sleep 600")).expect("it opens");
        let bigger = Size {
            columns: 120,
            rows: 40,
        };
        sessions.resize(id, bigger).unwrap();

        let watching = sessions
            .watch(id, Box::new(|_| {}))
            .expect("the view opens");

        assert_eq!(watching.size, bigger);
    }

    #[test]
    fn a_closed_session_is_no_longer_running() {
        let sessions = Sessions::new();
        let id = sessions.open(&opening("sleep 600")).expect("it opens");
        assert_eq!(sessions.running(), vec![id]);

        sessions.close(id).expect("it closes");

        assert_eq!(sessions.running(), Vec::<u32>::new());
    }

    #[test]
    fn a_session_that_is_not_running_is_an_error_and_not_a_panic() {
        let sessions = Sessions::new();

        assert!(sessions.input(7, "hi").is_err());
        assert!(sessions.resize(7, SIZE).is_err());
        assert!(sessions.close(7).is_err());
        assert!(sessions.watch(7, Box::new(|_| {})).is_err());
        assert!(sessions.unwatch(7, 1).is_err());
    }

    #[test]
    fn a_program_that_cannot_be_started_is_an_error() {
        let sessions = Sessions::new();
        let mut opening = opening("true");
        opening.program = Some("/definitely/not/a/program".to_owned());

        assert!(sessions.open(&opening).is_err());
    }

    #[test]
    fn a_character_split_across_two_reads_arrives_whole() {
        let mut text = Utf8Stream::default();
        let wide = "中".as_bytes();

        let first = text.push(&wide[..1]);
        let second = text.push(&wide[1..]);

        assert_eq!(first, "");
        assert_eq!(second, "中");
    }

    #[test]
    fn text_around_a_split_character_is_not_held_back() {
        let mut text = Utf8Stream::default();
        let mut bytes = b"before ".to_vec();
        bytes.extend_from_slice(&"中".as_bytes()[..2]);

        assert_eq!(text.push(&bytes), "before ");
        assert_eq!(text.push(&"中".as_bytes()[2..]), "中");
    }

    #[test]
    fn bytes_that_are_not_a_character_are_shown_as_one_replacement_each() {
        let mut text = Utf8Stream::default();

        assert_eq!(text.push(b"a\xffb\xfec"), "a\u{fffd}b\u{fffd}c");
    }
}
