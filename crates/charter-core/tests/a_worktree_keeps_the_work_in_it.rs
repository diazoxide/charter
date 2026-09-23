//! The guards that decide whether work survives, and the verbs that report what they did.
//!
//! Every test here exists because a mutation of the guard it covers survived the first
//! adversarial sweep: twelve live guards were held by nothing, and `worktree::list` was not
//! executed by the whole crate's suite — a `panic!()` at its top left all eight test binaries
//! green.

mod support;

use charter_core::worktree::{self, Base};

fn cut(f: &support::Fixture, piece: &str) -> worktree::Added {
    worktree::add(&f.plane, &f.ws, &f.repo, piece, None).expect("a piece is cut")
}

// ---------------------------------------------------------------------------------------
// Removal keeps work                                                                      #
// ---------------------------------------------------------------------------------------

#[test]
fn a_piece_with_uncommitted_changes_is_not_removed() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    std::fs::write(added.path.join("wip.txt"), "unsaved\n").unwrap();

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap_err();

    assert!(format!("{refusal}").contains("uncommitted"), "{refusal}");
    assert!(added.path.is_dir(), "and the tree is still there");
}

#[test]
fn a_piece_holding_commits_that_exist_nowhere_else_is_not_removed() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    f.commit(&added.path, "work");

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap_err();

    assert!(format!("{refusal}").contains("nowhere else"), "{refusal}");
    assert!(added.path.is_dir());
}

#[test]
fn a_piece_that_is_only_as_far_as_its_base_has_nothing_to_lose_and_goes() {
    // "Has no upstream" fires on a piece created a minute ago that has nothing to lose, and a
    // guard that fires on the harmless common case is how `--force` becomes a habit
    // (charter #104). The rule is commits reachable from NO OTHER REF.
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");

    worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap();

    assert!(!added.path.exists());
}

#[test]
fn a_tree_charter_could_not_read_is_not_a_tree_charter_clears_for_deletion() {
    // Before charter #917 an unreadable tree fell through to `git worktree remove` as clean:
    // a failed `git status` writes nothing to stdout, and `""` reads as "no changes".
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    std::fs::write(added.path.join(".git"), "gitdir: /nonexistent/nowhere\n").unwrap();

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap_err();

    let said = format!("{refusal}");
    // The DIRT sentence specifically: with `Dirt::Unknown` folded into `Clean` the removal
    // falls through to the unique-commits guard, which refuses for a different reason — and
    // a test that accepts either cannot tell the two apart.
    assert!(said.contains("uncommitted changes"), "{said}");
    assert!(added.path.exists(), "and nothing was removed");
}

#[test]
fn a_failure_to_read_unique_commits_says_that_and_not_something_else() {
    // Its own sentence. "Could not determine whether this holds uncommitted changes" is not
    // what failed, and a guard that stops a deletion for the wrong stated reason sends the
    // operator to look at the wrong thing.
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    // A tree that is clean, in a repository git still recognises, whose REF GRAPH cannot be
    // walked: `rev-list --branches` meets a ref naming an object that is not there. Removing
    // `.git/refs` wholesale is too blunt — the repo stops being a repo and an earlier guard
    // answers instead, which is a test proving something other than it claims.
    std::fs::write(
        f.clone.join(".git/refs/heads/broken"),
        "0000000000000000000000000000000000000001\n",
    )
    .unwrap();

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap_err();

    let said = format!("{refusal}");
    assert!(
        said.contains("exist nowhere else") || said.contains("could not determine"),
        "{said}"
    );
    assert!(added.path.exists());
}

#[test]
fn forcing_is_how_the_operator_says_to_discard_it() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    f.commit(&added.path, "work");

    worktree::remove(&f.plane, &f.ws, &f.repo, "piece", true, false).unwrap();

    assert!(!added.path.exists());
}

#[test]
fn removing_a_piece_keeps_its_branch() {
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap();

    let branches =
        String::from_utf8(support::git(&f.clone, &["branch", "--list", "piece"]).stdout).unwrap();
    assert!(
        branches.contains("piece"),
        "a branch costs nothing; a deleted one costs a reflog hunt"
    );
}

// ---------------------------------------------------------------------------------------
// `list` — which nothing in the suite used to execute at all                              #
// ---------------------------------------------------------------------------------------

