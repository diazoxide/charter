//! The race charter-app#81 lost, and the two halves of the answer.
//!
//! Read `src/lib.rs` first: `ETXTBSY` belongs to the inode, not to the name, and the two
//! directions it bites from are different bugs.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The positive control, and the reason the test below is worth anything.
///
/// It builds by hand the state a fork leaves behind — one other process holding a writable
/// descriptor on the program's inode — and shows that `execve` then refuses, **after** the
/// rename that charter-app#81 proposed as the fix. Nothing here touches `stand_in::program`:
/// this is the kernel's behaviour, pinned, so that a green suite means the window was closed
/// rather than that this platform stopped caring.
///
/// If this ever goes green — a kernel that does not check `i_writecount` — then the race test
/// below proves nothing and has to be replaced, not relaxed.
#[test]
fn a_descriptor_another_process_holds_is_text_file_busy_and_a_rename_does_not_free_it() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let written = dir.path().join("prog.writing");
    let renamed = dir.path().join("prog");
    std::fs::write(&written, "#!/bin/sh\nexit 0\n").expect("the program is written");
    std::fs::set_permissions(&written, std::fs::Permissions::from_mode(0o755)).expect("runnable");

    // A child holding the program open for writing, exactly as a child forked while this
    // thread was writing it would. Standard output is the one descriptor a child keeps
    // across its own `execve`, which is what makes this deterministic instead of a race.
    let holder = std::fs::OpenOptions::new()
        .write(true)
        .open(&written)
        .expect("the program opens for writing");
    let mut sleeping = Command::new("/bin/sleep");
    sleeping.arg("30").stdout(Stdio::from(holder));
    let mut sleeper = sleeping.spawn().expect("/bin/sleep runs");
    // The `Command` owns this process's copy of the descriptor until it is dropped. Dropped
    // here, so what is left is the child's copy alone — the thing a rename cannot reach.
    drop(sleeping);

    std::fs::rename(&written, &renamed).expect("the program is renamed into place");
    let answer = Command::new(&renamed).status();

    let refused = answer.expect_err(
        "exec of a program another process holds open for writing was allowed, so the kernel \
         no longer answers ETXTBSY here and the race test below is no longer evidence",
    );
    assert_eq!(
        refused.kind(),
        std::io::ErrorKind::ExecutableFileBusy,
        "expected Text file busy, got {refused:?}"
    );

    let _ = sleeper.kill();
    let _ = sleeper.wait();
}

/// The race, run enough times that the old spelling would realistically have lost it.
///
/// **This is evidence, not a guarantee.** A green run says this shape survived a few hundred
/// rounds against four threads forking as fast as the machine allows; it cannot say the
/// window is closed, because no number of green rounds can. The argument that it is closed is
/// the one in `src/lib.rs` — this process never opens the program for writing, so there is no
/// descriptor for a fork to copy — and this test is what would catch that argument being
/// wrong in practice. Written the old way (`fs::write` then run it), it goes red.
#[test]
fn a_stand_in_runs_every_round_while_other_threads_fork() {
    const ROUNDS: usize = 250;
    const FORKERS: usize = 4;

    let dir = tempfile::tempdir().expect("a temp dir");
    let stop = Arc::new(AtomicBool::new(false));
    // Every `Command::spawn` is a fork, and a fork copies this process's descriptor table.
    // These threads are the rest of the test suite: nothing they run matters, only that they
    // fork constantly while the program below is being created.
    let forkers: Vec<_> = (0..FORKERS)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = Command::new("/bin/echo")
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
            })
        })
        .collect();

    let mut ran = 0usize;
    for round in 0..ROUNDS {
        let prog = stand_in::program(dir.path(), &format!("prog-{round}"), "#!/bin/sh\nexit 7\n");
        let status = Command::new(&prog)
            .status()
            .unwrap_or_else(|e| panic!("round {round}: the stand-in could not be run: {e}"));
        assert_eq!(
            status.code(),
            Some(7),
            "round {round}: the stand-in ran but not as written"
        );
        ran += 1;
    }

    stop.store(true, Ordering::Relaxed);
    for forker in forkers {
        forker.join().expect("a forking thread ends");
    }
    assert_eq!(ran, ROUNDS);
}

/// The bytes are the caller's, byte for byte — the write goes through `/bin/sh`, and a
/// stand-in is full of the characters a shell would otherwise eat.
#[test]
fn a_stand_in_holds_exactly_the_bytes_it_was_given() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let awkward = "#!/bin/sh\nprintf '100%%\\n' \"$*\" # $HOME `date` \\ \"'\n";

    let prog = stand_in::program(dir.path(), "awkward", awkward);

    assert_eq!(
        std::fs::read_to_string(&prog).expect("it is there"),
        awkward
    );
    assert_eq!(
        std::fs::metadata(&prog)
            .expect("it is there")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    // And nothing is left beside it: a leftover `.awkward.…writing` in a plane's root would
    // be read by charter as part of the plane.
    let mut left: Vec<String> = std::fs::read_dir(dir.path())
        .expect("the directory is read")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    left.sort();
    assert_eq!(left, ["awkward"]);
}

/// A name with a directory in it lands where it was asked for, with the temporary file beside
/// it rather than in the caller's directory — `git` hooks are written as `hooks/pre-receive`.
#[test]
fn a_stand_in_under_a_subdirectory_is_written_there() {
    let dir = tempfile::tempdir().expect("a temp dir");
    std::fs::create_dir(dir.path().join("hooks")).expect("a hooks directory");

    let prog = stand_in::program(dir.path(), "hooks/pre-receive", "#!/bin/sh\nexit 3\n");

    assert_eq!(prog, dir.path().join("hooks/pre-receive"));
    assert_eq!(
        Command::new(&prog).status().expect("it runs").code(),
        Some(3)
    );
}
