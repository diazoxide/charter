//! The race charter-app#81 lost, and the two halves of the answer.
//!
//! Read `src/lib.rs` first: `ETXTBSY` belongs to the inode, not to the name, the two
//! directions it bites from are different bugs, and Darwin has stopped answering it at all —
//! which is what decides, below, which of these tests is evidence on which kernel
//! (charter-app#184).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Builds by hand the state a fork leaves behind — one other process holding a writable
/// descriptor on the program's own inode — and hands back what the kernel then did about
/// running it.
///
/// The situation, not the verdict: the two tests below read it differently because the two
/// kernels charter is built for answer it differently, and `main` has to be able to hold
/// both (charter-app#184).
///
/// `exec_as` is where the program is moved to once the descriptor is held, and the name it is
/// then run under. Passing it the program's own path is the plain shape; passing it another
/// name is the half of charter-app#81 that a rename was wrongly expected to fix, because what
/// the holder holds is the inode and a rename hands `execve` that same inode.
fn what_happens_when_another_process_holds_it_open_for_writing(
    program: &std::path::Path,
    exec_as: &std::path::Path,
) -> std::io::Result<std::process::ExitStatus> {
    // Standard output is the one descriptor a child keeps across its own `execve`, which is
    // what makes this deterministic instead of a race.
    let holder = std::fs::OpenOptions::new()
        .write(true)
        .open(program)
        .expect("the program opens for writing");
    let mut sleeping = Command::new("/bin/sleep");
    sleeping.arg("30").stdout(Stdio::from(holder));
    let mut sleeper = sleeping.spawn().expect("/bin/sleep runs");
    // The `Command` owns this process's copy of the descriptor until it is dropped. Dropped
    // here, so what is left is the child's copy alone — the thing a rename cannot reach.
    drop(sleeping);

    if exec_as != program {
        std::fs::rename(program, exec_as).expect("the program is renamed into place");
    }
    let answer = Command::new(exec_as).status();

    let _ = sleeper.kill();
    let _ = sleeper.wait();
    answer
}

/// A real program rather than a shell script, on every machine this suite runs on: the test
/// binary itself.
///
/// It used to be a copy of `/bin/echo`, and cannot be any more (charter-app#184). Every
/// executable macOS ships is an `x86_64 arm64e` fat binary, and `arm64e` code runs only as one
/// of Apple's own platform binaries — so on Apple silicon a copy of one is `SIGKILL`ed the
/// moment it is exec'd, with nothing holding it open, however it was written, and whether or
/// not it is re-signed ad hoc afterwards. A binary this build produced carries no such rule,
/// and it needs no particular `/bin` to exist.
fn this_binary() -> std::path::PathBuf {
    std::env::current_exe().expect("a test knows the binary it is running in")
}

/// What a copy of [`this_binary`] is ever asked to do: name its tests and stop.
///
/// Asked anything else — asked nothing at all — it would run this file again from inside this
/// file, which is a fork bomb with a temp directory attached.
fn list_the_tests_in(program: &std::path::Path) -> std::process::Output {
    Command::new(program)
        .args(["--list", "--format", "terse"])
        .output()
        .expect("the copy is runnable at all")
}

/// A `#!` script at `dir/name`, written the plain way rather than through `stand_in`: these
/// two tests are about the kernel, and a stand-in written safely is the thing being argued
/// about rather than a tool to argue with.
fn a_script(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 7\n").expect("the program is written");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("runnable");
    path
}

/// The positive control, and the reason the race test below is worth anything **here**.
///
/// It shows that `execve` refuses a program another process holds open for writing, **after**
/// the rename that charter-app#81 proposed as the fix. Nothing here touches
/// `stand_in::program`: this is the kernel's behaviour, pinned, so that a green suite means
/// the window was closed rather than that this platform stopped caring.
///
/// Darwin is no longer one of these platforms; the test below it says what it does instead,
/// and why that is a fact about the kernel and not about this test's setup.
#[cfg(not(target_os = "macos"))]
#[test]
fn a_descriptor_another_process_holds_is_text_file_busy_and_a_rename_does_not_free_it() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let written = a_script(dir.path(), "prog.writing");
    let renamed = dir.path().join("prog");

    let answer = what_happens_when_another_process_holds_it_open_for_writing(&written, &renamed);

    let refused = answer.expect_err(
        "exec of a program another process holds open for writing was allowed, so the kernel \
         no longer answers ETXTBSY here and the race test below is no longer evidence",
    );
    assert_eq!(
        refused.kind(),
        std::io::ErrorKind::ExecutableFileBusy,
        "expected Text file busy, got {refused:?}"
    );
}

