//! Every sentence a cut can put in the window says branch and folder (ADR 0072 §3/§4, V23d).
//!
//! `charter worktree` keeps its own words, where the difference between a directory and a
//! branch is the point; the window gets [`Refusal::in_window`] and [`Note::in_window`]. The
//! match in each is exhaustive, so a new refusal cannot reach the window without a sentence
//! chosen for it. These hold the words; what git itself printed, and a path, pass through.

mod support;

use purlis_core::contain::Elsewhere;
use purlis_core::worktree::confine::Outside;
use purlis_core::worktree::git::GitUnavailable;
use purlis_core::worktree::name::BadBranch;
use purlis_core::worktree::{self, Note, Refusal};

/// Words the first hour never shows (ADR 0072 §3), as charter's own nouns.
const KEPT_OUT: &[&str] = &["piece", "worktree", "plane"];

fn says_none_of_them(said: &str) {
    let lower = said.to_lowercase();
    for word in KEPT_OUT {
        assert!(!lower.contains(word), "{word:?} reached the window: {said}");
    }
}

/// One of every refusal, with names that hold none of the words themselves.
fn every_refusal() -> Vec<Refusal> {
    let s = |v: &str| v.to_string();
    vec![
        Refusal::BadWorkspace(s("alpha")),
        Refusal::BadRepo(s("api")),
        Refusal::BadPiece(s("-x")),
        Refusal::PieceGoesElsewhere {
            piece: s("nul"),
            why: Elsewhere::Device,
        },
        Refusal::PieceGoesElsewhere {
            piece: s("alpha."),
            why: Elsewhere::Stripped,
        },
        Refusal::BadBranch(BadBranch::Empty),
        Refusal::BadBranchName(s("a..b")),
        Refusal::Outside(Outside::WalksUp { path: s("x/../y") }),
        Refusal::Relocated(s("/elsewhere")),
        Refusal::NotARepo(s("api")),
        Refusal::BranchTaken {
            repo: s("api"),
            branch: s("spike"),
        },
        Refusal::NoSuchPiece {
            ws: s("alpha"),
            repo: s("api"),
            piece: s("spike"),
        },
        Refusal::Dirty { piece: s("spike") },
        Refusal::Uncommitted {
            piece: s("spike"),
            changes: vec![s("a.txt")],
        },
        Refusal::DirtUnknown { piece: s("spike") },
        Refusal::UniqueUnknown { piece: s("spike") },
        // The two `what`s charter really writes: `list`'s, and `head_of`'s tree path.
        Refusal::Unreadable {
            what: s("the worktrees of api"),
            why: s("fatal: not a git repository"),
        },
        Refusal::Unreadable {
            what: s("/p/workspaces/alpha/.worktrees/api/chat-1"),
            why: s("git could not read HEAD"),
        },
        Refusal::GitUnavailable(GitUnavailable::from(std::io::Error::other("not found"))),
        Refusal::CutTakenBack {
            branch: s("chat-1"),
            why: s("could not lock config file"),
            kept: None,
        },
        Refusal::CutTakenBack {
            branch: s("chat-1"),
            why: s("could not lock config file"),
            kept: Some(s("'chat-1' has uncommitted changes")),
        },
        Refusal::BaseNotRecorded {
            branch: s("spike"),
            why: s("locked"),
        },
        Refusal::WouldLoseWork {
            piece: s("spike"),
            count: 1,
            commits: vec![s("abc1234 work")],
        },
        Refusal::GitRefused {
            what: s("worktree add"),
            err: s("fatal: no"),
        },
        Refusal::Io {
            what: "create",
            path: s("/p/workspaces/alpha/.worktrees/api"),
            why: s("denied"),
        },
    ]
}

#[test]
fn no_refusal_a_cut_can_meet_says_piece_or_worktree_in_the_window() {
    purlis_core::unsteered!();
    for refusal in every_refusal() {
        let said = refusal.in_window();
        says_none_of_them(&said);
        // Nor a repair for a terminal: the window's are its own rows (#989).
        for command in ["git -C", "--force", "charter worktree"] {
            assert!(
                !said.contains(command),
                "{command:?} reached the window: {said}"
            );
        }
    }
}

#[test]
fn a_taken_branch_is_refused_in_the_windows_words_and_the_cli_keeps_its_own() {
    purlis_core::unsteered!();
    let taken = Refusal::BranchTaken {
        repo: "api".into(),
        branch: "spike".into(),
    };

    assert_eq!(
        taken.in_window(),
        "branch 'spike' already exists in api. Pick another name, or delete that branch if \
         nothing on it is needed."
    );
    assert!(taken.to_string().contains("Pick another piece name"));
}

#[test]
fn what_a_cut_found_to_say_is_said_of_the_branch_in_the_window() {
    purlis_core::unsteered!();
    for note in [
        Note::Dirty { repo: "api".into() },
        Note::DirtUnknown { repo: "api".into() },
        Note::Unwired {
            why: "a file in /p/workspaces/alpha/.worktrees/api/x".into(),
        },
    ] {
        says_none_of_them(&note.in_window());
        assert!(
            !note.to_string().is_empty(),
            "and the CLI still has its sentence"
        );
    }
    assert_eq!(
        Note::Dirty { repo: "api".into() }.in_window(),
        "api has uncommitted changes. They stay where they are, and the new branch does not \
         have them."
    );
}

