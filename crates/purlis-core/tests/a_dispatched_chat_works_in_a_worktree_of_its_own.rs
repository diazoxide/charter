//! A dispatch may give its persona chat a worktree of its own, on a branch of its own (#1453).
//!
//! These hold the core's half against real git: what the worktree is called and where it is
//! cut, that two tasks never share a folder or a branch, what the brokered route refuses, that
//! purlis merges nothing by itself, what the person's own merge lands and what it refuses
//! (#1511), what a discard takes and that it never takes a commit, and when
//! purlis takes a merged worktree away by itself. The app's half, which chat it starts there and what its record
//! and report say, is `app/src-tauri/src/handoff.rs`'s tests.

mod support;

use std::path::Path;

use purlis_core::dispatchplace::{
    self, Discarded, Ground, NotDone, NotMerged, Refused, Repo, Tidied, Tree, Where,
};
use purlis_core::dispatchrecord::{self, Removed};
use purlis_core::worktree::{self, git::Isolated, standing};

/// The repo of `f`'s one clone, as a dispatch from a chat standing in it resolves it.
fn repo(f: &support::Fixture) -> Repo {
    Repo {
        workspace: f.ws.clone(),
        repo: f.repo.clone(),
    }
}

/// A worktree cut for the task `task` of a dispatch purlis just minted.
fn cut(f: &support::Fixture, task: &str) -> purlis_core::chatpiece::Cut {
    cut_for(f, task, &dispatchrecord::mint())
}

/// A worktree cut for the task `task` of dispatch `id`.
fn cut_for(f: &support::Fixture, task: &str, id: &str) -> purlis_core::chatpiece::Cut {
    dispatchplace::cut(&f.plane, &repo(f), task, id, &Isolated::default())
        .expect("the worktree is cut")
}

fn tree(cut: &purlis_core::chatpiece::Cut) -> Tree {
    Tree {
        workspace: cut.workspace.clone(),
        repo: cut.repo.clone(),
        piece: cut.piece.clone(),
        branch: Some(cut.branch.clone()),
    }
}

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

/// What the clone's `main` points at.
fn main_at(f: &support::Fixture) -> String {
    String::from_utf8_lossy(&support::git(&f.clone, &["rev-parse", "refs/heads/main"]).stdout)
        .trim()
        .to_owned()
}

#[test]
fn two_worktree_tasks_from_one_asker_never_share_a_folder_or_a_branch() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");

    // The same asker, the same repo, the same task name, twice.
    let first = cut(&f, "check the queue");
    let second = cut(&f, "check the queue");

    assert_ne!(first.path, second.path);
    assert_ne!(first.branch, second.branch);
    assert_ne!(first.piece, second.piece);
    for one in [&first, &second] {
        // Under purlis's own folder for the workspace's worktrees, named for the task.
        assert_eq!(
            one.path.parent(),
            Some(f.workspace().join(".worktrees/api").as_path())
        );
        assert!(one.piece.starts_with("check-the-queue-"), "{}", one.piece);
        assert_eq!(one.branch, one.piece, "the branch is the folder's name");
        assert!(one.path.join("README.md").is_file());
        assert!(has_branch(&f, &one.branch));
        assert_eq!(one.base, worktree::Base::Branch("main".to_owned()));
    }
}

#[test]
fn a_worktree_is_cut_from_the_repo_the_asking_chat_works_in_wherever_in_it_that_chat_stands() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    std::fs::create_dir_all(f.clone.join("src/deep")).unwrap();
    let own = cut(&f, "the asker's own");

    for cwd in [
        f.clone.clone(),
        f.clone.join("src/deep"),
        // A chat that itself works in a worktree: its repo is the clone that was cut from.
        own.path.clone(),
    ] {
        assert_eq!(
            dispatchplace::ground(&f.plane, Some(Where::Worktree), None, Some(&cwd)),
            Ok(Ground::Worktree(repo(&f))),
            "{}",
            cwd.display()
        );
    }
    // And the project's root, a workspace's own folder and the worktrees' folder are no repo.
    for cwd in [
        f.plane.clone(),
        f.workspace(),
        f.workspace().join(".worktrees"),
        f.workspace().join(".worktrees/api"),
    ] {
        assert_eq!(
            dispatchplace::ground(&f.plane, Some(Where::Worktree), None, Some(&cwd)),
            Err(Refused::NotInARepo),
            "{}",
            cwd.display()
        );
    }
}

