//! The one gate every program charter starts goes through, so that a descriptor which is not
//! close-on-exec yet cannot escape into a child.
//!
//! # Why this exists
//!
//! `portable-pty` 0.9.0 opens a terminal in three steps (`src/unix.rs`, `fn openpty`):
//!
//! 1. `libc::openpty(&mut master, &mut slave, …)` — both descriptors come back **without**
//!    `FD_CLOEXEC`,
//! 2. `cloexec(master)`,
//! 3. `cloexec(slave)`,
//!
//! with `ttyname_r` on the slave between 1 and 2. Any fork-and-exec elsewhere in the process
//! between steps 1 and 3 hands the child a copy of that terminal's **slave**, and the exec does
//! not close it. The program charter meant to run in that terminal can then end and the
//! terminal stays open, because a terminal ends when its LAST slave descriptor goes — so the
//! chat's view never closes, for as long as the unrelated child lives (charter-app#53).
//!
//! The programs charter starts *in* a terminal are not the hazard: `portable-pty` gives them a
//! `pre_exec` that calls `close_random_fds()`, which closes everything above 2 in the child.
//! Every plain [`std::process::Command`] is: on unix those take the `posix_spawn` path and
//! close nothing.
//!
//! # The remedy
//!
//! The one Go has used in its runtime for years (`syscall.ForkLock`): a process-wide
//! [`RwLock`] taken for **write** around opening a terminal — until both of its ends are
//! close-on-exec — and for **read** around every fork. Spawns still run in parallel with each
//! other, because that is what a read lock is for; only the window is closed.
//!
//! A lock cannot guard a call that does not take it, so `clippy.toml` disallows
//! [`std::process::Command`]'s own `spawn`, `output` and `status` in this crate. [`spawn`],
//! [`output`] and [`status`] here are the allowed way, and they are the only place the
//! disallowed methods are called.
//!
//! The narrower fix belongs upstream: `openpty` could make both ends close-on-exec as they are
//! created (on Linux, by opening `/dev/ptmx` and the slave with `O_CLOEXEC` rather than calling
//! `openpty(3)`), and then no lock would be needed at all.

use std::io;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{PoisonError, RwLock};

/// Held for write while a terminal is being opened, and for read across every fork.
///
/// It guards no data — what it guards is a moment — so a poisoned lock is taken anyway:
/// there is nothing a panicking thread could have left half-written.
static FORKING: RwLock<()> = RwLock::new(());

/// Opens a terminal with every fork in this process held off until both of its ends are
/// close-on-exec.
///
/// `open` must do nothing but open the terminal. It runs with every other thread's spawn
/// blocked, so anything else put in here is time the rest of charter spends waiting.
pub fn while_a_terminal_is_opened<T>(open: impl FnOnce() -> T) -> T {
    let _held = FORKING.write().unwrap_or_else(PoisonError::into_inner);
    open()
}

/// Starts `command`, as [`Command::spawn`] does, without a terminal half-open anywhere.
#[allow(clippy::disallowed_methods, reason = "this is the one allowed fork")]
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    let _held = FORKING.read().unwrap_or_else(PoisonError::into_inner);
    command.spawn()
}

/// Runs `command` to the end and collects what it wrote, as [`Command::output`] does.
///
/// The lock is released as soon as the program has been started, and never held across the
/// wait: a `git` that takes a minute must not be a minute in which no chat can open a
/// terminal. Both pipes are read while waiting, so neither can fill and stop the program.
///
/// `stdin` is closed, which is what [`Command::output`] does when a caller sets none. A caller
/// that means to feed a program uses [`spawn`] and waits itself.
pub fn output(command: &mut Command) -> io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn(command)?.wait_with_output()
}

