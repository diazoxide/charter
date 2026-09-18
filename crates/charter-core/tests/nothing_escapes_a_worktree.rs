//! The three ways a worktree path can leave the workspace it belongs to.
//!
//! Written before the code they gate. This repo's history is why: five review rounds on M1.1
//! found containment holes, four of them the same shape — the gate one level shallower than
//! the write — and CI was green for every one of them.
//!
//! The removal cases point their symlink at a **registered worktree**, not at a plain
//! directory. Pointing at a plain directory makes `git worktree remove` refuse on its own, so
//! the test passes whether or not charter has a guard at all, and proves nothing about
//! charter. That was a real defect in an earlier draft of these tests.

mod support;

use charter_core::worktree;

// ---------------------------------------------------------------------------------------
// A repo or piece name carrying `..`                                                      #
// ---------------------------------------------------------------------------------------

#[test]
fn a_repo_or_piece_name_that_walks_out_of_a_directory_never_builds_a_path() {
    let f = support::plane_with_clone("thing");

    for bad in [
        "..",
        "../..",
        "../escape",
        "a/../../b",
        "a/b",
        "a\\b",
        "/etc",
        "C:x",
        "",
        ".",
        ".hidden",
        "-force",
        "--force",
        "a b",
        "alpha\0evil",
    ] {
        assert!(
            worktree::path_for(&f.plane, &f.ws, bad, "piece").is_err(),
            "repo {bad:?} must not build a path"
        );
        assert!(
            worktree::path_for(&f.plane, &f.ws, &f.repo, bad).is_err(),
            "piece {bad:?} must not build a path"
        );
    }
}

#[test]
fn a_name_the_gate_refuses_never_reaches_the_filesystem_at_all() {
    // Not the same test as above: this one pins that the refusal happens BEFORE any
    // directory is made. A guard that refuses after `create_dir_all` has already followed a
    // link is not a guard.
    let f = support::plane_with_clone("thing");
    let root = f.workspace().join(".worktrees");

    let _ = worktree::add(&f.plane, &f.ws, "../../../etc", "piece", None);

    assert!(
        !root.exists(),
        "nothing may be created on the way to refusing a name"
    );
}

// ---------------------------------------------------------------------------------------
// A branch name carrying `..`, or shaped like an argument                                 #
// ---------------------------------------------------------------------------------------

#[test]
fn a_branch_name_that_git_would_read_as_an_option_never_reaches_its_argv() {
    // `git check-ref-format --branch --upload-pack=…` is the injection, so delegating this
    // rule to git cannot prevent it. It is charter's.
    let f = support::plane_with_clone("thing");

    for bad in ["-force", "--upload-pack=/bin/sh", "--exec=evil", "-"] {
        let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "piece", Some(bad))
            .expect_err("a name git would read as an option is refused");
        assert!(
            format!("{refusal}").contains("option"),
            "{bad:?}: {refusal}"
        );
    }
}

#[test]
fn a_branch_name_carrying_dot_dot_is_refused() {
    let f = support::plane_with_clone("thing");

    for bad in [
        "a..b",
        "refs/../evil",
        "../escape",
        "a b",
        "x/",
        "a~1",
        "@{",
    ] {
        assert!(
            worktree::add(&f.plane, &f.ws, &f.repo, "piece", Some(bad)).is_err(),
            "{bad:?} must not become a branch"
        );
    }
}

#[test]
fn a_branch_named_at_is_never_merged_as_a_bare_name() {
    // `@` is a legal branch name, and `git merge --ff-only @` resolves HEAD rather than the
    // branch: measured on git 2.50.1 it prints "Already up to date", exits 0 and leaves HEAD
    // where it was. A bare name makes charter report a merge that landed nothing.
    let f = support::plane_with_clone("thing");
    let added = match worktree::add(&f.plane, &f.ws, &f.repo, "piece", Some("@")) {
        // Refusing `@` outright is an acceptable answer.
        Err(_) => return,
        Ok(added) => added,
    };
    f.commit(&added.path, "the piece's work");
    let before = String::from_utf8(support::git(&f.clone, &["rev-parse", "HEAD"]).stdout).unwrap();

    let merged = worktree::merge(&f.plane, &f.ws, &f.repo, "piece");

    let after = String::from_utf8(support::git(&f.clone, &["rev-parse", "HEAD"]).stdout).unwrap();
    if merged.is_ok() {
        assert_ne!(
            before, after,
            "a merge reported as successful must actually have landed the work"
        );
    }
}

