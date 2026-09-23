//! What each row says, on planes built for the one thing a test is about.
//!
//! Byte-for-byte agreement with Python is the recorded `doctor-*` scenarios' job; these pin each
//! branch of each row on its own, and above all the rule the whole module is built on — a
//! row that could not look never says OK.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

/// git for a test's own setup, never the code under test: pinned identity, and no developer
/// config reaching the fixture.
fn git(dir: &Path, args: &[&str]) {
    let out = crate::forklock::output(
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid"),
    )
    .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A plane with its three baseline directories, resolved (macOS temp dirs are links).
fn plane(toml: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("charter.toml"), toml).unwrap();
    for d in ["personas", "inventory", "workspaces"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    (dir, root)
}

fn doctor(root: &Path) -> Doctor {
    Doctor::at(root, root, false, true)
}

fn row(rows: &[Row], name: &str) -> Row {
    rows.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row named {name}"))
        .clone()
}

fn one(root: &Path, name: &str) -> Row {
    row(&doctor(root).run(), name)
}

// ---- the table, the JSON, the exit ----------------------------------------------------------

#[test]
fn json_is_what_pythons_json_dumps_indent_2_prints() {
    let rows = vec![
        Row::ok("git", "git version 2.50.1"),
        Row::warn("frame", "tmux not found — «x»", "brew install tmux"),
    ];
    assert_eq!(
        json(&rows),
        "[\n  {\n    \"name\": \"git\",\n    \"status\": \"ok\",\n    \"detail\": \"git version \
         2.50.1\",\n    \"hint\": \"\"\n  },\n  {\n    \"name\": \"frame\",\n    \"status\": \
         \"warn\",\n    \"detail\": \"tmux not found \\u2014 \\u00abx\\u00bb\",\n    \"hint\": \
         \"brew install tmux\"\n  }\n]\n"
    );
}

#[test]
fn a_green_row_draws_no_remedy_and_a_yellow_one_does() {
    let rows = vec![
        Row {
            hint: "never drawn".into(),
            ..Row::ok("git", "fine")
        },
        Row::warn("memory indexes", "1 dangling", "prune it"),
    ];
    let text = table(&rows, false);
    assert!(!text.contains("never drawn"), "{text}");
    assert!(text.contains("\n        \u{2192} prune it\n"), "{text}");
}

#[test]
fn the_name_column_is_the_widest_name_and_two() {
    let rows = vec![Row::ok("git", "a"), Row::ok("memory indexes", "b")];
    let text = table(&rows, false);
    assert!(text.contains("  \u{2713}  git             a\n"), "{text}");
    assert!(text.contains("  \u{2713}  memory indexes  b\n"), "{text}");
}

#[test]
fn a_row_with_nothing_to_say_has_no_trailing_space() {
    let text = table(&[Row::fail("gh", "", "Install gh")], false);
    assert!(
        text.contains("  \u{2717}  gh\n        \u{2192} Install gh\n"),
        "{text:?}"
    );
}

#[test]
fn the_verdict_names_every_blocker_and_only_a_blocker_fails_the_exit() {
    let failing = [Row::fail("git", "", "x"), Row::warn("frame", "", "y")];
    assert!(table(&failing, false).ends_with(
        "\u{2717} 1 blocker(s): git. Fix the \u{2192} hints above, then re-run `charter doctor`.\n"
    ));
    assert_eq!(exit_code(&failing), 1);
    let warning = [Row::ok("git", ""), Row::warn("frame", "", "y")];
    assert!(
        table(&warning, false)
            .ends_with("! 1 optional item(s) pending \u{2014} see hints above.\n")
    );
    assert_eq!(exit_code(&warning), 0);
    let fine = [Row::ok("git", "")];
    assert!(
        table(&fine, false)
            .ends_with("\u{2713} All set \u{2014} you can discover and clone repos.\n")
    );
    assert!(table(&fine, false).starts_with("charter preflight:\n\n"));
}

#[test]
fn colour_is_drawn_only_when_asked_for() {
    let rows = [Row::ok("git", "a")];
    assert!(table(&rows, true).contains("\x1b[32m\u{2713}\x1b[0m"));
    assert!(!table(&rows, false).contains('\x1b'));
}

// ---- every row is there, and none that could not look is green -------------------------------

/// Python's `_FIXED_CHECK_NAMES`, with the forge pair spliced in after `git identity` as
/// `check_names` does. No row may go missing: a doctor that stops printing one tells its
/// reader the problem it reported has gone.
const PYTHON_ROWS: [&str; 39] = [
    "python3",
    "git",
    "git identity",
    "gh",
    "gh auth",
    "git auth",
    "charter.toml",
    "harness profiles",
    "schema",
    "plane root",
    "index lock",
    "session root",
    "session layer",
    "harness",
    "frame",
    "ended tab",
    "plane-root guard",
    "guard seen",
    "nested plane",
    "workspace clones",
    "workspace layer",
    "changes",
    "inventory",
    "vaults",
    "vault registry",
    "version lock",
    "memory indexes",
    "personas",
    "persona grant",
    "front door",
    "news",
    "ask rules",
    "handoff gate",
    "shadowed docs",
    "credential paths",
    "mcp",
    "plugin install",
    "plugin",
    "plugin files",
];

#[test]
fn every_row_python_prints_is_printed_in_pythons_order() {
    let (_d, root) = plane("schema = 1\n[[forge]]\nkind = \"github\"\n");
    let names: Vec<String> = doctor(&root).run().into_iter().map(|r| r.name).collect();
    assert_eq!(names, PYTHON_ROWS);
}

#[test]
fn a_row_that_did_not_look_is_never_green() {
    let (_d, root) = plane("schema = 1\n");
    for r in doctor(&root).run() {
        if r.detail.starts_with("not checked") || r.hint == deferred::DEFERRED_HINT {
            assert_eq!(r.status, Status::Warn, "{r:?}");
        }
    }
}

#[test]
fn every_deferred_row_says_it_did_not_check_and_why() {
    let (_d, root) = plane("schema = 1\n");
    let deferred: Vec<Row> = doctor(&root)
        .run()
        .into_iter()
        .filter(|r| r.hint == deferred::DEFERRED_HINT)
        .collect();
    assert!(deferred.len() >= 20, "{deferred:?}");
    for r in deferred {
        assert!(r.detail.starts_with("not checked ("), "{r:?}");
        assert!(r.detail.ends_with(')'), "{r:?}");
    }
}

#[test]
fn a_deferred_row_is_told_apart_from_a_check_that_ran_and_could_not_finish() {
    // The app's status line counts warnings, and a deferred row is a fact about this build:
    // about twenty of them, on every plane. Counting them would draw a warning count that
    // never goes down. A check that RAN and could not finish is a real warning and has to be
    // counted — so the two answers have to differ.
    assert!(deferred::row("vaults", deferred::VAULTS).deferred());
    assert!(deferred::python3().deferred());
    assert!(!Row::not_checked("git", "git timed out").deferred());
    assert!(!Row::warn("x", "y", "z").deferred());
    assert!(!Row::ok("x", "y").deferred());
    assert!(!Row::fail("x", "y", "z").deferred());

    // And across a whole run: every row the table says it did not check because of this
    // build is deferred, and no ported row is.
    let (_d, root) = plane("schema = 1\n");
    let rows = doctor(&root).run();
    let deferred: Vec<&str> = rows
        .iter()
        .filter(|r| r.deferred())
        .map(|r| r.name.as_str())
        .collect();
    assert!(deferred.contains(&"vaults"), "{deferred:?}");
    assert!(!deferred.contains(&"charter.toml"), "{deferred:?}");
    assert!(!deferred.contains(&"schema"), "{deferred:?}");
}

// ---- forges ---------------------------------------------------------------------------------

#[test]
fn the_forge_rows_are_named_for_the_forges_the_plane_declares() {
    let clis = |toml: &str| {
        let (_d, root) = plane(toml);
        config::forge_clis(&doctor(&root))
    };
    assert_eq!(clis("schema = 1\n"), ["glab"]);
    assert_eq!(clis("[[forge]]\nkind = \"github\"\n"), ["gh"]);
    assert_eq!(
        clis(
            "[[forge]]\nkind = \"github\"\n[[forge]]\nkind = \"github\"\n[[forge]]\nkind = \"gitlab\"\n"
        ),
        ["gh", "glab"]
    );
    // One bad block takes the whole declaration back to the default, as Python's does.
    assert_eq!(
        clis("[[forge]]\nkind = \"github\"\n[[forge]]\nkind = \"bitbucket\"\n"),
        ["glab"]
    );
    assert_eq!(
        clis("[[forge]]\nkind = \"github\"\nhost = \"a/b\"\n"),
        ["glab"]
    );
}

// ---- charter.toml and schema ----------------------------------------------------------------

#[test]
fn a_plane_that_parses_names_itself() {
    let (_d, root) = plane("schema = 1\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Ok);
    assert_eq!(r.detail, format!("parsed cleanly ({})", root.display()));
    assert_eq!(one(&root, "schema").detail, "up to date (schema 1)");
}

#[test]
fn a_file_that_is_not_toml_is_a_blocker_and_the_schema_row_still_reads_the_plane() {
    let (_d, root) = plane("[harness\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Fail);
    assert!(
        r.detail.contains("charter.toml is not valid TOML: "),
        "{r:?}"
    );
    assert!(!r.detail.contains('\n'), "{r:?}");
    assert!(r.hint.starts_with("Fix or remove charter.toml"), "{r:?}");
    assert_eq!(one(&root, "schema").status, Status::Ok);
}

#[test]
fn a_plane_from_the_future_is_refused_by_both_rows() {
    let (_d, root) = plane("schema = 2\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Fail);
    assert!(
        r.detail.ends_with(
            "declares schema 2, but this charter understands 1. Upgrade charter: update the app."
        ),
        "{r:?}"
    );
    assert!(r.hint.contains("see the `schema` row"), "{r:?}");
    let s = one(&root, "schema");
    assert_eq!(s.status, Status::Fail);
    assert_eq!(s.detail, r.detail);
}

#[test]
fn a_schema_that_is_not_a_number_is_quoted_back_as_python_reprs_it() {
    let (_d, root) = plane("schema = \"2\"\n");
    let r = one(&root, "schema");
    assert_eq!(r.status, Status::Fail);
    assert!(
        r.detail.contains("declares schema '2', which is not"),
        "{r:?}"
    );
    let (_d, root) = plane("schema = true\n");
    assert!(
        one(&root, "schema")
            .detail
            .contains("declares schema True,")
    );
}

#[test]
fn schema_names_a_missing_directory_and_one_occupied_by_a_file() {
    let (_d, root) = plane("schema = 1\n");
    std::fs::remove_dir(root.join("inventory")).unwrap();
    std::fs::remove_dir(root.join("workspaces")).unwrap();
    std::fs::write(root.join("workspaces"), "").unwrap();
    let r = one(&root, "schema");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "2 issue(s): missing directory: inventory/; workspaces/ is occupied by a file, not a \
         directory — reinit will refuse to touch it"
    );
}

#[test]
fn a_forge_block_that_does_not_resolve_is_named() {
    let (_d, root) = plane("[[forge]]\nkind = \"github\"\n[[forge]]\nkind = \"bitbucket\"\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(r.detail, "1 [[forge]] block(s) failed to resolve");
    assert!(
        r.hint.starts_with(
            "[[forge]] block 1: unknown forge kind 'bitbucket' — known kinds: github, gitlab — \
             those hosts are NOT covered"
        ),
        "{r:?}"
    );
}

#[test]
fn a_harness_default_charter_cannot_launch_is_named_and_a_launchable_one_is_not() {
    let (_d, root) = plane("[harness]\ndefault = \"clyde\"\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "[harness] default = \"clyde\" is not a harness charter can launch"
    );
    let (_d, root) = plane("[harness]\ndefault = \"codex\"\n");
    assert_eq!(one(&root, "charter.toml").status, Status::Ok);
}

#[test]
fn a_frame_arrangement_is_not_called_parsed_cleanly_when_nothing_looked_at_it() {
    let (_d, root) = plane("[[frame.component]]\nname = \"x\"\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(r.hint, deferred::DEFERRED_HINT);
}

#[test]
fn worktrees_declared_outside_the_plane_are_named_and_a_sibling_is_not() {
    let (_d, root) = plane("[plane]\nworktrees = \"../../far/away\"\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "[plane] worktrees points outside the plane and is being ignored"
    );
    assert!(
        r.hint.starts_with("'../../far/away' resolves to '"),
        "{r:?}"
    );
    let (_d, root) = plane("[plane]\nworktrees = \"../charter.worktrees\"\n");
    assert_eq!(one(&root, "charter.toml").status, Status::Ok);
}

#[test]
fn no_plane_is_said_out_loud_rather_than_reported_green() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let rows = doctor(&root).run();
    let r = row(&rows, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        format!("no control plane found (cwd: {})", root.display())
    );
    assert_eq!(row(&rows, "schema").detail, "no control plane found");
    assert_eq!(row(&rows, "plane root").detail, "no control plane found");
}

// ---- version lock ---------------------------------------------------------------------------

#[test]
fn a_plane_that_pins_nothing_is_fine_and_one_that_pins_is_not_called_in_sync() {
    let (_d, root) = plane("schema = 1\n");
    assert_eq!(one(&root, "version lock").detail, "not pinned");
    let (_d, root) = plane("[charter]\nversion = \"0.62.1\"\n");
    let r = one(&root, "version lock");
    assert_eq!(r.status, Status::Warn);
    assert!(r.detail.starts_with("not checked (pinned 0.62.1;"), "{r:?}");
    let (_d, root) = plane("charter = \"0.62.1\"\n");
    let r = one(&root, "version lock");
    assert_eq!(
        r.detail,
        "not checked ('str' object has no attribute 'get')"
    );
}

// ---- plane root and index lock ----------------------------------------------------------------

fn repo_plane() -> (tempfile::TempDir, PathBuf) {
    let (d, root) = plane("schema = 1\n");
    git(&root, &["init", "-q", "-b", "main", "."]);
    git(&root, &["add", "charter.toml"]);
    git(&root, &["commit", "-q", "-m", "plane"]);
    (d, root)
}

#[test]
fn a_plane_that_is_not_a_repository_says_so() {
    let (_d, root) = plane("schema = 1\n");
    assert_eq!(one(&root, "plane root").detail, "not a git repository");
    assert_eq!(
        one(&root, "index lock").detail,
        "none held on the plane's index"
    );
}

#[test]
fn a_clean_root_on_its_default_branch_is_fine() {
    let (_d, root) = repo_plane();
    let r = one(&root, "plane root");
    assert_eq!((r.status, r.detail.as_str()), (Status::Ok, "clean on main"));
}

#[test]
fn a_root_off_its_branch_and_dirty_says_both_and_where_the_work_belongs() {
    let (_d, root) = repo_plane();
    git(&root, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(root.join("charter.toml"), "schema = 1\n# edited\n").unwrap();
    let r = one(&root, "plane root");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(r.detail, "on feature, not main, 1 uncommitted file(s)");
    assert_eq!(
        r.hint,
        format!(
            "Put the root back: git -C {} checkout main. Commit control-plane content with \
             `charter save`. Anything that is not control plane belongs in a workspace clone — \
             charter workspace create <task>, then charter clone <repo>; the plane root is one \
             working tree every session shares.",
            root.display()
        )
    );
}

#[test]
fn a_detached_root_is_named() {
    let (_d, root) = repo_plane();
    git(&root, &["checkout", "-q", "--detach"]);
    let r = one(&root, "plane root");
    assert_eq!(r.detail, "detached HEAD");
    assert!(r.hint.starts_with(&format!(
        "Put the root back on a branch: git -C {} checkout main.",
        root.display()
    )));
}

#[test]
fn a_memory_commit_that_never_landed_is_a_finding_until_git_says_it_did() {
    let (_d, root) = repo_plane();
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::fs::write(
        root.join(".charter/plane-push.json"),
        r#"{"outcome": "stranded", "branch": "main", "head": "deadbeef"}"#,
    )
    .unwrap();
    let r = one(&root, "plane root");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(r.detail, "a memory commit was committed but never pushed");
    assert!(r.hint.contains("`git reset --hard origin/main`"), "{r:?}");
    assert!(r.hint.contains("plane-push.json."), "{r:?}");
}

#[test]
fn a_push_record_whose_commit_reached_the_upstream_is_spent_whatever_it_says() {
    let (d, origin) = repo_plane();
    let root = d.path().join("clone");
    git(
        d.path(),
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            root.to_str().unwrap(),
        ],
    );
    let root = std::fs::canonicalize(&root).unwrap();
    for dir in ["personas", "inventory", "workspaces", ".charter"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let head = crate::forklock::output(
        Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["rev-parse", "HEAD"]),
    )
    .unwrap();
    let head = String::from_utf8(head.stdout).unwrap();
    std::fs::write(
        root.join(".charter/plane-push.json"),
        format!(
            r#"{{"outcome": "stranded", "branch": "main", "head": "{}"}}"#,
            head.trim()
        ),
    )
    .unwrap();
    let r = one(&root, "plane root");
    assert_eq!(
        (r.status, r.detail.as_str()),
        (Status::Ok, "clean on main"),
        "{r:?}"
    );
}

#[test]
fn a_memory_commit_pushed_under_another_name_is_named_by_that_name() {
    let (_d, root) = repo_plane();
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::fs::write(
        root.join(".charter/plane-push.json"),
        r#"{"outcome": "branched", "branch": "main", "landed": "charter/abc", "url": "https://x.invalid/pr"}"#,
    )
    .unwrap();
    let r = one(&root, "plane root");
    assert_eq!(r.detail, "a memory commit went to 'charter/abc', not main");
    assert!(
        r.hint.starts_with(
            "'main' requires a pull request, so charter pushed charter/abc instead. Open it: \
             https://x.invalid/pr"
        ),
        "{r:?}"
    );
}

#[test]
fn an_index_lock_being_written_is_stated_and_a_crashed_one_is_a_warning() {
    let (_d, root) = repo_plane();
    let lock = root.join(".git/index.lock");
    std::fs::write(&lock, "").unwrap();
    let r = one(&root, "index lock");
    assert_eq!(r.status, Status::Ok);
    assert!(r.detail.starts_with("held now — 0 byte(s), "), "{r:?}");
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 3600);
    std::fs::File::options()
        .write(true)
        .open(&lock)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let r = one(&root, "index lock");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(r.detail, format!("{} — 0 byte(s), 3h old", lock.display()));
    assert!(r.hint.ends_with(&format!(
        "rm -f {}  — charter never removes a lock.",
        lock.display()
    )));
}

#[test]
fn age_is_coarse_and_never_negative() {
    assert_eq!(git::age_phrase(-5.0), "0s");
    assert_eq!(git::age_phrase(59.9), "59s");
    assert_eq!(git::age_phrase(60.0), "1m");
    assert_eq!(git::age_phrase(7200.0), "2h");
    assert_eq!(git::age_phrase(3.0 * 86_400.0), "3d");
}

// ---- nested plane ---------------------------------------------------------------------------

#[test]
fn a_plane_inside_another_planes_workspaces_is_named_whichever_way_it_was_reached() {
    let (_d, outer) = plane("schema = 1\n");
    let inner = outer.join("workspaces/ws/inner");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("charter.toml"), "schema = 1\n").unwrap();

    let walked = Doctor::at(&inner, &inner, false, true).run();
    let r = row(&walked, "nested plane");
    assert_eq!(r.status, Status::Warn);
    assert!(r.detail.starts_with("standing inside "), "{r:?}");
    assert!(r.hint.contains("does not hop outward"), "{r:?}");

    let pinned = Doctor::at(&inner, &inner, true, true).run();
    let r = row(&pinned, "nested plane");
    assert!(r.detail.starts_with("pinned inside "), "{r:?}");
    assert!(r.hint.contains("unset CHARTER_ROOT"), "{r:?}");

    assert_eq!(one(&outer, "nested plane").detail, "not nested");
    let from_inside = Doctor::at(&outer, &inner, true, true).run();
    assert!(
        row(&from_inside, "nested plane")
            .detail
            .starts_with("standing in ")
    );
}

// ---- workspace clones -----------------------------------------------------------------------

#[test]
fn a_clone_behind_its_upstream_is_named_in_whichever_workspace_it_is() {
    let (_d, root) = plane("schema = 1\n");
    assert_eq!(
        one(&root, "workspace clones").detail,
        "no clones in any workspace — nothing to check"
    );
    let origin = root.join("origin-repo");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "-q", "-b", "main", "."]);
    git(&origin, &["commit", "-q", "--allow-empty", "-m", "one"]);
    std::fs::create_dir_all(root.join("workspaces/beta")).unwrap();
    git(
        &root.join("workspaces/beta"),
        &["clone", "-q", origin.to_str().unwrap(), "svc"],
    );
    assert_eq!(
        one(&root, "workspace clones").detail,
        "1 clone(s) across all workspaces, none behind"
    );
    git(&origin, &["commit", "-q", "--allow-empty", "-m", "two"]);
    git(&root.join("workspaces/beta/svc"), &["fetch", "-q"]);
    let r = one(&root, "workspace clones");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(r.detail, "beta/svc (1 behind)");
    assert!(r.hint.starts_with("→ charter sync --all"), "{r:?}");
}

