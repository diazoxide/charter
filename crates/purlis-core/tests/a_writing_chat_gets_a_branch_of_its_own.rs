//! A chat that starts in a repo's clone gets a branch of its own by default (GL-1, ADR 0072 §4).
//!
//! The branch is a piece: a git worktree cut off the clone's HEAD, under the workspace's
//! `.worktrees/`. These tests hold the core's half — which directories are a repo's clone, what
//! the new branch is called, that a start which did not happen leaves nothing behind, and that
//! a start which did is logged `claimed` — so the window and the CLI cannot disagree about it.

mod support;

use purlis_core::chatpiece::{self, Held, Naming, Undone};
use purlis_core::worktree;

/// Whether the clone has `branch`, asked without `support::git`, which asserts success.
fn has_branch(f: &support::Fixture, branch: &str) -> bool {
    let mut ask = support::unsigned();
    ask.arg("-C").arg(&f.clone).args([
        "show-ref",
        "--verify",
        "--quiet",
        &format!("refs/heads/{branch}"),
    ]);
    purlis_core::forklock::output(&mut ask)
        .expect("git runs")
        .status
        .success()
}

#[test]
fn a_chat_starting_in_a_repos_clone_is_a_writing_chat() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");

    assert_eq!(
        chatpiece::clone_at(&f.plane, &f.clone),
        Some((f.ws.clone(), f.repo.clone()))
    );
}

#[test]
fn a_chat_starting_anywhere_but_a_clone_is_not() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "already", None).unwrap();
    std::fs::create_dir_all(f.clone.join("src")).unwrap();
    std::fs::create_dir_all(f.workspace().join("memory")).unwrap();

    for not_a_clone in [
        f.plane.clone(),
        f.workspace(),
        f.workspace().join("memory"),
        f.clone.join("src"),
        added.path.clone(),
    ] {
        assert_eq!(
            chatpiece::clone_at(&f.plane, &not_a_clone),
            None,
            "{} is not a repo's clone",
            not_a_clone.display()
        );
    }
}

#[test]
fn an_unnamed_chat_gets_the_first_free_chat_branch() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");

    let first = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();
    let second = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();

    assert_eq!(first.branch, "chat-1");
    assert_eq!(second.branch, "chat-2");
    assert_eq!(
        first.path,
        worktree::path_for(&f.plane, &f.ws, &f.repo, "chat-1").unwrap()
    );
    assert!(
        first.path.join("README.md").is_file(),
        "the clone's HEAD is checked out there"
    );
}

#[test]
fn a_named_chat_gets_a_branch_after_its_name_and_the_next_free_one_after_that() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let named = Naming::After(Some("Fix login!".to_string()));

    let first = chatpiece::cut(&f.plane, &f.ws, &f.repo, &named).unwrap();
    let second = chatpiece::cut(&f.plane, &f.ws, &f.repo, &named).unwrap();

    assert_eq!(first.branch, "fix-login");
    assert_eq!(second.branch, "fix-login-2");
}

#[test]
fn a_branch_left_behind_by_an_earlier_chat_is_skipped_and_kept() {
    purlis_core::unsteered!();
    // `Remove folder` leaves the branch (ADR 0072 §4), so `chat-1` can exist with no piece.
    let f = support::plane_with_clone("api");
    support::git(&f.clone, &["branch", "chat-1"]);

    let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();

    assert_eq!(cut.branch, "chat-2");
    assert!(has_branch(&f, "chat-1"));
}

#[test]
fn a_branch_name_the_operator_typed_is_used_exactly_or_refused() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");

    let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::Exactly("spike".into())).unwrap();
    let again = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::Exactly("spike".into()));

    assert_eq!(cut.branch, "spike");
    assert!(
        again.is_err(),
        "a typed name that is taken is not quietly renamed"
    );
}

#[test]
fn a_chat_that_did_not_start_leaves_neither_its_folder_nor_its_branch() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();

    assert_eq!(chatpiece::undo(&f.plane, &cut).unwrap(), Undone::Gone);

    assert!(!cut.path.exists());
    assert!(!has_branch(&f, "chat-1"));
    assert_eq!(
        chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None))
            .unwrap()
            .branch,
        "chat-1",
        "so the next chat is not pushed to chat-2 by a start that never happened"
    );
}

#[test]
fn a_chat_that_started_is_logged_as_having_claimed_its_branch() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();
    let who = purlis_core::pieces::Who {
        session: None,
        persona: None,
        host: "here".into(),
        log: "here".into(),
    };
    let then = chrono::Utc::now() - chrono::Duration::days(3);

    assert!(chatpiece::claim(&f.plane, &cut, &who, then).is_some());

    assert_eq!(
        purlis_core::pieces::said(&f.plane, &f.ws, &f.repo, &cut.piece, chrono::Utc::now()),
        "silent 3d",
        "claimed, and nothing declared since"
    );
}

