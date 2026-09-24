//! The `CHARTER_*` variables that steer which plane, state directory and harness a process acts
//! for, read in one place (charter-app#243).
//!
//! A charter chat exports them — `CHARTER_ROOT` pins the plane, `CHARTER_HARNESS` names the
//! harness — and `cargo test` run inside that chat inherits them. A unit test that builds a
//! plane of its own and then asks the product which plane it is on was answered with the
//! CHAT's plane, and a test that writes a push record wrote it into the operator's
//! `$CHARTER_HOME`. So every read of one of [`STEERING`] goes through [`var_os`], and under
//! `cfg(test)` — this crate's own unit tests, and nothing else — a steering variable reads as
//! unset: the one environment such a test can control, since setting a variable in-process
//! needs `unsafe` and would leak into every test running beside it.
//!
//! **Only the names read here.** `HOME`, `PATH` and the rest pass through untouched, and so
//! does [`crate::fence::VAR`]: the fence is what stops a test acting on a plane it did not
//! make, and hiding it would switch that guard off exactly where it matters.
//! Nor is `CHARTER_CONFIG_HOME`: it is how a test or a launcher ISOLATES the machine store,
//! and the fence holds a fenced build off the operator's store when it is missing.
//! `CHARTER_PERSONA` and `CHARTER_SESSION_ID` are not here either: they reach the library as a
//! named environment the caller hands in (`active::Ids::of`, the CLI's `workspace_env` and
//! `persona_env`), which a test builds for itself, and `active`'s own test proves the real
//! variable is read by setting it on a child — which this module would have hidden.
//!
//! A unit test therefore cannot see a steering variable at all; what a set one does is proven
//! by the integration tests, which link the library without `cfg(test)` and run the binary
//! with it set. They read the real environment, so a steering variable in the shell that runs
//! them reaches them too.

use std::ffi::OsString;

/// The variables the library reads straight from its own environment to decide what it acts
/// on: the plane, its state directory, the workspace, the harness, and the worktree root —
/// which, set in the operator's shell, turns every worktree verb into a refusal.
pub(crate) const STEERING: [&str; 5] = [
    "CHARTER_ROOT",
    "CHARTER_HOME",
    "CHARTER_WORKSPACE",
    "CHARTER_HARNESS",
    "CHARTER_WORKTREES",
];

/// `name` from this process's environment — except a [`STEERING`] name under `cfg(test)`,
/// which reads as unset.
pub(crate) fn var_os(name: &str) -> Option<OsString> {
    if cfg!(test) && STEERING.contains(&name) {
        return None;
    }
    std::env::var_os(name)
}

/// [`var_os`], as a `String`; a value that is not UTF-8 reads as unset, as `std::env::var`'s
/// callers here already treated it.
pub(crate) fn var(name: &str) -> Option<String> {
    var_os(name).and_then(|v| v.into_string().ok())
}

#[cfg(test)]
mod tests {
    /// Tests that build a plane, a state directory or a harness of their own and then ask the
    /// product which one it is acting on — each of them read one of the variables a charter
    /// chat exports.
    const ASKS_THE_ENVIRONMENT: [&str; 5] = [
        "plane::tests::a_worktree_whose_plane_was_never_committed_resolves_to_the_plane_it_was_cut_from",
        "plane::tests::a_worktree_whose_main_tree_has_no_plane_above_it_is_still_not_a_plane",
        "doctor::tests::a_session_in_a_clone_of_its_own_is_told_its_trust_is_its_own",
        "planegit::tests::the_record_is_written_where_the_operator_put_their_state_and_read_back_from_there",
        "planegit::tests::a_record_is_written_for_what_did_not_land_and_carries_the_commit_it_is_about",
    ];

    #[test]
    fn a_suite_run_inside_a_chat_acts_on_what_its_tests_built_and_not_on_the_chats_plane() {
        // charter-app#243, simulated whether or not the suite runs in a chat: the chat's
        // variables are set on a child ([`crate::testrun`]).
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let chats_plane = top.join("chats-plane");
        std::fs::create_dir_all(&chats_plane).unwrap();
        std::fs::write(chats_plane.join("charter.toml"), "[plane]\n").unwrap();
        let chats_state = top.join("chats-state");

        crate::testrun::rerun(
            &ASKS_THE_ENVIRONMENT,
            &[
                ("CHARTER_ROOT", chats_plane.as_os_str()),
                ("CHARTER_HOME", chats_state.as_os_str()),
                ("CHARTER_PERSONA", "steward".as_ref()),
                ("CHARTER_WORKSPACE", "ide".as_ref()),
                ("CHARTER_SESSION_ID", "3".as_ref()),
                ("CHARTER_HARNESS", "codex".as_ref()),
                ("CHARTER_HARNESS_PROFILE", "claude".as_ref()),
            ],
        );

        assert!(
            !chats_state.exists(),
            "a test wrote into the chat's state directory"
        );
    }
}
