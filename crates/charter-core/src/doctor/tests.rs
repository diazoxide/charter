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
            .args(crate::testgit::unsigned(args))
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
const PYTHON_ROWS: [&str; 40] = [
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
    "superseded plugin",
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
    // #373 checks python3 and the three plugin rows now; the rest are still deferred.
    assert!(deferred.len() >= 15, "{deferred:?}");
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
fn share_standing_in_for_the_mode_is_named_as_the_deprecated_alias() {
    // charter-app#292, ADR 0051.
    let (_d, root) = plane("[memory]\nshare = \"push\"\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "[memory] share is deprecated, and is being read as [plane] mode = \"push\""
    );
    assert_eq!(
        r.hint,
        "Write mode = \"push\" under [plane] in charter.toml and remove share from [memory] — \
         [plane] mode says how far every save goes, not only a memory's."
    );
}

#[test]
fn share_local_is_what_init_always_wrote_and_is_not_reported() {
    let (_d, root) = plane("[memory]\nshare = \"local\"\n");
    assert_eq!(one(&root, "charter.toml").status, Status::Ok);
}

#[test]
fn a_pr_mode_on_a_plane_whose_origin_is_no_forge_charter_knows_is_named() {
    for mode in ["pr", "pr-merge"] {
        let (_d, root) = plane(&format!("[plane]\nmode = \"{mode}\"\n"));
        let r = one(&root, "charter.toml");
        assert_eq!(r.status, Status::Warn, "{mode}");
        assert_eq!(
            r.detail,
            format!(
                "[plane] mode = \"{mode}\" opens a pull request, and this plane's origin is not \
                 a GitHub or GitLab forge charter knows"
            )
        );
        assert_eq!(
            r.hint,
            "Saves stop at a local commit, and the plane shows as blocked, until origin is on a \
             forge a [[forge]] block declares, or mode is commit or push."
        );
    }
    let (_d, root) = plane("[plane]\nmode = \"push\"\n");
    assert_eq!(one(&root, "charter.toml").status, Status::Ok);
}

#[test]
fn share_that_plane_mode_overrides_is_named_as_dead() {
    let (_d, root) = plane("[memory]\nshare = \"commit\"\n[plane]\nmode = \"push\"\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "[memory] share is deprecated, and [plane] mode overrides it, so nothing reads it"
    );
    assert_eq!(r.hint, "Remove share from [memory] in charter.toml.");
}

#[test]
fn share_that_only_this_machines_local_mode_overrides_is_not_called_dead() {
    // Every other clone still reads share, so removing it would change their mode.
    let (_d, root) = plane("[memory]\nshare = \"push\"\n");
    std::fs::write(
        root.join("charter.local.toml"),
        "[plane]\nmode = \"commit\"\n",
    )
    .unwrap();
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "[memory] share is deprecated, and is still read as [plane] mode = \"push\" by every \
         clone without this machine's charter.local.toml"
    );
    assert_eq!(
        r.hint,
        "Write mode = \"push\" under [plane] in charter.toml and remove share from [memory] — \
         [plane] mode says how far every save goes, not only a memory's."
    );
}

#[test]
fn a_save_setting_the_local_file_holds_and_nothing_reads_is_named() {
    let (_d, root) = plane("schema = 1\n");
    std::fs::write(root.join("charter.local.toml"), "[plane]\nmode = \"prr\"\n").unwrap();
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "plane.mode in charter.local.toml is not a mode — one of off, commit, push, pr, pr-merge"
    );
}

#[test]
fn a_save_setting_charter_does_not_read_is_named() {
    let (_d, root) = plane("[plane]\nmod = \"push\"\n");
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Warn);
    assert_eq!(
        r.detail,
        "plane.mod in charter.toml is not read — [plane] holds mode, branch, save_branch, sign, \
         autosave, autosave_after and worktrees"
    );
}

