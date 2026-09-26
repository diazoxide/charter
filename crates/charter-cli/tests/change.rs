//! `charter change create|add|drop|list|show|forget`: a cross-repo change declared and read
//! through the binary, with no network (charter#467, ADR 0060).
//!
//! Ported from the behaviour of `cli-final`'s `tests/test_commands_change.py`. Exit 2 is a
//! named refusal and 1 is something wrong, so every refusal test asserts WHICH one fired.

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

/// A plane with one workspace, `alpha`, holding the clones `svc`, `web` and `.github`.
struct Plane {
    _tmp: tempfile::TempDir,
    base: PathBuf,
    root: PathBuf,
    home: PathBuf,
}

impl Plane {
    fn new() -> Plane {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("plane");
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(".gitconfig"),
            "[commit]\n\tgpgsign = false\n[user]\n\tname = Tester\n\temail = t@e.invalid\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let plane = Plane {
            _tmp: tmp,
            base,
            root,
            home,
        };
        for repo in ["svc", "web", ".github"] {
            plane.clone_named(repo);
        }
        plane
    }

    fn clone_named(&self, repo: &str) {
        let dir = self.ws().join(repo);
        std::fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "-q", "-b", "main", "."]);
    }

    fn ws(&self) -> PathBuf {
        self.root.join("workspaces/alpha")
    }

    fn record(&self, slug: &str) -> PathBuf {
        self.ws().join("changes").join(format!("{slug}.json"))
    }

    fn read(&self, slug: &str) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.record(slug)).unwrap()).unwrap()
    }

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

    fn charter(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_charter"))
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("CHARTER_ROOT", &self.root)
            .env("HOME", &self.home)
            .env("USER", "someone")
            .env("GIT_CONFIG_GLOBAL", self.home.join(".gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("PATH", "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin")
            .output()
            .expect("charter runs")
    }

    /// `charter change <args> -w alpha`: (exit code, stdout, stderr).
    fn change(&self, args: &[&str]) -> (i32, String, String) {
        let mut all = vec!["change"];
        all.extend_from_slice(args);
        all.extend_from_slice(&["-w", "alpha"]);
        let out = self.charter(&all);
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn ok(&self, args: &[&str]) -> String {
        let (code, out, err) = self.change(args);
        assert_eq!(
            code, 0,
            "charter change {args:?}\nstdout: {out}\nstderr: {err}"
        );
        out + &err
    }
}

// ---- declaring a change ------------------------------------------------------------------

#[test]
fn create_then_add_then_show_prints_the_member_on_its_default_branch() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump the component API"]);
    plane.ok(&["add", "api-2", "svc"]);
    let rec = plane.read("api-2");
    assert_eq!(rec["members"][0]["branch"], "change/api-2");
    assert_eq!(rec["by"], "Tester");
    let (_, out, _) = plane.change(&["show", "api-2"]);
    assert!(out.contains("api-2 · 1 member(s)"), "{out}");
    assert!(out.contains("why: bump the component API"), "{out}");
    assert!(out.contains("svc  branch change/api-2"), "{out}");
    assert!(!out.contains("needs"), "{out}");
    assert!(!out.contains("excluded"), "{out}");
}

#[test]
fn an_explicit_branch_and_its_needs_are_stored_and_shown() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", "svc"]);
    plane.ok(&[
        "add", "api-2", "web", "--branch", "feat/x", "--needs", "svc",
    ]);
    let rec = plane.read("api-2");
    assert_eq!(rec["members"][1]["branch"], "feat/x");
    assert_eq!(rec["members"][1]["needs"], serde_json::json!(["svc"]));
    let (_, out, _) = plane.change(&["show", "api-2"]);
    assert!(out.contains("web  branch feat/x   needs: svc"), "{out}");
}

#[test]
fn the_record_on_disk_holds_intent_only_in_the_formats_bytes() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", "svc"]);
    let text = std::fs::read_to_string(plane.record("api-2")).unwrap();
    let rec: serde_json::Value = serde_json::from_str(&text).unwrap();
    let keys: Vec<&str> = rec
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["change", "why", "created", "by", "members", "excluded"]
    );
    assert!(text.starts_with("{\n  \"change\": \"api-2\",\n"), "{text}");
    assert!(text.ends_with("}\n"));
}

#[test]
fn dot_github_is_accepted_because_it_is_a_real_repository() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", ".github"]);
    assert_eq!(plane.read("api-2")["members"][0]["repo"], ".github");
}

