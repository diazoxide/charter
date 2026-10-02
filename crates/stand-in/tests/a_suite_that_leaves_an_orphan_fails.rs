//! `tools/no-orphans.sh`, the CI step that fails a suite which leaves a process behind (#923).
#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

fn the_check() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/no-orphans.sh")
}

#[test]
fn a_command_that_leaves_a_process_behind_fails_and_names_it() {
    // A sleep with a marker in its argument, so its line in the report can be told apart from
    // anything else this user started meanwhile. Not `--reap`: on a shared machine that would
    // kill those too. The test kills its own sleep, by the pid the report names.
    let said = Command::new(the_check())
        .args(["--grace", "1", "--", "/bin/sh", "-c"])
        .arg("sleep 47.923 </dev/null >/dev/null 2>&1 & exit 0")
        .output()
        .expect("the check runs");

    let stderr = String::from_utf8_lossy(&said.stderr);
    let orphan = stderr.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("pid ")?;
        let (pid, cmd) = rest.split_once(": ")?;
        cmd.contains("sleep 47.923")
            .then(|| pid.parse::<u32>().ok())?
    });
    if let Some(pid) = orphan {
        drop(stand_in::Ends::group(pid));
    }
    assert_eq!(said.status.code(), Some(1), "{stderr}");
    assert!(orphan.is_some(), "the orphan was not named: {stderr}");
}

#[test]
fn a_command_that_cleans_up_after_itself_keeps_its_own_status() {
    let said = Command::new(the_check())
        .args([
            "--grace",
            "0",
            "--",
            "/bin/sh",
            "-c",
            "sleep 0.1 & wait; exit 3",
        ])
        .output()
        .expect("the check runs");

    let stderr = String::from_utf8_lossy(&said.stderr);
    // Anything else this user started meanwhile would be counted too, which is why CI runs it
    // on a machine of its own; here, only a report that names this command's own work fails.
    if said.status.code() == Some(1) {
        assert!(
            !stderr
                .lines()
                .any(|line| line.trim().starts_with("pid ") && line.contains("sleep 0.1")),
            "{stderr}"
        );
    } else {
        assert_eq!(said.status.code(), Some(3), "{stderr}");
    }
}

#[test]
fn a_check_that_cannot_list_the_processes_fails_rather_than_passing_blind() {
    // A `ps` that fails, or answers nothing, must not read as "nothing was left behind".
    let dir = tempfile::tempdir().expect("a directory");
    for (name, ps) in [
        ("failing", "#!/bin/sh\nexit 1\n"),
        ("silent", "#!/bin/sh\nexit 0\n"),
    ] {
        let bin = dir.path().join(name);
        std::fs::create_dir(&bin).expect("a bin directory");
        stand_in::program(&bin, "ps", ps);
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let said = Command::new(the_check())
            .args(["--grace", "0", "--", "true"])
            .env("PATH", path)
            .output()
            .expect("the check runs");

        let stderr = String::from_utf8_lossy(&said.stderr);
        assert_eq!(said.status.code(), Some(3), "a {name} ps: {stderr}");
        assert!(stderr.contains("nothing was checked"), "{stderr}");
    }
}

#[test]
fn a_grace_with_no_value_is_a_usage_error() {
    let said = Command::new(the_check())
        .arg("--grace")
        .output()
        .expect("the check runs");

    assert_eq!(said.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&said.stderr).contains("usage:"));
}