#[test]
fn a_folder_already_where_the_worktree_would_go_is_a_refusal_and_is_left_alone() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let id = dispatchrecord::mint();
    let piece = dispatchplace::piece_name("check the queue", &id);
    let stray = worktree::path_for(&f.plane, &f.ws, &f.repo, &piece).unwrap();
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("notes.txt"), "mine\n").unwrap();

    let refused = dispatchplace::cut(
        &f.plane,
        &repo(&f),
        "check the queue",
        &id,
        &Isolated::default(),
    )
    .expect_err("never the next free name, and never a folder purlis did not make");

    let said = refused.say();
    assert!(
        said.starts_with(&format!(
            "purlis could not cut a worktree for this task: something is already at the \
             folder '{piece}' would take in api"
        )),
        "{said}"
    );
    assert_eq!(
        std::fs::read_to_string(stray.join("notes.txt")).unwrap(),
        "mine\n"
    );
    assert!(!has_branch(&f, &piece), "no branch was cut for it");

    // And a branch already carrying that name is refused the same way, with no folder made.
    let other = dispatchrecord::mint();
    let taken = dispatchplace::piece_name("check the queue", &other);
    support::git(&f.clone, &["branch", &taken]);
    let refused = dispatchplace::cut(
        &f.plane,
        &repo(&f),
        "check the queue",
        &other,
        &Isolated::default(),
    )
    .expect_err("the branch is taken");
    assert!(
        refused.say().contains("already exists in api"),
        "{refused:?}"
    );
    assert!(
        worktree::path_for(&f.plane, &f.ws, &f.repo, &taken)
            .unwrap()
            .symlink_metadata()
            .is_err()
    );
}

#[test]
fn a_repo_whose_own_config_names_a_program_is_refused_by_the_broker_and_nothing_is_cut() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    // What a chat that can write the clone's config could have left: a filter git would run,
    // outside any sandbox, at the checkout a `worktree add` makes.
    support::git(&f.clone, &["config", "filter.x.smudge", "cat"]);

    let refused = dispatchplace::cut(
        &f.plane,
        &repo(&f),
        "check the queue",
        &dispatchrecord::mint(),
        &Isolated::default(),
    )
    .expect_err("the brokered route refuses the repository");

    let said = refused.say();
    assert!(
        said.starts_with("purlis could not cut a worktree for this task: "),
        "{said}"
    );
    assert!(said.contains("sets `filter.x.smudge` ("), "{said}");
    assert!(
        said.contains(
            "which names a program git would run outside the chat's sandbox, so the app will \
             not run git there for the chat."
        ),
        "{said}"
    );
    assert!(
        !f.workspace().join(".worktrees").exists(),
        "nothing was cut"
    );
}

#[test]
fn a_dirty_clone_s_changes_stay_behind_and_the_cut_says_so() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    std::fs::write(f.clone.join("README.md"), "the asker's unsaved work\n").unwrap();

    let cut = cut(&f, "check the queue");

    assert_eq!(
        std::fs::read_to_string(cut.path.join("README.md")).unwrap(),
        "one\n",
        "the worktree holds what is committed, never the asking chat's unsaved changes"
    );
    let notes: Vec<String> = cut.notes.iter().map(worktree::Note::in_window).collect();
    assert!(
        notes.iter().any(|note| note
            == "api has uncommitted changes. They stay where they are, and the new branch \
                does not have them."),
        "{notes:?}"
    );
    // The asking chat's own tree is as it left it.
    assert_eq!(
        std::fs::read_to_string(f.clone.join("README.md")).unwrap(),
        "the asker's unsaved work\n"
    );
}

#[test]
fn a_worktree_whose_chat_did_not_start_is_taken_back_folder_and_branch() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let cut = cut(&f, "check the queue");

    let undone = dispatchplace::take_back(&f.plane, &cut, &Isolated::default());

    assert_eq!(undone, Ok(purlis_core::chatpiece::Undone::Gone));
    assert!(cut.path.symlink_metadata().is_err());
    assert!(!has_branch(&f, &cut.branch));
}

/// Which repository a branch's folder belongs to is purlis's own record, and the folder only
/// confirms it (D-1453-27): a folder that names another git directory is refused by every look
/// purlis takes at it and by every removal, and no git runs in it.
#[test]
fn a_folder_that_names_another_git_directory_is_refused_by_every_look_and_removal() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let cut = cut(&f, "check the queue");
    let pointer = cut.path.join(".git");
    let honest = std::fs::read_to_string(&pointer).unwrap();

    // A repository made inside the folder, and the folder's `.git` line turned to name it.
    let planted = cut.path.join("planted");
    std::fs::create_dir_all(&planted).unwrap();
    support::git(&planted, &["init", "-q"]);
    let ran = f.plane.join("a-program-ran");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // What that repository's own settings could name: a program a `status` there runs.
        let program = f.plane.join("program.sh");
        std::fs::write(&program, format!("#!/bin/sh\ntouch '{}'\n", ran.display())).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        support::git(
            &planted,
            &["config", "core.fsmonitor", &program.display().to_string()],
        );
    }
    std::fs::remove_file(&pointer).unwrap();
    std::fs::write(
        &pointer,
        format!("gitdir: {}\n", planted.join(".git").display()),
    )
    .unwrap();

    let said = format!(
        "The folder of '{}' is no longer wired to the branch purlis cut it on, so purlis will \
         not run git in it; nothing was changed, and deleting the folder yourself leaves the \
         branch where it is.",
        cut.piece
    );
    let isolation = Isolated::default();
    // What a discard would take, the read the window makes first.
    assert_eq!(
        dispatchplace::at_risk(&f.plane, &tree(&cut), &isolation),
        Err(NotDone::Git(said.clone()))
    );
    // The discard itself.
    assert_eq!(
        dispatchplace::discard(&f.plane, &tree(&cut), &isolation),
        Err(NotDone::Git(said.clone()))
    );
    // The look purlis takes by itself when a chat is closed and when the project opens.
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&cut), &isolation),
        Tidied::Kept
    );
    // And taking back a cut whose chat did not start.
    assert_eq!(
        dispatchplace::take_back(&f.plane, &cut, &isolation),
        Err(said)
    );

    assert!(cut.path.join("README.md").is_file(), "the folder stands");
    assert!(planted.join(".git").is_dir(), "and what is in it");
    assert!(has_branch(&f, &cut.branch), "and its branch");
    assert!(!ran.exists(), "no git ran against the planted repository");

    // The line git wrote, back: the same looks answer, and the untouched folder is tidied.
    std::fs::remove_dir_all(&planted).unwrap();
    std::fs::remove_file(&pointer).unwrap();
    std::fs::write(&pointer, honest).unwrap();
    let risk = dispatchplace::at_risk(&f.plane, &tree(&cut), &isolation)
        .expect("git answers")
        .expect("the folder is there");
    assert_eq!(risk.branch.as_deref(), Some(cut.branch.as_str()));
    assert_eq!(risk.changes, Some(Vec::new()));
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&cut), &isolation),
        Tidied::Removed
    );
    assert!(cut.path.symlink_metadata().is_err());
}