#[test]
fn a_cut_taken_back_says_it_was_taken_back_and_what_remains_if_anything_does() {
    purlis_core::unsteered!();
    let back = |kept: Option<&str>| Refusal::CutTakenBack {
        branch: "chat-1".into(),
        why: "could not lock config file".into(),
        kept: kept.map(str::to_string),
    };

    assert_eq!(
        back(None).in_window(),
        "purlis could not record the branch chat-1 was cut from (could not lock config \
         file), so it took chat-1 back. Nothing was left behind."
    );
    assert_eq!(
        back(Some("git said no")).in_window(),
        "purlis could not record the branch chat-1 was cut from (could not lock config \
         file), and could not take it back (git said no): the branch chat-1 and its folder \
         remain."
    );
}

/// A refusal charter put in words itself, met for real: the terminal keeps its repair, the
/// window gets a sentence with no worktree and no command in it (#989).
fn stuck_in_the_windows_words(refusal: Refusal, terminal_says: &str) {
    assert!(
        matches!(refusal, Refusal::Stuck { .. }),
        "charter's own refusal, not git's: {refusal:?}"
    );
    assert!(refusal.to_string().contains(terminal_says), "{refusal}");
    let said = refusal.in_window();
    says_none_of_them(&said);
    for command in ["git -C", "--force", "charter worktree"] {
        assert!(
            !said.contains(command),
            "{command:?} reached the window: {said}"
        );
    }
}

/// A branch cut off the fixture's clone. Named `spike`, so a sentence that names it holds none
/// of [`KEPT_OUT`] by the name alone.
fn cut(f: &support::Fixture, name: &str) -> worktree::Added {
    worktree::add(&f.plane, &f.ws, &f.repo, name, None).expect("a branch is cut")
}

fn merged(f: &support::Fixture) -> Refusal {
    worktree::merge(&f.plane, &f.ws, &f.repo, "spike").expect_err("refused")
}

#[test]
fn a_merge_of_a_branch_on_no_branch_is_refused_in_the_windows_words() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "spike");
    support::git(&added.path, &["checkout", "-q", "--detach"]);

    stuck_in_the_windows_words(merged(&f), "git -C");
}

#[test]
fn a_merge_of_a_branch_cut_from_a_detached_head_is_refused_in_the_windows_words() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    support::git(&f.clone, &["checkout", "-q", "--detach"]);
    cut(&f, "spike");

    stuck_in_the_windows_words(merged(&f), "detached HEAD");
}

#[test]
fn a_merge_with_no_recorded_base_is_refused_in_the_windows_words() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "spike");
    f.commit(&added.path, "work");
    support::git(
        &f.clone,
        &["config", "--unset-all", "branch.spike.charterBase"],
    );

    stuck_in_the_windows_words(merged(&f), "was not recorded");
}

#[test]
fn a_merge_with_two_recorded_bases_is_refused_in_the_windows_words() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "spike");
    f.commit(&added.path, "work");
    support::git(
        &f.clone,
        &["config", "--add", "branch.spike.charterBase", "other"],
    );

    stuck_in_the_windows_words(merged(&f), "more than one");
}

#[test]
fn a_merge_into_a_clone_on_another_branch_is_refused_in_the_windows_words() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "spike");
    f.commit(&added.path, "work");
    support::git(&f.clone, &["switch", "-q", "-c", "elsewhere"]);

    stuck_in_the_windows_words(merged(&f), "switch main");
}

#[test]
fn a_merge_that_does_not_fast_forward_is_refused_in_the_windows_words() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "spike");
    f.commit(&added.path, "theirs");
    f.commit(&f.clone, "mine");

    stuck_in_the_windows_words(merged(&f), "does not fast-forward");
}

#[test]
fn a_merge_with_nothing_to_land_is_refused_in_the_windows_words() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "spike");

    let refusal = merged(&f);
    assert!(refusal.in_window().contains("nothing to land"), "{refusal}");
    stuck_in_the_windows_words(refusal, "Nothing was merged");
}

#[cfg(unix)]
#[test]
fn a_removal_git_will_not_clear_is_refused_in_the_windows_words() {
    purlis_core::unsteered!();
    // A folder that is gone while git still lists it is cleared without a check. Here git
    // cannot clear what it keeps about it: the directory it keeps that in is read-only.
    use std::os::unix::fs::PermissionsExt;
    let f = support::plane_with_clone("thing");
    let added = cut(&f, "spike");
    std::fs::remove_dir_all(&added.path).unwrap();
    let kept = f.clone.join(".git/worktrees");
    std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o555)).unwrap();

    let refusal = worktree::remove(&f.plane, &f.ws, &f.repo, "spike", false, false);
    std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o755)).unwrap();

    stuck_in_the_windows_words(refusal.expect_err("refused"), "worktree prune");
}