/// What Darwin does instead, measured on macOS 26.2 (Darwin 25.2) and pinned here because the
/// answer decides what the race test below is worth (charter-app#184).
///
/// **The setup was checked before the kernel was blamed**, since this repo has been wrong about
/// exactly that before: `ETXTBSY` belongs to the inode and not to the name, and a
/// write-to-temp-then-rename fix for charter-app#81 was disproven on CI at round 20 of 250. So
/// the question asked first was whether a held descriptor on the exact inode being exec'd is
/// still what this builds. It is — `lsof` on the holding child shows it open `1w` on the same
/// inode number the exec resolves to, and the exec is allowed with the rename taken out
/// altogether. The situation is right; the answer has changed.
///
/// And it has changed into something worse rather than into nothing:
///
/// - **A `#!` script is simply allowed.** What `execve` loads is `/bin/sh`, and the script is
///   an argument, so the file somebody holds open is never the image being validated. This is
///   the shape every `stand_in::program` makes, so the race below is no evidence on Darwin and
///   is retired there in the same commit as this test.
/// - **A Mach-O that has already been run once is killed.** Not `ETXTBSY` — `SIGKILL`, with no
///   errno for a caller to read, because the writable open invalidates the code signature the
///   kernel cached for that vnode at the first exec. A Mach-O that has never been exec'd runs
///   (12 rounds each way, no exceptions), which is why both halves are pinned: the first-exec
///   case is the one that would quietly stop being enforced.
///
/// The second is the live hazard on this platform, and it is `stand_in::copy_of`'s: charter
/// writes a stand-in `claude`, a chat runs it, the next test writes it again.
#[cfg(target_os = "macos")]
#[test]
fn darwin_allows_a_held_script_and_kills_a_held_program_rather_than_answering_text_file_busy() {
    use std::os::unix::process::ExitStatusExt as _;

    let dir = tempfile::tempdir().expect("a temp dir");

    let script = a_script(dir.path(), "script");
    let ran = what_happens_when_another_process_holds_it_open_for_writing(&script, &script)
        .expect("Darwin allows the exec of a held script");
    assert_eq!(
        ran.code(),
        Some(7),
        "Darwin answered something other than 'it just runs' for a script another process \
         holds open for writing, so the ground under the race test has moved again and the \
         retirement below has to be read again with it"
    );

    // A real program rather than a script, and one this build made rather than one macOS
    // ships, for the reason [`this_binary`] gives.
    let program = stand_in::copy_of(&this_binary(), dir.path(), "program");
    // Exec'd once, plainly, which is what puts its signature in the kernel's cache — and the
    // difference between a run and a kill below.
    assert!(
        list_the_tests_in(&program).status.success(),
        "the copy runs at all"
    );

    let killed = what_happens_when_another_process_holds_it_open_for_writing(&program, &program)
        .expect("Darwin allows the exec and kills what it started");
    assert_eq!(
        killed.signal(),
        Some(9),
        "Darwin let a program run whose inode another process holds open for writing, after \
         having validated that program once — so the last thing keeping `copy_of`'s discipline \
         load-bearing on this platform has gone too, and charter-app#39 is open again"
    );
}

/// The race, run enough times that the old spelling would realistically have lost it.
///
/// **This is evidence, not a guarantee.** A green run says this shape survived a few hundred
/// rounds against four threads forking as fast as the machine allows; it cannot say the
/// window is closed, because no number of green rounds can. The argument that it is closed is
/// the one in `src/lib.rs` — this process never opens the program for writing, so there is no
/// descriptor for a fork to copy — and this test is what would catch that argument being
/// wrong in practice. Written the old way (`fs::write` then run it), it goes red.
///
/// **Retired on macOS** (charter-app#184). What it races is a `#!` script, and Darwin no longer
/// refuses to exec one somebody holds open for writing — the test above measures that and
/// pins it. Seventy seconds of every local `cargo test` were going into a green that had
/// stopped meaning anything, and a green that means nothing is worse than an absence: the
/// issue was raised precisely because this one still read as evidence. It keeps running on
/// Linux, which is the kernel that still answers `ETXTBSY` and the one CI gates on.
#[cfg_attr(
    target_os = "macos",
    ignore = "Darwin no longer answers ETXTBSY for a script (charter-app#184), so this proves \
              nothing here; the test above pins what it does instead"
)]
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

/// A copy of a real binary under another name, for the tests that need a program which is not
/// a shell script — charter reads the file NAME to decide which harness a chat is.
///
/// What is copied is this test's own binary and no longer `/bin/echo`, for the reason
/// [`this_binary`] gives (charter-app#184). The copy is asked to name its tests, and naming
/// this one is how a run that is really the program it was copied from is told from a run that
/// merely exited zero.
#[test]
fn a_copied_program_runs_under_its_new_name() {
    let dir = tempfile::tempdir().expect("a temp dir");

    let prog = stand_in::copy_of(&this_binary(), dir.path(), "claude");

    assert_eq!(prog, dir.path().join("claude"));
    let out = list_the_tests_in(&prog);
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("a_copied_program_runs_under_its_new_name"),
        "the copy ran, but it is not the program it was copied from: {out:?}"
    );
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