/// Every git call purlis runs in a branch's folder is given the folder's git directory and the
/// folder itself, so git discovers nothing there (D-1453-27): a `.git` line that still names
/// the right directory is all that is read from the folder.
#[test]
fn the_reads_of_a_branch_s_folder_answer_from_the_git_directory_the_clone_keeps_for_it() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let cut = cut(&f, "check the queue");
    f.commit(&cut.path, "fix");
    std::fs::write(cut.path.join("scratch.txt"), "not committed\n").unwrap();

    let git_dir = std::fs::canonicalize(f.clone.join(".git")).unwrap();
    let own = worktree::pointer::verified(&git_dir, &cut.path, &cut.piece)
        .expect("the folder names its own git directory");
    assert_eq!(own, git_dir.join("worktrees").join(&cut.piece));

    let risk = dispatchplace::at_risk(&f.plane, &tree(&cut), &Isolated::default())
        .expect("git answers")
        .expect("the folder is there");
    assert_eq!(risk.branch.as_deref(), Some(cut.branch.as_str()));
    assert_eq!(risk.changes, Some(vec!["?? scratch.txt".to_owned()]));
    assert_eq!(risk.unmerged, Some(1));
    assert!(risk.commits[0].ends_with(" fix"), "{:?}", risk.commits);
}

#[test]
fn nothing_a_worktree_task_commits_reaches_the_branch_it_was_cut_from() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let before = main_at(&f);
    let cut = cut(&f, "check the queue");
    f.commit(&cut.path, "fix");

    // Its chat closed, and the project was opened again: purlis looked, and merged nothing.
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&cut), &Isolated::default()),
        Tidied::Kept
    );
    assert_eq!(main_at(&f), before, "main is where it was");
    assert!(cut.path.join("fix").is_file(), "the worktree is kept");
    assert!(has_branch(&f, &cut.branch));
    assert!(!f.clone.join("fix").exists());
}

#[test]
fn what_goes_with_a_discarded_folder_is_named_before_anything_is_removed() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let cut = cut(&f, "check the queue");
    f.commit(&cut.path, "fix");
    std::fs::write(cut.path.join("scratch.txt"), "not committed\n").unwrap();
    std::fs::write(cut.path.join("README.md"), "edited\n").unwrap();
    // And what git ignores there, which no guard counts and a removal still deletes.
    std::fs::write(cut.path.join(".gitignore"), "build/\n").unwrap();
    std::fs::create_dir_all(cut.path.join("build")).unwrap();
    std::fs::write(cut.path.join("build/out.o"), "built\n").unwrap();

    let risk = dispatchplace::at_risk(&f.plane, &tree(&cut), &Isolated::default())
        .expect("git answers")
        .expect("the folder is there");

    assert_eq!(risk.branch.as_deref(), Some(cut.branch.as_str()));
    // Every untracked file by its own path: a new folder is not one line (fold-in 3).
    std::fs::create_dir_all(cut.path.join("notes/deep")).unwrap();
    std::fs::write(cut.path.join("notes/a.md"), "a\n").unwrap();
    std::fs::write(cut.path.join("notes/deep/b.md"), "b\n").unwrap();
    let risk = dispatchplace::at_risk(&f.plane, &tree(&cut), &Isolated::default())
        .expect("git answers")
        .expect("the folder is there");
    let mut changes = risk.changes.clone().expect("git said");
    changes.sort();
    assert_eq!(
        changes,
        [
            " M README.md",
            "?? .gitignore",
            "?? notes/a.md",
            "?? notes/deep/b.md",
            "?? scratch.txt"
        ]
    );
    // A folder ignored whole is one entry, never its every file.
    assert_eq!(risk.ignored, Some(vec!["build/".to_owned()]));
    assert_eq!(risk.unmerged, Some(1));
    assert_eq!(risk.commits.len(), 1);
    assert!(risk.commits[0].ends_with(" fix"), "{:?}", risk.commits);
    assert!(!risk.nothing());
    // Reading it removed nothing.
    assert!(cut.path.join("scratch.txt").is_file());
    assert!(cut.path.join("build/out.o").is_file());
    assert!(has_branch(&f, &cut.branch));

    // A worktree with nothing on it has nothing to lose, and one that is gone has no answer.
    let clean = self::cut(&f, "nothing done");
    let risk = dispatchplace::at_risk(&f.plane, &tree(&clean), &Isolated::default())
        .unwrap()
        .unwrap();
    assert!(risk.nothing(), "{risk:?}");
    assert_eq!(risk.ignored, Some(Vec::new()));
    let gone = Tree {
        piece: "never-cut-00000000".to_owned(),
        ..tree(&clean)
    };
    assert_eq!(
        dispatchplace::at_risk(&f.plane, &gone, &Isolated::default()),
        Ok(None)
    );
}

