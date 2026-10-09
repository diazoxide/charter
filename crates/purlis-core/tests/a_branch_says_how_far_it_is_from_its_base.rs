//! FM-5 (#1108): **a branch says how far it is from its base** — how many commits it has that
//! the branch it was cut from lacks (ahead), and how many that branch gained since (behind) —
//! for the branch cockpit's header (#1103, V86 F2).
//!
//! An automatic read of a branch an agent can write, so it runs where FM-4's status runs: in
//! the bounded reader's child, in-process through gitoxide, starting no git (V88a, D-88f).

mod support;

use purlis_core::files::{self, Branch};
use purlis_core::worktree;

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
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let apart =
        files::ahead_behind(&reader(), &f.plane, Branch::piece(&f.ws, &f.repo, "piece")).unwrap();

    assert_eq!((apart.ahead, apart.behind), (0, 0));
    assert_eq!(apart.base.as_deref(), Some("main"));
}

#[test]
fn a_branch_with_no_recorded_base_says_it_has_none_rather_than_a_count() {
    purlis_core::unsteered!();
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

/// The repo's own folder on `main`, following `<remote>/main` (#1130): the upstream holds one
/// commit `main` lacks (`theirs`), and `main` one the upstream lacks (`local`). No network: the
/// remote-tracking ref is written where a fetch would have left it, and the remote is declared
/// with the fetch refspec a clone writes, which git's own `@{upstream}` needs to map
/// `refs/heads/main` to `refs/remotes/<remote>/main`. Nothing is ever fetched from it.
fn following_an_upstream(f: &support::Fixture, remote: &str) {
    f.commit(&f.clone, "pushed");
    support::git(&f.clone, &["checkout", "-q", "-b", "elsewhere"]);
    f.commit(&f.clone, "theirs");
    support::git(
        &f.clone,
        &["update-ref", &format!("refs/remotes/{remote}/main"), "HEAD"],
    );
    support::git(&f.clone, &["checkout", "-q", "main"]);
    support::git(&f.clone, &["branch", "-q", "-D", "elsewhere"]);
    f.commit(&f.clone, "local");
    let url = f.clone.display().to_string();
    support::git(&f.clone, &["config", &format!("remote.{remote}.url"), &url]);
    support::git(
        &f.clone,
        &[
            "config",
            &format!("remote.{remote}.fetch"),
            &format!("+refs/heads/*:refs/remotes/{remote}/*"),
        ],
    );
    support::git(&f.clone, &["config", "branch.main.remote", remote]);
    support::git(
        &f.clone,
        &["config", "branch.main.merge", "refs/heads/main"],
    );
}

#[test]
fn the_repos_own_folder_counts_from_its_upstream_as_git_does() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    following_an_upstream(&f, "origin");

    let apart = files::ahead_behind(&reader(), &f.plane, Branch::repo(&f.ws, &f.repo)).unwrap();

    let (behind, ahead) = git_counts(&f.clone, "@{upstream}");
    assert_eq!((apart.ahead, apart.behind), (ahead, behind));
    assert_eq!((apart.ahead, apart.behind), (1, 1));
    assert_eq!(apart.base.as_deref(), Some("origin/main"));
}

#[test]
fn a_recorded_base_that_names_no_commit_is_treated_as_none() {
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
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
