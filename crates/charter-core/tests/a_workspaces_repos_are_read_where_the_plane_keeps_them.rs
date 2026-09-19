//! What the repos panel is allowed to show, against real git and a real plane.
//!
//! Two things are being held here. The **boundary**: every clone charter reads is one this
//! workspace holds, reached without passing through a link, and anything else is *said* and
//! not dropped. And the **third state**: a tree charter could not read never reads as clean.
//!
//! Where a guard could pass for the wrong reason, the test carries its own positive control
//! — the fsmonitor one runs git twice, once unhardened, so a probe that never fires is a
//! failure here rather than a test that is green whatever the code does.

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use charter_core::repos::{self, Head};
use charter_core::workspaces::Plane;

/// A second git repository, somewhere a workspace has no business reaching.
fn outside() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let at = std::fs::canonicalize(dir.path()).unwrap();
    support::git(&at, &["init", "-q", "-b", "main", "."]);
    std::fs::write(at.join("SECRET.md"), "not yours\n").unwrap();
    support::git(&at, &["add", "-A"]);
    support::git(&at, &["commit", "-q", "-m", "one"]);
    (dir, at)
}

/// The names `clones` found, in order.
fn named(found: &repos::Clones) -> Vec<&str> {
    found.repos.iter().map(|repo| repo.name.as_str()).collect()
}

// ---------------------------------------------------------------------------------------
// What is a repo                                                                          #
// ---------------------------------------------------------------------------------------

#[test]
fn a_workspaces_repos_are_the_directories_in_it_holding_a_git_directory() {
    let f = support::plane_with_clone("thing");
    // The stores a workspace always has, which are directories and are not repos.
    std::fs::create_dir_all(f.workspace().join("memory")).unwrap();
    std::fs::create_dir_all(f.workspace().join("todos")).unwrap();
    // charter's own, which start with a dot.
    std::fs::create_dir_all(f.workspace().join(".worktrees/thing")).unwrap();
    std::fs::write(f.workspace().join("workspace.json"), "{}\n").unwrap();

    let found = repos::clones(&f.plane, &f.ws).expect("the workspace reads");

    assert_eq!(named(&found), ["thing"]);
    assert!(found.refused.is_empty(), "{:?}", found.refused);
}

#[test]
fn a_linked_worktree_is_not_a_clone_because_its_git_is_a_file() {
    // Git draws this line, not charter: a clone's `.git` is a directory, a linked worktree's
    // is a file holding `gitdir:`. A worktree listed as a repo would be a second row for one
    // repository, with its own branch, which is the piece view and not this one.
    let f = support::plane_with_clone("thing");
    let piece = f.workspace().join("piece");
    support::git(
        &f.clone,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "piece",
            &piece.display().to_string(),
        ],
    );

    let found = repos::clones(&f.plane, &f.ws).expect("the workspace reads");

    assert_eq!(named(&found), ["thing"]);
}

#[test]
fn the_repos_the_manifest_names_are_read_and_a_name_that_is_not_one_is_dropped() {
    let f = support::plane_with_clone("thing");
    std::fs::write(
        f.workspace().join("workspace.json"),
        r#"{"name": "alpha", "repos": [{"name": "thing"}, {"name": "../escape"}, {"name": "later"}]}"#,
    )
    .unwrap();

    let declared = repos::declared(&Plane::open(&f.plane), &f.ws);

    // `later` is membership with no clone yet, which is worth drawing. `../escape` is not a
    // name, and a name that is not one is never shown — a panel is where a person reads
    // what the plane holds, and `../escape` is not something this plane holds.
    assert_eq!(declared, ["thing", "later"]);
}

// ---------------------------------------------------------------------------------------
// The boundary                                                                            #
// ---------------------------------------------------------------------------------------

#[test]
fn a_repo_reached_through_a_symlink_is_refused_and_said_rather_than_dropped() {
    // A committed `workspaces/<ws>/<legal-name> -> elsewhere` travels to every machine that
    // clones the plane. The name is fine, the directory above it is fine, and the only place
    // this is visible is a check on the path charter is about to hand to git.
    let f = support::plane_with_clone("thing");
    let (_keep, elsewhere) = outside();
    std::os::unix::fs::symlink(&elsewhere, f.workspace().join("alias")).unwrap();

    let found = repos::clones(&f.plane, &f.ws).expect("the workspace reads");

    assert_eq!(
        named(&found),
        ["thing"],
        "the link is not a repo of this workspace"
    );
    let (name, why) = found
        .refused
        .iter()
        .find(|(name, _)| name == "alias")
        .expect("the link is reported, not silently skipped");
    assert_eq!(name, "alias");
    assert!(why.contains("symlink"), "{why}");
}

