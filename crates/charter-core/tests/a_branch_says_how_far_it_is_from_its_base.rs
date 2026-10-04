//! FM-5 (#1108): **a branch says how far it is from its base** — how many commits it has that
//! the branch it was cut from lacks (ahead), and how many that branch gained since (behind) —
//! for the branch cockpit's header (#1103, V86 F2).
//!
//! An automatic read of a branch an agent can write, so it runs where FM-4's status runs: in
//! the bounded reader's child, in-process through gitoxide, starting no git (V88a, D-88f).

mod support;

use charter_core::files::{self, Branch};
use charter_core::worktree;

/// The bounded reader, as the app starts it — but this test binary run again, picking
/// [`reader_child`], rather than the app's.
fn reader() -> files::Reader {
    files::Reader::new(
        std::env::current_exe().expect("the test binary"),
        [
            "reader_child",
            "--exact",
            "--nocapture",
            "--test-threads=1",
            files::READ_ARG,
        ]
        .map(std::ffi::OsString::from),
    )
}

/// The reader's child: in a run of this binary that [`reader`] started, it answers the one
/// question it was asked and exits; in any other run it does nothing.
#[test]
fn reader_child() {
    charter_core::unsteered!();
    if let Some(code) = files::serve_if_asked() {
        std::process::exit(code);
    }
}

fn cut(f: &support::Fixture, piece: &str) -> std::path::PathBuf {
    worktree::add(&f.plane, &f.ws, &f.repo, piece, None)
        .expect("a branch is cut")
        .path
}

/// What git itself counts, `<behind> <ahead>`: the oracle the read is held to.
fn git_counts(tree: &std::path::Path, base: &str) -> (usize, usize) {
    let out = support::git(
        tree,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{base}...HEAD"),
        ],
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let mut words = text.split_whitespace().map(|w| w.parse::<usize>().unwrap());
    (words.next().unwrap(), words.next().unwrap())
}

#[test]
fn a_branch_counts_what_it_has_and_what_its_base_gained_since_as_git_does() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    f.commit(&piece, "first");
    f.commit(&piece, "second");
    f.commit(&f.clone, "on-main");

    let apart =
        files::ahead_behind(&reader(), &f.plane, Branch::piece(&f.ws, &f.repo, "piece")).unwrap();

    let (behind, ahead) = git_counts(&piece, "main");
    assert_eq!((apart.ahead, apart.behind), (ahead, behind));
    assert_eq!((apart.ahead, apart.behind), (2, 1));
    assert_eq!(apart.base.as_deref(), Some("main"));
}

#[test]
fn a_branch_just_cut_is_level_with_its_base() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let apart =
        files::ahead_behind(&reader(), &f.plane, Branch::piece(&f.ws, &f.repo, "piece")).unwrap();

    assert_eq!((apart.ahead, apart.behind), (0, 0));
    assert_eq!(apart.base.as_deref(), Some("main"));
}

#[test]
fn a_branch_with_no_recorded_base_says_it_has_none_rather_than_a_count() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    // The repo's own folder, and a branch cut by plain git: neither has a base recorded.
    let plain = f.workspace().join(".worktrees").join(&f.repo).join("plain");
    support::git(
        &f.clone,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "plain",
            &plain.display().to_string(),
        ],
    );
    f.commit(&plain, "unrecorded");

    for branch in [
        Branch::repo(&f.ws, &f.repo),
        Branch::piece(&f.ws, &f.repo, "plain"),
    ] {
        let apart = files::ahead_behind(&reader(), &f.plane, branch).unwrap();
        assert_eq!(apart.base, None, "{branch:?}");
        assert_eq!((apart.ahead, apart.behind), (0, 0), "{branch:?}");
    }
}

#[test]
fn a_recorded_base_that_names_no_commit_is_treated_as_none() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    support::git(
        &piece,
        &[
            "config",
            "--replace-all",
            "branch.piece.charterBase",
            "gone-away",
        ],
    );

    let apart =
        files::ahead_behind(&reader(), &f.plane, Branch::piece(&f.ws, &f.repo, "piece")).unwrap();

    assert_eq!(apart.base, None);
}

#[test]
fn a_branch_that_is_not_there_is_refused_in_the_glossary_s_words() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let said = files::ahead_behind(
        &reader(),
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "nowhere"),
    )
    .unwrap_err()
    .to_string()
    .to_lowercase();

    for word in ["worktree", "piece", "clone"] {
        assert!(!said.contains(word), "{said:?} says {word}");
    }
}
