//! Which plane the binary acts on, asked of the binary itself.
//!
//! A subprocess rather than a unit test: the answer depends on an environment variable, and
//! setting one in-process is `unsafe` under this edition — which the workspace forbids — and
//! would leak into every other test in the binary besides.

use std::path::{Path, PathBuf};
use std::process::Command;

fn charter() -> PathBuf {
    // `CARGO_BIN_EXE_<name>` is the binary cargo just built for this test.
    PathBuf::from(env!("CARGO_BIN_EXE_charter"))
}

fn plane(at: PathBuf) -> PathBuf {
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(at.join("charter.toml"), "schema = 1\n").unwrap();
    at
}

fn root_seen(cwd: &Path, charter_root: Option<&Path>) -> String {
    let mut command = Command::new(charter());
    command
        .arg("root")
        .current_dir(cwd)
        .env_remove("CHARTER_ROOT");
    if let Some(root) = charter_root {
        command.env("CHARTER_ROOT", root);
    }
    let out = command.output().expect("the binary runs");
    assert!(
        out.status.success(),
        "charter root failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn with_no_variable_set_the_plane_is_the_nearest_one_above_the_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("here"));
    let deep = root.join("workspaces/alpha/svc");
    std::fs::create_dir_all(&deep).unwrap();

    assert_eq!(
        root_seen(&deep, None),
        root.canonicalize().unwrap().display().to_string()
    );
}

#[test]
fn charter_root_wins_over_the_plane_the_directory_sits_in() {
    // The variable is how a caller PINS a plane. A hook, a test and a launched chat all run
    // inside some other plane's directory and must not act on it.
    let tmp = tempfile::tempdir().unwrap();
    let inside = plane(tmp.path().join("inside"));
    let pinned = plane(tmp.path().join("pinned"));

    assert_eq!(
        root_seen(&inside, Some(&pinned)),
        pinned.display().to_string()
    );
}

#[test]
fn an_empty_variable_is_treated_as_unset_rather_than_as_the_root_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("here"));

    assert_eq!(
        root_seen(&root, Some(Path::new(""))),
        root.canonicalize().unwrap().display().to_string()
    );
}
