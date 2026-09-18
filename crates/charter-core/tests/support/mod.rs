//! A plane with one clone in it, built from nothing, for tests that need real git.
//!
//! **Pinned author and dates**, so two runs of the same steps produce the same commit shas —
//! the differential compares what two implementations did to one repository, and that is only
//! meaningful if the repository itself is reproducible.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Fixture {
    _dir: tempfile::TempDir,
    pub plane: PathBuf,
    pub ws: String,
    pub repo: String,
    pub clone: PathBuf,
}

const WHO: [(&str, &str); 6] = [
    ("GIT_AUTHOR_NAME", "charter tests"),
    ("GIT_AUTHOR_EMAIL", "tests@example.invalid"),
    ("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00"),
    ("GIT_COMMITTER_NAME", "charter tests"),
    ("GIT_COMMITTER_EMAIL", "tests@example.invalid"),
    ("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00"),
];

/// git, for the test's own setup. Not the code under test.
pub fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    for (k, v) in WHO {
        cmd.env(k, v);
    }
    // A developer's own `init.defaultBranch`, hooks or templates must not reach a fixture.
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
    let out = cmd.output().expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

pub fn plane_with_clone(repo: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    // Resolved: on macOS a temp dir is under `/var/folders/…`, itself a link to
    // `/private/var/…`, and a test that compares an unresolved path against a resolved one
    // fails for a reason that has nothing to do with what it is testing.
    let plane = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
    let ws = "alpha".to_string();
    let clone = plane.join("workspaces").join(&ws).join(repo);
    std::fs::create_dir_all(&clone).unwrap();
    git(&clone, &["init", "-q", "-b", "main", "."]);
    std::fs::write(clone.join("README.md"), "one\n").unwrap();
    git(&clone, &["add", "README.md"]);
    git(&clone, &["commit", "-q", "-m", "one"]);
    Fixture {
        _dir: dir,
        plane,
        ws,
        repo: repo.to_string(),
        clone,
    }
}

impl Fixture {
    /// A commit in any tree of this fixture.
    pub fn commit(&self, tree: &Path, message: &str) {
        std::fs::write(tree.join(message), message).unwrap();
        git(tree, &["add", "-A"]);
        git(tree, &["commit", "-q", "-m", message]);
    }

    /// The workspace directory, which is what a worktree path must stay inside.
    pub fn workspace(&self) -> PathBuf {
        self.plane.join("workspaces").join(&self.ws)
    }
}