#[cfg(unix)]
#[test]
fn a_workspace_charter_cannot_list_is_named_and_never_read_as_empty() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, root) = plane("schema = 1\n");
    let shut = root.join("workspaces/shut");
    std::fs::create_dir_all(shut.join("memory")).unwrap();
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads through a mode of 000, and the question then does not arise.
    if std::fs::read_dir(&shut).is_ok() {
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let rows = doctor(&root).run();
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
    let r = row(&rows, "workspace clones");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert_eq!(
        r.detail,
        "0 clone(s) across 0 of 1 workspace(s), none behind; workspaces/shut cannot be checked"
    );
    assert_eq!(
        r.hint,
        "workspaces/shut cannot be checked — restoring read access to it clears this."
    );
    let m = row(&rows, "memory indexes");
    assert_eq!(m.status, Status::Warn, "{m:?}");
    assert!(
        m.detail
            .ends_with("; workspaces/shut/memory cannot be checked"),
        "{m:?}"
    );
}

// ---- memory indexes -------------------------------------------------------------------------

fn memory(root: &Path, base: &str, index: &str, files: &[&str]) {
    let dir = root.join(base);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("MEMORY.md"), index).unwrap();
    for f in files {
        std::fs::write(dir.join(f), "# x\n").unwrap();
    }
}