#[test]
fn list_shows_every_change_with_its_counts() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump the API"]);
    plane.ok(&["add", "api-2", "svc"]);
    plane.ok(&["drop", "api-2", "web", "--why", "no API use"]);
    plane.ok(&["create", "b", "--why", "second"]);
    let (code, out, _) = plane.change(&["list"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("api-2  1 member(s), 1 excluded  ·  bump the API"),
        "{out}"
    );
    assert!(out.contains("b      0 member(s)  ·  second"), "{out}");
}

#[test]
fn an_empty_workspace_lists_nothing_and_says_how_to_start() {
    let plane = Plane::new();
    let said = plane.ok(&["list"]);
    assert!(said.contains("No changes in workspace 'alpha'"), "{said}");
    assert!(said.contains("charter change create"), "{said}");
}

// ---- the four refusals, each named, each exit 2 ------------------------------------------

#[test]
fn a_repo_with_no_clone_is_refused_and_names_charter_clone() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let (code, _, err) = plane.change(&["add", "api-2", "ghost"]);
    assert_eq!(code, 2);
    assert!(
        err.contains("ghost: no clone in workspace 'alpha'"),
        "{err}"
    );
    assert!(err.contains("charter clone ghost -w alpha"), "{err}");
    assert!(
        plane.read("api-2")["members"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_unknown_change_is_named_as_such() {
    let plane = Plane::new();
    for args in [
        &["add", "nope", "svc"][..],
        &["show", "nope"],
        &["drop", "nope", "svc", "--why", "x"],
        &["forget", "nope"],
    ] {
        let (code, _, err) = plane.change(args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(
            err.contains("no change nope in workspace 'alpha'"),
            "{args:?}: {err}"
        );
    }
}

#[test]
fn a_member_added_twice_is_named_as_such() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", "svc"]);
    let (code, _, err) = plane.change(&["add", "api-2", "svc"]);
    assert_eq!(code, 2);
    assert!(
        err.contains("'svc' is already a member of 'api-2'"),
        "{err}"
    );
}

#[test]
fn an_ordering_that_cannot_be_true_is_refused_and_nothing_is_written() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", "svc"]);
    let before = std::fs::read(plane.record("api-2")).unwrap();
    let (code, _, err) = plane.change(&["add", "api-2", "web", "--needs", "ghost"]);
    assert_eq!(code, 2);
    assert!(err.contains("member 'web' needs 'ghost'"), "{err}");
    assert_eq!(std::fs::read(plane.record("api-2")).unwrap(), before);
}

#[test]
fn a_cycle_is_refused_naming_both_members() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", "svc"]);
    plane.ok(&["add", "api-2", "web", "--needs", "svc"]);
    // A cycle can only arrive through the file; the command refuses to write one back.
    let text = std::fs::read_to_string(plane.record("api-2"))
        .unwrap()
        .replacen("\"needs\": []", "\"needs\": [\"web\"]", 1);
    std::fs::write(plane.record("api-2"), text).unwrap();
    let (code, _, err) = plane.change(&["show", "api-2"]);
    assert_eq!(
        code, 1,
        "a record that cannot be true is a defect in a file: {err}"
    );
    assert!(
        err.contains("ordering cycle: 'svc' → 'web' → 'svc'"),
        "{err}"
    );
}

// ---- what is refused with 1: the request is malformed -------------------------------------

#[test]
fn creating_a_change_twice_is_refused() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let (code, _, err) = plane.change(&["create", "api-2", "--why", "again"]);
    assert_eq!(code, 2);
    assert!(err.contains("change 'api-2' already exists"), "{err}");
    assert_eq!(plane.read("api-2")["why"], "bump");
}

#[test]
fn a_slug_that_is_not_a_name_is_refused_and_touches_nothing() {
    let plane = Plane::new();
    for slug in ["../escape", ".hidden", "a b", "a/b"] {
        let (code, _, err) = plane.change(&["create", slug, "--why", "x"]);
        assert_eq!(code, 1, "{slug}: {err}");
        assert!(err.contains("is not a change name"), "{slug}: {err}");
    }
    assert!(!plane.ws().join("changes").exists());
    assert!(!plane.root.join("workspaces/escape.json").exists());
}

#[test]
fn a_why_that_is_empty_or_not_one_line_never_reaches_the_record() {
    let plane = Plane::new();
    let (code, _, err) = plane.change(&["create", "a", "--why", "  "]);
    assert_eq!(code, 1);
    assert!(err.contains("--why is required"), "{err}");
    let (code, _, err) = plane.change(&["create", "a", "--why", "one\nforged row"]);
    assert_eq!(code, 1);
    assert!(err.contains("--why must be one plain line"), "{err}");
    assert!(!plane.record("a").exists());
    let out = plane.charter(&["change", "create", "a", "-w", "alpha"]);
    assert_ne!(
        out.status.code(),
        Some(0),
        "--why is required by the parser"
    );
}