#[test]
fn a_workspace_lists_its_own_pieces_and_no_others() {
    let f = support::plane_with_clone("thing");
    cut(&f, "mine");
    // git reports the clone itself, and any worktree registered anywhere on the machine.
    let elsewhere = tempfile::tempdir().unwrap();
    support::git(
        &f.clone,
        &[
            "worktree",
            "add",
            "-q",
            &elsewhere.path().join("theirs").display().to_string(),
            "-b",
            "theirs",
        ],
    );

    let listed = worktree::list(&f.plane, &f.ws, &f.repo).unwrap();

    let names: Vec<&str> = listed.iter().map(|p| p.piece.as_str()).collect();
    assert_eq!(names, ["mine"], "only what is under this workspace's root");
    assert_eq!(listed[0].branch.as_deref(), Some("mine"));
}

#[test]
fn a_repo_name_that_walks_out_never_reaches_git_through_list() {
    // `list` and `clone_dir` are public entry points that did not go through `path_for`, so
    // `repo` was joined straight on: `list(plane, ws, "../beta/repo")` ran git in another
    // workspace's clone.
    let f = support::plane_with_clone("thing");

    for bad in ["../beta/repo", "..", "a/b", "/etc"] {
        assert!(
            worktree::list(&f.plane, &f.ws, bad).is_err(),
            "repo {bad:?} must not be listed"
        );
    }
}

#[test]
fn a_registration_whose_directory_is_gone_is_cleared_without_pretending_to_check_a_tree() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    std::fs::remove_dir_all(&added.path).unwrap();

    let removed = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap();

    assert!(removed.was_stale);
    assert!(worktree::list(&f.plane, &f.ws, &f.repo).unwrap().is_empty());
}

// ---------------------------------------------------------------------------------------
// What `add` records, and what `merge` does with it                                       #
// ---------------------------------------------------------------------------------------

#[test]
fn a_piece_records_the_branch_it_was_cut_from() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");

    assert_eq!(added.base, Base::Branch("main".into()));
    let recorded = support::git(
        &f.clone,
        &["config", "--get-all", "branch.piece.charterBase"],
    );
    assert_eq!(String::from_utf8(recorded.stdout).unwrap().trim(), "main");
}

#[test]
fn a_piece_cut_from_a_detached_head_records_the_commit_rather_than_nothing() {
    // The spec says the sha is recorded so `merge` can say what happened at cut time. Without
    // it, `merge` blamed the wrong cause — "a piece cut by the Python charter has no such
    // record" — for a piece this code cut a minute earlier.
    let f = support::plane_with_clone("thing");
    support::git(&f.clone, &["checkout", "-q", "--detach"]);

    let added = cut(&f, "piece");

    assert!(matches!(added.base, Base::Detached(_)), "{:?}", added.base);
    let recorded = support::git(
        &f.clone,
        &["config", "--get-all", "branch.piece.charterBase"],
    );
    let recorded = String::from_utf8(recorded.stdout).unwrap();
    assert!(recorded.trim().starts_with("detached:"), "{recorded:?}");

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();
    let said = format!("{refusal}");
    assert!(said.contains("detached HEAD"), "{said}");
    assert!(
        !said.contains("Python charter"),
        "it must not blame a cause charter can see is wrong: {said}"
    );
}

#[test]
fn a_branch_that_already_exists_is_refused_and_named() {
    let f = support::plane_with_clone("thing");
    support::git(&f.clone, &["branch", "taken"]);

    let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "taken", None).unwrap_err();

    let said = format!("{refusal}");
    assert!(said.contains("branch 'taken' already exists"), "{said}");
    assert!(said.contains("Pick another piece name"), "{said}");
}

#[test]
fn a_name_git_will_not_accept_is_refused_because_git_said_so() {
    let f = support::plane_with_clone("thing");

    let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "piece", Some("a..b")).unwrap_err();

    assert!(format!("{refusal}").contains("not a name git"), "{refusal}");
}

#[test]
fn a_piece_that_fast_forwards_lands_in_the_clone() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    f.commit(&added.path, "work");

    let merged = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap();

    let head = String::from_utf8(support::git(&f.clone, &["rev-parse", "HEAD"]).stdout).unwrap();
    assert_eq!(head.trim(), merged.now);
    assert_ne!(merged.now, merged.was, "the clone's HEAD moved");
}

#[test]
fn a_merge_that_would_land_nothing_is_not_reported_as_a_merge() {
    // git says "Already up to date" at exit 0. Charter reporting that as a successful merge
    // is the lie the `@` case produced one level down.
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    assert!(
        format!("{refusal}").contains("nothing to land"),
        "{refusal}"
    );
}

