//! The two halves of the CI column, tested against each other.
//!
//! `glrefresh` writes `.charter/cache/glstate.json` and `cistate` reads it. The key is a PATH
//! STRING, so the only thing holding the two together is that both spell the same checkout the
//! same way — and nothing in either module's own tests can see that, because each one writes
//! the key the other is about to build.
//!
//! So these tests never write a fixture cache. They run the real refresher and then ask the
//! real reader, which is the only arrangement in which a disagreement about the key shows up
//! as a failure rather than as a CI column that is permanently, silently empty.
//!
//! **Nothing here reaches a forge.** The clones have no `origin`, so `state_for_repo` degrades
//! to the empty state — which is exactly the entry a plane with an unreachable forge gets, and
//! is still an entry the reader must find.

use std::path::{Path, PathBuf};

use charter_core::cistate::{self, Reading};
use charter_core::repos::{self, Head};
use charter_core::{glrefresh, worktree::git};

/// A plane with one workspace holding two clones, each on a branch of its own.
///
/// The plane is a directory INSIDE the temporary one, so a test can put a link beside it
/// without writing into the shared temp directory every other test is using.
fn plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let here = std::fs::canonicalize(dir.path()).unwrap();
    let at = here.join("plane");
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(at.join("charter.toml"), "schema = 1\n").unwrap();
    clone(&at.join("workspaces/alpha/svc"), "main");
    clone(&at.join("workspaces/alpha/tool"), "release/1.2");
    (dir, at)
}