#[test]
fn a_pr_mode_on_a_github_origin_is_not_reported() {
    let (_d, root) =
        plane("[[forge]]\nkind = \"github\"\nowner = \"o\"\n\n[plane]\nmode = \"pr\"\n");
    git(&root, &["init", "-q"]);
    git(
        &root,
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
    let r = one(&root, "charter.toml");
    assert_eq!(r.status, Status::Ok, "{r:?}");
}

#[test]
fn a_save_finding_does_not_hide_that_the_frame_arrangement_went_unread() {
    let (_d, root) = plane("[plane]\nmode = \"pr\"\n\n[[frame.component]]\nkind = \"x\"\n");
    let r = one(&root, "charter.toml");
    assert!(r.detail.starts_with("not checked ("), "{r:?}");
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
fn a_push_record_cannot_forge_a_row_of_the_table() {
    let (_d, root) = repo_plane();
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::fs::write(
        root.join(".charter/plane-push.json"),
        r#"{"outcome": "branched", "branch": "main\n  ✓  forged branch", "landed": "x\n  ✓  forged landed", "url": "u\n  ✓  forged url"}"#,
    )
    .unwrap();
    let r = one(&root, "plane root");
    assert_eq!(
        r.detail,
        "a memory commit went to 'x\\x0a  ✓  forged landed', not main\\x0a  ✓  forged branch"
    );
    assert!(r.hint.contains("Open it: u\\x0a  ✓  forged url "), "{r:?}");
    let table = super::table(std::slice::from_ref(&r), false);
    assert!(!table.contains("\n  ✓  forged"), "{table}");

    std::fs::write(
        root.join(".charter/plane-push.json"),
        r#"{"outcome": "stranded", "branch": "main\n  ✓  forged branch"}"#,
    )
    .unwrap();
    let r = one(&root, "plane root");
    assert!(!r.hint.contains('\n'), "{r:?}");
    assert!(
        r.hint.contains("origin/main\\x0a  ✓  forged branch`"),
        "{r:?}"
    );
}

/// The head of `root`, as a PR mode's push record names it.
fn head_of(root: &Path) -> String {
    let head = crate::forklock::output(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "HEAD"]),
    )
    .unwrap();
    String::from_utf8(head.stdout).unwrap().trim().to_string()
}

