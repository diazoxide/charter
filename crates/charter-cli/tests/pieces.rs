//! `charter worktree` (alias `wt`): a piece is cut, declared and removed through the binary,
//! against real git repositories with real worktrees (charter#368).
//!
//! What is asserted is what the rest of charter reads back: the claim and the declarations in
//! the piece log (`pieces::claims`, `pieces::declarations`, `pieces::summary`), the worktree
//! git lists, and — for every refusal — that nothing was lost and the refusal names what would
//! have been.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use charter_core::pieces;

const IDENTITY: [(&str, &str); 6] = [
    ("GIT_AUTHOR_NAME", "Tester"),
    ("GIT_AUTHOR_EMAIL", "t@e.invalid"),
    ("GIT_COMMITTER_NAME", "Tester"),
    ("GIT_COMMITTER_EMAIL", "t@e.invalid"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_TERMINAL_PROMPT", "0"),
];

/// A plane with one workspace, `alpha`, holding one clone, `svc`, with one commit.
struct Plane {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    clone: PathBuf,
}

impl Plane {
    fn new() -> Plane {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("plane");
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".gitconfig"), "[commit]\n\tgpgsign = false\n").unwrap();
        let clone = root.join("workspaces/alpha/svc");
        std::fs::create_dir_all(&clone).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let plane = Plane {
            _tmp: tmp,
            root,
            home,
            clone,
        };
        plane.git(&plane.clone, &["init", "-q", "-b", "main", "."]);
        std::fs::write(plane.clone.join("README.md"), "one\n").unwrap();
        plane.git(&plane.clone, &["add", "-A"]);
        plane.git(&plane.clone, &["commit", "-q", "-m", "one"]);
        plane
    }

    /// git, for the test's own setup — never charter's runner.
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

    fn piece(&self, name: &str) -> PathBuf {
        self.root.join("workspaces/alpha/.worktrees/svc").join(name)
    }

    fn charter(&self, args: &[&str]) -> Output {
        self.charter_in(&self.root, args)
    }

    fn charter_in(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_charter"))
            .args(args)
            .current_dir(cwd)
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

    /// `charter wt add svc <piece> -w alpha`, which must succeed.
    fn cut(&self, name: &str) -> PathBuf {
        let out = self.charter(&["wt", "add", "svc", name, "-w", "alpha"]);
        assert!(out.status.success(), "{}", said(&out));
        self.piece(name)
    }

    /// The worktrees git itself lists for the clone.
    fn listed_by_git(&self) -> String {
        self.git(&self.clone, &["worktree", "list", "--porcelain"])
    }
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn key(piece: &str) -> (String, String) {
    ("svc".to_string(), piece.to_string())
}

// ---------------------------------------------------------------------------------------
// add                                                                                     #
// ---------------------------------------------------------------------------------------

#[test]
fn a_piece_cut_through_the_cli_is_claimed_and_counted() {
    let p = Plane::new();

    let path = p.cut("p1");

    assert!(path.join("README.md").is_file(), "the tree is checked out");
    assert!(p.listed_by_git().contains(&path.display().to_string()));
    let claims = pieces::claims(&p.root, "alpha");
    let claim = claims.get(&key("p1")).expect("the cut is claimed");
    assert_eq!(claim["event"], "claimed");
    assert_eq!(claim["session"], "session-under-test");
    let summary = pieces::summary(&p.root, "alpha", chrono::Utc::now()).expect("one piece");
    assert_eq!(summary.total, 1);
    assert_eq!(summary.quiet.len(), 1, "cut and not yet declared: silent");
}

#[test]
fn a_piece_already_held_is_refused_with_its_own_exit_code() {
    let p = Plane::new();
    p.cut("p1");

    let out = p.charter(&["wt", "add", "svc", "p1", "-w", "alpha"]);

    assert_eq!(out.status.code(), Some(2), "{}", said(&out));
    assert!(said(&out).contains("already claimed"), "{}", said(&out));
    let claims = pieces::events(&p.root, "alpha");
    assert_eq!(claims.len(), 1, "the loser records nothing: {claims:?}");
}

#[test]
fn a_repo_the_workspace_has_not_cloned_is_refused() {
    let p = Plane::new();

    let out = p.charter(&["wt", "add", "nope", "p1", "-w", "alpha"]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(pieces::events(&p.root, "alpha").is_empty());
}

// ---------------------------------------------------------------------------------------
// done / abandon                                                                          #
// ---------------------------------------------------------------------------------------

#[test]
fn done_declares_the_piece_it_is_run_from() {
    let p = Plane::new();
    let path = p.cut("p1");
    std::fs::create_dir_all(path.join("src")).unwrap();

    let out = p.charter_in(&path.join("src"), &["wt", "done"]);

    assert!(out.status.success(), "{}", said(&out));
    let declared = pieces::declarations(&p.root, "alpha");
    assert_eq!(declared[&key("p1")]["event"], "done");
    let summary = pieces::summary(&p.root, "alpha", chrono::Utc::now()).unwrap();
    assert_eq!((summary.total, summary.done), (1, 1));
    assert!(summary.quiet.is_empty());
}

#[test]
fn abandon_records_its_reason_and_the_later_word_wins() {
    let p = Plane::new();
    let path = p.cut("p1");
    assert!(p.charter_in(&path, &["wt", "done"]).status.success());

    let out = p.charter_in(&path, &["wt", "abandon", "blocked on the schema review"]);

    assert!(out.status.success(), "{}", said(&out));
    assert!(
        said(&out).contains("already declared done"),
        "a change of mind is said: {}",
        said(&out)
    );
    let entry = &pieces::declarations(&p.root, "alpha")[&key("p1")];
    assert_eq!(entry["event"], "abandoned");
    assert_eq!(entry["reason"], "blocked on the schema review");
    assert_eq!(
        pieces::events(&p.root, "alpha").len(),
        3,
        "the earlier word stays"
    );
}

#[test]
fn abandon_without_a_reason_is_refused_and_records_nothing() {
    let p = Plane::new();
    let path = p.cut("p1");

    for args in [vec!["wt", "abandon"], vec!["wt", "abandon", "   "]] {
        let out = p.charter_in(&path, &args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", said(&out));
        assert!(said(&out).contains("reason"), "{}", said(&out));
    }
    assert!(pieces::declarations(&p.root, "alpha").is_empty());
}

#[test]
fn done_outside_every_piece_is_refused() {
    let p = Plane::new();
    p.cut("p1");

    for cwd in [p.root.clone(), p.clone.clone()] {
        let out = p.charter_in(&cwd, &["wt", "done"]);
        assert_eq!(out.status.code(), Some(1), "{}", said(&out));
        assert!(
            said(&out).contains("Not inside a worktree"),
            "{}",
            said(&out)
        );
    }
    assert!(pieces::declarations(&p.root, "alpha").is_empty());
}

// ---------------------------------------------------------------------------------------
// list / history                                                                          #
// ---------------------------------------------------------------------------------------

#[test]
fn list_joins_what_git_has_to_what_each_piece_said() {
    let p = Plane::new();
    let one = p.cut("p1");
    let two = p.cut("p2");
    p.cut("p3");
    assert!(p.charter_in(&one, &["wt", "done"]).status.success());
    assert!(
        p.charter_in(&two, &["wt", "abandon", "wrong approach"])
            .status
            .success()
    );
    std::fs::write(one.join("wip.txt"), "x\n").unwrap();

    let out = p.charter(&["wt", "list", "-w", "alpha"]);

    assert!(out.status.success(), "{}", said(&out));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let row = |piece: &str| {
        text.lines()
            .find(|l| l.split_whitespace().next() == Some(piece))
            .unwrap_or_else(|| panic!("no row for {piece}: {text}"))
            .to_string()
    };
    assert!(
        row("p1").contains("dirty") && row("p1").ends_with("done"),
        "{text}"
    );
    assert!(row("p2").ends_with("abandoned: wrong approach"), "{text}");
    assert!(
        row("p3").contains("clean") && row("p3").contains("silent"),
        "{text}"
    );
}

#[test]
fn history_keeps_a_piece_that_no_longer_exists() {
    let p = Plane::new();
    let path = p.cut("p1");
    assert!(p.charter_in(&path, &["wt", "done"]).status.success());
    assert!(
        p.charter(&["wt", "remove", "svc", "p1", "-w", "alpha"])
            .status
            .success()
    );

    let out = p.charter(&["wt", "history", "-w", "alpha"]);

    assert!(out.status.success(), "{}", said(&out));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("p1") && text.contains("claimed"), "{text}");
    assert!(text.contains("done"), "{text}");
}

// ---------------------------------------------------------------------------------------
// remove                                                                                  #
// ---------------------------------------------------------------------------------------

#[test]
fn a_piece_with_nothing_to_lose_is_removed_through_git() {
    let p = Plane::new();
    let path = p.cut("p1");

    let out = p.charter(&["wt", "remove", "svc", "p1", "-w", "alpha"]);

    assert!(out.status.success(), "{}", said(&out));
    assert!(!path.exists());
    assert!(
        !p.listed_by_git().contains(&path.display().to_string()),
        "git no longer has it registered, so it went through `git worktree remove`"
    );
    // The branch stays unless asked: removing a tree is not deleting its work.
    p.git(&p.clone, &["rev-parse", "--verify", "refs/heads/p1"]);
}

#[test]
fn uncommitted_changes_are_named_and_kept_unless_forced() {
    let p = Plane::new();
    let path = p.cut("p1");
    std::fs::write(path.join("notes.txt"), "unsaved\n").unwrap();
    std::fs::write(path.join("README.md"), "edited\n").unwrap();

    let out = p.charter(&["wt", "remove", "svc", "p1", "-w", "alpha"]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    let text = said(&out);
    assert!(text.contains("uncommitted"), "{text}");
    assert!(
        text.contains("notes.txt"),
        "the untracked file is named: {text}"
    );
    assert!(
        text.contains("README.md"),
        "the modified file is named: {text}"
    );
    assert!(text.contains("--force"), "{text}");
    assert_eq!(
        std::fs::read_to_string(path.join("notes.txt")).unwrap(),
        "unsaved\n"
    );

    let forced = p.charter(&["wt", "remove", "svc", "p1", "-w", "alpha", "--force"]);
    assert!(forced.status.success(), "{}", said(&forced));
    assert!(!path.exists());
}

#[test]
fn commits_no_other_ref_reaches_are_named_and_kept_unless_forced() {
    let p = Plane::new();
    let path = p.cut("p1");
    std::fs::write(path.join("feature.txt"), "work\n").unwrap();
    p.git(&path, &["add", "-A"]);
    p.git(&path, &["commit", "-q", "-m", "the unpushed feature"]);

    let out = p.charter(&[
        "wt",
        "remove",
        "svc",
        "p1",
        "-w",
        "alpha",
        "--delete-branch",
    ]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    let text = said(&out);
    assert!(
        text.contains("1 commit(s) that exist nowhere else"),
        "{text}"
    );
    assert!(
        text.contains("the unpushed feature"),
        "the commit is named: {text}"
    );
    assert!(path.join("feature.txt").is_file());
    p.git(&p.clone, &["rev-parse", "--verify", "refs/heads/p1"]);

    let forced = p.charter(&[
        "wt",
        "remove",
        "svc",
        "p1",
        "-w",
        "alpha",
        "--delete-branch",
        "--force",
    ]);
    assert!(forced.status.success(), "{}", said(&forced));
    assert!(!path.exists());
}

#[test]
fn pushed_work_is_not_at_risk() {
    let p = Plane::new();
    let path = p.cut("p1");
    std::fs::write(path.join("feature.txt"), "work\n").unwrap();
    p.git(&path, &["add", "-A"]);
    p.git(&path, &["commit", "-q", "-m", "feature"]);
    // Reached by another ref, as a pushed branch's remote-tracking copy would be.
    p.git(&p.clone, &["branch", "kept", "p1"]);

    let out = p.charter(&["wt", "remove", "svc", "p1", "-w", "alpha"]);

    assert!(out.status.success(), "{}", said(&out));
}

#[test]
fn a_tree_git_cannot_read_is_not_cleared_for_removal() {
    // An unreadable tree is not a clean one: `git status` failing writes nothing to stdout.
    let p = Plane::new();
    let path = p.cut("p1");
    std::fs::write(path.join(".git"), "gitdir: /nonexistent/nowhere\n").unwrap();

    let out = p.charter(&["wt", "remove", "svc", "p1", "-w", "alpha"]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(said(&out).contains("could not determine"), "{}", said(&out));
    assert!(path.is_dir());
}

#[test]
fn delete_branch_says_whether_git_deleted_it() {
    let p = Plane::new();
    p.cut("p1");

    let out = p.charter(&[
        "wt",
        "remove",
        "svc",
        "p1",
        "-w",
        "alpha",
        "--delete-branch",
    ]);

    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("Deleted branch p1"), "{}", said(&out));
    let left = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", "refs/heads/p1"])
        .current_dir(&p.clone)
        .status()
        .unwrap();
    assert!(!left.success(), "the branch is gone");
}

#[test]
fn history_refuses_a_workspace_name_that_leaves_the_plane() {
    let p = Plane::new();

    let out = p.charter(&["wt", "history", "-w", "../.."]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
}

#[test]
fn a_piece_that_does_not_exist_is_refused() {
    let p = Plane::new();

    let out = p.charter(&["wt", "remove", "svc", "nope", "-w", "alpha"]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(said(&out).contains("no worktree 'nope'"), "{}", said(&out));
}

#[test]
fn a_deleted_folder_leaves_a_registration_that_remove_clears() {
    let p = Plane::new();
    let path = p.cut("p1");
    std::fs::remove_dir_all(&path).unwrap();

    let out = p.charter(&["wt", "remove", "svc", "p1", "-w", "alpha"]);

    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("stale"), "{}", said(&out));
    assert!(!p.listed_by_git().contains(&path.display().to_string()));
}
