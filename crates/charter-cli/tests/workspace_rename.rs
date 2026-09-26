//! `charter workspace rename` (and `mv`), asked of the binary: a real plane with a clone and a
//! linked worktree, renamed, and refused while the app has a chat open in it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const IDENTITY: [(&str, &str); 5] = [
    ("GIT_AUTHOR_NAME", "Tester"),
    ("GIT_AUTHOR_EMAIL", "t@e.invalid"),
    ("GIT_COMMITTER_NAME", "Tester"),
    ("GIT_COMMITTER_EMAIL", "t@e.invalid"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
];

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

fn git(at: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .envs(IDENTITY)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("p");
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
        std::fs::write(root.join("charter.toml"), "").unwrap();
        std::fs::write(
            root.join(".gitignore"),
            "/.charter/\n/workspaces/*/*\n!/workspaces/.gitkeep\n",
        )
        .unwrap();
        std::fs::write(root.join("workspaces/.gitkeep"), "").unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-qm", "plane"]);
        let svc = root.join("workspaces/alpha/svc");
        std::fs::create_dir_all(&svc).unwrap();
        git(&svc, &["init", "-q", "-b", "main"]);
        std::fs::write(svc.join("README.md"), "hi\n").unwrap();
        git(&svc, &["add", "-A"]);
        git(&svc, &["commit", "-qm", "first"]);
        git(
            &svc,
            &["worktree", "add", "-q", "-b", "p1", "../.worktrees/svc/p1"],
        );
        Self {
            _tmp: tmp,
            root,
            home,
        }
    }

    fn charter(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_charter"))
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("CHARTER_ROOT", &self.root)
            .env("CHARTER_CONFIG_HOME", self.home.join("config"))
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .envs(IDENTITY)
            .output()
            .expect("the binary runs")
    }
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_workspace_with_a_clone_and_a_worktree_is_renamed_and_both_still_work() {
    let world = World::new();

    let out = world.charter(&["workspace", "mv", "alpha", "beta"]);

    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("Renamed workspace 'alpha' to 'beta'."));
    let piece = world.root.join("workspaces/beta/.worktrees/svc/p1");
    assert_eq!(
        git(&piece, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "p1"
    );
    let listed = git(
        &world.root.join("workspaces/beta/svc"),
        &["worktree", "list", "--porcelain"],
    );
    assert!(!listed.contains("prunable"), "{listed}");
    assert!(listed.contains(&piece.display().to_string()), "{listed}");
}

#[test]
fn a_chat_the_app_has_open_in_it_stops_the_rename_and_is_named() {
    let world = World::new();
    let app = world.root.join(".charter/app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("reopen.json"),
        serde_json::json!({
            "version": 1,
            "at": 0,
            "chats": [{
                "program": "claude",
                "cwd": world.root.join("workspaces/alpha/svc"),
                "name": "3",
                "persona": "steward",
            }],
        })
        .to_string(),
    )
    .unwrap();
    // The app, as a terminal can tell it is there: something accepting on its socket.
    let _listening = std::os::unix::net::UnixListener::bind(app.join("hooks.sock")).unwrap();

    let out = world.charter(&["workspace", "rename", "alpha", "beta"]);

    assert!(!out.status.success());
    assert!(
        said(&out).contains("a chat is running in it: steward 3"),
        "{}",
        said(&out)
    );
    assert!(world.root.join("workspaces/alpha/svc").is_dir());
}

#[test]
fn a_taken_name_is_refused() {
    let world = World::new();
    std::fs::create_dir_all(world.root.join("workspaces/beta")).unwrap();

    let out = world.charter(&["workspace", "rename", "alpha", "beta"]);

    assert!(!out.status.success());
    assert!(
        said(&out).contains("workspace 'beta' already exists"),
        "{}",
        said(&out)
    );
    assert!(world.root.join("workspaces/alpha/svc").is_dir());
}
