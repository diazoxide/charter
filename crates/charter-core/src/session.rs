//! A session: one program running in its own pseudo-terminal, its screen kept by an [`Engine`].
//!
//! Two threads serve each session. The reader feeds everything the program writes into the
//! engine. The writer delivers typed input and the engine's replies to the program. Neither
//! ever waits on the other, so a program that stops reading its input cannot stop its output
//! from being read.
//!
//! Dropping a session ends its program and everything the program started in its process
//! group: there is no daemon, and nothing outlives the app. A program that moves itself into
//! a new session of its own (a daemon) has left on purpose and is not followed.

use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::engine::{Engine, Screen, Size};

/// What to run, where, and at what size.
#[derive(Debug, Clone)]
pub struct Spec {
    pub program: OsString,
    pub args: Vec<OsString>,
    /// Where the program starts. `None` is the app's own working directory. A directory that
    /// does not exist is refused, never silently replaced.
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
    pub size: Size,
}

impl Spec {
    pub fn new(program: impl Into<OsString>, size: Size) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            size,
        }
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
}

/// How a program ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    Code(u32),
    /// Killed by a signal, named by the operating system.
    Signal(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("could not open a pseudo-terminal: {0}")]
    Pty(String),
    #[error("could not start {program:?}: {reason}")]
    Spawn { program: OsString, reason: String },
    #[error("the program is not reading its input, and {INPUT_QUEUE} writes are already waiting")]
    InputBackedUp,
    #[error("the session's terminal is closed")]
    Closed,
    #[error("the session's terminal failed: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Session {
    engine: Arc<Mutex<Box<dyn Engine>>>,
    input: SyncSender<Vec<u8>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
}

impl Session {
    /// Starts `spec.program` in a new pseudo-terminal whose screen `engine` keeps.
    pub fn spawn(spec: Spec, engine: Box<dyn Engine>) -> Result<Self, SessionError> {
        let spawn_error = |reason: String| SessionError::Spawn {
            program: spec.program.clone(),
            reason,
        };
        let cwd = match &spec.cwd {
            Some(dir) if dir.is_dir() => dir.clone(),
            Some(dir) => return Err(spawn_error(format!("{} is not a directory", dir.display()))),
            None => std::env::current_dir()?,
        };
        let size = spec.size.at_least_min();

        let pair = native_pty_system()
            .openpty(pty_size(size))
            .map_err(pty_error)?;
        // Everything that can fail is done before the program starts, so a failure never
        // leaves a program running that no session owns.
        let reader = pair.master.try_clone_reader().map_err(pty_error)?;
        let writer = pair.master.take_writer().map_err(pty_error)?;
        let engine = Arc::new(Mutex::new(engine));
        let (input, queued) = mpsc::sync_channel(INPUT_QUEUE);

        thread::Builder::new()
            .name("charter-session-writer".into())
            .spawn(move || deliver(queued, writer))?;
        thread::Builder::new()
            .name("charter-session-reader".into())
            .spawn({
                let engine = Arc::clone(&engine);
                let replies = input.clone();
                move || pump(reader, &engine, &replies)
            })?;

        let mut command = CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| spawn_error(err.to_string()))?;
        // Only the program may hold the terminal's other end, or the reader never sees it close.
        drop(pair.slave);

        Ok(Self {
            engine,
            input,
            master: Mutex::new(pair.master),
            child: Arc::new(Mutex::new(child)),
        })
    }

    /// Sends input to the program, as if typed. Never blocks: when the program has stopped
    /// reading and the queue is full, the input is refused with [`SessionError::InputBackedUp`].
    pub fn write(&self, bytes: &[u8]) -> Result<(), SessionError> {
        self.input
            .try_send(bytes.to_vec())
            .map_err(|err| match err {
                TrySendError::Full(_) => SessionError::InputBackedUp,
                TrySendError::Disconnected(_) => SessionError::Closed,
            })
    }

    /// Resizes the terminal; the program is told through the usual window-size signal. Sizes
    /// below [`Size::MIN`] are raised to it.
    pub fn resize(&self, size: Size) -> Result<(), SessionError> {
        let size = size.at_least_min();
        // The engine first: the program redraws for the new size as soon as it is told, and
        // that output must land on a grid of the new size.
        lock(&self.engine).resize(size);
        lock(&self.master).resize(pty_size(size)).map_err(pty_error)
    }

    /// The screen as the program has drawn it so far.
    pub fn screen(&self) -> Screen {
        lock(&self.engine).screen()
    }

    /// How the program ended, waiting at most `timeout`. `Ok(None)` means it is still running.
    pub fn wait(&self, timeout: Duration) -> Result<Option<Exit>, SessionError> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = lock(&self.child).try_wait()? {
                return Ok(Some(match status.signal() {
                    Some(signal) => Exit::Signal(signal.to_owned()),
                    None => Exit::Code(status.exit_code()),
                }));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(POLL);
        }
    }

    /// The operating system's id for the program, while it runs.
    pub fn process_id(&self) -> Option<u32> {
        lock(&self.child).process_id()
    }
}

