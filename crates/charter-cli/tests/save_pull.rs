//! `charter save --pull`, asked of the binary (#375).
//!
//! A chat the app did not start has no auto-save and no incoming loop: outside the app the
//! plane is saved only through `charter save`. `--pull` is that loop's fetch-and-fast-forward
//! half, run from a terminal, before the save.
//!
//! **The stand-in forge**, as `repo_commands.rs` builds it: `origin` is GitHub's SSH form, and
//! the test's own `$HOME/.gitconfig` rewrites the HTTPS URL charter builds onto a local bare
//! repository. Nothing reaches a network.

use std::path::{Path, PathBuf};
use std::process::Command;

const IDENTITY: [(&str, &str); 6] = [
    ("GIT_AUTHOR_NAME", "Tester"),
    ("GIT_AUTHOR_EMAIL", "t@e.invalid"),
    ("GIT_COMMITTER_NAME", "Tester"),
    ("GIT_COMMITTER_EMAIL", "t@e.invalid"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_TERMINAL_PROMPT", "0"),
];

/// A plane pushed to a stand-in forge, and somebody else's commit on the forge after it.
struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl World {
    fn new() -> World {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let (root, home, forge) = (base.join("plane"), base.join("home"), base.join("forge"));
        for dir in [&root, &home, &forge.join("acme")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(
            home.join(".gitconfig"),
            format!(
                "[url \"file://{}/acme/\"]\n\tinsteadOf = https://github.com/acme/\n",
                forge.display()
            ),
        )
        .unwrap();
        let world = World {
            _tmp: tmp,
            root: root.clone(),
            home,
        };
        std::fs::write(
            root.join("charter.toml"),
            "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n",
        )
        .unwrap();
        std::fs::write(root.join(".gitignore"), ".charter/\n").unwrap();
        std::fs::write(root.join("README.md"), "one\n").unwrap();
        world.git(&root, &["init", "-q", "-b", "main", "."]);
        world.git(&root, &["add", "-A"]);
        world.git(&root, &["commit", "-q", "-m", "one"]);
        world.git(
            &root,
            &["remote", "add", "origin", "git@github.com:acme/plane.git"],
        );
        let bare = forge.join("acme/plane.git");
        world.git(
            &forge,
            &["init", "-q", "--bare", "-b", "main", "acme/plane.git"],
        );
        world.git(&root, &["push", "-q", &bare.display().to_string(), "main"]);
        let theirs = base.join("theirs");
        world.git(
            &base,
            &[
                "clone",
                "-q",
                &bare.display().to_string(),
                &theirs.display().to_string(),
            ],
        );
        std::fs::write(theirs.join("theirs.md"), "theirs\n").unwrap();
        world.git(&theirs, &["add", "-A"]);
        world.git(&theirs, &["commit", "-q", "-m", "theirs"]);
        world.git(&theirs, &["push", "-q", "origin", "main"]);
        world
    }

    /// git, for the test's own setup — never charter's runner.
    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", &self.home)
            .envs(IDENTITY)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn charter(&self, args: &[&str]) -> (i32, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_charter"))
            .args(args)
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("CHARTER_ROOT", &self.root)
            .env("CHARTER_HOME", self.home.join(".charter"))
            .env("NO_COLOR", "1")
            .envs(IDENTITY)
            .output()
            .expect("the binary runs");
        (
            out.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }

    fn head(&self) -> String {
        self.git(&self.root, &["rev-parse", "HEAD"])
    }
}

#[test]
fn save_pull_fast_forwards_a_clean_plane_onto_what_came_in() {
    let world = World::new();

    let (code, said) = world.charter(&["save", "--pull", "--no-push"]);

    assert_eq!(code, 0, "{said}");
    assert!(world.root.join("theirs.md").is_file(), "{said}");
    assert!(said.contains("Brought in 1 incoming commit"), "{said}");
}

#[test]
fn save_pull_in_a_plane_with_unsaved_work_leaves_what_came_in_and_still_saves() {
    let world = World::new();
    std::fs::write(world.root.join("mine.md"), "mine\n").unwrap();
    let before = world.head();

    let (code, said) = world.charter(&["save", "--pull", "--no-push"]);

    assert_eq!(code, 0, "{said}");
    assert!(!world.root.join("theirs.md").exists(), "{said}");
    assert!(said.contains("not brought in"), "{said}");
    assert!(said.contains("unsaved changes"), "{said}");
    assert_eq!(
        world.git(&world.root, &["rev-parse", "HEAD~1"]),
        before,
        "the work was committed: {said}"
    );
    assert_eq!(world.git(&world.root, &["status", "--porcelain"]), "");
}

#[test]
fn save_pull_in_a_plane_with_conflicts_refuses_and_saves_nothing() {
    let world = World::new();
    let root = &world.root;
    world.git(root, &["checkout", "-q", "-b", "side"]);
    std::fs::write(root.join("README.md"), "side\n").unwrap();
    world.git(root, &["commit", "-q", "-am", "side"]);
    world.git(root, &["checkout", "-q", "main"]);
    std::fs::write(root.join("README.md"), "main\n").unwrap();
    world.git(root, &["commit", "-q", "-am", "main"]);
    let merge = Command::new("git")
        .args(["merge", "side"])
        .current_dir(root)
        .env("HOME", &world.home)
        .envs(IDENTITY)
        .output()
        .unwrap();
    assert!(!merge.status.success(), "the merge stops on its conflict");
    let before = world.head();

    let (code, said) = world.charter(&["save", "--pull", "--no-push"]);

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("conflicts"), "{said}");
    assert!(said.contains("README.md"), "{said}");
    assert!(said.contains("Nothing was saved"), "{said}");
    assert_eq!(world.head(), before, "no commit: {said}");
}