#[test]
fn a_folder_already_where_a_branch_would_go_is_skipped_and_left_alone() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let stray = worktree::path_for(&f.plane, &f.ws, &f.repo, "chat-1").unwrap();
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("notes.txt"), "mine\n").unwrap();

    let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();

    assert_eq!(cut.branch, "chat-2");
    assert_eq!(
        std::fs::read_to_string(stray.join("notes.txt")).unwrap(),
        "mine\n"
    );
}

/// What git says about the clone: `status` runs, so the repository is still readable.
fn clone_is_healthy(f: &support::Fixture) -> bool {
    let mut ask = support::unsigned();
    ask.arg("-C").arg(&f.clone).args(["status", "--porcelain"]);
    purlis_core::forklock::output(&mut ask)
        .expect("git runs")
        .status
        .success()
}

#[test]
fn a_chats_name_never_costs_it_its_branch() {
    purlis_core::unsteered!();
    // GL-1 review B1: each of these slugged to something git refuses, or that a
    // case-insensitive filesystem reads as the repository's own HEAD.
    let f = support::plane_with_clone("api");
    let cases = [
        ("v1..v2 diff", "v1.v2-diff"),
        ("notes.lock", "notes"),
        ("HEAD", "chat-1"),
        ("Head", "chat-2"),
        ("da39a3ee5e6b4b0d3255bfef95601890afd80709", "chat-3"),
    ];

    for (name, branch) in cases {
        let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(Some(name.into())))
            .unwrap_or_else(|why| panic!("{name:?} cost the chat its start: {why}"));
        assert_eq!(cut.branch, branch, "{name:?}");
    }
    assert!(
        clone_is_healthy(&f),
        "the clone still reads after a chat named HEAD"
    );
    assert_eq!(
        chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None))
            .unwrap()
            .branch,
        "chat-4",
        "and the next cut after it still works"
    );
}

#[test]
fn a_branch_something_landed_on_is_kept_and_said_to_be() {
    purlis_core::unsteered!();
    // GL-1 review S2: `undo` answered Ok while git kept the branch.
    let f = support::plane_with_clone("api");
    let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();
    f.commit(&cut.path, "work");
    support::git(&f.clone, &["branch", "kept-too", "chat-1"]);
    // The commit is reachable from another ref now, so the folder may go, and `-d` still
    // refuses the branch: it is not merged into HEAD.

    let undone = chatpiece::undo(&f.plane, &cut).unwrap();

    assert_eq!(undone, Undone::BranchKept);
    assert!(!cut.path.exists());
    assert!(has_branch(&f, "chat-1"));
}

#[test]
fn a_cut_whose_base_could_not_be_recorded_takes_itself_back() {
    purlis_core::unsteered!();
    // GL-1 review S1: git made the folder and the branch, then the record of the base could
    // not be written, and both were left behind. A config lock held by someone else is how
    // `git config` comes to refuse.
    let f = support::plane_with_clone("api");
    let lock = f.clone.join(".git/config.lock");
    std::fs::write(&lock, "").unwrap();

    let refused = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None));

    std::fs::remove_file(&lock).unwrap();
    assert!(
        matches!(
            refused,
            Err(worktree::Refusal::CutTakenBack { kept: None, .. })
        ),
        "{refused:?}"
    );
    assert!(
        !worktree::path_for(&f.plane, &f.ws, &f.repo, "chat-1")
            .unwrap()
            .exists()
    );
    assert!(!has_branch(&f, "chat-1"));
}

#[test]
fn a_cut_dropped_without_being_kept_is_taken_back() {
    purlis_core::unsteered!();
    // A panic between the cut and the start drops the guard on the way out.
    let f = support::plane_with_clone("api");
    let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();
    let path = cut.path.clone();

    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = Held::new(&f.plane, cut);
        panic!("the start fell over");
    }));

    assert!(unwound.is_err());
    assert!(!path.exists());
    assert!(!has_branch(&f, "chat-1"));
}

#[test]
fn a_kept_cut_stays() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let cut = chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None)).unwrap();

    let kept = Held::new(&f.plane, cut).keep();

    assert!(kept.path.is_dir());
    assert!(has_branch(&f, "chat-1"));
}

#[test]
fn chats_started_together_in_one_repo_each_get_a_branch() {
    purlis_core::unsteered!();
    // GL-1 review S3: off the main thread, two starts at once both saw `chat-1` free.
    let f = support::plane_with_clone("api");

    let mut got: Vec<String> = std::thread::scope(|s| {
        let cuts: Vec<_> = (0..4)
            .map(|_| s.spawn(|| chatpiece::cut(&f.plane, &f.ws, &f.repo, &Naming::After(None))))
            .collect();
        cuts.into_iter()
            .map(|one| {
                one.join()
                    .unwrap()
                    .expect("every start gets a branch")
                    .branch
            })
            .collect()
    });
    got.sort();

    assert_eq!(got, ["chat-1", "chat-2", "chat-3", "chat-4"]);
}