// ---------------------------------------------------------------------------------------
// A removal reaching outside its workspace                                                #
// ---------------------------------------------------------------------------------------

/// A second workspace holding a **registered, live** worktree — the thing worth stealing.
///
/// Registered on purpose: `git worktree remove` refuses a path that is not a working tree, so
/// a test that points its link at a plain directory is answered by git and never asks charter
/// anything.
fn victim_worktree(f: &support::Fixture) -> std::path::PathBuf {
    let victim = f
        .plane
        .join("workspaces")
        .join("beta")
        .join(".worktrees")
        .join(&f.repo)
        .join("live");
    std::fs::create_dir_all(victim.parent().unwrap()).unwrap();
    support::git(
        &f.clone,
        &[
            "worktree",
            "add",
            "-q",
            &victim.display().to_string(),
            "-b",
            "betas-work",
        ],
    );
    assert!(
        victim.join(".git").exists(),
        "the victim is a real worktree"
    );
    victim
}

#[test]
fn a_removal_whose_path_resolves_into_another_workspace_is_refused() {
    // `contain::writable` says Ok here: the target IS under `workspaces/`. Measured, this is
    // a live deletion — `git worktree remove` resolves the link and takes workspace B's tree
    // at exit 0, with every tree-safety guard passing because it ran against B's clean tree.
    let f = support::plane_with_clone("thing");
    let victim = victim_worktree(&f);
    let mine = f.workspace().join(".worktrees").join(&f.repo);
    std::fs::create_dir_all(&mine).unwrap();
    std::os::unix::fs::symlink(&victim, mine.join("piece")).unwrap();

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", true, false)
        .expect_err("a path landing in another workspace is not this workspace's to remove");

    assert!(
        victim.exists() && victim.join(".git").exists(),
        "the other workspace's worktree is untouched: {refusal}"
    );
    assert!(
        !format!("{refusal}").is_empty(),
        "and the refusal says something"
    );
}

#[test]
fn a_worktrees_root_that_is_a_symlink_is_refused_rather_than_followed() {
    // The component whose corruption defeats every check below it. Resolving the root and
    // comparing paths against THAT is vacuous: the root resolves to wherever the link points,
    // so everything under it "starts with" it and passes.
    let f = support::plane_with_clone("thing");
    let victim = victim_worktree(&f);
    let betas_root = f.plane.join("workspaces").join("beta").join(".worktrees");
    let root = f.workspace().join(".worktrees");
    std::os::unix::fs::symlink(&betas_root, &root).unwrap();
    assert!(
        root.join(&f.repo).join("live").exists(),
        "the link really does reach the victim, or this test proves nothing"
    );

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "live", true, false)
        .expect_err("a worktree root reached through a link is refused");

    assert!(
        victim.exists(),
        "nothing behind the link is removed: {refusal}"
    );
}

#[test]
fn a_repo_directory_under_the_root_that_is_a_symlink_is_refused_rather_than_followed() {
    let f = support::plane_with_clone("thing");
    let victim = victim_worktree(&f);
    let root = f.workspace().join(".worktrees");
    std::fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(victim.parent().unwrap(), root.join(&f.repo)).unwrap();
    assert!(
        root.join(&f.repo).join("live").exists(),
        "the link really does reach the victim, or this test proves nothing"
    );

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "live", true, false)
        .expect_err("a repo directory reached through a link is refused");

    assert!(
        victim.exists(),
        "nothing behind the link is removed: {refusal}"
    );
}

#[test]
fn a_piece_replaced_by_a_symlink_after_it_was_cut_is_refused_at_removal() {
    // The gate is asked about the path NOW, not remembered from when it was created.
    let f = support::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    let victim = victim_worktree(&f);
    std::fs::remove_dir_all(&added.path).unwrap();
    std::os::unix::fs::symlink(&victim, &added.path).unwrap();

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", true, false)
        .expect_err("a piece path that became a link is refused");

    assert!(
        victim.exists() && victim.join(".git").exists(),
        "the tree the link pointed at is untouched: {refusal}"
    );
}

#[test]
fn an_ordinary_piece_is_still_created_and_removed() {
    // The guard against a containment rule so tight that the feature stops working. Every
    // test above is a refusal; without this one they would all pass on a function that
    // refuses everything.
    let f = support::plane_with_clone("thing");

    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    assert!(added.path.is_dir());
    assert_eq!(added.branch, "piece");

    worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap();
    assert!(!added.path.exists());
}
