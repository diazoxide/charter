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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
fn a_piece_name_the_next_machine_reads_as_something_else_is_never_cut() {
    charter_core::unsteered!();
    // charter-app#96. `piece_name_ok` is `contain::segment_ok` plus charter's alphabet, and
    // measured on macOS that pair says `true` to `nul` and to `alpha.` — a directory under
    // `.worktrees/<repo>/` and a branch name recorded in the clone's git config, both of
    // which reach every machine the branch does.
    let f = support::plane_with_clone("thing");

    for bad in ["nul", "NUL", "con", "aux", "lpt9", "com1.txt", "alpha."] {
        let refusal = worktree::add(&f.plane, &f.ws, &f.repo, bad, None)
            .expect_err("a piece name that means another directory elsewhere is refused");
        assert!(
            format!("{refusal}").contains("charter will not cut a worktree called"),
            "{bad:?}: {refusal}"
        );
        assert!(
            !f.workspace()
                .join(".worktrees")
                .join(&f.repo)
                .join(bad)
                .exists(),
            "{bad:?} was refused and must have left nothing behind"
        );
    }
    // The gate is the name's shape and not a ban on the letters: this one is ordinary.
    worktree::add(&f.plane, &f.ws, &f.repo, "nul-notes", None).expect("an ordinary piece name");
}

#[test]
fn a_branch_name_that_git_would_read_as_an_option_never_reaches_its_argv() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
fn a_worktrees_root_that_is_a_symlink_cuts_nothing_either() {
    charter_core::unsteered!();
    // The removal side of this is covered above; `add` has its own confinement calls and
    // needs its own test, or a mutation that deletes them goes unnoticed.
    let f = support::plane_with_clone("thing");
    let outside = tempfile::tempdir().unwrap();
    let root = f.workspace().join(".worktrees");
    std::os::unix::fs::symlink(outside.path(), &root).unwrap();

    let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect_err("a root reached through a link is refused");

    assert!(
        std::fs::read_dir(outside.path()).unwrap().next().is_none(),
        "nothing was created behind the link: {refusal}"
    );
}

#[test]
fn a_repo_directory_that_is_a_symlink_cuts_nothing_either() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let outside = tempfile::tempdir().unwrap();
    let root = f.workspace().join(".worktrees");
    std::fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join(&f.repo)).unwrap();

    let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect_err("a repo directory reached through a link is refused");

    assert!(
        std::fs::read_dir(outside.path()).unwrap().next().is_none(),
        "nothing was created behind the link: {refusal}"
    );
}

#[test]
fn a_branch_name_that_resolves_to_another_branch_is_used_under_the_name_git_printed() {
    charter_core::unsteered!();
    // `git check-ref-format --branch '@{-1}'` prints the PREVIOUS branch and exits 0. Using
    // the string the operator typed would record a base under a branch that does not exist.
    let f = support::plane_with_clone("thing");
    support::git(&f.clone, &["switch", "-q", "-c", "other"]);
    support::git(&f.clone, &["switch", "-q", "main"]);

    match worktree::add(&f.plane, &f.ws, &f.repo, "piece", Some("@{-1}")) {
        // `other` already exists, so taking git's answer means seeing that it is taken.
        Err(refusal) => {
            let said = format!("{refusal}");
            // CHARTER's refusal, which it can only produce by having resolved the name
            // itself and looked it up. Git's own error for the same input mentions `other`
            // too, so asserting on the name alone does not tell the two apart.
            assert!(
                said.contains("Pick another piece name"),
                "charter looked it up: {said}"
            );
            assert!(said.contains("branch 'other' already exists"), "{said}");
        }
        Ok(added) => {
            assert_eq!(
                added.branch, "other",
                "the name git printed, not the one passed"
            );
        }
    }
}

#[test]
fn a_tree_whose_head_cannot_be_read_is_not_called_detached() {
    charter_core::unsteered!();
    // A failed `rev-parse` became `Detached("")`, which `merge` reported as "on a detached
    // HEAD at " — an empty sha and the wrong diagnosis.
    let f = support::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    std::fs::write(
        added.path.join(".git"),
        "gitdir: /nonexistent/nowhere
",
    )
    .unwrap();

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    let said = format!("{refusal}");
    assert!(!said.contains("detached HEAD at \n"), "{said}");
    assert!(
        said.contains("could not read") || said.contains("could not determine"),
        "{said}"
    );
}

#[test]
fn a_merge_never_reads_a_tree_through_a_link_out_of_the_workspace() {
    charter_core::unsteered!();
    // `merge` does not write to the piece, so it is easy to think it needs no confinement.
    // It reads HEAD there, and then merges THAT branch into this workspace's clone — so a
    // link makes another workspace's work land here under this piece's name.
    let f = support::plane_with_clone("thing");
    let victim = victim_worktree(&f);
    // The victim is made genuinely mergeable — work on it, and a recorded base that would
    // fast-forward. Without that, `merge` refuses for an unrelated reason (no base recorded)
    // and the test passes whether or not the confinement check is there at all.
    f.commit(&victim, "betas-work");
    support::git(
        &f.clone,
        &[
            "config",
            "--replace-all",
            "branch.betas-work.charterBase",
            "main",
        ],
    );
    let mine = f.workspace().join(".worktrees").join(&f.repo);
    std::fs::create_dir_all(&mine).unwrap();
    std::os::unix::fs::symlink(&victim, mine.join("piece")).unwrap();
    let before = String::from_utf8(support::git(&f.clone, &["rev-parse", "HEAD"]).stdout).unwrap();

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece")
        .expect_err("a piece reached through a link is not this workspace's to merge");

    let after = String::from_utf8(support::git(&f.clone, &["rev-parse", "HEAD"]).stdout).unwrap();
    assert_eq!(before, after, "nothing landed: {refusal}");
}

#[test]
fn an_ordinary_piece_is_still_created_and_removed() {
    charter_core::unsteered!();
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