#[test]
fn commits_made_on_no_branch_are_named_as_what_a_discard_would_lose() {
    purlis_core::unsteered!();
    // #1453 review, M2. A discard keeps the branch purlis cut. A folder its chat detached, or
    // switched to another branch, is not on that branch: what it committed on no branch is
    // reachable from nothing once the folder is gone, and the read says so.
    let f = support::plane_with_clone("api");
    let detached = cut(&f, "detached work");
    support::git(&detached.path, &["switch", "-q", "--detach"]);
    f.commit(&detached.path, "orphan");

    let risk = dispatchplace::at_risk(&f.plane, &tree(&detached), &Isolated::default())
        .expect("git answers")
        .expect("the folder is there");

    assert_eq!(risk.branch, None, "the folder is on no branch");
    assert_eq!(risk.unmerged, Some(1));
    assert_eq!(risk.commits.len(), 1);
    assert!(risk.commits[0].ends_with(" orphan"), "{:?}", risk.commits);
    assert!(!risk.nothing());

    // On another branch, the commits stay on that branch, which a discard never touches.
    let moved = cut(&f, "switched away");
    support::git(&moved.path, &["switch", "-q", "-c", "somewhere-else"]);
    f.commit(&moved.path, "elsewhere");
    let risk = dispatchplace::at_risk(&f.plane, &tree(&moved), &Isolated::default())
        .unwrap()
        .unwrap();
    assert_eq!(risk.branch.as_deref(), Some("somewhere-else"));
    assert_eq!(
        dispatchplace::discard(&f.plane, &tree(&moved), &Isolated::default()),
        // The branch purlis cut holds nothing and goes; the one the chat made stays.
        Ok(Discarded::BranchGone)
    );
    assert!(has_branch(&f, "somewhere-else"));
    assert!(!has_branch(&f, &moved.branch));
}

#[test]
fn what_purlis_itself_hid_in_the_folder_is_not_listed_as_the_task_s() {
    purlis_core::unsteered!();
    // Fold-in 8: in a project with a layer, every worktree holds purlis's own hidden files.
    // They are not what a discard is asked about, and they do not keep a merged folder.
    let f = support::plane_with_clone("api");
    f.give_the_plane_a_layer();
    let cut = cut(&f, "check the queue");
    assert!(
        cut.path.join(".claude/settings.json").is_file(),
        "the layer is in the folder"
    );
    std::fs::write(cut.path.join(".gitignore"), "out/\n").unwrap();
    support::git(&cut.path, &["add", ".gitignore"]);
    support::git(&cut.path, &["commit", "-q", "-m", "ignore out"]);

    let risk = dispatchplace::at_risk(&f.plane, &tree(&cut), &Isolated::default())
        .unwrap()
        .unwrap();
    assert_eq!(risk.changes, Some(Vec::new()));
    assert_eq!(
        risk.ignored,
        Some(Vec::new()),
        "only purlis's own is hidden there"
    );

    std::fs::create_dir_all(cut.path.join("out")).unwrap();
    std::fs::write(cut.path.join("out/results.json"), "{}\n").unwrap();
    let risk = dispatchplace::at_risk(&f.plane, &tree(&cut), &Isolated::default())
        .unwrap()
        .unwrap();
    assert_eq!(risk.ignored, Some(vec!["out/".to_owned()]));
    // And the merged look keeps it for that folder, and takes it once it is gone: purlis's own
    // layer never keeps a folder.
    support::git(&f.clone, &["merge", "-q", "--ff-only", &cut.branch]);
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&cut), &Isolated::default()),
        Tidied::Kept
    );
    std::fs::remove_dir_all(cut.path.join("out")).unwrap();
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&cut), &Isolated::default()),
        Tidied::Removed
    );
}

