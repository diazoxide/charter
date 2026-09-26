//! A `CHARTER_*` steering variable that IS set reaches the code that reads it.
//!
//! `steer` hides every steering variable from this crate's unit tests (`cfg(test)`), and the
//! `unsteered!` guard sheds them from these integration tests, so between the two nothing in
//! charter-core's own suite ever ran with one set. What a set one does was proven only by
//! charter-cli's tests, which the nightly mutation run does not build, and `steer::var_os`,
//! `steer::var` and `wscmd::select::warn_env_override` could each be replaced by "unset" with
//! nothing in charter-core noticing (#464).
//!
//! So this test re-runs itself with the variables set ON PURPOSE
//! ([`charter_core::testrun::rerun_steered`]), and the child asks the library what it sees.

use std::path::Path;

use charter_core::active::Ids;
use charter_core::repocmd::Say;

/// Set on the child, and only there: it is the half of the test that runs steered.
const CHILD: &str = "STEERING_REACHES_THE_LIBRARY_CHILD";

const ME: &str = "a_steering_variable_set_on_the_process_is_what_the_library_reads";

#[test]
fn a_steering_variable_set_on_the_process_is_what_the_library_reads() {
    charter_core::unsteered!();
    let Some(home) = std::env::var_os(CHILD) else {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("state-the-operator-chose");
        charter_core::testrun::rerun_steered(
            &[ME],
            &[
                (CHILD, home.as_os_str()),
                ("CHARTER_HOME", home.as_os_str()),
                ("CHARTER_WORKSPACE", "elsewhere".as_ref()),
            ],
        );
        return;
    };

    // `CHARTER_HOME`, read through `steer::var_os`: the state directory is the one it names,
    // not the plane's own `.charter`.
    let plane = tempfile::tempdir().unwrap();
    std::fs::write(plane.path().join("charter.toml"), "").unwrap();
    std::fs::create_dir_all(plane.path().join("workspaces")).unwrap();
    assert_eq!(
        charter_core::plane::state_dir(plane.path()),
        Path::new(&home),
        "a CHARTER_HOME that is set was not where the state went"
    );

    // `CHARTER_WORKSPACE`, read through `steer::var`: choosing another workspace while it
    // names one is said out loud, naming the one that will really be acted on.
    assert_eq!(
        warnings(plane.path(), "default"),
        [
            "$CHARTER_WORKSPACE='elsewhere' is set and takes precedence — commands in this \
             session will still act on 'elsewhere', not 'default'."
        ]
    );
    // Choosing the very workspace it names is no override, and nothing is said.
    std::fs::create_dir_all(plane.path().join("workspaces/elsewhere")).unwrap();
    assert!(warnings(plane.path(), "elsewhere").is_empty());
}

/// The warnings `charter workspace use <name>` says, for a session of its own.
fn warnings(plane: &Path, name: &str) -> Vec<String> {
    let ids = Ids {
        session: Some(format!("session-for-{name}")),
        terminal: None,
    };
    let mut said = Vec::new();
    let code = charter_core::wscmd::select::use_workspace(
        plane,
        name,
        &ids,
        false,
        false,
        "2026-05-04T11:32:17Z".parse().unwrap(),
        &mut |line: Say| said.push(line),
    );
    assert_eq!(code, 0, "{said:?}");
    said.into_iter()
        .filter_map(|line| match line {
            Say::Warn(text) => Some(text),
            _ => None,
        })
        .collect()
}