#[test]
fn indexes_that_agree_with_their_files_are_counted() {
    let (_d, root) = plane("schema = 1\n");
    memory(&root, "personas/steward/memory", "- [A](a.md)\n", &["a.md"]);
    std::fs::write(
        root.join("personas/steward/persona.md"),
        "---\nrole: x\n---\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    // steward, _shared and ws:alpha — a base with no `memory/` is still counted, as Python
    // counts it.
    assert_eq!(one(&root, "memory indexes").detail, "3 base(s) consistent");
}

#[test]
fn a_dangling_link_and_an_unindexed_file_are_named_with_their_repair() {
    let (_d, root) = plane("schema = 1\n");
    std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    memory(
        &root,
        "workspaces/alpha/memory",
        "- [Gone](gone.md)\n",
        &["kept.md"],
    );
    let r = one(&root, "memory indexes");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(r.detail, "1 dangling, 1 unindexed");
    assert_eq!(
        r.hint,
        "ws:alpha (1 dangling, 1 unindexed)  → charter workspace optimize --all --apply  (links \
         unindexed files)  → a dangling link is proposal-only: prune it, or write the memory it \
         names"
    );
}

#[cfg(unix)]
#[test]
fn an_index_linked_out_of_the_plane_is_refused_and_not_counted_consistent() {
    let (_d, root) = plane("schema = 1\n");
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "x").unwrap();
    std::fs::create_dir_all(root.join("personas/_shared/memory")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret"),
        root.join("personas/_shared/memory/MEMORY.md"),
    )
    .unwrap();
    let r = one(&root, "memory indexes");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(r.detail, "1 index(es) charter will not touch");
    assert!(r.hint.starts_with("_shared: '"), "{r:?}");
    assert!(r.hint.contains("outside the directories a control plane keeps its data in (persona-state, personas, workspaces). A committed symlink there redirects the write"), "{r:?}");
}

