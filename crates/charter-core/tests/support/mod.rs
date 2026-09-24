//! A plane with one clone in it, built from nothing, for tests that need real git.
//!
//! **Pinned author and dates**, so two runs of the same steps produce the same commit shas —
//! the differential compares what two implementations did to one repository, and that is only
//! meaningful if the repository itself is reproducible.
//!
//! Not every test binary that includes this needs every helper in it, and an unused one here
//! is not a defect in the helper.
#![allow(dead_code)]

pub mod forge_cli;

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

/// The template every fixture repository is made from: `commit.gpgsign` and `tag.gpgsign` off,
/// and `core.excludesFile` pointed at nothing (charter-app#191, #262).
pub const TEMPLATE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/git-template");

/// `git`, with every repository it creates made from [`TEMPLATE`], so none asks a developer's
/// signer (charter-app#191) — `src/testgit.rs`'s `unsigned`, for the integration tests.
pub fn unsigned() -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-c").arg(format!("init.templateDir={TEMPLATE}"));
    cmd
}

/// git, for the test's own setup. Not the code under test.
pub fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = unsigned();
    cmd.arg("-C").arg(dir).args(args);
    for (k, v) in WHO {
        cmd.env(k, v);
    }
    // A developer's own `init.defaultBranch`, hooks or templates must not reach a fixture.
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
    let out = charter_core::forklock::output(&mut cmd).expect("git runs");
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
    /// Give the plane a layer worth carrying: the settings that hold `$CHARTER_HARNESS`, the
    /// plugin and a shared ask rule; a machine-local deny; and one persona agent.
    ///
    /// Not part of `plane_with_clone`, because a plane with nothing to carry is its own case —
    /// `want` is empty there, charter writes nothing, and no chat is refused over it.
    pub fn give_the_plane_a_layer(&self) {
        let claude = self.plane.join(".claude");
        std::fs::create_dir_all(claude.join("agents")).unwrap();
        std::fs::write(
            claude.join("settings.json"),
            concat!(
                "{\n",
                "  \"enabledPlugins\": {\"charter@charter\": true},\n",
                "  \"env\": {\"CHARTER_HARNESS\": \"claude-code\"},\n",
                "  \"permissions\": {\"allow\": [\"Bash(ls *)\"], ",
                "\"ask\": [\"Bash(charter handoff *)\"]},\n",
                "  \"hooks\": {\"PreToolUse\": []}\n",
                "}\n"
            ),
        )
        .unwrap();
        std::fs::write(
            claude.join("settings.local.json"),
            "{\"permissions\": {\"deny\": [\"Bash(rm -rf /*)\"]}}\n",
        )
        .unwrap();
        std::fs::write(
            claude.join("agents").join("steward.md"),
            "# steward\n\nThe control plane steward.\n",
        )
        .unwrap();
    }

    /// What `git status` says in `tree` — empty when charter left nothing showing.
    pub fn status(&self, tree: &Path) -> String {
        String::from_utf8_lossy(&git(tree, &["status", "--porcelain"]).stdout).into_owned()
    }

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
