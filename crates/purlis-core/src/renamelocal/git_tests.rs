//! The git reads `purlis migrate` makes at the project's top.

use super::*;

#[test]
fn where_git_keeps_its_excludes_is_asked_through_the_hardened_git_runner() {
    // #1476: asked through `worktree::git`, so git starts with a constructed environment, no
    // program a config names, and a folder's `.git` link checked before git follows it.
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(exclude_of(dir.path()), Ok(None));
    let asked = crate::worktree::git::tally::asked(dir.path());
    assert!(
        asked
            .iter()
            .any(|args| args == &["rev-parse", "--git-path", "info/exclude"]),
        "{asked:?}"
    );
}

#[test]
fn a_folder_whose_git_link_purlis_will_not_follow_is_said_never_skipped() {
    // #1476: the person's own git may still read that link, so `migrate` says it could not make
    // git ignore the state rather than moving it unignored.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".git"),
        format!("gitdir: {}\n", dir.path().join("gone").display()),
    )
    .unwrap();
    assert_eq!(
        exclude_of(dir.path()),
        Err(crate::worktree::link::GONE.to_owned())
    );
}
