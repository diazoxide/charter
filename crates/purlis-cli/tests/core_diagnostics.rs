//! A diagnostic `purlis-core` raises while the `charter` binary is running reaches standard
//! error (#647). The app keeps its diagnostics in a log file; the CLI has a terminal, so the
//! core's warnings go there, as the message alone and redacted the same way.

use std::process::Command;

/// `command` without any spelling of the variables that choose a project, which a suite run
/// inside a chat inherits under both names (V93k): the test sets its own after.
fn unsteered(mut command: Command) -> Command {
    for rest in purlis_core::envvar::SELECTING {
        for spelling in purlis_core::envvar::spellings(&format!("PURLIS_{rest}")) {
            command.env_remove(spelling);
        }
    }
    command
}

#[test]
fn a_warning_raised_in_the_core_under_the_cli_is_said_on_standard_error() {
    let dir = tempfile::tempdir().expect("a directory");

    let said = unsteered(Command::new(env!("CARGO_BIN_EXE_purlis")))
        .arg("--version")
        .current_dir(dir.path())
        .env_remove("CHARTER_ROOT")
        .env("CHARTER_CONFIG_HOME", dir.path())
        .env("CHARTER_TEST_CORE_WARNS", "1")
        .output()
        .expect("the binary runs");

    assert!(said.status.success(), "{said:?}");
    let stderr = String::from_utf8_lossy(&said.stderr);
    assert_eq!(
        stderr, "purlis: a warning purlis-core raised on purpose, for a test\n",
        "the core's warning, as the message alone"
    );
}