#[test]
fn a_git_that_is_a_symlink_does_not_make_the_directory_holding_it_a_clone() {
    // The directory itself is ordinary and passes every path check. What is redirected is
    // the repository git would act on, which is a component below the one that was gated.
    let f = support::plane_with_clone("thing");
    let (_keep, elsewhere) = outside();
    let sneak = f.workspace().join("sneak");
    std::fs::create_dir_all(&sneak).unwrap();
    std::os::unix::fs::symlink(elsewhere.join(".git"), sneak.join(".git")).unwrap();

    let found = repos::clones(&f.plane, &f.ws).expect("the workspace reads");

    assert_eq!(named(&found), ["thing"]);
}

#[test]
fn a_directory_whose_name_could_not_name_a_repo_is_refused_before_it_is_joined() {
    let f = support::plane_with_clone("thing");
    // A legal directory name that is not a name charter mints. `repo_name_ok` is a question
    // about the string, so it answers the same whether or not anything is there.
    let odd = f.workspace().join("-leading");
    std::fs::create_dir_all(odd.join(".git")).unwrap();

    let found = repos::clones(&f.plane, &f.ws).expect("the workspace reads");

    assert_eq!(named(&found), ["thing"]);
    assert!(
        found.refused.iter().any(|(name, _)| name == "-leading"),
        "{:?}",
        found.refused
    );
}

#[test]
fn a_workspace_that_is_a_link_out_of_the_plane_holds_no_repos_at_all() {
    let f = support::plane_with_clone("thing");
    let (_keep, elsewhere) = outside();
    std::os::unix::fs::symlink(&elsewhere, f.plane.join("workspaces/ghost")).unwrap();

    let refusal = repos::clones(&f.plane, "ghost").expect_err("a link anchors nothing");

    assert!(
        format!("{refusal}").contains("symlink") || format!("{refusal}").contains("does not"),
        "{refusal}"
    );
}

#[test]
fn a_name_that_is_not_a_workspace_this_plane_has_reads_nothing() {
    let f = support::plane_with_clone("thing");

    assert!(repos::clones(&f.plane, "../..").is_err());
    assert!(repos::clones(&f.plane, "/etc").is_err());
}

// ---------------------------------------------------------------------------------------
// What git said                                                                           #
// ---------------------------------------------------------------------------------------

#[test]
fn a_clean_checkout_reads_as_its_branch_with_nothing_to_report() {
    let f = support::plane_with_clone("thing");

    let state = repos::state_of(&f.clone).expect("the tree reads");

    assert_eq!(state.head, Head::Branch("main".into()));
    assert_eq!(state.upstream, None);
    assert!(state.clean());
    assert_eq!((state.ahead, state.behind), (0, 0));
}

#[test]
fn a_changed_file_and_an_untracked_one_are_counted_apart() {
    let f = support::plane_with_clone("thing");
    std::fs::write(f.clone.join("README.md"), "two\n").unwrap();
    std::fs::write(f.clone.join("scratch"), "x\n").unwrap();

    let state = repos::state_of(&f.clone).expect("the tree reads");

    assert_eq!((state.tracked, state.untracked), (1, 1));
    assert!(!state.clean());
}

#[test]
fn how_far_a_branch_is_from_its_upstream_comes_back_from_real_git() {
    let f = support::plane_with_clone("thing");
    let from = f.clone.display().to_string();
    support::git(&f.workspace(), &["clone", "-q", &from, "copy"]);
    let copy = f.workspace().join("copy");
    f.commit(&copy, "local");

    let state = repos::state_of(&copy).expect("the tree reads");

    assert_eq!(state.head, Head::Branch("main".into()));
    assert_eq!(state.upstream.as_deref(), Some("origin/main"));
    assert_eq!((state.ahead, state.behind), (1, 0));
}