impl Drop for Session {
    /// Returns at once. Ending and reaping the program happens on a thread of its own, so
    /// closing a tab or quitting with many sessions never waits on a program.
    fn drop(&mut self) {
        let child = Arc::clone(&self.child);
        let reap = move || end(&child);
        if thread::Builder::new()
            .name("charter-session-reaper".into())
            .spawn(reap)
            .is_err()
        {
            // No thread to spare: ending it here is slower, but it still ends.
            end(&self.child);
        }
    }
}

/// How many writes may wait for a program that is not reading its input.
pub const INPUT_QUEUE: usize = 1024;
const POLL: Duration = Duration::from_millis(10);
/// How long a program gets to exit on its hangup before its group is killed.
const HANGUP_GRACE: Duration = Duration::from_millis(500);

/// Hangs up the program's process group, then kills whatever is left of it, then reaps.
#[cfg(unix)]
fn end(child: &Mutex<Box<dyn Child + Send + Sync>>) {
    use rustix::process::{Pid, Signal, kill_process_group};

    let mut child = lock(child);
    if !matches!(child.try_wait(), Ok(None)) {
        return;
    }
    // portable-pty starts every program as the leader of a new session, so its process group
    // id is its pid, and the group holds everything it started that did not leave on purpose.
    // Until it is reaped below, the leader keeps that id from being reused.
    let Some(group) = child.process_id().and_then(|pid| Pid::from_raw(pid as i32)) else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    };
    let _ = kill_process_group(group, Signal::HUP);
    thread::sleep(HANGUP_GRACE);
    let _ = kill_process_group(group, Signal::KILL);
    let _ = child.wait();
}

#[cfg(not(unix))]
fn end(child: &Mutex<Box<dyn Child + Send + Sync>>) {
    let mut child = lock(child);
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    let _ = child.wait();
}

/// Feeds the program's output to the engine until the terminal closes, queueing the answers
/// the engine owes the program. It never waits on the writer.
fn pump(
    mut reader: Box<dyn Read + Send>,
    engine: &Mutex<Box<dyn Engine>>,
    replies: &SyncSender<Vec<u8>>,
) {
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => read,
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        let answers = {
            let mut engine = lock(engine);
            engine.advance(&buffer[..read]);
            engine.take_replies()
        };
        // A program that floods queries without reading the answers loses the answers it
        // would never have read; its output keeps flowing either way.
        if !answers.is_empty()
            && matches!(
                replies.try_send(answers),
                Err(TrySendError::Disconnected(_))
            )
        {
            return;
        }
    }
}

/// Writes queued input to the program until the session is gone or the terminal closes.
fn deliver(queued: Receiver<Vec<u8>>, mut writer: Box<dyn Write + Send>) {
    for bytes in queued {
        if writer
            .write_all(&bytes)
            .and_then(|()| writer.flush())
            .is_err()
        {
            return;
        }
    }
}

fn pty_error(err: impl std::fmt::Display) -> SessionError {
    SessionError::Pty(err.to_string())
}

