//! purlis reads a folder's `.git` file itself before it runs git there (#1055).
//!
//! A linked worktree's `.git` is a file in the folder a chat works in. One rewritten to name a
//! git directory the chat made would have git read that directory's configuration, outside any
//! sandbox, and run the programs it names. Every call the hardened runner makes in such a
//! folder is refused with a sentence instead, and no program is run.

mod support;

use std::path::{Path, PathBuf};

use purlis_core::worktree::{self, git, link};

/// A piece whose `.git` file was rewritten to a git directory of the chat's own making, with a
/// content filter that leaves `ran` behind when git runs it, and a tracked file changed so a
/// status has to read it through that filter.
struct Rewritten {
    f: support::Fixture,
    piece: PathBuf,
    ran: PathBuf,
    /// What the `.git` file said before.
    before: String,
}

fn rewritten() -> Rewritten {
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect("a piece is cut")
        .path;
    let before = std::fs::read_to_string(piece.join(".git")).unwrap();
    let ran = f.plane.join("the-filter-ran");
    // A git directory under a name no rule holds: a whole repository, with an index for this
    // folder, whose config defines a filter.
    let store = piece.join("store");
    support::git(
        &f.plane,
        &[
            "clone",
            "-q",
            "--bare",
            &f.clone.display().to_string(),
            &store.display().to_string(),
        ],
    );
    let config = store.join("config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!(
        "[core]\n\tbare = false\n[filter \"evil\"]\n\tclean = \"sh -c 'touch {}; cat'\"\n\tsmudge = cat\n",
        ran.display()
    ));
    std::fs::write(&config, text).unwrap();
    let pinned = [
        format!("--git-dir={}", store.display()),
        format!("--work-tree={}", piece.display()),
    ];
    // An index with no file's size or times in it, so a status reads every tracked file, and
    // reads it through the filter.
    support::git(&piece, &[&pinned[0], &pinned[1], "read-tree", "HEAD"]);
    std::fs::write(piece.join(".gitattributes"), "* filter=evil\n").unwrap();
    std::fs::write(piece.join("README.md"), "one, and more\n").unwrap();
    std::fs::write(piece.join(".git"), format!("gitdir: {}\n", store.display())).unwrap();
    Rewritten {
        f,
        piece,
        ran,
        before,
    }
}