#[test]
fn a_branch_that_would_reach_git_as_a_flag_is_refused() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let (code, _, err) = plane.change(&["add", "api-2", "svc", "--branch=-b"]);
    assert_eq!(code, 1);
    assert!(err.contains("begins with '-'"), "{err}");
    assert!(
        plane.read("api-2")["members"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_member_that_is_a_path_a_link_out_or_not_a_checkout_is_not_a_member() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let outside = plane.base.join("outside");
    std::fs::create_dir_all(outside.join(".git")).unwrap();
    std::os::unix::fs::symlink(&outside, plane.ws().join("linked")).unwrap();
    std::fs::create_dir_all(plane.ws().join("plain")).unwrap();
    for repo in ["../alpha/svc", "linked", "plain", "svc*"] {
        let (code, _, err) = plane.change(&["add", "api-2", repo]);
        assert_eq!(code, 2, "{repo}: {err}");
        assert!(
            err.contains("no clone in workspace 'alpha'"),
            "{repo}: {err}"
        );
    }
    assert!(
        plane.read("api-2")["members"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn there_is_no_all_flag_and_no_pattern() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let (code, _, err) = plane.change(&["add", "api-2", "--all"]);
    assert_ne!(code, 0);
    assert!(err.contains("--all"), "{err}");
    let help = plane.charter(&["change", "add", "--help"]);
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(!help.contains("--all"), "{help}");
}

// ---- drop and forget ----------------------------------------------------------------------

#[test]
fn a_dropped_member_joins_excluded_with_its_reason_and_a_timestamp() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", "svc"]);
    let said = plane.ok(&["drop", "api-2", "svc", "--why", "not needed after all"]);
    assert!(said.contains("'svc' excluded — 0 member(s) left"), "{said}");
    let rec = plane.read("api-2");
    assert!(rec["members"].as_array().unwrap().is_empty());
    assert_eq!(rec["excluded"][0]["repo"], "svc");
    assert_eq!(rec["excluded"][0]["why"], "not needed after all");
    assert!(
        rec["excluded"][0]["at"]
            .as_str()
            .unwrap()
            .ends_with("+00:00")
    );
    let (_, out, _) = plane.change(&["show", "api-2"]);
    assert!(out.contains("excluded:"), "{out}");
    assert!(out.contains("svc  not needed after all"), "{out}");
}

#[test]
fn a_repo_that_was_never_a_member_can_be_excluded() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let said = plane.ok(&["drop", "api-2", "web", "--why", "no API use"]);
    assert!(said.contains("'web' excluded (never a member)"), "{said}");
}

#[test]
fn dropping_a_blocker_is_refused_and_names_its_dependents() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", "svc"]);
    plane.ok(&["add", "api-2", "web", "--needs", "svc"]);
    let (code, _, err) = plane.change(&["drop", "api-2", "svc", "--why", "x"]);
    assert_eq!(code, 2);
    assert!(err.contains("'web' still needs it to land first"), "{err}");
}

#[test]
fn dropping_the_same_repo_twice_is_refused() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["drop", "api-2", "web", "--why", "x"]);
    let (code, _, err) = plane.change(&["drop", "api-2", "web", "--why", "y"]);
    assert_eq!(code, 2);
    assert!(
        err.contains("'web' is already excluded from 'api-2'"),
        "{err}"
    );
}

#[test]
fn re_adding_an_excluded_repo_lifts_the_exclusion_loudly() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["drop", "api-2", "web", "--why", "no API use"]);
    let said = plane.ok(&["add", "api-2", "web"]);
    assert!(said.contains("is lifted: no API use"), "{said}");
    assert!(
        plane.read("api-2")["excluded"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn forget_deletes_the_record_and_leaves_the_landing_log() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let log = plane.ws().join("changes/log/host.jsonl");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "{}\n").unwrap();
    let said = plane.ok(&["forget", "api-2"]);
    assert!(said.contains("change 'api-2' forgotten"), "{said}");
    assert!(!plane.record("api-2").exists());
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "{}\n");
}

// ---- untrusted records ---------------------------------------------------------------------

#[test]
fn a_record_charter_cannot_read_is_named_in_the_listing_and_the_exit_says_so() {
    let plane = Plane::new();
    plane.ok(&["create", "good", "--why", "fine"]);
    std::fs::write(
        plane.record("bad"),
        r#"{"change": "bad", "why": "x", "created": "t", "by": "b", "members": [], "excluded": [], "state": "landed"}"#,
    )
    .unwrap();
    let (code, out, err) = plane.change(&["list"]);
    assert_eq!(code, 1);
    assert!(out.contains("good"), "{out}");
    assert!(
        err.contains("bad: change 'bad': unknown key state"),
        "{err}"
    );
}

