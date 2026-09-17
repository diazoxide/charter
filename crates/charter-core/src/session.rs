//! A session: one program running in its own pseudo-terminal, its screen kept by an [`Engine`].
//!
//! Three threads serve each session. The reader feeds everything the program writes into the
//! engine. The writer delivers typed input and the engine's replies to the program. Neither
//! ever waits on the other, so a program that stops reading its input cannot stop its output
//! from being read. The third gives up on synchronized updates the program opens and never
//! closes, so output held back for a pane is never held for ever.
//!
//! Dropping a session ends its program and everything the program started in its process
//! group: there is no daemon, and nothing outlives the app. A program that moves itself into
//! a new session of its own (a daemon) has left on purpose and is not followed.

use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
#[cfg(all(test, unix))]
use std::sync::mpsc::TryRecvError;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
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

/// A session's terminal, and the views watching it. One lock covers both, so a view opens on
/// a screen with the output that follows it, and nothing in between is lost or sent twice.
struct Terminal {
    engine: Box<dyn Engine>,
    views: Vec<Watcher>,
    opened: u64,
}

/// One view's side of a session: where its bytes go, and the ones held back from it because
/// sending them would leave its pane inside an open synchronized update.
struct Watcher {
    id: u64,
    sends: Sender<Vec<u8>>,
    held: Vec<u8>,
}

impl Terminal {
    /// Whether any view is holding bytes: whether the stream a pane has been sent so far
    /// stops inside an update that has not closed.
    fn holding(&self) -> bool {
        self.views.iter().any(|view| !view.held.is_empty())
    }

    /// Holds `output` back from every view. Sent now it would leave a pane inside an open
    /// update, where xterm.js draws nothing until its own safety timeout.
    fn hold(&mut self, output: &[u8]) {
        for view in &mut self.views {
            view.held.extend_from_slice(output);
        }
    }

    /// Sends `output` to every view, behind whatever was held back for it. A view whose
    /// reader has gone is dropped here; until then its bytes wait in its own queue, and
    /// whoever calls this never waits for it.
    fn hand_over(&mut self, output: &[u8]) {
        self.views.retain_mut(|view| {
            let bytes = if view.held.is_empty() {
                output.to_vec()
            } else {
                let mut held = std::mem::take(&mut view.held);
                held.extend_from_slice(output);
                held
            };
            view.sends.send(bytes).is_ok()
        });
    }

    /// Ends an update the engine has given up on for its own screen, for the views as well:
    /// each is sent what it holds, closed, because half a frame drawn beats a whole one that
    /// waits on a pane's own safety timeout. Nothing held is nothing to do.
    fn give_up_on_the_update(&mut self) {
        self.views.retain_mut(|view| {
            if view.held.is_empty() {
                return true;
            }
            let mut held = std::mem::take(&mut view.held);
            held.extend_from_slice(END_UPDATE);
            view.sends.send(held).is_ok()
        });
    }

    /// Closes every view, because the program is gone. What is held back goes over first:
    /// no update the program left open will ever close now, so its last half frame is drawn
    /// instead of dropped.
    fn close_views(&mut self) {
        self.give_up_on_the_update();
        self.views.clear();
    }

    /// The same, for whoever has just read the engine and may have ended an update by doing
    /// it: a screen read applies one whose deadline has passed, and a snapshot ends one
    /// outright. Either way the views are holding bytes the engine no longer is.
    fn give_up_if_the_engine_has(&mut self) {
        if self.engine.open_update().is_none() {
            self.give_up_on_the_update();
        }
    }
}

/// A view of a session: the bytes that draw its screen as it was when the view opened, then
/// everything the program writes from that moment on. A pane showing the session writes them
/// all to its terminal, in the order they arrive.
///
/// The queue is the session's only claim on whoever is reading it: the session's own thread
/// never waits for a view, and a view that is read slowly holds its bytes, not the session.
/// Dropping the view closes it.
pub struct View {
    /// The size the screen the view opened on was drawn for. A terminal cannot be told its own
    /// size by what it is sent, so the pane is told here.
    pub size: Size,
    pub output: Receiver<Vec<u8>>,
    open: Attachment,
}

impl View {
    /// The view taken apart, for a pane that reads its queue somewhere other than where it
    /// holds the view open. Dropping the [`Attachment`] closes the view at once, whoever still
    /// holds the queue.
    pub fn into_parts(self) -> (Attachment, Size, Receiver<Vec<u8>>) {
        (self.open, self.size, self.output)
    }
}