/// Runs `command` to the end on charter's own streams, as [`Command::status`] does. The lock
/// is released before the wait, for the reason [`output`] gives.
pub fn status(command: &mut Command) -> io::Result<ExitStatus> {
    spawn(command)?.wait()
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;

    /// Long enough that a thread which is genuinely blocked cannot finish inside it, short
    /// enough that a test which is wrong fails quickly.
    const A_WHILE: Duration = Duration::from_millis(300);
    /// How long a test waits for something that should happen at once.
    const PATIENCE: Duration = Duration::from_secs(10);

    #[test]
    fn nothing_can_be_started_while_a_terminal_is_being_opened() {
        // The whole point. If this ever passes with the read lock in `spawn` deleted, the
        // lock is guarding nothing and charter-app#53 is open again.
        let (started, was_started) = mpsc::channel();
        let (opening, is_open) = mpsc::channel();

        let forker = std::thread::spawn(move || {
            // Waits until the terminal is half-open, then tries to fork.
            is_open.recv().expect("the terminal is being opened");
            let mut sleeper = Command::new("/bin/sleep");
            sleeper.arg("30");
            let child = spawn(&mut sleeper).expect("a program starts");
            started.send(()).expect("somebody is listening");
            child
        });

        // Asked inside the lock, so that what is asserted is what happened while it was held
        // and not what happened after the closure returned.
        let held = while_a_terminal_is_opened(|| {
            opening.send(()).expect("the other thread is waiting");
            was_started.recv_timeout(A_WHILE)
        });

        assert_eq!(
            held,
            Err(mpsc::RecvTimeoutError::Timeout),
            "a program was started while a terminal was half-open, which is the window a \
             child inherits the terminal's slave through (charter-app#53)"
        );
        // And it goes through the moment the terminal is open, rather than being blocked for
        // ever by a lock nobody releases.
        assert_eq!(was_started.recv_timeout(PATIENCE), Ok(()));
        let mut child = forker.join().expect("the thread finishes");
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn waiting_for_a_program_to_finish_does_not_hold_a_terminal_back() {
        // `output` and `status` wait for the program. Waiting under the read lock would mean
        // one slow `git` blocking every chat that wants to open, which is a worse bug than
        // the one this module exists for.
        let (ran, was_run) = mpsc::channel();
        let runner = std::thread::spawn(move || {
            let mut sleeper = Command::new("/bin/sleep");
            sleeper.arg("3");
            let out = output(&mut sleeper);
            ran.send(()).expect("somebody is listening");
            out
        });

        // Long enough for the other thread to have reached its wait, and much shorter than
        // the program it is waiting for.
        std::thread::sleep(Duration::from_millis(200));
        let began = Instant::now();
        while_a_terminal_is_opened(|| ());
        let took = began.elapsed();

        assert!(
            took < A_WHILE,
            "opening a terminal waited {took:?} for a program somebody else was waiting on"
        );
        assert!(was_run.recv_timeout(PATIENCE).is_ok());
        assert!(
            runner.join().expect("the thread finishes").is_ok(),
            "the program still ran"
        );
    }

    #[test]
    fn two_programs_can_start_at_the_same_time() {
        // A read lock, not a mutex: charter opens worktrees, asks git and refreshes a cache
        // at once, and serialising every fork behind one another would be a cost paid on
        // every panel for a window that is microseconds wide.
        let (through, went_through) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();

        let first = std::thread::spawn({
            let through = through.clone();
            move || {
                let _held = FORKING.read().unwrap_or_else(PoisonError::into_inner);
                through.send(1).expect("somebody is listening");
                let _ = released.recv();
            }
        });
        assert_eq!(went_through.recv_timeout(PATIENCE), Ok(1));

        let second = std::thread::spawn(move || {
            let _held = FORKING.read().unwrap_or_else(PoisonError::into_inner);
            through.send(2).expect("somebody is listening");
        });

        assert_eq!(
            went_through.recv_timeout(A_WHILE),
            Ok(2),
            "a second fork waited for the first, which would serialise every spawn charter makes"
        );
        drop(release);
        first.join().expect("the thread finishes");
        second.join().expect("the thread finishes");
    }

    #[test]
    fn what_a_program_wrote_and_how_it_ended_come_back_the_way_the_standard_library_reports_them() {
        let mut echo = Command::new("/bin/sh");
        echo.args(["-c", "printf out; printf err >&2; exit 4"]);

        let out = output(&mut echo).expect("the program runs");

        assert_eq!(out.status.code(), Some(4));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "out");
        assert_eq!(String::from_utf8_lossy(&out.stderr), "err");

        let mut failing = Command::new("/bin/sh");
        failing.args(["-c", "exit 9"]);
        assert_eq!(
            status(&mut failing).expect("the program runs").code(),
            Some(9)
        );
    }
}
