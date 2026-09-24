//! charter-app#53, put under load rather than argued about.
//!
//! `portable-pty` 0.9.0's `openpty` hands back a master and a slave that are **not**
//! close-on-exec, and only makes them so a few syscalls later. A fork elsewhere in the
//! process during that window gives the child a copy of the slave, and the exec does not
//! close it. A terminal ends when its last slave descriptor goes — so a chat's own program
//! can exit and its view stay open for as long as that unrelated child lives.
//!
//! # Why it is shaped like this
//!
//! A hit needs a fork to land inside a window microseconds wide, so the whole test is an
//! argument about one number: **the share of wall-clock time during which some window is
//! open.** A first version opened terminals one after another on a single thread and was
//! measured, against unguarded code, to catch nothing (charter-app#105): `Session::spawn`
//! spends most of its time on the program's own fork and exec, so one thread is inside a
//! window for barely a percent of its life, and three hundred forks found none of them.
//!
//! So the terminals are opened by several threads at once — a window open on any of them is
//! a window a fork can land in — and the forks keep going for exactly as long as terminals
//! are being opened, because the same run showed the two loops drifting apart is enough to
//! prove nothing while looking like a pass.
//!
//! **What this test is worth is what a run against unguarded code says it is worth**, and
//! that run is on the pull request that added it. If it is ever green with
//! `forklock::while_a_terminal_is_opened` reduced to calling its closure, it has not been
//! seen to catch anything and must not be left standing as evidence that the lock is needed.
#![cfg(unix)]

use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::time::{Duration, Instant};

use charter_core::engine::{AlacrittyEngine, Size};
use charter_core::forklock;
use charter_core::session::{Exit, Session, Spec};

const SIZE: Size = Size {
    columns: 80,
    rows: 24,
};

/// How many threads open terminals at once. Not for speed: a window open on any one of them
/// is a window a fork can land in, and this multiplies the share of time that is true of.
const OPENERS: usize = 6;
/// How many terminals one thread opens before asking any of them whether they closed. A burst
/// keeps the openings next to each other; checking between them would spend the time waiting
/// on a shell instead, with every window shut.
const A_BURST: usize = 8;
/// The fewest terminals one thread opens, however quickly the forking finishes.
const AT_LEAST: usize = 40;
/// A stop, so that a machine where the forking never finishes still ends.
const AT_MOST: usize = 4_000;
/// How many programs are started alongside them. Each is one fork, and any one of them may
/// land inside a window; nothing but their number makes that likely.
const OTHER_PROGRAMS: usize = 300;
/// How long one of those lives: longer than the whole run, so a terminal it is holding is
/// still held when it is asked about, and short enough that a run killed outright leaves
/// nothing behind for long.
const HELD_FOR: &str = "60";
/// How long a terminal may take to close after its program has gone. Generous on purpose:
/// three hundred processes are being started on the same machine while this waits.
const PATIENCE: Duration = Duration::from_secs(10);

/// The programs started alongside the terminals, killed however this test ends — including a
/// panic, which is the run that matters.
struct Bystanders(Vec<Child>);

impl Drop for Bystanders {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// One thread's worth of opening terminals, ending when the forking has and it has opened
/// enough of them. Answers with what went wrong, or with how many it opened.
fn open_terminals_until(forking_is_over: &AtomicBool) -> Result<usize, String> {
    let mut opened = 0;
    loop {
        // The sessions are held for the whole burst: dropping one ends its program, and a
        // program charter ended is not a program that ended on its own.
        let burst: Vec<(Session, mpsc::Receiver<Exit>)> = (0..A_BURST)
            .map(|_| {
                let session = Session::spawn(
                    // It exits at once, so everything after this is the terminal closing —
                    // or not closing — and nothing else.
                    Spec::new("/bin/sh", SIZE).args(["-c", "exit 0"]),
                    Box::new(AlacrittyEngine::new(SIZE, 100)),
                )
                .expect("the session starts");
                let (tx, rx) = mpsc::channel();
                session.when_it_ends(Box::new(move |exit| {
                    let _ = tx.send(exit);
                }));
                (session, rx)
            })
            .collect();

        for (session, ended) in burst {
            let began = Instant::now();
            let said = ended.recv_timeout(PATIENCE);
            opened += 1;
            if said.is_err() {
                return Err(format!(
                    "a terminal never closed: its program exited, and {:?} later the terminal \
                     was still open. Something else charter started is holding its slave, \
                     which is the window in portable-pty's `openpty` (charter-app#53). It was \
                     terminal {opened} on this thread.",
                    began.elapsed()
                ));
            }
            drop(session);
        }

        if opened >= AT_MOST || (opened >= AT_LEAST && forking_is_over.load(Ordering::SeqCst)) {
            return Ok(opened);
        }
    }
}

#[test]
fn no_program_started_while_a_terminal_opens_can_hold_that_terminal_open() {
    charter_core::unsteered!();
    let forking_is_over = Arc::new(AtomicBool::new(false));
    // Everything starts together, and the openers keep going until the forking is done, so
    // that every fork is made while terminals are being opened. A run whose forks all
    // happened before or after the openings would prove nothing and look like a pass.
    let together = Arc::new(Barrier::new(OPENERS + 1));

    let forking = std::thread::spawn({
        let together = Arc::clone(&together);
        let forking_is_over = Arc::clone(&forking_is_over);
        move || {
            let mut started = Bystanders(Vec::new());
            together.wait();
            for _ in 0..OTHER_PROGRAMS {
                // No pause between them. A fork has to land inside a window microseconds
                // wide, and the only thing that makes that likely is how many forks there
                // are while the windows are open.
                match forklock::spawn(Command::new("/bin/sleep").arg(HELD_FOR)) {
                    Ok(child) => started.0.push(child),
                    // A machine that will not start another process is not a finding about
                    // charter.
                    Err(_) => break,
                }
            }
            forking_is_over.store(true, Ordering::SeqCst);
            started
        }
    });

    let openers: Vec<_> = (0..OPENERS)
        .map(|_| {
            std::thread::spawn({
                let together = Arc::clone(&together);
                let forking_is_over = Arc::clone(&forking_is_over);
                move || {
                    together.wait();
                    open_terminals_until(&forking_is_over)
                }
            })
        })
        .collect();

    let mut terminals = 0;
    let mut trouble = Vec::new();
    for opener in openers {
        match opener.join().expect("the thread finishes") {
            Ok(opened) => terminals += opened,
            Err(why) => trouble.push(why),
        }
    }
    // Dropped, which is what kills the three hundred programs — and only now, because
    // killing them earlier would be killing the evidence.
    drop(forking.join().expect("the forking thread finishes"));

    assert!(trouble.is_empty(), "{}", trouble.join("\n"));
    assert!(
        terminals >= OPENERS * AT_LEAST,
        "only {terminals} terminals were opened, which is too few to have asked the question"
    );
}
