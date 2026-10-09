//! `purlis worktree remove` never removes a branch folder a dispatch record names (#1534), as
//! the explorer's own Remove never does: a task's folder is discarded from its Changes tab,
//! where the guards a task's folder needs hold. Through the binary, against a real clone with a
//! real worktree; what decides each case is pinned in `purlis_core::dispatchplace`'s tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const IDENTITY: [(&str, &str); 6] = [
    ("GIT_AUTHOR_NAME", "Tester"),
    ("GIT_AUTHOR_EMAIL", "t@e.invalid"),
    ("GIT_COMMITTER_NAME", "Tester"),
    ("GIT_COMMITTER_EMAIL", "t@e.invalid"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_TERMINAL_PROMPT", "0"),
];

/// The id of the record these tests write: a ULID, as purlis names a record's file.
const ID: &str = "01K6Z3V9QJ8M4T2W7XB5RC0DEF";

/// A project with one workspace, `alpha`, holding one clone, `svc`, with one commit.
struct Project {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    clone: PathBuf,
}

impl Project {
    fn new() -> Project {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("plane");
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".gitconfig"), "[commit]\n\tgpgsign = false\n").unwrap();
        let clone = root.join("workspaces/alpha/svc");
        std::fs::create_dir_all(&clone).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let project = Project {
            _tmp: tmp,
            root,
            home,
            clone,
        };
        project.git(&project.clone, &["init", "-q", "-b", "main", "."]);
        std::fs::write(project.clone.join("README.md"), "one\n").unwrap();
        project.git(&project.clone, &["add", "-A"]);
        project.git(&project.clone, &["commit", "-q", "-m", "one"]);
        project
    }

    /// git, for the test's own setup.
    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", self.home.join(".gitconfig"))
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

    fn purlis(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_purlis"))
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("CHARTER_ROOT", &self.root)
            .env("CHARTER_SESSION_ID", "session-under-test")
            .env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", self.home.join(".gitconfig"))
            .env("PATH", "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin")
            .env("USER", "tester")
            .env("NO_COLOR", "1")
            .envs(IDENTITY)
            .output()
            .expect("the binary runs")
    }

    /// A branch folder `name` cut through the binary, with a commit of its own on its branch.
    fn cut(&self, name: &str) -> PathBuf {
        let out = self.purlis(&["wt", "add", "svc", name, "-w", "alpha"]);
        assert!(out.status.success(), "{}", said(&out));
        let folder = self.root.join("workspaces/alpha/.worktrees/svc").join(name);
        std::fs::write(folder.join("work.txt"), "the task's work\n").unwrap();
        self.git(&folder, &["add", "-A"]);
        self.git(&folder, &["commit", "-q", "-m", "the task's work"]);
        folder
    }

    /// A record file in the project's dispatch store that does not read as a record (a write
    /// cut short), whose text names `piece` of `svc` in `alpha`.
    fn a_torn_record_naming(&self, piece: &str) {
        let dir = purlis_core::dispatchrecord::dir(&self.root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{ID}.json")),
            format!(
                "{{\"v\": 1, \"id\": \"{ID}\", \"place\": {{\"workspace\": \"alpha\", \
                 \"worktree\": {{\"repo\": \"svc\", \"piece\": \"{piece}\""
            ),
        )
        .unwrap();
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
fn a_folder_a_dispatch_record_names_is_kept_forced_or_not_and_its_branch_with_it() {
    let p = Project::new();
    let folder = p.cut("check-b5rc0def");
    p.a_torn_record_naming("check-b5rc0def");

    for args in [
        &["wt", "remove", "svc", "check-b5rc0def", "-w", "alpha"][..],
        &[
            "wt",
            "remove",
            "svc",
            "check-b5rc0def",
            "-w",
            "alpha",
            "--force",
            "--delete-branch",
        ][..],
    ] {
        let out = p.purlis(args);

        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", said(&out));
        let text = said(&out);
        assert!(
            text.contains("is named by a dispatch record purlis cannot read"),
            "{args:?}: {text}"
        );
        assert!(text.contains("Nothing was removed."), "{args:?}: {text}");
        assert!(
            folder.join("work.txt").is_file(),
            "{args:?}: the folder is kept"
        );
        p.git(
            &p.clone,
            &["rev-parse", "--verify", "refs/heads/check-b5rc0def"],
        );
    }

    // Another folder of the same repo is the person's own, and goes as ever.
    p.cut("mine");
    let out = p.purlis(&["wt", "remove", "svc", "mine", "-w", "alpha", "--force"]);
    assert!(out.status.success(), "{}", said(&out));
}

/// On a file system that folds case, as macOS's does by default, `CHECK-B5RC0DEF` is the
/// folder `check-b5rc0def`, which git would then remove: the guard matches names without
/// regard to case (#1534). Gated to macOS, where the folder typed in another case is there.
#[cfg(target_os = "macos")]
#[test]
fn a_task_s_folder_typed_in_another_case_is_kept_all_the_same() {
    let p = Project::new();
    let folder = p.cut("check-b5rc0def");
    p.a_torn_record_naming("check-b5rc0def");

    let out = p.purlis(&[
        "wt",
        "remove",
        "svc",
        "CHECK-B5RC0DEF",
        "-w",
        "alpha",
        "--force",
    ]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    let text = said(&out);
    assert!(
        text.contains("is named by a dispatch record purlis cannot read"),
        "{text}"
    );
    assert!(folder.join("work.txt").is_file(), "the folder is kept");
    p.git(
        &p.clone,
        &["rev-parse", "--verify", "refs/heads/check-b5rc0def"],
    );
}