/// git as a person's own terminal runs it: no purlis in front of it.
fn plain_status(dir: &Path) {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["status", "--porcelain"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    let out = purlis_core::forklock::output(&mut cmd).expect("git runs");
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn the_trap_is_live_and_the_runner_does_not_spring_it() {
    purlis_core::unsteered!();
    let r = rewritten();
    // The control: git, left to follow the file, runs the filter.
    plain_status(&r.piece);
    assert!(
        r.ran.exists(),
        "the rewritten link runs nothing: the test proves nothing"
    );
    std::fs::remove_file(&r.ran).unwrap();

    // Each way the runner starts git, in the folder and in a folder below it.
    let below = r.piece.join("src");
    std::fs::create_dir_all(&below).unwrap();
    for dir in [&r.piece, &below] {
        let status = git::run(dir, &["status", "--porcelain"], git::READ).unwrap();
        assert_eq!(status.code, Some(128), "{status:?}");
        assert_eq!(status.out, "");
        assert_eq!(status.err.trim(), link::CHANGED);

        let untimed = git::run_untimed(dir, &["diff", "--stat"]).unwrap();
        assert_eq!(
            (untimed.code, untimed.err.trim()),
            (Some(128), link::CHANGED)
        );

        let fed = git::run_with_input(
            dir,
            &["check-ignore", "--stdin", "-z"],
            b"README.md\0".to_vec(),
            git::READ,
        )
        .unwrap();
        assert_eq!(fed.code, Some(128));
        assert_eq!(String::from_utf8_lossy(&fed.err).trim(), link::CHANGED);

        let network = git::run_network(dir, None, &["fetch", "--dry-run"]).unwrap();
        assert_eq!(
            (network.code, network.err.trim()),
            (Some(128), link::CHANGED)
        );
    }
    assert!(
        !r.ran.exists(),
        "a call ran the program the rewritten link names"
    );
}

#[test]
fn the_reads_and_guards_that_run_git_in_a_branch_folder_refuse_it_and_run_nothing() {
    purlis_core::unsteered!();
    let r = rewritten();

    // The row's state: unreadable, in the sentence, and never clean.
    let state = purlis_core::repos::state_of(&r.piece).expect_err("not read");
    assert!(state.why.contains(link::CHANGED), "{state:?}");

    // Removal reads the folder's dirt first, and a folder it cannot read is not removed.
    let removal = worktree::remove(&r.f.plane, &r.f.ws, &r.f.repo, "piece", false, false)
        .expect_err("not removed");
    assert!(r.piece.is_dir(), "{removal}");

    // A merge reads the folder's branch first.
    worktree::merge(&r.f.plane, &r.f.ws, &r.f.repo, "piece").expect_err("not merged");

    assert!(
        !r.ran.exists(),
        "a read ran the program the rewritten link names"
    );
}

#[test]
fn the_link_git_wrote_is_read_as_before() {
    purlis_core::unsteered!();
    let r = rewritten();
    std::fs::write(r.piece.join(".git"), &r.before).unwrap();
    std::fs::remove_file(r.piece.join(".gitattributes")).unwrap();

    let status = git::run(&r.piece, &["status", "--porcelain"], git::READ).unwrap();
    assert!(status.ok(), "{status:?}");
    assert!(status.out.contains("README.md"), "{status:?}");
    // From a folder below it too, where git is told the tree's top.
    let below = r.piece.join("src");
    std::fs::create_dir_all(&below).unwrap();
    let top = git::run(&below, &["rev-parse", "--show-toplevel"], git::READ).unwrap();
    assert_eq!(
        std::fs::canonicalize(top.line()).unwrap(),
        std::fs::canonicalize(&r.piece).unwrap()
    );
    let branch = git::run(&below, &["branch", "--show-current"], git::READ).unwrap();
    assert_eq!(branch.line(), "piece");
    assert!(!r.ran.exists());
}

#[test]
fn a_link_to_a_sibling_branch_folder_s_git_directory_is_refused() {
    purlis_core::unsteered!();
    // No program runs here: the folder would be read, and written, as another branch.
    let f = support::plane_with_clone("thing");
    let one = worktree::add(&f.plane, &f.ws, &f.repo, "one", None)
        .unwrap()
        .path;
    let two = worktree::add(&f.plane, &f.ws, &f.repo, "two", None)
        .unwrap()
        .path;
    let theirs = std::fs::read_to_string(two.join(".git")).unwrap();
    std::fs::write(one.join(".git"), theirs).unwrap();

    let branch = git::run(&one, &["branch", "--show-current"], git::READ).unwrap();
    assert_eq!((branch.code, branch.err.trim()), (Some(128), link::CHANGED));
}

#[test]
fn a_branch_folder_moved_by_hand_says_how_to_repair_it() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .unwrap()
        .path;
    // What a move leaves: the folder's link still names its entry, and the entry's record
    // names where the folder was.
    let entry = f.clone.join(".git/worktrees/piece");
    std::fs::write(
        entry.join("gitdir"),
        format!(
            "{}\n",
            piece.with_file_name("was-here").join(".git").display()
        ),
    )
    .unwrap();

    let status = git::run(&piece, &["status", "--porcelain"], git::READ).unwrap();
    assert_eq!((status.code, status.err.trim()), (Some(128), link::MOVED));
}

#[test]
fn a_git_directory_made_inside_the_folder_is_refused_in_either_shape_git_lays_out() {
    purlis_core::unsteered!();
    // Where a sandbox cannot hold the name, a chat can leave a whole `.git` in its folder:
    // one with a worktree entry that names the folder back, or with a module. Neither is the
    // clone purlis cut the folder from, and both are inside the folder.
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .unwrap()
        .path;
    let entry = piece.join("own/.git/worktrees/piece");
    let module = piece.join("own/.git/modules/lib");
    std::fs::create_dir_all(&entry).unwrap();
    std::fs::create_dir_all(&module).unwrap();
    std::fs::write(entry.join("commondir"), "../..\n").unwrap();
    std::fs::write(
        entry.join("gitdir"),
        format!("{}\n", piece.join(".git").display()),
    )
    .unwrap();
    // And the clone's own module, which a branch folder is never a checkout of.
    let clones = f.clone.join(".git/modules/lib");
    std::fs::create_dir_all(&clones).unwrap();
    for at in [entry, module, clones] {
        std::fs::write(piece.join(".git"), format!("gitdir: {}\n", at.display())).unwrap();
        let status = git::run(&piece, &["status", "--porcelain"], git::READ).unwrap();
        assert_eq!(
            (status.code, status.err.trim()),
            (Some(128), link::CHANGED),
            "{}",
            at.display()
        );
    }
}

#[test]
fn a_link_that_names_nothing_there_says_that() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .unwrap()
        .path;
    std::fs::write(piece.join(".git"), "gitdir: /nonexistent/nowhere\n").unwrap();

    let status = git::run(&piece, &["status", "--porcelain"], git::READ).unwrap();
    assert_eq!((status.code, status.err.trim()), (Some(128), link::GONE));
}

