//! charter-app#53, put under load rather than argued about.
//!
//! `portable-pty` 0.9.0's `openpty` hands back a master and a slave that are **not**
//! close-on-exec, and only makes them so a few syscalls later. A fork elsewhere in the
//! process during that window gives the child a copy of the slave, and the exec does not
//! close it. A terminal ends when its last slave descriptor goes — so a chat's own program
//! can exit and its view stay open for as long as that unrelated child lives.
//!
//! The window is a few syscalls wide, so it cannot be hit on demand. What it can be is
//! hammered: terminals are opened in bursts while another thread starts one long-lived
//! program after another, and every terminal is then held to the promise the app depends on
//! — that `when_it_ends` fires once the program has gone. A terminal something else is
//! holding open does not fire, and the failure says which one.
//!
//! **This test is worth exactly what a run against unguarded code says it is worth.** If it
//! passes with `forklock::while_a_terminal_is_opened` reduced to calling its closure, then it
//! has not been seen to catch anything and must not be left standing as evidence that the
//! lock is needed — make it harsher or say plainly that the window was never reproduced.
#![cfg(unix)]

use std::process::{Child, Command};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use charter_core::engine::{AlacrittyEngine, Size};
use charter_core::forklock;
use charter_core::session::{Exit, Session, Spec};

const SIZE: Size = Size {
    columns: 80,
    rows: 24,
};

/// How many terminals are opened before any of them is asked about. The burst is the point:
/// it is the shape that spends most of its time inside `openpty`, which is where the window
/// is. Asking about each one as it opened would spend that time waiting on a shell instead.
const A_BURST: usize = 25;
/// How many bursts — two hundred and fifty terminals before anything is concluded.
const BURSTS: usize = 10;
/// How many programs are started alongside them. Each is one fork, and any one of them may
/// land inside a window; nothing but their number makes that likely.
const OTHER_PROGRAMS: usize = 300;
/// How long one of those programs lives: longer than the whole run, so a terminal it is
/// holding is still being held when it is asked about, and short enough that a run killed
/// outright leaves nothing behind for long.
const HELD_FOR: &str = "45";
/// How long a terminal may take to close after its program has gone. Generous on purpose:
/// three hundred processes are being started on the same machine while this waits.
const PATIENCE: Duration = Duration::from_secs(20);

/// The programs started alongside the terminals, killed however this test ends — including
/// a panic, which is the run that matters.
struct Bystanders(Vec<Child>);

impl Drop for Bystanders {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn no_program_started_while_a_terminal_opens_can_hold_that_terminal_open() {
    // Both sides start together, so that every fork is made while terminals are being
    // opened. A run whose forks all happened before the first `openpty` would prove nothing
    // and would look exactly like a pass.
    let together = Arc::new(Barrier::new(2));
    let (stop, when_to_stop) = mpsc::channel::<()>();
    let bystanders = std::thread::spawn({
        let together = Arc::clone(&together);
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
                if matches!(
                    when_to_stop.try_recv(),
                    Err(mpsc::TryRecvError::Disconnected)
                ) {
                    return started;
                }
            }
            // Kept alive until every terminal has been asked about: killing them earlier
            // would be killing the evidence.
            let _ = when_to_stop.recv();
            started
        }
    });

    let mut asked = 0;
    together.wait();
    for burst in 0..BURSTS {
        // The sessions are held for the whole burst. Dropping one ends its program, and a
        // program charter ended is not a program that ended on its own.
        let opened: Vec<(usize, Session, mpsc::Receiver<Exit>)> = (0..A_BURST)
            .map(|which| {
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
                (burst * A_BURST + which, session, rx)
            })
            .collect();

        for (which, session, ended) in opened {
            let began = Instant::now();
            let said = ended.recv_timeout(PATIENCE);
            let waited = began.elapsed();
            let total = BURSTS * A_BURST;
            asked += 1;
            assert!(
                said.is_ok(),
                "terminal {which} of {total} never closed: its program exited, and {waited:?} \
                 later the terminal was still open. Something else charter started is holding \
                 its slave, which is the window in portable-pty's `openpty` (charter-app#53).",
            );
            drop(session);
        }
    }

    assert_eq!(asked, BURSTS * A_BURST, "every terminal was asked about");
    // Only now: dropping this is what lets the other thread hand the programs back, and
    // dropping those is what kills them.
    drop(stop);
    drop(bystanders.join().expect("the other thread finishes"));
}
