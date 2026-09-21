//! charter-app#129, in the shape that caused it: a charter process started by the test suite,
//! from a directory inside a plane the run did not make.
//!
//! **The measured damage.** `charter-app` is checked out at `workspaces/ide/charter-app`,
//! inside the operator's own plane. On 2026-09-21 a run started the app from there, the walk
//! up found `/Users/aharon/IdeaProjects/charter`, and 49 of the suite's chats were written
//! into that plane's `.charter/app/reopen.json`. The operator found them by opening the app:
//! 49 dead tabs, each failing on a binary that no longer existed, all filed under "Outside
//! every workspace" because the recorded `cwd` was empty.
//!
//! **What is asserted here is not "the suite pins `$CHARTER_ROOT`".** Pinning is how a run
//! avoids the write, and a suite that can reach the operator's plane and merely chooses not
//! to is one refactor away from reaching it again. What is asserted is that a test build
//! **cannot**: the binary these tests spawn is fenced (`charter-core`'s `fenced` feature,
//! turned on from `[dev-dependencies]`), and a fenced build dies on the plane rather than
//! answering with it.
//!
//! The fence is a tree, and the two planes below are SIBLINGS under one temporary directory
//! — so the test says nothing about `/tmp` and needs no plane anywhere near the operator's to
//! prove the rule.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A plane at `at`: a directory with the one file that makes it one.
fn a_plane(at: &Path) -> PathBuf {
    std::fs::create_dir_all(at).expect("the plane's directory");
    std::fs::write(at.join("charter.toml"), "").expect("its charter.toml");
    at.canonicalize().expect("the plane resolves")
}

/// `charter root` — the command whose whole job is to answer with the resolved plane — run in
/// `cwd`, fenced to `fence`, with `$CHARTER_ROOT` as `root` says.
fn asking_for_the_root(cwd: &Path, fence: &Path, root: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .arg("root")
        .current_dir(cwd)
        .env("CHARTER_PLANE_FENCE", fence)
        .env("NO_COLOR", "1");
    match root {
        Some(root) => command.env("CHARTER_ROOT", root),
        None => command.env_remove("CHARTER_ROOT"),
    };
    command.output().expect("the binary runs")
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Every refusal says the same four things, so every test can ask for them at once: which
/// plane, which fence, that this is a fenced build, and the issue that bought the rule.
fn is_the_fence_refusing(out: &Output, about: &Path, fence: &Path) {
    assert!(
        !out.status.success(),
        "the fenced binary answered instead of dying:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let said = err(out);
    assert!(said.contains("a fenced build refused"), "{said}");
    assert!(
        said.contains(&about.display().to_string()),
        "the refusal does not name the plane it refused:\n{said}"
    );
    assert!(
        said.contains(&fence.display().to_string()),
        "the refusal does not name the fence:\n{said}"
    );
    assert!(said.contains("charter-app#129"), "{said}");
}

#[test]
fn a_run_from_a_checkout_inside_a_plane_dies_on_that_plane_instead_of_acting_on_it() {
    // The exact shape of charter-app#129: no `$CHARTER_ROOT` at all, and a working directory
    // two levels inside somebody else's plane — `workspaces/ide/<checkout>`, which is where
    // this repository actually sits.
    let tmp = tempfile::tempdir().expect("a directory");
    let fence = tmp.path().join("the-run");
    std::fs::create_dir_all(&fence).expect("the run's own tree");
    let operators = a_plane(&tmp.path().join("the-operators-plane"));
    let checkout = operators.join("workspaces").join("ide").join("charter-app");
    std::fs::create_dir_all(&checkout).expect("a checkout inside the plane");

    let out = asking_for_the_root(&checkout, &fence, None);

    is_the_fence_refusing(&out, &operators, &fence);
}

#[test]
fn pinning_the_plane_is_not_an_exemption_from_the_fence() {
    // `$CHARTER_ROOT` is how a caller says which plane it means, and a run that means the
    // operator's plane is exactly the run this exists to stop. A fence that the pin could
    // step over would be a fence only against the mistake nobody makes twice.
    let tmp = tempfile::tempdir().expect("a directory");
    let fence = tmp.path().join("the-run");
    std::fs::create_dir_all(&fence).expect("the run's own tree");
    let operators = a_plane(&tmp.path().join("the-operators-plane"));

    let out = asking_for_the_root(&fence, &fence, Some(&operators));

    is_the_fence_refusing(&out, &operators, &fence);
}

#[test]
fn a_plane_the_run_made_inside_its_own_fence_is_answered_as_it_always_was() {
    // The other half, and the one that makes the two above worth anything: the fence is not
    // "every plane is refused". Every fixture in this workspace is a `tempfile::tempdir()`,
    // and a plane in one goes on resolving exactly as it did.
    let tmp = tempfile::tempdir().expect("a directory");
    let fence = tmp.path().join("the-run");
    let ours = a_plane(&fence.join("a-plane-this-test-made"));
    let under = ours.join("workspaces").join("alpha");
    std::fs::create_dir_all(&under).expect("a directory below it");

    let walked = asking_for_the_root(&under, &fence, None);
    let pinned = asking_for_the_root(&fence, &fence, Some(&ours));

    assert!(walked.status.success(), "{}", err(&walked));
    assert!(pinned.status.success(), "{}", err(&pinned));
    let said = String::from_utf8_lossy(&walked.stdout).trim().to_owned();
    assert_eq!(said, ours.display().to_string());
    assert_eq!(
        String::from_utf8_lossy(&pinned.stdout).trim(),
        ours.display().to_string()
    );
}

#[test]
fn the_fence_is_in_the_binary_these_tests_run_and_not_only_in_the_test() {
    // The mechanism that puts it there is feature unification through `[dev-dependencies]`,
    // and it is invisible in every source file: nothing in `main.rs` mentions the fence, and
    // a `cargo build --release` of the same code has none. If that wiring is ever dropped
    // from `crates/charter-cli/Cargo.toml`, the three tests above go green by doing nothing
    // — the binary would simply answer — so the presence of the guard is asserted on its own.
    let tmp = tempfile::tempdir().expect("a directory");
    let fence = tmp.path().join("the-run");
    std::fs::create_dir_all(&fence).expect("the run's own tree");
    let outside = a_plane(&tmp.path().join("outside"));

    let out = asking_for_the_root(&fence, &fence, Some(&outside));

    assert_eq!(
        out.status.code(),
        Some(101),
        "a fenced binary ends on a panic, and this one ended some other way:\n{}",
        err(&out)
    );
}