// ---- front door -----------------------------------------------------------------------------

#[test]
fn the_front_door_is_the_declared_persona_or_a_warning_that_there_is_none() {
    let (_d, root) = plane("[persona]\ndefault = \"steward\"\n");
    let r = one(&root, "front door");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "charter.toml [persona] default names 'steward', which is not a persona — this plane \
         has no front door and every session starts with no identity"
    );
    std::fs::create_dir_all(root.join("personas/steward")).unwrap();
    std::fs::write(root.join("personas/steward/persona.md"), "x").unwrap();
    assert_eq!(
        one(&root, "front door").detail,
        "'steward' via charter.toml [persona] default"
    );
}

#[test]
fn with_no_front_door_declared_the_personas_that_exist_are_counted() {
    let (_d, root) = plane("schema = 1\n");
    assert_eq!(one(&root, "front door").detail, "none declared");
    std::fs::write(root.join("personas/devops.md"), "x").unwrap();
    let r = one(&root, "front door");
    assert_eq!(r.status, Status::Ok);
    assert!(
        r.detail.starts_with("none declared — 1 persona(s) exist"),
        "{r:?}"
    );
    std::fs::write(root.join("personas/.default"), "devops\n").unwrap();
    assert_eq!(
        one(&root, "front door").detail,
        "'devops' via personas/.default"
    );
}

