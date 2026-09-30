//! `charter stop --all`, the kill switch from a terminal (OV-1), through the binary.
//!
//! It throws the same switch the title bar does: the machine's `halted` marker, which the app
//! acts on within seconds and which keeps every chat from starting until the operator re-arms
//! in the window. Nothing here re-arms.

use std::path::Path;
use std::process::{Command, Output};

fn charter(cwd: &Path, config: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(args)
        .current_dir(cwd)
        .env_remove("CHARTER_ROOT")
        .env("CHARTER_CONFIG_HOME", config)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .output()
        .expect("the binary runs")
}

/// A directory to run in that is no plane, and a config home inside it. Made up front because
/// the fence resolves the path it is handed, and a directory that does not exist resolves
/// nowhere (charter-app#129).
fn outside_any_plane() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("a directory");
    let config = dir.path().join("config-home");
    std::fs::create_dir_all(&config).expect("the config home");
    (dir, config)
}

#[test]
fn stop_all_halts_every_agent_on_the_machine_from_anywhere_and_journals_it() {
    let (dir, config) = outside_any_plane();

    let said = charter(dir.path(), &config, &["stop", "--all"]);

    assert!(said.status.success(), "{said:?}");
    assert!(
        charter_core::halt::stopped_on_disk(&config),
        "no stop was written"
    );
    let journal = charter_core::halt::journal(&config);
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].event, charter_core::halt::Event::Stop);
    assert_eq!(journal[0].by, charter_core::halt::Actor::Cli);
    let told =
        String::from_utf8_lossy(&said.stdout).into_owned() + &String::from_utf8_lossy(&said.stderr);
    assert!(
        told.contains("re-arm"),
        "the operator is not told how to undo it: {told}"
    );
    assert!(
        told.contains("every chat and shell charter started"),
        "the stop claims more or less than it does: {told}"
    );
}

#[cfg(unix)]
#[test]
fn a_stop_that_cannot_be_written_fails_loudly_rather_than_stopping_nothing_quietly() {
    // A directory made read-only in advance is how a stop from a terminal is defeated: the app
    // never hears it. The command must not say it stopped anything.
    use std::os::unix::fs::PermissionsExt;
    let (dir, config) = outside_any_plane();
    let charter_dir = charter_core::machine::dir(&config);
    std::fs::create_dir_all(&charter_dir).expect("charter's directory");
    std::fs::set_permissions(&charter_dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");

    let said = charter(dir.path(), &config, &["stop", "--all"]);

    std::fs::set_permissions(&charter_dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    assert!(!said.status.success(), "{said:?}");
    let err = String::from_utf8_lossy(&said.stderr);
    assert!(err.contains("NOT stopped"), "{err}");
}

#[test]
fn stop_without_all_is_refused_and_stops_nothing() {
    // v1 has one scope. A bare `stop` that stopped everything would be the widest act charter
    // has, taken on the narrowest command line.
    let (dir, config) = outside_any_plane();

    let said = charter(dir.path(), &config, &["stop"]);

    assert!(!said.status.success(), "{said:?}");
    assert!(!charter_core::halt::stopped_on_disk(&config));
}