fn pty_size(size: Size) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// A poisoned lock only means another thread panicked while holding it; the terminal state
/// inside is still the best there is.
fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(all(test, unix))]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::engine::AlacrittyEngine;

    const SIZE: Size = Size {
        columns: 80,
        rows: 24,
    };
    const PATIENCE: Duration = Duration::from_secs(10);

    fn sh(script: &str) -> Session {
        Session::spawn(
            Spec::new("/bin/sh", SIZE).args(["-c", script]),
            Box::new(AlacrittyEngine::new(SIZE, 1000)),
        )
        .expect("the session starts")
    }

    /// Waits until the screen satisfies `ready`, failing with the screen it last saw.
    fn screen_until(session: &Session, ready: impl Fn(&Screen) -> bool) -> Screen {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let screen = session.screen();
            if ready(&screen) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "the screen never got there: {:#?}",
                screen.lines
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn shows(text: &str) -> impl Fn(&Screen) -> bool + '_ {
        move |screen| screen.lines.iter().any(|line| line.contains(text))
    }

    #[test]
    fn what_the_program_prints_reaches_the_screen() {
        let session = sh("printf 'hello from the pty'");

        screen_until(&session, shows("hello from the pty"));
    }

    #[test]
    fn the_exit_code_is_reported_once_the_program_exits() {
        let session = sh("exit 3");

        assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(3)));
    }

    #[test]
    fn typed_input_reaches_the_program() {
        let session = sh("read line; echo \"got <$line>\"");

        session.write(b"ping\r").unwrap();

        screen_until(&session, shows("got <ping>"));
    }

    #[test]
    fn a_resize_reaches_the_program() {
        let session = sh("read _; stty size");

        session
            .resize(Size {
                columns: 100,
                rows: 30,
            })
            .unwrap();
        session.write(b"\r").unwrap();

        screen_until(&session, shows("30 100"));
    }

    #[test]
    fn a_program_that_asks_where_the_cursor_is_gets_an_answer() {
        // Without the reply the program blocks on its read forever.
        let session = sh(
            "stty raw -echo; printf '\\033[6n'; dd bs=1 count=6 2>/dev/null >/dev/null; stty sane; echo answered",
        );

        screen_until(&session, shows("answered"));
    }

    #[test]
    fn dropping_a_session_ends_its_program() {
        let session = sh("sleep 600");
        let pid = session.process_id().expect("a running program has a pid");

        drop(session);

        let deadline = Instant::now() + PATIENCE;
        while alive(pid) {
            assert!(
                Instant::now() < deadline,
                "process {pid} outlived its session"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn fifty_sessions_stream_at_once_and_each_keeps_its_own_screen() {
        let sessions: Vec<Session> = (0..50)
            .map(|n| {
                sh(&format!(
                    "i=0; while [ $i -lt 2000 ]; do echo \"session {n} line $i\"; i=$((i+1)); done"
                ))
            })
            .collect();

        for (n, session) in sessions.iter().enumerate() {
            assert_eq!(
                session.wait(Duration::from_secs(60)).unwrap(),
                Some(Exit::Code(0)),
                "session {n} exits cleanly"
            );
            screen_until(session, shows(&format!("session {n} line 1999")));
        }
    }

    #[test]
    fn a_flood_of_terminal_queries_never_stalls_the_session() {
        // The program asks 20,000 times and never reads the answers, so they back up. Output
        // must keep flowing, and input must still be accepted or refused, never block.
        let session = sh(
            "stty raw -echo; i=0; while [ $i -lt 20000 ]; do printf '\\033[6n'; i=$((i+1)); done; printf done-flooding; sleep 600",
        );

        screen_until(&session, shows("done-flooding"));
        let started = Instant::now();
        let _ = session.write(b"x");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "write blocked for {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_zero_size_resize_keeps_the_session_running() {
        let session = sh("read _; echo survived");

        session
            .resize(Size {
                columns: 0,
                rows: 0,
            })
            .unwrap();
        session.resize(SIZE).unwrap();
        session.write(b"\r").unwrap();

        screen_until(&session, shows("survived"));
    }

    #[test]
    fn dropping_a_session_ends_programs_it_started_even_ones_ignoring_hangup() {
        let session = sh("trap '' HUP; (trap '' HUP; sleep 600) & echo \"grandchild=$!\"; wait");
        let screen = screen_until(&session, shows("grandchild="));
        let grandchild: u32 = screen
            .lines
            .iter()
            .find_map(|line| line.strip_prefix("grandchild="))
            .and_then(|pid| pid.trim().parse().ok())
            .expect("the script prints its child's pid");

        drop(session);

        let deadline = Instant::now() + PATIENCE;
        while alive(grandchild) {
            assert!(
                Instant::now() < deadline,
                "process {grandchild} outlived its session"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn dropping_many_sessions_returns_at_once() {
        let sessions: Vec<Session> = (0..50).map(|_| sh("sleep 600")).collect();
        for session in &sessions {
            screen_until(session, |_| session.process_id().is_some());
        }

        let started = Instant::now();
        drop(sessions);

        assert!(
            started.elapsed() < Duration::from_secs(1),
            "dropping took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_working_directory_that_does_not_exist_is_refused() {
        let mut spec = Spec::new("/bin/sh", SIZE).args(["-c", "pwd"]);
        spec.cwd = Some("/definitely/not/here".into());

        let result = Session::spawn(spec, Box::new(AlacrittyEngine::new(SIZE, 100)));

        assert!(
            matches!(result, Err(SessionError::Spawn { .. })),
            "got {:?}",
            result.err()
        );
    }

    #[test]
    fn the_program_runs_in_the_working_directory_it_was_given() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path().canonicalize().unwrap();
        let mut spec = Spec::new("/bin/sh", SIZE).args(["-c", "pwd -P"]);
        spec.cwd = Some(dir.clone());

        let session = Session::spawn(spec, Box::new(AlacrittyEngine::new(SIZE, 100))).unwrap();

        screen_until(&session, shows(dir.to_str().unwrap()));
    }

    #[test]
    fn a_program_killed_by_a_signal_is_not_reported_as_an_exit_code() {
        let session = sh("kill -9 $$");

        assert!(matches!(session.wait(PATIENCE), Ok(Some(Exit::Signal(_)))));
    }

    fn alive(pid: u32) -> bool {
        // `ps` exits non-zero once no such process exists; a zombie awaiting its parent counts as gone.
        Command::new("ps")
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
}
