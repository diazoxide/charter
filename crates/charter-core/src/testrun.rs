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

use std::ffi::OsStr;
use std::process::Command;

/// Run `tests` (full paths, matched exactly) in a child of this test binary with `env` set,
/// and answer what it printed. Fails the calling test unless every one of them ran and passed:
/// a filter that matches nothing passes too, so the count is the evidence they ran.
pub(crate) fn rerun(tests: &[&str], env: &[(&str, &OsStr)]) -> String {
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
