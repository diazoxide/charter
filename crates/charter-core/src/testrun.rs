//! This test binary, re-run as a child with an environment of the test's choosing — for a
//! test whose subject IS the environment: the variables a charter chat exports
//! (charter-app#243), or the `HOME` the product's git reads its global config from
//! (charter-app#242).
//!
//! **Why a child.** Setting a variable in this process needs `unsafe` (the workspace forbids
//! it) and would leak into every test running beside this one. A child gets exactly the
//! environment it is handed.
//!
//! **What the child never inherits.** Every `CHARTER_*` variable of the process running the
//! suite is removed first — the chat the suite may be running in is not the environment under
//! test — except [`crate::fence::VAR`], the fence that keeps a test off planes it did not make.
//!
//! **And the integration tests** (charter-app#267). `cfg(test)` hides a steering variable
//! from this crate's unit tests only ([`crate::steer`]); the tests under `tests/` link the
//! library as the product does, and a `CHARTER_WORKTREES` or `CHARTER_HOME` in the shell that
//! ran them reached every one — failing ~45, and able to write into the real state
//! directory. So each of them opens with [`unsteered!`](crate::unsteered), which re-runs that
//! one test here, without the shell's variables, whenever one of them is set. This module is
//! compiled into the library so those crates can reach it; the product never calls it.

use std::ffi::OsStr;
use std::process::Command;

/// Run `tests` (full paths, matched exactly) in a child of this test binary with `env` set,
/// and answer what it printed. Fails the calling test unless every one of them ran and passed:
/// a filter that matches nothing passes too, so the count is the evidence they ran.
pub fn rerun(tests: &[&str], env: &[(&str, &OsStr)]) -> String {
    let mut child = Command::new(std::env::current_exe().expect("the test binary"));
    child.args(["--exact", "--test-threads=1", "--nocapture"]);
    child.args(tests);
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("CHARTER_") && name != crate::fence::VAR {
            child.env_remove(&*name);
        }
    }
    for (name, value) in env {
        child.env(name, value);
    }
    let out = crate::forklock::output(&mut child).expect("the test binary re-runs");
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains(&format!("test result: ok. {} passed", tests.len())),
        "{said}"
    );
    said
}

/// Whether this process has a steering variable set — one of [`crate::steer::STEERING`] — and
/// so has re-run the calling test without it, in a child ([`rerun`]). The caller returns when
/// it has: the child ran the test, and failed this one if the test failed there.
///
/// The test is named by its thread, which is how libtest names one. Read straight from the
/// environment, not through [`crate::steer::var_os`], which hides these under `cfg(test)`.
pub fn rerun_if_steered() -> bool {
    if !crate::steer::STEERING
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return false;
    }
    let me = std::thread::current();
    let name = me
        .name()
        .filter(|name| *name != "main")
        .expect("libtest names the thread a test runs on after the test");
    rerun(&[name], &[]);
    true
}

/// The first line of every test in `crates/charter-core/tests/`: when the shell that ran the
/// suite set a steering variable, run this test again without it and stop here
/// ([`testrun::rerun_if_steered`](crate::testrun::rerun_if_steered)).
///
/// Not for a `#[should_panic]` test: the child's panic fails the re-run, and the parent then
/// panics for the wrong reason. The directory's own check refuses one.
#[macro_export]
macro_rules! unsteered {
    () => {
        if $crate::testrun::rerun_if_steered() {
            return;
        }
    };
}

#[cfg(test)]
mod tests {
    /// Set on the child: where the test's body writes what it saw.
    const SAW: &str = "TESTRUN_GUARD_SAW";

    #[test]
    fn a_test_started_with_a_steering_variable_set_runs_again_without_it() {
        if let Some(saw) = std::env::var_os(SAW) {
            if super::rerun_if_steered() {
                return;
            }
            let seen: Vec<&str> = ["CHARTER_WORKTREES", "CHARTER_HOME"]
                .into_iter()
                .filter(|name| std::env::var_os(name).is_some())
                .collect();
            std::fs::write(saw, format!("still set: {seen:?}")).unwrap();
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let saw = dir.path().join("saw");

        super::rerun(
            &["testrun::tests::a_test_started_with_a_steering_variable_set_runs_again_without_it"],
            &[
                ("CHARTER_WORKTREES", "/nowhere/worktrees".as_ref()),
                ("CHARTER_HOME", "/nowhere/state".as_ref()),
                (SAW, saw.as_os_str()),
            ],
        );

        assert_eq!(
            std::fs::read_to_string(&saw).ok().as_deref(),
            Some("still set: []"),
            "the body ran, and without either variable"
        );
    }
}
