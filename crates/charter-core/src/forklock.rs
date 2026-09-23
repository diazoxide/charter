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
/// `open` must do nothing but open the terminal, and **must never start a program**. It runs
/// with every other thread's spawn blocked, so anything else put in here is time the rest of
/// charter spends waiting — and a [`spawn`] inside it would ask this same lock for a read
/// while this thread holds it for write, which [`RwLock`] answers by never returning.
pub fn while_a_terminal_is_opened<T>(open: impl FnOnce() -> T) -> T {
    while_descriptors_are_made(open)
}

/// Makes descriptors with every fork in this process held off until they are close-on-exec —
/// [`while_a_terminal_is_opened`]'s lock, for anything else that is made in two steps.
///
/// **A socket on macOS is.** The standard library's `UnixStream::pair` there is `socketpair(2)`
/// and then an `ioctl(FIOCLEX)` on each end, because Darwin has no `SOCK_CLOEXEC`; a spawn on
/// another thread between the two hands its child both ends. For the executor's pair that child
/// could be a *different* extension's program, holding a channel charter promised was one
/// extension's alone (`crate::executor`). The window is not theoretical: programs started
/// beside a thread making pairs outside this lock inherited one in 112 of 300 runs, and 141 of
/// 300 on another (charter-app#212's review). Linux makes the pair close-on-exec in the one
/// call, and the lock costs it a microsecond.
///
/// The same rule as the terminal's: `make` makes descriptors and nothing else, and never starts
/// a program.
pub fn while_descriptors_are_made<T>(make: impl FnOnce() -> T) -> T {
    let _held = FORKING.write().unwrap_or_else(PoisonError::into_inner);
    make()
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
pub(crate) mod tests {
    use std::sync::{Mutex, MutexGuard, mpsc};
    use std::time::{Duration, Instant};

    use super::*;

    /// Long enough that a thread which is genuinely blocked cannot finish inside it, short
    /// enough that a test which is wrong fails quickly.
    const A_WHILE: Duration = Duration::from_millis(300);
    /// How long a test waits for something that should happen at once.
    const PATIENCE: Duration = Duration::from_secs(10);

    /// The three tests below are each about what [`FORKING`] does to a thread that wants it,
    /// and [`FORKING`] is one lock for the whole process — so run two of them at once and
    /// each is measuring the other rather than the subject (charter-app#183).
    ///
    /// They deadlocked each other by construction, not by bad luck: `two_programs_…` holds a
    /// read lock for `A_WHILE`, which blocks the write lock `waiting_for_…` is timing, and a
    /// queued writer is what [`RwLock`] then makes `two_programs_…`'s second reader wait for.
    /// Both failed on every one of five runs of the whole crate on macOS, and the wait that
    /// came out — a little over `A_WHILE` — reads exactly like a real regression in the lock.
    ///
    /// So they are serialised against each other and against nothing else: `--test-threads=1`
    /// would buy the same thing by giving up the other twelve hundred tests' parallelism.
    /// Every other thread in this binary only ever takes the lock for READ, for the microsecond
    /// a `spawn` takes, which is not something any bar below can notice.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// Hold [`FORKING`] as a fork does, for a test elsewhere that asks what waits for one.
    /// Taken under [`alone`], for the reason [`ONE_AT_A_TIME`] gives.
    pub(crate) fn as_a_fork_does() -> std::sync::RwLockReadGuard<'static, ()> {
        FORKING.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Held for as long as one of the three tests is using [`FORKING`].
    ///
    /// The poison is taken rather than unwrapped, for [`FORKING`]'s own reason: it guards no
    /// data, so a test that panicked left nothing half-written — and a poisoned unwrap here
    /// would report the first failure again in the next two tests instead of their own.
    pub(crate) fn alone() -> MutexGuard<'static, ()> {
        ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[test]
    fn nothing_can_be_started_while_a_terminal_is_being_opened() {
        let _alone = alone();
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
        let _alone = alone();
        // `output` and `status` wait for the program. Waiting under the read lock would mean
        // one slow `git` blocking every chat that wants to open, which is a worse bug than
        // the one this module exists for.
        //
        // **The program ends when this test says so, not on a clock** (charter-app#183).
        // It was a `/bin/sleep 3` measured against a bar of `A_WHILE`, and a bar is a
        // statement about the machine as much as about the lock: it read 399 ms against 400
        // on a busy laptop and looked exactly like a regression in the lock. What is claimed
        // here is an ORDER — the terminal opened while somebody was still waiting on a
        // program — so the program is one that cannot finish until the line far below writes
        // the file it is waiting for. Held under the read lock, opening would wait for a
        // program that is waiting for opening: a hang with no end, which no machine is fast
        // enough to squeak under. `PATIENCE` is what turns that hang into a failure with a
        // sentence on it.
        let dir = tempfile::tempdir().expect("a temp dir");
        let running = dir.path().join("the-program-is-running");
        let finish = dir.path().join("the-program-may-finish");
        let mut waiter = Command::new("/bin/sh");
        waiter
            .arg("-c")
            .arg(r#": > "$1"; until [ -e "$2" ]; do sleep 0.01; done"#)
            .arg("sh")
            .arg(&running)
            .arg(&finish);

        let (ran, was_run) = mpsc::channel();
        let runner = std::thread::spawn(move || {
            let out = output(&mut waiter);
            ran.send(()).expect("somebody is listening");
            out
        });

        // Not a guess at how long the other thread needs: the program says when it is up,
        // and it says so from inside its own run, so by the time this returns `output` is
        // past its fork and into the wait this test is about.
        let up = Instant::now();
        while !running.exists() {
            assert!(up.elapsed() < PATIENCE, "the program never started");
            std::thread::sleep(Duration::from_millis(5));
        }

        // On its own thread, because the failure being caught is opening never returning,
        // and a test that hangs says less than one that fails.
        let (opened, was_opened) = mpsc::channel();
        let opener = std::thread::spawn(move || {
            while_a_terminal_is_opened(|| ());
            opened.send(()).expect("somebody is listening");
        });

        assert_eq!(
            was_opened.recv_timeout(PATIENCE),
            Ok(()),
            "opening a terminal is still waiting for a program somebody else is waiting on, \
             and that program is waiting for this test — so the wait is being done under the \
             lock (charter-app#53's fix turned into a worse bug)"
        );
        // And the program really was still running while that happened, rather than having
        // finished early and left the lock free for reasons that prove nothing.
        assert_eq!(
            was_run.try_recv(),
            Err(mpsc::TryRecvError::Empty),
            "the program ended before this test let it, so this run is not evidence"
        );

        std::fs::write(&finish, []).expect("the program is let go");
        opener.join().expect("the thread finishes");
        assert!(was_run.recv_timeout(PATIENCE).is_ok());
        assert!(
            runner.join().expect("the thread finishes").is_ok(),
            "the program still ran"
        );
    }

    #[test]
    fn two_programs_can_start_at_the_same_time() {
        let _alone = alone();
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
