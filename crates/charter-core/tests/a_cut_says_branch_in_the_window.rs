//! Every sentence a cut can put in the window says branch and folder (ADR 0072 §3/§4, V23d).
//!
//! `charter worktree` keeps its own words, where the difference between a directory and a
//! branch is the point; the window gets [`Refusal::in_window`] and [`Note::in_window`]. The
//! match in each is exhaustive, so a new refusal cannot reach the window without a sentence
//! chosen for it. These hold the words; what git itself printed, and a path, pass through.

use charter_core::contain::Elsewhere;
use charter_core::worktree::confine::Outside;
use charter_core::worktree::name::BadBranch;
use charter_core::worktree::{Note, Refusal};

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
        Refusal::Unreadable {
            what: s("spike"),
            why: s("denied"),
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
    for refusal in every_refusal() {
        says_none_of_them(&refusal.in_window());
    }
}

#[test]
fn a_taken_branch_is_refused_in_the_windows_words_and_the_cli_keeps_its_own() {
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