/// A directory `repos::clones` counts as a clone, on `branch`.
///
/// A `.git` DIRECTORY holding a `HEAD`, which is the whole of what both sides read here:
/// `clones` asks whether `.git` is a directory, and `branch_of` reads `HEAD` out of it.
/// `a_real_checkout_…` below is the one that makes git itself write both.
fn clone(at: &Path, branch: &str) {
    std::fs::create_dir_all(at.join(".git")).unwrap();
    std::fs::write(at.join(".git/HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
}

/// Every clone, as the PANEL finds them.
fn rows(plane: &Path) -> Vec<repos::Repo> {
    repos::clones(plane, "alpha")
        .expect("the workspace reads")
        .repos
}

/// One instant, well inside the window a cached answer is served for.
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

fn refresh_alpha(at: &Path) {
    glrefresh::refresh(at, &glrefresh::trees(at, "alpha").unwrap().trees, now());
}

#[test]
fn what_the_refresher_wrote_is_what_the_panel_finds() {
    let (_keep, at) = plane();

    refresh_alpha(&at);

    let cache = cistate::read(&at).expect("the cache the refresher just wrote reads");
    assert!(!cache.is_empty(), "the refresher wrote no entries at all");
    for repo in rows(&at) {
        let branch = glrefresh::branch_of(&repo.path);
        let reading = cache.about(&repo.path, &branch);
        assert!(
            matches!(reading, Reading::Fetched { .. }),
            "the panel cannot find {}'s entry on {branch}: {reading:?}",
            repo.name
        );
    }
}

#[test]
fn a_real_checkout_is_read_the_same_way_by_git_status_and_by_head() {
    // The two sides read the branch out of two different places — the refresher out of `HEAD`
    // with no subprocess, the panel out of `git status --branch` — and `cistate` drops any
    // entry whose branch does not match what it was handed. So the agreement has to be
    // checked against git itself once, not only against a HEAD this test wrote.
    let dir = tempfile::tempdir().unwrap();
    let at = std::fs::canonicalize(dir.path()).unwrap().join("plane");
    let tree = at.join("workspaces/alpha/svc");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(at.join("charter.toml"), "schema = 1\n").unwrap();
    let made = git::run(&tree, &["init", "-q", "-b", "release/1.2", "."], git::READ)
        .expect("git runs in the test environment");
    assert!(made.ok(), "git init said: {}", made.err);

    refresh_alpha(&at);

    let drawn = repos::state_of(&tree).expect("git reads the tree it just made");
    // Unborn, because nothing is committed — which is still a branch, and still the one the
    // panel asks the cache about.
    let branch = match &drawn.head {
        Head::Branch(name) | Head::Unborn(name) => name.clone(),
        other => panic!("git named no branch: {other:?}"),
    };
    assert_eq!(branch, glrefresh::branch_of(&tree), "two readings of HEAD");
    assert!(
        matches!(
            cistate::read(&at).unwrap().about(&tree, &branch),
            Reading::Fetched { .. }
        ),
        "the panel's branch does not match the one the refresher recorded"
    );
}

#[test]
fn the_branch_the_refresher_recorded_keeps_the_slashes_in_its_name() {
    let (_keep, at) = plane();

    refresh_alpha(&at);

    let cache = cistate::read(&at).expect("the cache reads");
    let tool = at.join("workspaces/alpha/tool");
    assert!(
        matches!(cache.about(&tool, "release/1.2"), Reading::Fetched { .. }),
        "the slash in the branch name was not kept"
    );
    assert!(
        matches!(cache.about(&tool, "release"), Reading::NotFetched(_)),
        "an entry was served for a branch this checkout is not on"
    );
}

#[test]
fn a_plane_reached_under_another_spelling_still_finds_its_own_entries() {
    // `/tmp` is a link to `/private/tmp` on macOS and `$CHARTER_ROOT` may be set to either, so
    // the refresher and the app routinely spell one checkout two ways. This is the case
    // `cistate`'s `contain::resolved` fallback exists for, driven end to end: written under
    // one spelling of the plane, read under the other.
    let (_keep, at) = plane();
    let aliased = at.parent().unwrap().join("plane-alias");
    std::os::unix::fs::symlink(&at, &aliased).unwrap();

    refresh_alpha(&at);

    let cache = cistate::read(&aliased).expect("the cache reads through the alias");
    let svc = aliased.join("workspaces/alpha/svc");
    let reading = cache.about(&svc, "main");
    assert!(
        matches!(reading, Reading::Fetched { .. }),
        "the entry written under the real path was not found under the alias: {reading:?}"
    );
}

#[test]
fn an_unreachable_forge_still_leaves_an_entry_that_names_the_branch_and_the_instant() {
    // The distinction `cistate` is built around: "nobody has looked" and "the last refresh
    // recorded no pipeline" are different answers, and only the second of them is an entry.
    // A refresh that could reach no forge must still write one, or every panel on a plane
    // whose token expired reads as never refreshed.
    let (_keep, at) = plane();

    refresh_alpha(&at);

    let cache = cistate::read(&at).expect("the cache reads");
    match cache.about(&at.join("workspaces/alpha/svc"), "main") {
        Reading::Fetched { state, change, .. } => {
            assert_eq!(state, None, "no forge answered, so there is no CI word");
            assert_eq!(change, None);
        }
        other => panic!("a refresh that reached no forge wrote no entry: {other:?}"),
    }
}

#[test]
fn a_second_refresh_keeps_the_entries_the_first_one_wrote() {
    // The cache is read and rewritten whole, so a workspace refreshed now must not blank the
    // rows of one refreshed a minute ago. Python's `refresh` loads before it writes for this
    // reason, and a writer that started from an empty document would pass every test above.
    let (_keep, at) = plane();
    clone(&at.join("workspaces/beta/other"), "main");

    glrefresh::refresh(&at, &glrefresh::trees(&at, "beta").unwrap().trees, now());
    refresh_alpha(&at);

    let cache = cistate::read(&at).expect("the cache reads");
    assert!(
        matches!(
            cache.about(&at.join("workspaces/beta/other"), "main"),
            Reading::Fetched { .. }
        ),
        "refreshing alpha dropped beta's row"
    );
}

#[test]
fn the_trees_refreshed_are_the_rows_the_panel_draws() {
    // Python's `workspace.repo_trees` is deliberately the one list both sides use: "a repo can
    // never be drawn without its forge state having been fetched, or fetched without being
    // drawn". This is that property, asked of the two functions the app actually calls.
    let (_keep, at) = plane();

    let refreshed = glrefresh::trees(&at, "alpha").unwrap().trees;
    let drawn: Vec<PathBuf> = rows(&at).into_iter().map(|repo| repo.path).collect();

    assert!(!drawn.is_empty(), "the fixture has no rows to compare");
    for row in &drawn {
        assert!(
            refreshed.contains(row),
            "{} is drawn and never refreshed",
            row.display()
        );
    }
}

#[test]
fn a_directory_charter_will_not_run_git_in_is_named_rather_than_skipped() {
    // `repos::clones` refuses a `.git` that is a symlink, because the repository git would act
    // on is not the one inside the workspace. The refresh inherits that — and has to CARRY the
    // reason, because a tree that silently disappears from the refresh is a row whose CI cell
    // stays empty with nothing anywhere to say why.
    let (_keep, at) = plane();
    let elsewhere = at.join("workspaces/alpha/svc/.git");
    let planted = at.join("workspaces/alpha/planted");
    std::fs::create_dir_all(&planted).unwrap();
    std::os::unix::fs::symlink(&elsewhere, planted.join(".git")).unwrap();

    let found = glrefresh::trees(&at, "alpha").unwrap();

    assert!(
        !found.trees.contains(&planted),
        "git was run in a tree reached through a link"
    );
    assert!(
        found
            .refused
            .iter()
            .any(|(name, why)| name == "planted" && why.contains("symlink")),
        "the refusal was dropped: {:?}",
        found.refused
    );
}

#[test]
fn a_worktree_is_refreshed_beside_the_clone_it_was_cut_from() {
    // A worktree carries its own branch, so it carries its own pipeline and its own open
    // change. Python refreshes them for that reason, and a port that listed only the clones
    // would leave every worktree row's CI cell empty on a monorepo plane.
    let (_keep, at) = plane();
    let piece = at.join("workspaces/alpha/.worktrees/svc/feature-x");
    std::fs::create_dir_all(&piece).unwrap();
    std::fs::write(piece.join(".git"), "gitdir: /nowhere\n").unwrap();

    let refreshed = glrefresh::trees(&at, "alpha").unwrap().trees;

    assert!(
        refreshed.contains(&piece),
        "the worktree was not refreshed: {refreshed:?}"
    );
}

#[test]
fn a_worktree_that_leaves_the_workspace_is_named_and_never_run_git_in() {
    // A committed `workspaces/<ws>/.worktrees/<repo>/<piece> -> elsewhere` travels to every
    // machine that clones the plane. `git -C` through it reads ANOTHER repository's `origin`,
    // and the refresh then asks that forge about this branch — charter #964's shape, reached
    // by a link instead of by an environment variable.
    let (_keep, at) = plane();
    let outside = at.parent().unwrap().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let pieces = at.join("workspaces/alpha/.worktrees/svc");
    std::fs::create_dir_all(&pieces).unwrap();
    std::os::unix::fs::symlink(&outside, pieces.join("escape")).unwrap();

    let found = glrefresh::trees(&at, "alpha").unwrap();

    assert!(
        !found.trees.iter().any(|tree| tree.ends_with("escape")),
        "git would have been run outside the workspace: {:?}",
        found.trees
    );
    assert!(
        found.refused.iter().any(|(name, _)| name == "escape"),
        "the refusal was dropped: {:?}",
        found.refused
    );
}