#[test]
fn a_pr_modes_open_pull_request_is_where_its_commits_are_meant_to_wait() {
    // `pr` and `pr-merge` push to the save branch and wait on a PR by design (charter-app#298):
    // not a memory commit gone astray.
    let (_d, root) = repo_plane();
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::fs::write(
        root.join(".charter/plane-push.json"),
        format!(
            r#"{{"outcome": "pr-open", "branch": "main", "landed": "charter/save/mac", "url": "https://x.invalid/pull/12", "number": 12, "head": "{}"}}"#,
            head_of(&root)
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
fn a_pr_mode_save_that_is_blocked_says_why_in_its_own_words() {
    let (_d, root) = repo_plane();
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::fs::write(
        root.join(".charter/plane-push.json"),
        format!(
            r#"{{"outcome": "blocked", "branch": "main", "detail": "pull request #12 was closed without merging", "head": "{}"}}"#,
            head_of(&root)
        ),
    )
    .unwrap();
    let r = one(&root, "plane root");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert_eq!(r.detail, "the plane's save is blocked");
    assert!(
        r.hint
            .contains("pull request #12 was closed without merging"),
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

#[test]
fn a_workspace_name_with_a_newline_in_it_cannot_forge_a_row() {
    // A chat can make a directory under `workspaces/` with any name (#353).
    let (_d, root) = plane("schema = 1\n");
    let ws = "x\n  \u{2713}  forged";
    memory(
        &root,
        &format!("workspaces/{ws}/memory"),
        "- [Gone](gone.md)\n",
        &[],
    );
    let r = one(&root, "memory indexes");
    assert!(!r.hint.contains('\n'), "{r:?}");
    assert!(
        r.hint
            .starts_with("ws:x\\x0a  \u{2713}  forged (1 dangling"),
        "{r:?}"
    );

    let origin = root.join("origin-repo");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "-q", "-b", "main", "."]);
    git(&origin, &["commit", "-q", "--allow-empty", "-m", "one"]);
    git(
        &root.join("workspaces").join(ws),
        &["clone", "-q", origin.to_str().unwrap(), "svc"],
    );
    git(&origin, &["commit", "-q", "--allow-empty", "-m", "two"]);
    git(
        &root.join("workspaces").join(ws).join("svc"),
        &["fetch", "-q"],
    );
    let r = one(&root, "workspace clones");
    assert_eq!(r.detail, "x\\x0a  \u{2713}  forged/svc (1 behind)");
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

// ---- routing (retired, charter#369) -----------------------------------------------------------

#[test]
fn a_persona_still_declaring_routing_is_read_and_told_the_key_is_ignored() {
    let (_d, root) = plane("[persona]\ndefault = \"steward\"\n");
    assert!(
        doctor(&root).run().iter().all(|r| r.name != "routing"),
        "no row where nothing declares it"
    );
    std::fs::create_dir_all(root.join("personas/steward")).unwrap();
    std::fs::write(
        root.join("personas/steward/persona.md"),
        "---\nname: steward\nrouting: require\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(root.join("personas/ops.md"), "---\nrouting: advise\n---\n").unwrap();

    let rows = doctor(&root).run();

    assert_eq!(
        row(&rows, "front door").detail,
        "'steward' via charter.toml [persona] default"
    );
    let r = row(&rows, "routing");
    assert_eq!(r.status, Status::Ok);
    assert_eq!(
        r.detail,
        "ignored — `routing:` is retired; personas are offered to the harness as sub-agents \
         (declared by ops, steward)"
    );
    assert_eq!(r.hint, "");
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    let at = names.iter().position(|n| *n == "front door").unwrap();
    assert_eq!(names[at + 1], "routing", "{names:?}");
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

#[test]
fn the_codex_note_says_how_the_app_arms_codex_and_that_a_trusted_project_config_is_read() {
    // #354: the app arms Codex with session `-c hooks.*` flags, not a plugin, and Codex reads
    // a project `.codex/config.toml` once the project is trusted (codex-cli 0.147.0).
    let (_d, root) = plane("schema = 1\n");
    let l = one(&root, "session layer");
    let codex = l
        .detail
        .split('\n')
        .find(|line| line.contains("codex: "))
        .unwrap_or_else(|| panic!("no codex line: {l:?}"));
    assert!(!codex.contains("is ignored"), "{codex}");
    assert!(!codex.contains("the plugin"), "{codex}");
    assert!(codex.contains("`-c hooks.*`"), "{codex}");
    assert!(codex.contains("once the project is trusted"), "{codex}");
}

#[test]
fn a_plane_format_up_to_this_charters_own_is_read_and_one_past_it_is_refused() {
    for (schema, read) in [
        ("", true),
        ("schema = 1\n", true),
        ("schema = 0\n", true),
        ("schema = -3\n", true),
        ("schema = 2\n", false),
    ] {
        let (_d, root) = plane(schema);
        let config = Config::load(&root);
        assert_eq!(config.table().is_some(), read, "{schema:?}: {config:?}");
        assert_eq!(
            matches!(config, Config::Refused(_)),
            !read,
            "{schema:?}: {config:?}"
        );
    }
}

#[test]
fn a_path_is_shortened_to_the_home_it_is_under_and_never_to_one_it_names() {
    let home = crate::profiles::home().expect("the test runs with a HOME");
    assert_eq!(short_path(&home.join("plane/x")), "~/plane/x");
    assert_eq!(short_path(&home), "~/.");
    assert_eq!(
        short_path(Path::new("/nowhere/near/home")),
        "/nowhere/near/home"
    );
    // A segment starting `~` is a home to a shell, so such a path is never abbreviated.
    assert_eq!(
        short_path(&home.join("~odd")),
        home.join("~odd").display().to_string()
    );
}

#[test]
fn a_toml_diagnostic_is_one_line_with_its_position_and_its_reason_and_no_drawing() {
    let one = |text: &str| toml_error(&text.parse::<toml::Table>().unwrap_err());
    assert_eq!(
        one("schema = [\n"),
        "TOML parse error at line 1, column 11: unclosed array, expected `]`"
    );
    assert_eq!(
        one("a = 1\na = 2\n"),
        "TOML parse error at line 2, column 1: duplicate key"
    );
    let (_d, root) = plane("a = 1\na = 2\n");
    let Config::Malformed(why) = Config::load(&root) else {
        panic!("a duplicate key is not TOML");
    };
    assert!(
        why.ends_with(
            "charter.toml is not valid TOML: TOML parse error at line 2, column 1: duplicate key"
        ),
        "{why}"
    );
}

#[test]
fn the_name_column_is_a_floor_and_a_wider_name_pushes_only_its_own_row() {
    let r = Row::ok("git", "2.50");
    // Narrower than the name: the name still gets its two spaces, as at exactly its width.
    assert_eq!(render(&r, 0, false), render(&r, 5, false));
    assert!(
        render(&r, 0, false).ends_with("git  2.50"),
        "{}",
        render(&r, 0, false)
    );
    assert!(render(&r, 8, false).ends_with("git     2.50"));
}

// ---- git auth: the one-credential policy, checked and never applied (charter-app#198) --------

/// A clone at `workspaces/alpha/<name>` whose `origin` is `url`.
fn clone_with_origin(root: &Path, name: &str, url: &str) -> PathBuf {
    let clone = root.join("workspaces/alpha").join(name);
    std::fs::create_dir_all(&clone).unwrap();
    git(&clone, &["init", "-q", "-b", "main", "."]);
    git(&clone, &["remote", "add", "origin", url]);
    clone
}

#[test]
fn git_auth_is_green_when_every_repo_carries_its_forges_token_only_policy() {
    let (_d, root) = plane("schema = 1\n");
    let clone = clone_with_origin(&root, "svc", "https://github.com/acme/svc.git");
    crate::gitpolicy::apply(&clone, &root);

    let r = one(&root, "git auth");

    assert_eq!(r.status, Status::Ok, "{r:?}");
    assert_eq!(
        r.detail,
        "token-only across 1 repo(s) (each forge's own HTTPS token; no SSH/signing)"
    );
}

#[test]
fn git_auth_names_a_drifted_clone_and_the_command_that_fixes_it() {
    let (_d, root) = plane("schema = 1\n");
    clone_with_origin(&root, "svc", "https://github.com/acme/svc.git");

    let r = one(&root, "git auth");

    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert_eq!(r.detail, "1/1 repo(s) not token-only: svc");
    assert_eq!(
        r.hint,
        "Apply the single-credential policy to every clone: charter git-policy --apply"
    );
}

#[test]
fn git_auth_only_reads_and_never_applies_the_policy_it_checks() {
    let (_d, root) = plane("schema = 1\n");
    let clone = clone_with_origin(&root, "svc", "https://github.com/acme/svc.git");
    let before = std::fs::read_to_string(clone.join(".git/config")).unwrap();

    one(&root, "git auth");

    assert_eq!(
        std::fs::read_to_string(clone.join(".git/config")).unwrap(),
        before
    );
    assert!(!crate::gitpolicy::check(&clone, &root).is_empty());
}

#[test]
fn git_auth_does_not_send_an_unmanaged_forge_to_an_apply_that_skips_it() {
    let (_d, root) = plane("schema = 1\n");
    clone_with_origin(&root, "svc", "https://git.nowhere.example/acme/svc.git");

    let r = one(&root, "git auth");

    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert_eq!(r.detail, "1/1 repo(s) not token-only: svc");
    assert!(
        r.hint.starts_with("1 repo(s) have an unrecognised forge"),
        "{r:?}"
    );
}

#[test]
fn git_auth_splits_its_hint_between_fixable_and_unmanaged_repos_and_names_at_most_three() {
    let (_d, root) = plane("schema = 1\n");
    for name in ["a", "b", "c"] {
        clone_with_origin(&root, name, &format!("https://github.com/acme/{name}.git"));
    }
    clone_with_origin(&root, "d", "https://git.nowhere.example/acme/d.git");

    let r = one(&root, "git auth");

    assert_eq!(r.detail, "4/4 repo(s) not token-only: a, b, c …");
    assert!(
        r.hint.starts_with(
            "charter git-policy --apply fixes 3 drifted repo(s); 1 more have an unrecognised forge"
        ),
        "{r:?}"
    );
}

#[cfg(unix)]
#[test]
fn git_auth_names_a_workspace_it_cannot_read_rather_than_counting_it_clean() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, root) = plane("schema = 1\n");
    let clone = clone_with_origin(&root, "svc", "https://github.com/acme/svc.git");
    crate::gitpolicy::apply(&clone, &root);
    let shut = root.join("workspaces/beta");
    std::fs::create_dir_all(&shut).unwrap();
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();

    let r = one(&root, "git auth");
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert_eq!(
        r.detail,
        "token-only across 1 repo(s); 1 director(ies) under workspaces/ cannot be checked"
    );
    assert_eq!(
        r.hint,
        "workspaces/beta cannot be checked — restoring read access to it clears this."
    );
}

// ---- plugin rows (#373, #374) ---------------------------------------------------------------

/// A machine inside `dir`: Claude Code's and Codex's folders exist, the bundle is the
/// repository's own plugin, and the binary is a file that exists.
fn plugin_machine(dir: &Path) -> crate::plugin_install::Machine {
    std::fs::create_dir_all(dir.join("claude")).unwrap();
    std::fs::create_dir_all(dir.join("codex")).unwrap();
    std::fs::write(dir.join("charter-bin"), "").unwrap();
    crate::plugin_install::Machine {
        claude_config: dir.join("claude"),
        codex_home: dir.join("codex"),
        charter_dir: dir.join("config/charter"),
        binary: dir.join("charter-bin"),
        bundle: Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../app/src-tauri/plugin")
                .canonicalize()
                .unwrap(),
        ),
    }
}

fn plugin_row(root: &Path, m: &crate::plugin_install::Machine, name: &str) -> Row {
    let rows = Doctor::at(root, root, true, false)
        .with_machine(m.clone())
        .run();
    row(&rows, name).clone()
}

#[test]
fn the_python3_row_is_green_now_the_python_charter_is_retired() {
    let (_d, root) = plane("schema = 1\n");
    let r = one(&root, "python3");
    assert_eq!(r.status, Status::Ok, "{r:?}");
    assert!(r.detail.contains("retired"), "{r:?}");
}

#[test]
fn a_machine_without_the_plugin_is_told_how_to_install_it_and_one_with_it_passes() {
    let (d, root) = plane("schema = 1\n");
    let m = plugin_machine(&d.path().canonicalize().unwrap().join("machine"));
    let r = plugin_row(&root, &m, "plugin install");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert!(
        r.detail.starts_with("not installed for claude and codex"),
        "{r:?}"
    );
    assert!(r.hint.contains("charter plugin install"), "{r:?}");

    let out = crate::plugin_install::run(&m, crate::plugin_install::Verb::Install, &[], false);
    assert!(!crate::plugin_install::failed(&out));
    for name in [
        "plugin install",
        "plugin",
        "plugin files",
        "superseded plugin",
    ] {
        let r = plugin_row(&root, &m, name);
        assert_eq!(r.status, Status::Ok, "{r:?}");
    }
    assert_eq!(
        plugin_row(&root, &m, "plugin install").detail,
        "installed for claude and codex"
    );
}

#[test]
fn an_older_copy_is_stale_and_one_whose_charter_is_gone_is_named() {
    let (d, root) = plane("schema = 1\n");
    let mut m = plugin_machine(&d.path().canonicalize().unwrap().join("machine"));
    crate::plugin_install::run(&m, crate::plugin_install::Verb::Install, &[], false);
    std::fs::remove_file(&m.binary).unwrap();
    let r = plugin_row(&root, &m, "plugin files");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert!(r.detail.contains("which is not there"), "{r:?}");
    assert!(r.hint.contains("lets the tool call through"), "{r:?}");

    // Another charter asking — a development build, one on PATH — is not a stale copy.
    m.binary = d.path().join("machine/elsewhere");
    assert_eq!(plugin_row(&root, &m, "plugin").status, Status::Ok);

    // A copy an older app wrote, whose skills differ from this one's, is.
    std::fs::write(
        m.charter_dir.join("plugin/skills/handoff/SKILL.md"),
        "an older skill\n",
    )
    .unwrap();
    let r = plugin_row(&root, &m, "plugin");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert!(
        r.detail
            .starts_with("installed for claude, but not what this charter would install now"),
        "{r:?}"
    );
}

#[test]
fn inside_a_chat_a_missing_install_is_said_without_a_warning() {
    let (d, root) = plane("schema = 1\n");
    let m = plugin_machine(&d.path().canonicalize().unwrap().join("machine"));
    let rows = Doctor::at(&root, &root, true, true).with_machine(m).run();
    let r = row(&rows, "plugin install");
    assert_eq!(r.status, Status::Ok, "{r:?}");
    assert!(
        r.detail.starts_with("not installed for claude and codex"),
        "{r:?}"
    );
}

#[test]
fn a_workspace_layer_that_still_carries_the_retired_plugin_is_named_with_its_repair() {
    let (d, root) = plane("schema = 1\n");
    let m = plugin_machine(&d.path().canonicalize().unwrap().join("machine"));
    std::fs::create_dir_all(root.join("workspaces/alpha/.claude")).unwrap();
    std::fs::write(
        root.join("workspaces/alpha/.claude/settings.json"),
        r#"{"enabledPlugins": {"charter@charter": true}}"#,
    )
    .unwrap();
    let r = plugin_row(&root, &m, "superseded plugin");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert!(
        r.detail.contains("workspaces/alpha/.claude/settings.json"),
        "{r:?}"
    );
    assert!(r.hint.contains("charter workspace reinit --all"), "{r:?}");
}

#[test]
fn every_file_that_enables_the_retired_plugin_is_named() {
    let (d, root) = plane("schema = 1\n");
    let m = plugin_machine(&d.path().canonicalize().unwrap().join("machine"));
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    std::fs::write(
        root.join(".claude/settings.json"),
        r#"{"enabledPlugins": {"charter@charter": true}}"#,
    )
    .unwrap();
    std::fs::write(
        m.codex_home.join("config.toml"),
        "[plugins.\"charter@charter\"]\nenabled = true\n",
    )
    .unwrap();
    let r = plugin_row(&root, &m, "superseded plugin");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert!(
        r.detail
            .contains(&root.join(".claude/settings.json").display().to_string()),
        "{r:?}"
    );
    assert!(
        r.detail
            .contains(&m.codex_home.join("config.toml").display().to_string()),
        "{r:?}"
    );
    assert!(r.hint.contains("charter plugin install"), "{r:?}");

    std::fs::write(root.join(".claude/settings.json"), "{}").unwrap();
    std::fs::write(m.codex_home.join("config.toml"), "").unwrap();
    assert_eq!(
        plugin_row(&root, &m, "superseded plugin").status,
        Status::Ok
    );
}

#[test]
fn a_doctor_a_test_names_never_reads_this_machines_harness_config() {
    let (_d, root) = plane("schema = 1\n");
    for name in [
        "plugin install",
        "plugin",
        "plugin files",
        "superseded plugin",
    ] {
        let r = one(&root, name);
        assert!(r.detail.starts_with("not checked"), "{r:?}");
        assert_eq!(
            r.status,
            Status::Warn,
            "a row that did not look is never green: {r:?}"
        );
    }
}