#[test]
fn a_discard_removes_the_folder_and_never_a_branch_that_holds_work() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let before = main_at(&f);
    let kept = cut(&f, "another task");
    f.commit(&kept.path, "theirs");
    let cut = cut(&f, "check the queue");
    f.commit(&cut.path, "fix");
    let tip = String::from_utf8_lossy(&support::git(&cut.path, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_owned();
    std::fs::write(cut.path.join("scratch.txt"), "not committed\n").unwrap();

    let discarded = dispatchplace::discard(&f.plane, &tree(&cut), &Isolated::default());

    // The folder is gone with what was uncommitted in it. The commit is not: its branch stays,
    // an ordinary branch of the repo, because deleting a branch that holds work is never
    // purlis's act (ADR 0072 section 4).
    assert_eq!(discarded, Ok(Discarded::BranchKept));
    assert!(cut.path.symlink_metadata().is_err(), "its folder is gone");
    assert!(has_branch(&f, &cut.branch), "its branch stays");
    assert_eq!(
        String::from_utf8_lossy(
            &support::git(
                &f.clone,
                &["rev-parse", &format!("refs/heads/{}", cut.branch)]
            )
            .stdout
        )
        .trim(),
        tip,
        "and holds the commit"
    );
    // Its sibling, the clone and the branch it was cut from are untouched, and nothing merged.
    assert!(kept.path.join("theirs").is_file());
    assert!(has_branch(&f, &kept.branch));
    assert_eq!(main_at(&f), before);
    assert_eq!(f.status(&f.clone), "");
    assert!(!f.clone.join("fix").exists());

    // A branch nothing was committed on is merged by git's own word, and goes with its folder.
    let empty = self::cut(&f, "only looked");
    std::fs::write(empty.path.join("scratch.txt"), "not committed\n").unwrap();
    assert_eq!(
        dispatchplace::discard(&f.plane, &tree(&empty), &Isolated::default()),
        Ok(Discarded::BranchGone)
    );
    assert!(empty.path.symlink_metadata().is_err());
    assert!(!has_branch(&f, &empty.branch));
}

#[test]
fn a_worktree_whose_branch_is_merged_is_taken_away_and_one_holding_anything_else_is_kept() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let iso = Isolated::default();

    // Merged into the branch it was cut from, and clean: gone, folder and branch.
    let merged = cut(&f, "merged work");
    f.commit(&merged.path, "landed");
    assert_eq!(
        standing::landed(&f.plane, &f.ws, &f.repo, &merged.piece, &merged.branch),
        standing::Landed::No
    );
    support::git(&f.clone, &["merge", "-q", "--ff-only", &merged.branch]);
    assert_eq!(
        standing::landed(&f.plane, &f.ws, &f.repo, &merged.piece, &merged.branch),
        standing::Landed::Yes {
            branch: merged.branch.clone(),
            base: "main".to_owned(),
        }
    );
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&merged), &iso),
        Tidied::Removed
    );
    assert!(merged.path.symlink_metadata().is_err());
    assert!(!has_branch(&f, &merged.branch));
    assert!(f.clone.join("landed").is_file(), "the work is in main");

    // Merged, with something uncommitted in the folder: never deleted, exactly as it is.
    let dirty = cut(&f, "merged but dirty");
    f.commit(&dirty.path, "also-landed");
    support::git(&f.clone, &["merge", "-q", "--ff-only", &dirty.branch]);
    std::fs::write(dirty.path.join("scratch.txt"), "not committed\n").unwrap();
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&dirty), &iso),
        Tidied::Kept
    );
    assert_eq!(
        std::fs::read_to_string(dirty.path.join("scratch.txt")).unwrap(),
        "not committed\n"
    );
    assert!(has_branch(&f, &dirty.branch));

    // Pushed nowhere and merged nowhere: kept.
    let unmerged = cut(&f, "unmerged work");
    f.commit(&unmerged.path, "pending");
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&unmerged), &iso),
        Tidied::Kept
    );
    assert!(unmerged.path.join("pending").is_file());

    // A task that committed nothing and left nothing leaves nothing behind.
    let nothing = cut(&f, "only looked");
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&nothing), &iso),
        Tidied::Removed
    );
    assert!(nothing.path.symlink_metadata().is_err());

    // #1453 review, M4. A task whose only output is something git ignores: a results folder,
    // a `.env` it wrote to run tests. git's safe removal does not count ignored files and
    // would delete them. The folder is kept, listed with Discard, whether or not the task
    // committed anything.
    std::fs::write(f.clone.join(".gitignore"), "out/\n.env\n").unwrap();
    support::git(&f.clone, &["add", ".gitignore"]);
    support::git(&f.clone, &["commit", "-q", "-m", "ignore out and .env"]);
    for ignored in ["out/results.json", ".env"] {
        let only_ignored = cut(&f, "only ignored output");
        let left = only_ignored.path.join(ignored);
        std::fs::create_dir_all(left.parent().unwrap()).unwrap();
        std::fs::write(&left, "what the task produced\n").unwrap();
        assert_eq!(
            f.status(&only_ignored.path),
            "",
            "git's own dirty check sees nothing: {ignored}"
        );
        assert_eq!(
            dispatchplace::tidy(&f.plane, &tree(&only_ignored), &iso),
            Tidied::Kept,
            "{ignored}"
        );
        assert_eq!(
            std::fs::read_to_string(&left).unwrap(),
            "what the task produced\n"
        );
        assert!(has_branch(&f, &only_ignored.branch));
    }
    // The same for one that did commit and was merged.
    let merged_with_output = cut(&f, "merged with output");
    f.commit(&merged_with_output.path, "landed-too");
    support::git(
        &f.clone,
        &["merge", "-q", "--ff-only", &merged_with_output.branch],
    );
    std::fs::create_dir_all(merged_with_output.path.join("out")).unwrap();
    std::fs::write(merged_with_output.path.join("out/coverage.db"), "x").unwrap();
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&merged_with_output), &iso),
        Tidied::Kept
    );
    assert!(merged_with_output.path.join("out/coverage.db").is_file());

    // A worktree its chat switched to another branch is not the branch purlis cut: left alone.
    let moved = cut(&f, "switched away");
    support::git(&moved.path, &["switch", "-q", "-c", "somewhere-else"]);
    assert_eq!(
        standing::landed(&f.plane, &f.ws, &f.repo, &moved.piece, &moved.branch),
        standing::Landed::Unknown
    );
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&moved), &iso),
        Tidied::Kept
    );
    assert!(moved.path.is_dir());
}

