//! A session: one program running in its own pseudo-terminal, its screen kept by an [`Engine`].
//!
//! A reader thread feeds everything the program writes into the engine, and writes the
//! engine's replies back, so a session keeps working whether or not any UI is watching it.
//! Dropping a session kills its program: there is no daemon, and nothing outlives the app.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
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

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("could not open a pseudo-terminal: {0}")]
    Pty(String),
    #[error("could not start {program:?}: {reason}")]
    Spawn { program: OsString, reason: String },
    #[error("the session's terminal failed: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Session {
    engine: Arc<Mutex<Box<dyn Engine>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
}

impl Session {
    /// Starts `spec.program` in a new pseudo-terminal whose screen `engine` keeps.
    pub fn spawn(spec: Spec, engine: Box<dyn Engine>) -> Result<Self, SessionError> {
        let pair = native_pty_system()
            .openpty(pty_size(spec.size))
            .map_err(pty_error)?;

        let mut command = CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.cwd(cwd);
        }
        command.env("TERM", "xterm-256color");
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| SessionError::Spawn {
                program: spec.program.clone(),
                reason: err.to_string(),
            })?;
        // Only the child may hold the terminal's other end, or the reader never sees it close.
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().map_err(pty_error)?;
        let writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(pty_error)?));
        let engine = Arc::new(Mutex::new(engine));

        thread::Builder::new()
            .name("charter-session-reader".into())
            .spawn({
                let engine = Arc::clone(&engine);
                let writer = Arc::clone(&writer);
                move || pump(reader, &engine, &writer)
            })?;

        Ok(Self {
            engine,
            writer,
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
        })
    }

    /// Sends input to the program, as if typed.
    pub fn write(&self, bytes: &[u8]) -> Result<(), SessionError> {
        let mut writer = lock(&self.writer);
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    /// Resizes the terminal; the program is told through the usual window-size signal.
    pub fn resize(&self, size: Size) -> Result<(), SessionError> {
        lock(&self.master)
            .resize(pty_size(size))
            .map_err(pty_error)?;
        lock(&self.engine).resize(size);
        Ok(())
    }

    /// The screen as the program has drawn it so far.
    pub fn screen(&self) -> Screen {
        lock(&self.engine).screen()
    }

    /// The program's exit code once it has exited, waiting at most `timeout`.
    pub fn wait(&self, timeout: Duration) -> Option<u32> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(status)) = lock(&self.child).try_wait() {
                return Some(status.exit_code());
            }
            if Instant::now() >= deadline {
                return None;
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
    fn drop(&mut self) {
        let child = self.child.get_mut().unwrap_or_else(PoisonError::into_inner);
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        // Reap it, so no zombie is left behind, but never hang the app on a program that
        // ignores the signal.
        let deadline = Instant::now() + REAP_PATIENCE;
        while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
            thread::sleep(POLL);
        }
    }
}

const POLL: Duration = Duration::from_millis(10);
const REAP_PATIENCE: Duration = Duration::from_secs(2);

/// Feeds the program's output to the engine until the terminal closes, answering what the
/// engine says the program asked.
fn pump(
    mut reader: Box<dyn Read + Send>,
    engine: &Mutex<Box<dyn Engine>>,
    writer: &Mutex<Box<dyn Write + Send>>,
) {
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        let replies = {
            let mut engine = lock(engine);
            engine.advance(&buffer[..read]);
            engine.take_replies()
        };
        if !replies.is_empty() {
            let mut writer = lock(writer);
            if writer
                .write_all(&replies)
                .and_then(|()| writer.flush())
                .is_err()
            {
                return;
            }
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

        assert_eq!(session.wait(PATIENCE), Some(3));
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
                session.wait(Duration::from_secs(60)),
                Some(0),
                "session {n} exits cleanly"
            );
            screen_until(session, shows(&format!("session {n} line 1999")));
        }
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