#[test]
fn a_front_door_that_walks_out_of_personas_is_not_a_persona() {
    let (_d, root) = plane("[persona]\ndefault = \"../workspaces\"\n");
    std::fs::write(root.join("workspaces.md"), "x").unwrap();
    assert_eq!(one(&root, "front door").status, Status::Warn);
}

// ---- harness profiles -------------------------------------------------------------------------

#[test]
fn the_built_in_profiles_are_listed_and_a_preflight_probes_none() {
    let (_d, root) = plane("schema = 1\n");
    let rows = doctor(&root).run();
    assert_eq!(
        row(&rows, "harness profiles").detail,
        "3 profile(s): claude, opencode, codex"
    );
    assert!(!rows.iter().any(|r| r.name.starts_with("profile ")));
}

#[test]
fn a_declared_profile_nobody_approved_is_not_probed_and_says_so() {
    let (_d, root) = plane("schema = 1\n");
    std::fs::write(
        root.join("charter.local.toml"),
        "[harness.claude-work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    )
    .unwrap();
    let rows = Doctor::at(&root, &root, false, false).run();
    let r = row(&rows, "profile claude-work");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "new, and not approved yet — charter asks before it runs a command it has not been shown"
    );
    assert_eq!(
        r.hint,
        "start a chat on 'claude-work' from the app's new-chat picker, which shows its command \
         and asks once"
    );
}