/// A view held open. Dropping it closes the view: the session stops sending to it, and
/// whoever is reading its queue sees that queue end.
pub struct Attachment {
    terminal: Weak<Mutex<Terminal>>,
    id: u64,
}

impl Drop for Attachment {
    fn drop(&mut self) {
        if let Some(terminal) = self.terminal.upgrade() {
            lock(&terminal).views.retain(|view| view.id != self.id);
        }
    }
}

pub struct Session {
    terminal: Arc<Mutex<Terminal>>,
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
        let terminal = Arc::new(Mutex::new(Terminal {
            engine,
            views: Vec::new(),
            opened: 0,
        }));
        let (input, queued) = mpsc::sync_channel(INPUT_QUEUE);
        // One wake is enough to send: what it says is "something is held", and the deadline
        // is read from the engine. A wake that does not fit is a wake already waiting.
        let (holding, held) = mpsc::sync_channel(1);

        thread::Builder::new()
            .name("charter-session-writer".into())
            .spawn(move || deliver(queued, writer))?;
        thread::Builder::new()
            .name("charter-session-updates".into())
            .spawn({
                let terminal = Arc::clone(&terminal);
                move || give_up_on_open_updates(held, &terminal)
            })?;
        thread::Builder::new()
            .name("charter-session-reader".into())
            .spawn({
                let terminal = Arc::clone(&terminal);
                let replies = input.clone();
                move || pump(reader, &terminal, &replies, &holding)
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
            terminal,
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
        lock(&self.terminal).engine.resize(size);
        lock(&self.master).resize(pty_size(size)).map_err(pty_error)
    }

    /// The screen as the program has drawn it so far.
    pub fn screen(&self) -> Screen {
        let (screen, answers) = {
            let mut terminal = lock(&self.terminal);
            let screen = terminal.engine.screen();
            terminal.give_up_if_the_engine_has();
            (screen, terminal.engine.take_replies())
        };
        self.answer(answers);
        screen
    }

    /// Opens a view of this session, for a pane that is about to show it.
    pub fn attach(&self) -> View {
        let (output, answers, size, id) = {
            let mut terminal = lock(&self.terminal);
            let snapshot = terminal.engine.snapshot();
            // The snapshot ended any open update, so the views already watching are holding
            // bytes the engine is not: they are given what they hold, ended, before this view
            // joins them. This view is sent none of it, because its snapshot is drawn from it.
            terminal.give_up_if_the_engine_has();
            let size = terminal.engine.screen().size;
            let answers = terminal.engine.take_replies();
            let (sender, output) = mpsc::channel();
            // The screen goes in first, ahead of any output the reading thread sends once
            // this lock is released.
            let _ = sender.send(snapshot);
            terminal.opened += 1;
            let id = terminal.opened;
            terminal.views.push(Watcher {
                id,
                sends: sender,
                held: Vec::new(),
            });
            (output, answers, size, id)
        };
        let view = View {
            size,
            output,
            open: Attachment {
                terminal: Arc::downgrade(&self.terminal),
                id,
            },
        };
        self.answer(answers);
        view
    }

    /// Hands the program what its terminal owes it: answers the engine gathered while
    /// something other than the reading thread was looking at it.
    fn answer(&self, answers: Vec<u8>) {
        if !answers.is_empty() {
            let _ = self.input.try_send(answers);
        }
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

    /// Ends the program and everything it started, and does not return until it is gone.
    /// Dropping a session does the same on a thread of its own, which is what closing a tab
    /// wants; quitting cannot wait on a thread, because the process is about to go.
    pub fn end(self) {
        let child = Arc::clone(&self.child);
        end(&child);
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
/// Ends a synchronized update: what a view holding one is sent when it is given up on, so
/// its pane draws what it has instead of waiting on its own safety timeout.
const END_UPDATE: &[u8] = b"\x1b[?2026l";
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
///
/// A view is never sent output that ends inside a synchronized update the program has opened
/// and not closed: xterm.js skips every render that falls while one is open and draws on a
/// one-second safety timeout instead (xterm.js#6071), so a pane sent a repaint in pieces
/// would draw once a second. The pieces wait here until the update closes, and `holding`
/// wakes the thread that gives up on one that never does.
fn pump(
    mut reader: Box<dyn Read + Send>,
    terminal: &Mutex<Terminal>,
    replies: &SyncSender<Vec<u8>>,
    holding: &SyncSender<()>,
) {
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            // The program's output has ended, and so has the program. Every view is closed,
            // so a pane showing it can say so instead of showing a screen that is now final.
            Ok(0) => return lock(terminal).close_views(),
            Ok(read) => read,
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return lock(terminal).close_views(),
        };
        let (answers, held) = {
            let mut terminal = lock(terminal);
            let output = &buffer[..read];
            terminal.engine.advance(output);
            let open = terminal.engine.open_update().is_some();
            if !open {
                // The update this output ends, if any, is closed in it: what the program
                // wrote is handed over exactly as it wrote it.
                terminal.hand_over(output);
            } else {
                // An update is left open, so this output is cut where the last one it closed
                // ended: everything before that can be drawn now, and only the piece inside
                // the open update waits. Cutting matters as much as holding — a harness
                // writing 4 KB at a time ends nearly every write inside an update, and each
                // new one puts the deadline off again, so holding whole writes would hold
                // whole seconds of drawn frames.
                let closed = closed_updates_end(output);
                if closed > 0 {
                    terminal.hand_over(&output[..closed]);
                }
                terminal.hold(&output[closed..]);
            }
            (terminal.engine.take_replies(), open)
        };
        // Outside the lock, and never waited on: a full queue is a wake already waiting.
        if held {
            let _ = holding.try_send(());
        }
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

/// How much of `output` a view can be given: everything up to and including the last
/// synchronized update that `output` closes, and none of the one it leaves open. Zero when it
/// closes none.
///
/// It looks for the sequence itself, which is what vte does with the bytes inside an update
/// too (`advance_sync_csi`): exactly `\x1b[?2026l` ends one, for vte and for xterm.js alike.
/// A sequence split across two reads is not found here — that read is held whole and goes
/// over behind the next one, late by one read and never cut inside an update.
fn closed_updates_end(output: &[u8]) -> usize {
    output
        .windows(END_UPDATE.len())
        .rposition(|window| window == END_UPDATE)
        .map_or(0, |at| at + END_UPDATE.len())
}

/// Gives up on a synchronized update a program opens and never closes, so output held back
/// for a pane is never held for ever. It waits until the deadline the engine keeps for the
/// open update — vte's, the same one the engine applies to its own screen — and hands over
/// whatever is still held once it passes. It ends when the session's reading thread does.
fn give_up_on_open_updates(woken: Receiver<()>, terminal: &Mutex<Terminal>) {
    // Nothing is held: there is nothing to wake for, and no deadline to keep.
    while woken.recv().is_ok() {
        loop {
            let deadline = {
                let mut terminal = lock(terminal);
                if !terminal.holding() {
                    break;
                }
                match terminal.engine.open_update() {
                    Some(deadline) if Instant::now() < deadline => deadline,
                    // The update is over, or it has run out of time. Either way the pane gets
                    // what is held, and the engine's screen says the same.
                    _ => {
                        terminal.give_up_on_the_update();
                        break;
                    }
                }
            };
            // More output may be held while this waits; the wake it sends ends the wait, and
            // the state is read again above.
            match woken.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
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

    /// What a view has been sent: the screen it opened on, then the output that followed.
    #[derive(Default)]
    struct Seen {
        opened_on: Vec<u8>,
        followed: Vec<u8>,
        pieces: usize,
    }

    impl Seen {
        /// Takes everything waiting for the view, without waiting for more.
        fn take(&mut self, view: &View) -> &mut Self {
            while let Ok(bytes) = view.output.try_recv() {
                self.pieces += 1;
                if self.pieces == 1 {
                    self.opened_on = bytes;
                } else {
                    self.followed.extend_from_slice(&bytes);
                }
            }
            self
        }

        /// The screen a terminal ends up with after being sent all of it.
        fn replayed(&self, size: Size) -> Screen {
            let mut engine = AlacrittyEngine::new(size, 1000);
            engine.advance(&self.opened_on);
            engine.advance(&self.followed);
            engine.screen()
        }
    }

    /// Waits until the view's screen shows `text`, failing with what it last showed.
    fn view_until(view: &View, text: &str) -> Screen {
        let mut seen = Seen::default();
        let deadline = Instant::now() + PATIENCE;
        loop {
            let screen = seen.take(view).replayed(view.size);
            if screen.lines.iter().any(|line| line.contains(text)) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "the view never showed {text:?}: {:#?}",
                screen.lines
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_view_opens_on_the_screen_as_it_already_is() {
        let session = sh("printf 'printed before the view opened'; sleep 600");
        screen_until(&session, shows("printed before the view opened"));

        let view = session.attach();

        assert_eq!(
            Seen::default().take(&view).replayed(view.size).lines,
            session.screen().lines,
            "the view's screen is the session's screen"
        );
    }

    #[test]
    fn a_view_is_closed_when_the_program_ends_so_a_pane_can_tell() {
        // A pane showing a program that has ended must be able to say so, rather than showing
        // a screen that will never change again.
        let session = sh("printf 'the last thing it printed'");
        let view = session.attach();
        view_until(&view, "the last thing it printed");

        assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));

        let deadline = Instant::now() + PATIENCE;
        loop {
            match view.output.try_recv() {
                Err(TryRecvError::Disconnected) => return,
                _ => assert!(
                    Instant::now() < deadline,
                    "the view was never closed after the program ended"
                ),
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn closing_a_view_stops_the_session_sending_to_it_at_once() {
        let session = sh("read _; printf 'after the view closed'; sleep 600");
        let (closing, _size, output) = session.attach().into_parts();
        let staying = session.attach();

        drop(closing);
        session.write(b"\r").unwrap();
        view_until(&staying, "after the view closed");

        // What the closed view was already sent is still there to read; nothing is added.
        while let Ok(_already_sent) = output.try_recv() {}
        assert!(
            matches!(output.try_recv(), Err(TryRecvError::Disconnected)),
            "the session was still holding the closed view's queue"
        );
    }

    #[test]
    fn ending_a_session_ends_its_program_before_it_returns() {
        // Quitting the app cannot leave a thread to do this: the process is about to go. The
        // program here survives the hangup, so only the kill that follows it can end it — and
        // it says so once it does, because until then a hangup would end it after all.
        let session = sh("trap '' HUP; echo guarded; while :; do sleep 600; done");
        screen_until(&session, shows("guarded"));
        let pid = session.process_id().expect("a running program has a pid");

        session.end();

        assert!(!alive(pid), "process {pid} outlived the session it was in");
    }

    #[test]
    fn a_view_is_told_the_size_its_screen_was_drawn_for() {
        let session = sh("sleep 600");
        let bigger = Size {
            columns: 120,
            rows: 40,
        };
        session.resize(bigger).unwrap();

        let view = session.attach();

        assert_eq!(view.size, bigger);
    }

    #[test]
    fn what_the_program_writes_after_a_view_opens_reaches_it() {
        let session = sh("read _; printf 'printed after the view opened'; sleep 600");
        let view = session.attach();

        session.write(b"\r").unwrap();

        view_until(&view, "printed after the view opened");
    }

    /// How many lines of the flood below `bytes` mention, in the order they appear.
    fn flood_lines(bytes: &[u8]) -> Vec<usize> {
        String::from_utf8_lossy(bytes)
            .split("line ")
            .skip(1)
            .filter_map(|rest| rest.split_once(" of the flood"))
            .filter_map(|(number, _)| number.parse().ok())
            .collect()
    }

    const FLOOD: usize = 100_000;

    #[test]
    fn views_that_open_mid_flood_are_sent_every_line_that_follows_exactly_once() {
        // The handover from the screen a view opens on to the output that follows it happens
        // while the program is writing: nothing in between may be lost, and nothing may
        // arrive twice. Each line says which one it is, so both show up here.
        let session = sh(&format!(
            "awk 'BEGIN {{ for (i = 0; i < {FLOOD}; i++) print \"line\", i, \"of the flood\" }}'"
        ));
        screen_until(&session, shows("of the flood"));

        let views: Vec<View> = (0..10).map(|_| session.attach()).collect();

        assert_eq!(
            session.wait(Duration::ZERO).unwrap(),
            None,
            "the program was already done, so no view opened mid-flood"
        );
        assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
        for (n, view) in views.iter().enumerate() {
            let last = FLOOD - 1;
            let mut seen = Seen::default();
            let deadline = Instant::now() + PATIENCE;
            loop {
                let followed = flood_lines(&seen.take(view).followed);
                if followed.last() == Some(&last) {
                    let first = *followed.first().expect("a view is sent output");
                    let on_screen = flood_lines(&seen.opened_on);
                    let highest = *on_screen.iter().max().expect("the screen shows the flood");
                    // One line further on is the next line; two is a line the screen holds
                    // only half of.
                    assert!(
                        first <= highest + 2,
                        "view {n} opened on line {highest} and was sent {first} next"
                    );
                    assert!(
                        followed.iter().copied().eq(first..=last),
                        "view {n} was not sent every line from {first} to {last} in order, \
                         once each"
                    );
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "view {n} was never sent line {last}, only up to {:?}",
                    followed.last()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    #[test]
    fn two_views_of_one_session_both_get_its_output() {
        let session = sh("read _; printf 'for both views'; sleep 600");
        let first = session.attach();
        let second = session.attach();

        session.write(b"\r").unwrap();

        view_until(&first, "for both views");
        view_until(&second, "for both views");
    }

    #[test]
    fn a_session_keeps_running_for_the_views_it_has_left_when_one_closes() {
        let session = sh("read _; printf 'after the other view closed'; sleep 600");
        let closing = session.attach();
        let staying = session.attach();

        drop(closing);
        session.write(b"\r").unwrap();

        view_until(&staying, "after the other view closed");
        screen_until(&session, shows("after the other view closed"));
    }

    #[test]
    fn a_view_keeps_working_after_the_session_is_resized() {
        let session = sh("read _; stty size; sleep 600");
        let view = session.attach();
        let bigger = Size {
            columns: 100,
            rows: 30,
        };

        session.resize(bigger).unwrap();
        session.write(b"\r").unwrap();

        let mut seen = Seen::default();
        let deadline = Instant::now() + PATIENCE;
        while !seen
            .take(&view)
            .replayed(bigger)
            .lines
            .iter()
            .any(|line| line.contains("30 100"))
        {
            assert!(Instant::now() < deadline, "the view never saw the new size");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_question_asked_inside_a_synchronized_update_is_answered_once_the_screen_is_read() {
        // The question is held back inside the update, so the answer is only owed once
        // something ends it — reading the screen, or a pane opening. Whatever ends it must
        // hand the answer over, or the program waits for it forever.
        let session = sh(
            "stty raw -echo; printf '\\033[?2026h\\033[6n'; dd bs=1 count=6 2>/dev/null >/dev/null; stty sane; echo answered",
        );

        screen_until(&session, shows("answered"));
    }

    #[test]
    fn a_question_asked_inside_a_synchronized_update_is_answered_when_a_pane_opens() {
        let session = sh(
            "stty raw -echo; printf '\\033[?2026h\\033[6n'; dd bs=1 count=6 2>/dev/null >/dev/null; stty sane; echo answered",
        );
        // Long enough for the question to have been written and held back.
        std::thread::sleep(Duration::from_millis(100));

        let view = session.attach();

        view_until(&view, "answered");
    }

    const BEGIN_UPDATE: &[u8] = b"\x1b[?2026h";
    const END_UPDATE: &[u8] = b"\x1b[?2026l";
    /// What a pane gets for nothing if the core hands it output that ends inside an open
    /// update: xterm.js draws on its own safety timeout, and on nothing else.
    const XTERM_SAFETY_TIMEOUT: Duration = Duration::from_secs(1);

    /// How many synchronized updates `bytes` leave open — how many a pane's terminal would
    /// still be inside after being written them, and so whether it can draw at all.
    fn updates_left_open(bytes: &[u8]) -> usize {
        let mut open = 0usize;
        for at in 0..bytes.len() {
            let rest = &bytes[at..];
            if rest.starts_with(BEGIN_UPDATE) {
                open += 1;
            } else if rest.starts_with(END_UPDATE) {
                open = open.saturating_sub(1);
            }
        }
        open
    }

    /// Everything a view has been sent, in the pieces it arrived in. A pane writes each piece
    /// to its terminal as it comes, so it is each piece, and not the whole, that decides
    /// whether the pane can draw.
    #[derive(Default)]
    struct Pieces(Vec<Vec<u8>>);

    impl Pieces {
        fn take(&mut self, view: &View) -> &mut Self {
            while let Ok(bytes) = view.output.try_recv() {
                self.0.push(bytes);
            }
            self
        }

        fn all(&self) -> Vec<u8> {
            self.0.concat()
        }

        fn holds(&self, text: &str) -> bool {
            String::from_utf8_lossy(&self.all()).contains(text)
        }

        /// The output that followed the screen the view opened on.
        fn followed(&self) -> Vec<u8> {
            self.0.iter().skip(1).flatten().copied().collect()
        }
    }

    /// Waits until everything a view has been sent holds `text`, failing with what it has.
    fn pieces_until(view: &View, text: &str, patience: Duration) -> Pieces {
        let mut pieces = Pieces::default();
        let deadline = Instant::now() + patience;
        while !pieces.take(view).holds(text) {
            assert!(
                Instant::now() < deadline,
                "the view was never sent {text:?}: {:?}",
                String::from_utf8_lossy(&pieces.all())
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        pieces
    }

    #[test]
    fn a_view_is_never_sent_output_that_ends_inside_an_open_update() {
        // A harness that opens an update and pauses before closing it: xterm.js skips every
        // render that falls while one is open (xterm.js#6071), so a pane sent the pieces as
        // they came would draw once a second and no oftener. The core hands over whole
        // updates instead.
        let session = sh("i=0; while [ $i -lt 5 ]; do \
               printf '\\033[?2026h\\033[H\\033[2Jframe %d opens\\r\\n' $i; \
               sleep 0.05; \
               printf 'frame %d closes\\r\\n\\033[?2026l' $i; \
               i=$((i + 1)); \
             done; printf 'all five frames\\r\\n'");
        let view = session.attach();

        let pieces = pieces_until(&view, "all five frames", PATIENCE);

        for (nth, piece) in pieces.0.iter().enumerate() {
            assert_eq!(
                updates_left_open(piece),
                0,
                "piece {nth} of {} leaves an update open: {:?}",
                pieces.0.len(),
                String::from_utf8_lossy(piece)
            );
        }
    }

    #[test]
    fn output_that_closes_no_update_has_nothing_that_can_be_drawn() {
        assert_eq!(closed_updates_end(b"\x1b[?2026hhalf a frame"), 0);
        assert_eq!(closed_updates_end(b"a plain line\r\n"), 0);
    }

    #[test]
    fn output_is_cut_after_the_last_update_it_closes() {
        let output = b"\x1b[?2026hone\x1b[?2026l\x1b[?2026htwo\x1b[?2026l\x1b[?2026hthree";

        let cut = closed_updates_end(output);

        assert_eq!(
            String::from_utf8_lossy(&output[..cut]),
            "\x1b[?2026hone\x1b[?2026l\x1b[?2026htwo\x1b[?2026l",
            "both closed updates are drawn, and the third is not begun for the pane"
        );
        assert_eq!(updates_left_open(&output[..cut]), 0);
    }

    #[test]
    fn a_repaint_that_closes_reaches_the_view_although_the_next_one_has_begun() {
        // The shape the benchmark measures. A harness writing 4 KB at a time puts the end of
        // one repaint and the start of the next in the same write, so every write ends inside
        // an open update — and each new one puts the deadline off again. Holding all of it
        // back would draw once, late: what has closed goes over now, and only the piece that
        // has not closed waits for the rest.
        let session = sh("printf '\\033[?2026hframe 0 opens\\r\\n'; i=0; \
             while [ $i -lt 8 ]; do \
               printf 'frame %d closes\\r\\n\\033[?2026l\\033[?2026hframe %d opens\\r\\n' \
                 $i $((i + 1)); \
               sleep 0.02; i=$((i + 1)); \
             done; sleep 30");
        let view = session.attach();

        let pieces = pieces_until(&view, "frame 7 closes", PATIENCE);

        let drawable = pieces
            .0
            .iter()
            .filter(|piece| String::from_utf8_lossy(piece).contains("closes"))
            .count();
        assert!(
            drawable >= 5,
            "eight repaints closed and the view could draw {drawable} of them: {:?}",
            pieces
                .0
                .iter()
                .map(|piece| String::from_utf8_lossy(piece).len())
                .collect::<Vec<_>>()
        );
        for (nth, piece) in pieces.0.iter().enumerate() {
            assert_eq!(
                updates_left_open(piece),
                0,
                "piece {nth} leaves an update open: {:?}",
                String::from_utf8_lossy(piece)
            );
        }
    }

    #[test]
    fn output_an_update_never_closes_reaches_the_view_on_a_deadline_of_its_own() {
        // Held back for ever is its own way of freezing a pane. A program that dies or hangs
        // mid-repaint must not hold its output past the point xterm.js would have drawn it
        // anyway — the core gives up on the update and ends it, as the engine does for its
        // own screen.
        let session = sh("printf '\\033[?2026h\\033[H\\033[2Jhalf a frame\\r\\n'; sleep 30");
        let view = session.attach();

        let pieces = pieces_until(&view, "half a frame", XTERM_SAFETY_TIMEOUT);

        assert_eq!(
            updates_left_open(&pieces.all()),
            0,
            "the view is left inside the update the program never closed: {:?}",
            String::from_utf8_lossy(&pieces.all())
        );
    }

    #[test]
    fn a_pane_opening_inside_an_update_leaves_no_other_pane_holding_it() {
        // Opening a view ends an update on the engine, because its snapshot has to carry
        // everything the engine has read. Every other view has to be told the update ended
        // too, or the next thing the program writes is handed to a pane still inside one.
        let session = sh("printf 'a frame starts\\r\\n'; sleep 0.02; \
             printf '\\033[?2026h\\033[H\\033[2Jheld back\\r\\n'; \
             read go; printf 'and more\\r\\n'; sleep 30");
        let first = session.attach();
        pieces_until(&first, "a frame starts", PATIENCE);
        // The update is open by now and its deadline has not passed: 20 ms in, of 150.
        std::thread::sleep(Duration::from_millis(60));

        let second = session.attach();
        session.write(b"\n").expect("the program is reading");

        let pieces = pieces_until(&first, "and more", PATIENCE);
        assert_eq!(
            updates_left_open(&pieces.all()),
            0,
            "the first pane is left inside an update the second pane's snapshot ended: {:?}",
            String::from_utf8_lossy(&pieces.all())
        );
        drop(second);
    }

    #[test]
    fn output_a_program_ends_inside_an_update_reaches_the_view_before_it_closes() {
        // The program is gone, so nothing will close the update and no deadline is worth
        // waiting for: what it wrote goes to the pane now, ended, or the pane keeps a screen
        // older than the one the engine holds.
        let session =
            sh("sleep 0.3; printf '\\033[?2026h\\033[H\\033[2Jits last half frame\\r\\n'");
        let view = session.attach();

        let pieces = pieces_until(&view, "its last half frame", PATIENCE);

        assert_eq!(
            updates_left_open(&pieces.all()),
            0,
            "the view is left inside the update the program died in: {:?}",
            String::from_utf8_lossy(&pieces.all())
        );
    }

    #[test]
    fn a_view_is_sent_every_byte_the_program_wrote_when_updates_are_held_across_writes() {
        // Holding output back may neither lose it nor repeat it: what the program wrote is
        // what the pane is written, in order, to the byte, however the writes fell and
        // whatever was held between them.
        let wrote = b"\x1b[?2026hone|two\x1b[?2026l\x1b[?2026hthree|four\x1b[?2026l".to_vec();
        let session = sh(
            "sleep 0.3; printf '\\033[?2026hone|'; sleep 0.05; printf 'two\\033[?2026l'; \
             sleep 0.05; printf '\\033[?2026hthree|'; sleep 0.05; \
             printf 'four\\033[?2026l'; sleep 30",
        );
        let view = session.attach();

        let pieces = pieces_until(&view, "four", PATIENCE);

        assert_eq!(
            String::from_utf8_lossy(&pieces.followed()),
            String::from_utf8_lossy(&wrote)
        );
    }

    #[test]
    fn a_program_that_closes_its_own_updates_has_its_output_passed_through_untouched() {
        // Holding output back may not rewrite it: what the program wrote is what the pane is
        // written, to the byte, whenever the program ends its own updates.
        let wrote = b"\x1b[?2026h\x1b[H\x1b[2Jone\x1b[?2026ltwo".to_vec();
        let session =
            sh("sleep 0.3; printf '\\033[?2026h\\033[H\\033[2Jone\\033[?2026ltwo'; sleep 30");
        let view = session.attach();

        let pieces = pieces_until(&view, "two", PATIENCE);

        assert_eq!(
            String::from_utf8_lossy(&pieces.followed()),
            String::from_utf8_lossy(&wrote)
        );
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