#[test]
fn a_detached_head_is_reported_as_the_commit_it_sits_on() {
    let f = support::plane_with_clone("thing");
    support::git(&f.clone, &["checkout", "-q", "--detach"]);

    let state = repos::state_of(&f.clone).expect("the tree reads");

    match state.head {
        Head::Detached(at) => assert!(!at.is_empty(), "the commit is named"),
        other => panic!("a detached HEAD is not {other:?}"),
    }
}

#[test]
fn a_branch_with_no_commit_on_it_yet_is_unborn_and_not_a_branch_named_after_a_sentence() {
    // `## No commits yet on main` is a sentence, not a ref. Read as a branch name it draws a
    // row saying the checkout is on a branch called "No commits yet on main".
    let f = support::plane_with_clone("thing");
    let fresh = f.workspace().join("fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    support::git(&fresh, &["init", "-q", "-b", "main", "."]);

    let state = repos::state_of(&fresh).expect("the tree reads");

    assert_eq!(state.head, Head::Unborn("main".into()));
}

#[test]
fn a_tree_charter_could_not_read_says_so_and_never_reads_as_clean() {
    // Python's status line answers all-false-and-zero on any failure, so a timeout and a
    // clean repo are the same row. `gitstate.py` exists in that same codebase to forbid it.
    let f = support::plane_with_clone("thing");
    let broken = f.workspace().join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join(".git"), "gitdir: /nonexistent/nowhere\n").unwrap();

    let refusal = repos::state_of(&broken).expect_err("a tree charter cannot read is not clean");

    let said = format!("{refusal}");
    assert!(said.contains("not the same as it being clean"), "{said}");
    // git's own words, so the operator is sent to the thing that is actually wrong.
    assert!(!refusal.why.is_empty(), "{refusal:?}");
}

#[test]
fn a_directory_that_is_not_a_repository_at_all_is_unreadable_rather_than_clean() {
    let f = support::plane_with_clone("thing");
    let plain = f.workspace().join("notes");
    std::fs::create_dir_all(&plain).unwrap();

    assert!(repos::state_of(&plain).is_err());
}

#[test]
fn the_fsmonitor_a_repository_names_is_never_run_by_the_panel() {
    // `status` is the verb that runs the fsmonitor, and the fsmonitor is a program named by
    // config. The panel runs `status` on every repo of every workspace the operator looks
    // at, so a clone somebody handed over is a clone that gets to name a program — unless
    // the call goes through the hardened runner, where `-c core.fsmonitor=false` on the
    // command line beats every config file.
    let f = support::plane_with_clone("thing");
    let marker = f.workspace().join("FSMONITOR-RAN");
    let hook = f.workspace().join("fsmonitor.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\ntouch {}\nexit 1\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    support::git(
        &f.clone,
        &["config", "core.fsmonitor", &hook.display().to_string()],
    );

    // The positive control, and the reason this test is worth anything: git run WITHOUT the
    // hardening does run the program the repository named. A version of git that did not
    // would make the assertion below pass whatever the code under test does.
    support::git(&f.clone, &["status", "--porcelain=v1", "--branch"]);
    assert!(
        marker.exists(),
        "the probe does not fire on this git, so the check below would prove nothing"
    );
    std::fs::remove_file(&marker).unwrap();

    let state = repos::state_of(&f.clone).expect("the tree still reads");

    assert!(
        !marker.exists(),
        "a program named by the repository's own config ran"
    );
    assert_eq!(state.head, Head::Branch("main".into()));
}

// ---------------------------------------------------------------------------------------
// The two halves together                                                                 #
// ---------------------------------------------------------------------------------------

#[test]
fn every_clone_a_workspace_holds_is_read_where_the_listing_said_it_was() {
    // The path that comes back from the listing is the path that was checked, and it is the
    // one git is pointed at. Building a second path from the name would be checking one
    // string and using another.
    let f = support::plane_with_clone("thing");

    let found = repos::clones(&f.plane, &f.ws).expect("the workspace reads");
    let repo = &found.repos[0];

    assert_eq!(repo.path, f.clone);
    assert_eq!(
        repos::state_of(&repo.path).expect("the tree reads").head,
        Head::Branch("main".into())
    );
    assert!(Path::new(&repo.path).starts_with(f.workspace()));
}