#[test]
fn a_merged_folder_goes_and_its_branch_stays_where_git_will_not_delete_it() {
    purlis_core::unsteered!();
    // Fold-in 6. The branch landed in the branch it was cut from, but the clone is on another
    // branch that does not hold it, so `git branch -d` refuses. The folder is removed and the
    // look says the branch stayed, which the record then says too.
    let f = support::plane_with_clone("api");
    let first = main_at(&f);
    let id = dispatchrecord::mint();
    let cut = cut_for(&f, "landed elsewhere", &id);
    f.commit(&cut.path, "landed");
    // `main` takes the branch's commit while the clone stands on a branch cut before it.
    support::git(&f.clone, &["switch", "-q", "-c", "side", &first]);
    support::git(
        &f.clone,
        &[
            "update-ref",
            "refs/heads/main",
            &format!("refs/heads/{}", cut.branch),
        ],
    );
    let record = recorded_as(&f.plane, &id, &cut, 7, true);

    let tidied = dispatchplace::tidy_recorded(
        &f.plane,
        &dispatchrecord::read(&f.plane, &record).unwrap(),
        &Isolated::default(),
    );

    assert_eq!(tidied, Tidied::FolderRemoved);
    assert!(cut.path.symlink_metadata().is_err(), "its folder is gone");
    assert!(has_branch(&f, &cut.branch), "git kept the branch");
    let after = dispatchrecord::read(&f.plane, &record).unwrap();
    assert_eq!(
        after.place.worktree.and_then(|tree| tree.removed),
        Some(Removed::MergedBranchKept)
    );
    assert_eq!(
        dispatchplace::standing(&f.plane, &dispatchrecord::read(&f.plane, &record).unwrap())
            .map(dispatchplace::Standing::word),
        Some("merged-branch-kept")
    );
}

/// The record of a worktree task whose persona chat is chat `worker`, under the id its
/// worktree was named for.
fn recorded_as(
    root: &Path,
    id: &str,
    cut: &purlis_core::chatpiece::Cut,
    worker: u32,
    ended: bool,
) -> String {
    use dispatchrecord::{Asker, ChatRef, Mode, Opening, Place, Worker, Worktree};
    let chat = |number: u32, name: &str| ChatRef {
        chat: number,
        id: None,
        name: name.to_owned(),
        persona: Some("steward".to_owned()),
    };
    let now = chrono::Utc::now();
    let record = dispatchrecord::open_as(
        root,
        id.to_owned(),
        Opening {
            mode: Mode::Task,
            asker: Asker {
                chat: chat(1, "steward 1"),
                workspace: Some(cut.workspace.clone()),
                by_person: false,
                session_record: None,
            },
            persona: Some("steward".to_owned()),
            worker: Worker {
                chat: chat(worker, "check the queue"),
                harness: None,
                profile: None,
                session_record: None,
            },
            task: Some("check the queue".to_owned()),
            place: Place {
                workspace: Some(cut.workspace.clone()),
                folder: None,
                worktree: Some(Worktree {
                    repo: cut.repo.clone(),
                    piece: cut.piece.clone(),
                    branch: Some(cut.branch.clone()),
                    removed: None,
                }),
            },
            brief: "b".to_owned(),
            report_owed: true,
        },
        now,
    )
    .expect("a record");
    if ended {
        dispatchrecord::close(root, &record.id, dispatchrecord::Ending::default(), now).unwrap();
    }
    record.id
}

