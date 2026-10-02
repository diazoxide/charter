//! A fixture process ends when the test that started it ends, however it ends (#923).
//!
//! The leak this answers: a test starts a program that is built to survive a hangup, the
//! test fails or the binary exits before the reaper thread's kill lands, and the program is
//! still there days later under pid 1. A [`stand_in::Ends`] guard kills the whole process
//! group synchronously when it drops — on the way out of a passing test, and on the unwind
//! out of a failing one.
#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A program leading a group of its own that ignores INT and HUP, with a helper of its own in
/// that group, and the helper's pid.
fn a_group_that_will_not_go_quietly(dir: &Path) -> (std::process::Child, u32) {
    use std::os::unix::process::CommandExt as _;
    let helper = dir.join("helper");
    let child = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!(
            "trap '' INT HUP; sleep 30 & echo $! > '{}.part' && mv '{0}.part' '{0}'; wait",
            helper.display()
        ))
        .stdin(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("/bin/sh runs");
    let deadline = Instant::now() + Duration::from_secs(30);
    let pid = loop {
        if let Some(pid) = std::fs::read_to_string(&helper)
            .ok()
            .and_then(|t| t.trim().parse().ok())
        {
            break pid;
        }
        assert!(Instant::now() < deadline, "the helper never said its pid");
        std::thread::sleep(Duration::from_millis(10));
    };
    (child, pid)
}

/// Whether `pid` is gone within a few seconds — a killed child whose parent is gone is a
/// zombie until init reaps it, and a zombie still answers `kill -0`.
fn gone(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Command::new("/bin/kill")
        .arg("-0")
        .arg(pid.to_string())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
    {
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

#[test]
fn a_guarded_group_is_killed_when_the_test_panics() {
    let dir = tempfile::tempdir().expect("a directory");
    let (mut child, helper) = a_group_that_will_not_go_quietly(dir.path());
    let leader = child.id();

    let unwound = std::panic::catch_unwind(|| {
        let _ends = stand_in::Ends::group(leader);
        panic!("the test failed with its fixture still running");
    });

    assert!(unwound.is_err());
    // Asked before the leader is waited for: waiting first would wait the helper out too.
    assert!(gone(helper), "the helper in the group outlived the panic");
    let _ = child.wait();
}

#[test]
fn a_group_named_in_a_marker_is_killed_by_the_pid_written_there() {
    // The program writes its own pid; the test only learns it later, or never, if it fails
    // first. The guard reads the marker when it drops.
    let dir = tempfile::tempdir().expect("a directory");
    let (mut child, helper) = a_group_that_will_not_go_quietly(dir.path());
    let marker = dir.path().join("leader");
    std::fs::write(&marker, child.id().to_string()).expect("the marker");

    drop(stand_in::Ends::named_in(&marker));

    assert!(gone(helper), "the helper outlived the guard");
    let _ = child.wait();
}

#[test]
fn a_marker_never_written_or_holding_no_pid_is_nothing_to_kill() {
    use std::os::unix::process::CommandExt as _;
    let dir = tempfile::tempdir().expect("a directory");
    let mut bystander = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("sleep runs");
    let _ours = stand_in::Ends::group(bystander.id());
    let garbled = dir.path().join("garbled");
    std::fs::write(&garbled, "not a pid").expect("the marker");

    drop(stand_in::Ends::named_in(dir.path().join("never")));
    drop(stand_in::Ends::named_in(&garbled));

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        bystander.try_wait().expect("a status").is_none(),
        "a guard with no pid to read killed something"
    );
}

#[test]
fn a_stubborn_fixture_ignores_a_hangup_and_still_ends_on_its_own() {
    use std::os::unix::process::CommandExt as _;
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(stand_in::stubborn_for("INT HUP", "", 1))
        .stdin(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("/bin/sh runs");
    let _ends = stand_in::Ends::group(child.id());
    std::thread::sleep(Duration::from_millis(200));
    let _ = Command::new("/bin/kill")
        .args(["-HUP", &child.id().to_string()])
        .status();
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        child.try_wait().expect("a status").is_none(),
        "the fixture did not survive the hangup it is meant to survive"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().expect("a status").is_none() {
        assert!(Instant::now() < deadline, "the fixture outlived its bound");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_pid_in_a_marker_older_than_its_process_is_someone_else_s_and_is_left_alone() {
    // A pid the kernel has handed out again since the fixture wrote it: the process holding it
    // now started after the marker was written. Killing it would kill something this test
    // never started — on a shared machine, another session's work.
    use std::os::unix::process::CommandExt as _;
    let dir = tempfile::tempdir().expect("a directory");
    let mut someone_else = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("sleep runs");
    let _ours = stand_in::Ends::group(someone_else.id());
    let marker = dir.path().join("pid");
    std::fs::write(&marker, someone_else.id().to_string()).expect("the marker");
    let in_2020 = Command::new("/usr/bin/touch")
        .args(["-t", "202001010000"])
        .arg(&marker)
        .status()
        .expect("touch runs");
    assert!(in_2020.success());

    drop(stand_in::Ends::named_in(&marker));

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        someone_else.try_wait().expect("a status").is_none(),
        "a process that started after the marker was written was killed"
    );
}

#[test]
fn a_guard_whose_pid_was_handed_to_another_process_leaves_that_one_alone() {
    // The same, for a guard handed the pid directly: it remembers when that process started,
    // and a different start is a different process.
    use std::os::unix::process::CommandExt as _;
    let mut child = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("sleep runs");
    let pid = child.id();
    let _ours = stand_in::Ends::group(pid);

    drop(stand_in::Ends::started_at(pid, 0));

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        child.try_wait().expect("a status").is_none(),
        "the guard killed a process that started later than the one it was made for"
    );
}

/// Now, in seconds since the epoch.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs()
}

#[test]
fn a_guard_that_cannot_ask_when_a_live_process_started_kills_nothing() {
    // `ps` would not start: out of processes or descriptors, or not there at all. The pid is
    // held by SOME process, and with no way to tell whether it is still the fixture, the guard
    // leaves it — a leak is a CI failure, a wrong kill is someone else's work gone.
    use std::os::unix::process::CommandExt as _;
    let mut child = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("sleep runs");
    let pid = child.id();
    let _ours = stand_in::Ends::group(pid);

    drop(stand_in::Ends::started_at(pid, now()).asking_ps("/definitely/not/ps"));

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        child.try_wait().expect("a status").is_none(),
        "a guard that could not read the start time killed the process anyway"
    );
}