#[test]
fn an_unreadable_record_is_never_rewritten_by_add() {
    let plane = Plane::new();
    std::fs::create_dir_all(plane.ws().join("changes")).unwrap();
    std::fs::write(plane.record("bad"), "{not json").unwrap();
    let (code, _, err) = plane.change(&["add", "bad", "svc"]);
    assert_eq!(code, 1);
    assert!(err.contains("the record is not JSON"), "{err}");
    assert_eq!(
        std::fs::read_to_string(plane.record("bad")).unwrap(),
        "{not json"
    );
}

#[test]
fn a_changes_directory_that_links_out_of_the_plane_is_neither_read_nor_written() {
    let plane = Plane::new();
    let outside = plane.base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, plane.ws().join("changes")).unwrap();
    let (code, _, err) = plane.change(&["create", "api-2", "--why", "bump"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("outside the directories"), "{err}");
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
    let (code, _, err) = plane.change(&["list"]);
    assert_eq!(code, 1, "{err}");
}

#[test]
fn a_hostile_member_name_renders_as_exactly_one_row() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let hostile = "evil\u{1b}[31m";
    plane.clone_named(hostile);
    plane.ok(&["add", "api-2", hostile]);
    let (_, out, _) = plane.change(&["show", "api-2"]);
    assert!(!out.contains('\u{1b}'), "{out:?}");
    assert!(out.contains("evil\\x1b[31m"), "{out}");
}

// ---- committed exactly when the workspace is LIVE -------------------------------------------

#[test]
fn a_change_in_a_live_workspace_is_staged_by_charter_save() {
    let plane = Plane::new();
    std::fs::write(
        plane.root.join(".gitignore"),
        "/.charter/\n/workspaces/*/*\n!/workspaces/.gitkeep\n",
    )
    .unwrap();
    std::fs::write(plane.root.join("workspaces/.gitkeep"), "").unwrap();
    plane.git(&plane.root, &["init", "-q", "-b", "main", "."]);
    plane.git(&plane.root, &["add", "-A"]);
    plane.git(&plane.root, &["commit", "-q", "-m", "plane"]);
    let live = plane.charter(&["workspace", "live", "alpha"]);
    assert!(
        live.status.success(),
        "{}",
        String::from_utf8_lossy(&live.stderr)
    );
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let log = plane.ws().join("changes/log/host.jsonl");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "{}\n").unwrap();

    let save = plane.charter(&["save", "--no-push"]);
    assert!(
        save.status.success(),
        "{}",
        String::from_utf8_lossy(&save.stderr)
    );
    let committed = plane.git(&plane.root, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(
        committed
            .lines()
            .any(|l| l == "workspaces/alpha/changes/api-2.json"),
        "{committed}"
    );
    assert!(!committed.contains("changes/log"), "{committed}");
}

#[test]
fn a_change_in_a_local_workspace_is_never_committed() {
    let plane = Plane::new();
    std::fs::write(
        plane.root.join(".gitignore"),
        "/.charter/\n/workspaces/*/*\n!/workspaces/.gitkeep\n",
    )
    .unwrap();
    std::fs::write(plane.root.join("workspaces/.gitkeep"), "").unwrap();
    plane.git(&plane.root, &["init", "-q", "-b", "main", "."]);
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let status = plane.git(
        &plane.root,
        &["status", "--porcelain", "--untracked-files=all"],
    );
    assert!(!status.contains("changes"), "{status}");
}

#[test]
fn an_exclusion_that_is_not_a_repo_name_is_refused_and_the_record_stays_readable() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    let (code, _, err) = plane.change(&["drop", "api-2", "..", "--why", "x"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("exclusion"), "{err}");
    plane.ok(&["show", "api-2"]);
}

#[test]
fn an_empty_branch_falls_back_to_the_default() {
    let plane = Plane::new();
    plane.ok(&["create", "api-2", "--why", "bump"]);
    plane.ok(&["add", "api-2", "svc", "--branch", ""]);
    assert_eq!(plane.read("api-2")["members"][0]["branch"], "change/api-2");
}

#[test]
fn a_workspace_that_does_not_exist_is_refused_and_nothing_is_created() {
    let plane = Plane::new();
    let out = plane.charter(&["change", "create", "a", "--why", "x", "-w", "typo"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!plane.root.join("workspaces/typo").exists());
}