#[test]
fn opening_the_project_takes_away_only_the_merged_worktrees_of_dispatches_that_ended() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let iso = Isolated::default();
    // Three tasks that committed nothing, so each has "landed": what differs is its dispatch.
    let [ended_id, running_id, reopened_id, forged_id] = [(); 4].map(|()| dispatchrecord::mint());
    let ended = cut_for(&f, "ended", &ended_id);
    let running = cut_for(&f, "still running", &running_id);
    let reopened = cut_for(&f, "its chat comes back", &reopened_id);
    recorded_as(&f.plane, &ended_id, &ended, 7, true);
    recorded_as(&f.plane, &running_id, &running, 8, false);
    recorded_as(&f.plane, &reopened_id, &reopened, 9, true);
    // A record that names a branch folder purlis did not cut for it: another dispatch's, cut
    // under another id. Merged and clean, and never looked at (fold-in 4).
    let anothers = cut(&f, "another's");
    recorded_as(&f.plane, &forged_id, &anothers, 6, true);

    let went = dispatchplace::tidy_all(&f.plane, |record| record.worker.chat.chat == 9, &iso);

    assert_eq!(went, 1);
    assert!(ended.path.symlink_metadata().is_err());
    assert!(
        running.path.is_dir(),
        "a running chat's folder is never looked at"
    );
    assert!(reopened.path.is_dir(), "nor one whose chat is coming back");
    let how = |id: &str| {
        dispatchrecord::read(&f.plane, id)
            .and_then(|record| record.place.worktree)
            .and_then(|tree| tree.removed)
    };
    assert_eq!(how(&ended_id), Some(Removed::Merged));
    assert_eq!(how(&running_id), None);
    assert_eq!(how(&reopened_id), None);
    assert_eq!(how(&forged_id), None);
    assert!(
        anothers.path.is_dir(),
        "a folder a record only claims is left alone"
    );
    assert!(has_branch(&f, &anothers.branch));
    // And the row that lists each says so.
    let stands = |id: &str| {
        dispatchplace::standing(&f.plane, &dispatchrecord::read(&f.plane, id).unwrap())
            .map(dispatchplace::Standing::word)
    };
    assert_eq!(stands(&ended_id), Some("merged"));
    assert_eq!(stands(&running_id), Some("kept"));
    // A second look finds nothing more to do.
    assert_eq!(
        dispatchplace::tidy_all(&f.plane, |record| record.worker.chat.chat == 9, &iso),
        0
    );
}

// ----- the person's merge (#1511) ---------------------------------------------------------

/// What `branch` points at in the clone.
fn branch_at(f: &support::Fixture, branch: &str) -> String {
    let named = format!("refs/heads/{branch}");
    String::from_utf8_lossy(&support::git(&f.clone, &["rev-parse", &named]).stdout)
        .trim()
        .to_owned()
}

#[test]
fn the_person_s_merge_lands_a_task_s_branch_in_the_branch_it_was_cut_from() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let iso = Isolated::default();
    let cut = cut(&f, "check the queue");
    f.commit(&cut.path, "fix");
    let tip = branch_at(&f, &cut.branch);

    // What the person is shown before they are asked: the branch purlis cut, and its commit.
    let asked = dispatchplace::merge_asked(&f.plane, &tree(&cut), &iso).expect("read");
    assert_eq!(asked.branch, cut.branch);
    assert_eq!(asked.tip, tip);
    assert_eq!(asked.uncommitted, Vec::<String>::new());
    assert_ne!(main_at(&f), tip, "asking merged nothing");

    let merged = dispatchplace::merge(&f.plane, &tree(&cut), &asked.tip, &iso).expect("merged");

    assert_eq!(merged.branch, cut.branch);
    assert_eq!(merged.now, tip);
    assert_eq!(main_at(&f), tip, "main is at the task's commit");
    assert!(f.clone.join("fix").is_file(), "the work is in the clone");
    // Merged and clean, so the look that follows takes its folder and branch away.
    assert_eq!(
        dispatchplace::tidy(&f.plane, &tree(&cut), &iso),
        Tidied::Removed
    );
    assert!(cut.path.symlink_metadata().is_err());
}

#[test]
fn a_merge_that_does_not_apply_cleanly_changes_nothing_and_says_why() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let iso = Isolated::default();
    let cut = cut(&f, "check the queue");
    // The task and the branch it was cut from each change the same file.
    std::fs::write(cut.path.join("README.md"), "the task's\n").unwrap();
    f.commit(&cut.path, "the task's line");
    std::fs::write(f.clone.join("README.md"), "the person's\n").unwrap();
    f.commit(&f.clone, "the person's line");
    let before = main_at(&f);
    let tip = branch_at(&f, &cut.branch);
    let asked = dispatchplace::merge_asked(&f.plane, &tree(&cut), &iso).expect("read");

    let refused = dispatchplace::merge(&f.plane, &tree(&cut), &asked.tip, &iso)
        .expect_err("it does not apply cleanly");

    let said = refused.in_window(&f.repo, &cut.branch);
    assert_eq!(
        said,
        format!(
            "'{branch}' does not fast-forward into main: main has moved on. Merge main into \
             '{branch}' where its conflicts belong, then merge again. Nothing was merged.",
            branch = cut.branch
        )
    );
    // Nothing changed: not the branch it was cut from, not the task's, not a file of either,
    // and no merge is left half done in the clone.
    assert_eq!(main_at(&f), before);
    assert_eq!(branch_at(&f, &cut.branch), tip);
    assert_eq!(
        std::fs::read_to_string(f.clone.join("README.md")).unwrap(),
        "the person's\n"
    );
    assert_eq!(
        std::fs::read_to_string(cut.path.join("README.md")).unwrap(),
        "the task's\n"
    );
    assert_eq!(f.status(&f.clone), "");
    assert!(!f.clone.join(".git/MERGE_HEAD").exists());
    assert!(cut.path.is_dir(), "its folder is kept");
}