#[test]
fn a_git_directory_kept_elsewhere_is_read_outside_a_workspace_and_not_below_one() {
    purlis_core::unsteered!();
    // `git init --separate-git-dir`: a person's own layout for a project's top or a scratch
    // checkout. Below a workspace it is the shape of a rewritten link, and is not followed.
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let top = root.join("project");
    let inside = top.join("workspaces/alpha/thing");
    std::fs::create_dir_all(&inside).unwrap();
    for (tree, store) in [(&top, "project.gitdir"), (&inside, "thing.gitdir")] {
        let kept = format!("--separate-git-dir={}", root.join(store).display());
        support::git(tree, &["init", "-q", "-b", "main", &kept, "."]);
        std::fs::write(tree.join("a.txt"), "a\n").unwrap();
    }

    let status = git::run(
        &top.join("workspaces"),
        &["status", "--porcelain"],
        git::READ,
    )
    .unwrap();
    assert!(status.ok(), "{status:?}");
    assert!(status.out.contains("a.txt"), "{status:?}");

    let refused = git::run(&inside, &["status", "--porcelain"], git::READ).unwrap();
    assert_eq!(
        (refused.code, refused.err.trim()),
        (Some(128), link::CHANGED)
    );
}

#[test]
fn no_git_is_started_inside_a_submodule_whose_folder_a_chat_prepared() {
    purlis_core::unsteered!();
    // `status` and `diff` look into a populated submodule by running git inside it, and that
    // git finds its repository through the submodule folder's own `.git`. A repository that
    // records a submodule is enough: the folder at that path is the chat's to prepare.
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let ran = root.join("the-filter-ran");
    let top = root.join("super");
    let sub = top.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    for (tree, store) in [(&top, "super.gitdir"), (&sub, "sub.gitdir")] {
        let kept = format!("--separate-git-dir={}", root.join(store).display());
        support::git(tree, &["init", "-q", "-b", "main", &kept, "."]);
        std::fs::write(tree.join("a.txt"), "a\n").unwrap();
        support::git(tree, &["add", "a.txt"]);
        support::git(tree, &["commit", "-q", "-m", "one"]);
    }
    // The submodule's folder as a chat would leave it: a git directory whose config defines a
    // filter, attributes that ask for it, and an index that makes a status read the file.
    let config = root.join("sub.gitdir/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!(
        "[filter \"evil\"]\n\tclean = \"sh -c 'touch {}; cat'\"\n\tsmudge = cat\n",
        ran.display()
    ));
    std::fs::write(&config, text).unwrap();
    std::fs::write(sub.join(".gitattributes"), "* filter=evil\n").unwrap();
    support::git(&sub, &["read-tree", "HEAD"]);
    let head = String::from_utf8_lossy(&support::git(&sub, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_owned();
    let entry = format!("160000,{head},sub");
    support::git(&top, &["update-index", "--add", "--cacheinfo", &entry]);
    support::git(&top, &["commit", "-q", "-m", "a submodule"]);

    // The control: git as a terminal runs it starts the child, and the child runs the filter.
    plain_status(&top);
    assert!(
        ran.exists(),
        "nothing ran inside the submodule: the test proves nothing"
    );
    std::fs::remove_file(&ran).unwrap();

    let read = |args: &[&str]| {
        let run = git::run(&top, args, git::READ).unwrap();
        assert!(run.code.is_some_and(|code| code < 2), "{args:?}: {run:?}");
    };
    read(&["status", "--porcelain"]);
    read(&[
        "status",
        "--porcelain=v2",
        "--branch",
        "--untracked-files=all",
    ]);
    read(&["diff", "--stat"]);
    read(&["diff", "--quiet"]);
    read(&["diff-files", "--name-only"]);
    read(&["diff-index", "--name-only", "HEAD"]);
    git::run_untimed(&top, &["diff", "HEAD", "--name-only"]).unwrap();
    // The runner that asks what the chat's own git would answer is held to the same.
    git::run_as_session(&top, &["status", "--porcelain"], git::READ).unwrap();
    git::run_as_session(&top, &["diff-files", "--name-only"], git::READ).unwrap();
    assert!(!ran.exists(), "a git was started inside the submodule");
}