// ---- inventory ------------------------------------------------------------------------------

#[test]
fn an_inventory_is_counted_when_built_and_its_absence_is_not_a_fault_on_a_plane_that_can_clone() {
    let (_d, root) = plane("schema = 1\n");
    let r = one(&root, "inventory");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert_eq!(
        r.detail,
        "empty, and this plane's own repo could not be derived"
    );
    assert_eq!(
        r.hint,
        "Run: charter discover  (builds inventory/repos.json)."
    );

    // A plane that is a checkout of its own repo can clone that repo without `discover`, so
    // the row is not a nag: `discover` on a personal account publishes every repo the owner
    // has into a tracked file, and telling someone to do that to silence this is bad advice.
    git(&root, &["init", "-q", "-b", "main", "."]);
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/plane.git",
        ],
    );
    let r = one(&root, "inventory");
    assert_eq!(
        (r.status, r.detail.as_str()),
        (
            Status::Ok,
            "not built — this plane's own repo is clonable without it"
        ),
        "{r:?}"
    );

    std::fs::write(
        root.join("inventory/repos.json"),
        r#"{"group": "acme", "count": 2, "repos": []}"#,
    )
    .unwrap();
    assert_eq!(one(&root, "inventory").detail, "2 repos mapped");
}

// ---- one_line -------------------------------------------------------------------------------