#[test]
fn a_merge_is_of_the_commit_the_person_was_shown_or_of_nothing() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let iso = Isolated::default();
    let cut = cut(&f, "check the queue");
    f.commit(&cut.path, "shown");
    let before = main_at(&f);
    let asked = dispatchplace::merge_asked(&f.plane, &tree(&cut), &iso).expect("read");
    // A commit lands on the branch between the question and the answer.
    f.commit(&cut.path, "not-shown");

    let refused = dispatchplace::merge(&f.plane, &tree(&cut), &asked.tip, &iso).err();

    assert_eq!(refused, Some(NotMerged::Moved));
    assert_eq!(main_at(&f), before, "nothing was merged");
    assert!(!f.clone.join("shown").exists());
}

#[test]
fn only_the_branch_purlis_cut_is_merged_and_only_from_a_clean_folder() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let iso = Isolated::default();
    let before = main_at(&f);

    // Something uncommitted in the folder: the question names it, and the merge is refused.
    let dirty = cut(&f, "left a file");
    f.commit(&dirty.path, "fix");
    std::fs::write(dirty.path.join("scratch.txt"), "not committed\n").unwrap();
    let asked = dispatchplace::merge_asked(&f.plane, &tree(&dirty), &iso).expect("read");
    assert_eq!(asked.uncommitted, ["?? scratch.txt"]);
    let refused = dispatchplace::merge(&f.plane, &tree(&dirty), &asked.tip, &iso)
        .expect_err("a folder holding uncommitted changes");
    assert_eq!(
        refused.in_window(&f.repo, &dirty.branch),
        format!(
            "'{}' has uncommitted changes, so purlis did not merge it. Commit or stash them \
             first.",
            dirty.piece
        )
    );
    assert_eq!(main_at(&f), before);

    // The chat switched its folder to a branch of its own making: that one is not merged.
    let switched = cut(&f, "switched away");
    support::git(&switched.path, &["switch", "-q", "-c", "its-own-idea"]);
    f.commit(&switched.path, "elsewhere");
    let off = NotMerged::OffItsBranch {
        on: Some("its-own-idea".to_owned()),
    };
    assert_eq!(
        dispatchplace::merge_asked(&f.plane, &tree(&switched), &iso),
        Err(off.clone())
    );
    assert_eq!(
        dispatchplace::merge(&f.plane, &tree(&switched), "anything", &iso).err(),
        Some(off)
    );
    assert_eq!(main_at(&f), before, "nothing was merged");
    assert!(!f.clone.join("elsewhere").exists());
}

#[test]
fn a_merge_is_refused_by_the_broker_where_the_repo_s_own_config_names_a_program() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let iso = Isolated::default();
    let cut = cut(&f, "check the queue");
    f.commit(&cut.path, "fix");
    let before = main_at(&f);
    let tip = branch_at(&f, &cut.branch);
    // What a task that is not sandboxed can write: a driver git would run for the merge.
    support::git(&f.clone, &["config", "merge.ours.driver", "true"]);

    let refused = dispatchplace::merge(&f.plane, &tree(&cut), &tip, &iso).err();

    assert!(
        matches!(refused, Some(NotMerged::Route(NotDone::Repo(_)))),
        "{refused:?}"
    );
    assert_eq!(main_at(&f), before, "no git ran for the merge");
}

#[test]
fn a_merge_lands_the_commit_it_was_given_and_not_the_branch_s_newer_one() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("api");
    let cut = cut(&f, "check the queue");
    f.commit(&cut.path, "shown");
    let shown = branch_at(&f, &cut.branch);
    // A commit lands on the branch after the person was shown it.
    f.commit(&cut.path, "not-shown");
    assert_ne!(branch_at(&f, &cut.branch), shown);

    let merged = worktree::merge_at(&f.plane, &f.ws, &f.repo, &cut.piece, Some(&shown))
        .expect("the shown commit fast-forwards");

    assert_eq!(merged.now, shown);
    assert_eq!(
        main_at(&f),
        shown,
        "main is at the shown commit, and no further"
    );
    assert!(f.clone.join("shown").is_file());
    assert!(!f.clone.join("not-shown").exists());
}