#[test]
fn a_piece_that_does_not_fast_forward_is_refused_with_the_repair_in_the_worktree() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    f.commit(&added.path, "theirs");
    f.commit(&f.clone, "mine");
    let before = String::from_utf8(support::git(&f.clone, &["rev-parse", "HEAD"]).stdout).unwrap();

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    let said = format!("{refusal}");
    assert!(said.contains("does not fast-forward"), "{said}");
    assert!(
        said.contains(&added.path.display().to_string()),
        "the repair runs where the conflicts belong: {said}"
    );
    let after = String::from_utf8(support::git(&f.clone, &["rev-parse", "HEAD"]).stdout).unwrap();
    assert_eq!(before, after, "and the clone's HEAD did not move");
}

#[test]
fn two_recorded_bases_are_a_refusal_and_not_a_last_one_wins() {
    // `.git/config` is writable by anything in the clone, and `git config --get` returns the
    // LAST value at exit 0 with no warning.
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    f.commit(&added.path, "work");
    support::git(
        &f.clone,
        &["config", "--add", "branch.piece.charterBase", "evil"],
    );

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    assert!(format!("{refusal}").contains("more than one"), "{refusal}");
}

#[test]
fn a_piece_with_no_recorded_base_says_so_rather_than_calling_it_foreign() {
    // Python charter records no charterBase, so every Python-cut piece is in this state for
    // the whole cutover.
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    f.commit(&added.path, "work");
    support::git(
        &f.clone,
        &["config", "--unset-all", "branch.piece.charterBase"],
    );

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    let said = format!("{refusal}");
    assert!(said.contains("was not recorded"), "{said}");
    assert!(!said.contains("did not cut"), "{said}");
}

#[test]
fn a_clone_that_is_not_on_the_recorded_base_is_refused() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    f.commit(&added.path, "work");
    support::git(&f.clone, &["switch", "-q", "-c", "elsewhere"]);

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    let said = format!("{refusal}");
    assert!(said.contains("cut from 'main'"), "{said}");
    assert!(said.contains("switch main"), "{said}");
}

#[test]
fn a_dirty_clone_is_refused_because_a_merge_writes_into_it() {
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "piece");
    f.commit(&added.path, "work");
    std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    assert!(format!("{refusal}").contains("uncommitted"), "{refusal}");
}

// ---------------------------------------------------------------------------------------
// The plane's own shape                                                                   #
// ---------------------------------------------------------------------------------------

#[test]
fn a_plane_that_relocates_its_worktree_root_is_refused_by_name_in_every_verb() {
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");
    std::fs::write(
        f.plane.join("charter.toml"),
        "schema = 1\n[plane]\nworktrees = \"../charter.worktrees\"\n",
    )
    .unwrap();

    for said in [
        format!(
            "{}",
            worktree::add(&f.plane, &f.ws, &f.repo, "other", None).unwrap_err()
        ),
        format!("{}", worktree::list(&f.plane, &f.ws, &f.repo).unwrap_err()),
        format!(
            "{}",
            worktree::remove(&f.plane, &f.ws, &f.repo, "piece", true, false).unwrap_err()
        ),
        format!(
            "{}",
            worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err()
        ),
    ] {
        assert!(said.contains("[plane] worktrees"), "{said}");
        assert!(said.contains("unset"), "the repair is named: {said}");
    }
}

#[test]
fn a_directory_that_is_not_a_git_repository_is_refused_with_the_repair() {
    let f = support::plane_with_clone("thing");
    std::fs::create_dir_all(f.workspace().join("notarepo")).unwrap();

    let refusal = worktree::add(&f.plane, &f.ws, "notarepo", "piece", None).unwrap_err();

    assert!(
        format!("{refusal}").contains("not a git repository"),
        "{refusal}"
    );
}

#[test]
fn a_workspace_that_is_a_committed_symlink_out_of_the_plane_cuts_nothing() {
    // charter #442: a committed `workspaces/<legal-name> -> elsewhere` travels to every
    // machine that clones the plane. Canonicalising the workspace directory would follow it
    // and move the whole boundary with it — the worktree would be checked out outside the
    // plane while the operator is told it is inside.
    let f = support::plane_with_clone("thing");
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(outside.path().join("thing")).unwrap();
    support::git(
        &outside.path().join("thing"),
        &["init", "-q", "-b", "main", "."],
    );
    let ghost = f.plane.join("workspaces").join("ghost");
    std::os::unix::fs::symlink(outside.path(), &ghost).unwrap();

    let refusal = worktree::add(&f.plane, "ghost", "thing", "piece", None).unwrap_err();

    assert!(
        !outside.path().join(".worktrees").exists(),
        "nothing was created outside the plane: {refusal}"
    );
}