#[test]
fn one_line_escapes_what_could_forge_a_line_and_keeps_every_glyph() {
    assert_eq!(one_line("a\nb\u{200b}c é", 160), "a\\x0ab\\u200bc é");
    assert_eq!(one_line("abcdef", 3), "abc\u{2026}");
}

// ---- the guards that turn "could not look" into a warning -----------------------------------

#[test]
fn a_git_status_that_fails_is_not_a_clean_root() {
    let (_d, root) = repo_plane();
    std::fs::write(root.join(".git/index"), "not an index").unwrap();
    let r = one(&root, "plane root");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert_eq!(r.detail, "not checked (git status exited 128)");
    assert_eq!(r.hint, NOT_CHECKED_HINT);
}

#[test]
fn a_version_pin_that_could_not_be_read_is_a_warning() {
    let (_d, root) = plane("charter = \"0.62.1\"\n");
    let r = one(&root, "version lock");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert_eq!(r.hint, NOT_CHECKED_HINT);
    let (_d, root) = plane("[harness\n");
    assert_eq!(one(&root, "version lock").status, Status::Warn);
    assert_eq!(one(&root, "front door").status, Status::Warn);
}

#[test]
fn only_a_typed_doctor_asks_git_whether_the_local_file_would_be_committed() {
    let (_d, root) = repo_plane();
    std::fs::write(
        root.join("charter.local.toml"),
        "[harness.claude-work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    )
    .unwrap();
    let typed = Doctor::at(&root, &root, false, false).run();
    let r = row(&typed, "harness profiles");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert!(
        r.detail.starts_with("git would commit charter.local.toml"),
        "{r:?}"
    );
    assert_eq!(r.hint, "charter reinit");
    let p = row(&typed, "profile claude-work");
    assert!(
        p.detail.starts_with("not probed — git would carry"),
        "{p:?}"
    );

    // The SessionStart hook pays for no git call on a config read: the file is read and its
    // profile listed, and whether git would carry it is left to the doctor a person types.
    let hook = Doctor::at(&root, &root, false, true).run();
    assert_eq!(
        row(&hook, "harness profiles").detail,
        "4 profile(s): claude, opencode, codex, claude-work"
    );
}

#[test]
fn a_session_in_a_clone_of_its_own_is_told_its_trust_is_its_own() {
    let (_d, root) = plane("schema = 1\n");
    let clone = root.join("workspaces/alpha/svc");
    std::fs::create_dir_all(&clone).unwrap();
    git(&clone, &["init", "-q", "-b", "main", "."]);
    let d = Doctor::at(&root, &clone, true, true);
    let rows = d.run();
    let r = row(&rows, "session root");
    assert!(
        r.detail.starts_with(&format!(
            "{} — not the plane ({})",
            clone.display(),
            root.display()
        )),
        "{r:?}"
    );
    let l = row(&rows, "session layer");
    assert_eq!(l.status, Status::Ok);
    assert!(
        l.detail.contains(&format!(
            "trust: {} is a git root of its own, so it carries its own trust acceptance",
            clone.display()
        )),
        "{l:?}"
    );
    assert_eq!(
        row(&doctor(&root).run(), "session root").detail,
        format!("{} — the plane", root.display())
    );
}
