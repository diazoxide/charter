//! #1189 (FM-11): the app's file commands read a branch's git in the bounded reader's child,
//! by gitoxide, and start no git in the app's process. What they read there is what the
//! hardened git answers here, where the command line still reads it: the offered list
//! (`git ls-files --cached --others --exclude-standard`), what a folder level marks ignored
//! (`git check-ignore`), and which folders are submodules (`git ls-files --stage`).
//!
//! Each test builds one branch holding every kind of path those answers tell apart, and holds
//! the reader's answer to the in-process one, path by path.
// Links are made with the Unix call; Windows reads the same code.
#![cfg(unix)]

mod support;

use std::path::Path;

use purlis_core::files::{self, Answer, Ask, Branch, Finder, Place};
use purlis_core::worktree;

/// The bounded reader, as the app starts it, but this test binary run again.
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

fn write(at: &Path, path: &str, text: &str) {
    let file = at.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// Every kind of path the three answers tell apart, in the folder `at`, which is a branch of the
/// fixture's clone: tracked, untracked, ignored by a folder rule, by a file rule and un-ignored
/// by a `!` rule, tracked though a rule ignores it, tracked and gone from the disk, untracked
/// in an untracked folder, an empty folder, links to an offered and to an ignored file, an
/// untracked repository nested in the branch, and a submodule the index records.
fn every_kind(at: &Path) {
    write(at, ".gitignore", "build/\n*.log\n!keep.log\n");
    write(at, "src/main.rs", "fn main() {}\n");
    write(at, "gone.txt", "soon gone\n");
    write(at, "forced.log", "tracked though ignored\n");
    support::git(at, &["add", ".gitignore", "src/main.rs", "gone.txt"]);
    support::git(at, &["add", "-f", "forced.log"]);
    let head = String::from_utf8(support::git(at, &["rev-parse", "HEAD"]).stdout).unwrap();
    let gitlink = format!("160000,{},sub", head.trim());
    support::git(at, &["update-index", "--add", "--cacheinfo", &gitlink]);
    support::git(at, &["commit", "-q", "-m", "every kind"]);
    std::fs::create_dir_all(at.join("sub")).unwrap();
    write(at, "sub/inside.txt", "a submodule's file\n");
    std::fs::remove_file(at.join("gone.txt")).unwrap();
    write(at, "src/new.rs", "// untracked\n");
    write(at, "build/out.bin", "ignored by a folder rule\n");
    write(at, "a.log", "ignored by a file rule\n");
    write(at, "keep.log", "un-ignored by a ! rule\n");
    write(at, "notes/deep/x.txt", "untracked in an untracked folder\n");
    write(at, "naïve.txt", "a name past ASCII\n");
    std::fs::create_dir_all(at.join("empty")).unwrap();
    std::os::unix::fs::symlink("src/main.rs", at.join("to-main")).unwrap();
    std::os::unix::fs::symlink("a.log", at.join("to-log")).unwrap();
    let nested = at.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    support::git(&nested, &["init", "-q", "-b", "main", "."]);
    write(&nested, "f.txt", "a nested repository's file\n");
}

/// The reader's offered list of `branch`, at most `most` of it.
fn offered(plane: &Path, branch: Branch<'_>, most: Option<usize>) -> files::Offered {
    match reader().ask(plane, branch, Ask::Offered { most }) {
        Ok(Answer::Offered(offered)) => offered,
        other => panic!("{other:?}"),
    }
}

const FOLDERS: [&str; 8] = [
    "",
    "src",
    "build",
    "notes",
    "notes/deep",
    "sub",
    "nested",
    "empty",
];

const PATHS: [&str; 13] = [
    "README.md",
    "src/main.rs",
    "src/new.rs",
    "a.log",
    "keep.log",
    "forced.log",
    "to-main",
    "to-log",
    "build/out.bin",
    "sub",
    "nested/f.txt",
    "gone.txt",
    "naïve.txt",
];

/// Holds every answer the reader gives about `branch` to the one git gives here.
fn the_reader_answers_as_git(plane: &Path, branch: Branch<'_>) {
    let listed = files::list(plane, branch).expect("git lists the branch");
    assert!(listed.contains(&"nested/".to_string()), "{listed:?}");
    assert!(listed.contains(&"gone.txt".to_string()), "{listed:?}");
    assert!(!listed.contains(&"a.log".to_string()), "{listed:?}");

    let read = offered(plane, branch, None);
    assert_eq!(read.files, listed);
    assert_eq!(read.total, listed.len());
    let cut = offered(plane, branch, Some(3));
    assert_eq!(cut.files, listed[..3]);
    assert_eq!(cut.total, listed.len());

    let reader = reader();
    let root = files::root(&reader, plane, branch).expect("the reader finds the branch");
    for folder in FOLDERS {
        let here = files::tree(plane, branch, folder).map_err(|e| e.to_string());
        let there = root.tree(&reader, folder).map_err(|e| e.to_string());
        assert_eq!(there, here, "the folder '{folder}'");
    }
    for path in PATHS {
        let here = files::open(plane, branch, path).map_err(|e| e.to_string());
        let there = root.open(&reader, path).map_err(|e| e.to_string());
        assert_eq!(there, here, "the path '{path}'");
    }

    let scope = [Place { plane, branch }];
    for query in ["main", "log", "txt"] {
        let here = Finder::default().find(&scope, query, 50);
        let there = Finder::default()
            .reading_with(self::reader())
            .find(&scope, query, 50);
        assert_eq!(there, here, "the query '{query}'");
    }
    let here = Finder::listing_at_most(3).find(&scope, "a", 50);
    let there = Finder::listing_at_most(3)
        .reading_with(self::reader())
        .find(&scope, "a", 50);
    assert_eq!(there.partial, here.partial);
}

#[test]
fn the_repos_own_folder_is_read_in_the_reader_as_git_reads_it_here() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    every_kind(&f.clone);

    the_reader_answers_as_git(&f.plane, Branch::repo(&f.ws, &f.repo));
}

#[test]
fn a_cut_branch_is_read_in_the_reader_as_git_reads_it_here() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect("a piece is cut")
        .path;
    every_kind(&piece);

    the_reader_answers_as_git(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"));
}

/// #1137, #1189: a search's listing, read in the reader's child, still hears the page's stop.
/// A `.gitignore` that is a FIFO holds the child's walk in its open until the reader's deadline
/// (30 s); a raised stop kills the child, and the page answers `Stopped` at once.
#[test]
fn a_stop_ends_a_listing_the_readers_child_is_held_in() {
    purlis_core::unsteered!();
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    let f = support::plane_with_clone("thing");
    write(&f.clone, "a.txt", "needle\n");
    let made = purlis_core::forklock::output(
        std::process::Command::new("mkfifo").arg(f.clone.join(".gitignore")),
    )
    .unwrap();
    assert!(made.status.success(), "{made:?}");

    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let raised = std::sync::Arc::clone(&stop);
    let plane = f.plane.clone();
    let (ws, repo) = (f.ws.clone(), f.repo.clone());
    let searching = std::thread::spawn(move || {
        let scope = [Place {
            plane: &plane,
            branch: Branch::repo(&ws, &repo),
        }];
        let mut search = files::search(&scope, "needle", files::SearchOptions::default())
            .unwrap()
            .reading_with(reader());
        let mut refused = Vec::new();
        let ended = search.more(usize::MAX, &stop, &mut |heard| {
            if let files::Searched::Refused { why, .. } = heard {
                refused.push(why);
            }
        });
        (ended, refused)
    });
    // Long enough for the child to be held in the FIFO's open.
    std::thread::sleep(Duration::from_millis(1500));
    let raised_at = Instant::now();
    raised.store(true, Ordering::Relaxed);
    let (ended, refused) = searching.join().unwrap();

    assert!(
        raised_at.elapsed() < Duration::from_secs(3),
        "{:?}",
        raised_at.elapsed()
    );
    assert_eq!(ended, files::Ended::Stopped);
    assert!(refused.is_empty(), "a stop is not a refusal: {refused:?}");
}

/// Every event a search of `scope` for `needle` hears to its end, read by the reader's child or,
/// with none, here.
fn searched(scope: &[Place<'_>], needle: &str, by_reader: bool) -> Vec<String> {
    use std::sync::atomic::AtomicBool;
    let search = files::search(scope, needle, files::SearchOptions::default()).unwrap();
    let mut search = if by_reader {
        search.reading_with(reader())
    } else {
        search
    };
    let never = AtomicBool::new(false);
    let mut heard = Vec::new();
    while search.more(usize::MAX, &never, &mut |one| {
        heard.push(format!("{one:?}"))
    }) != files::Ended::Done
    {}
    heard
}

/// #1130, #1189: a branch tracking a path of 4,095 bytes or more, an index gitoxide cannot read
/// yet, still has its files listed, opened, shown and searched by the app's file commands as
/// before #1189: the child says it cannot read the index, and each command reads that branch
/// with the hardened git here instead.
#[test]
fn a_branch_whose_index_gitoxide_cannot_read_is_read_as_git_reads_it_here() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let at = &f.clone;
    write(at, "blob.txt", "at the bottom\n");
    let blob =
        String::from_utf8(support::git(at, &["hash-object", "-w", "blob.txt"]).stdout).unwrap();
    std::fs::remove_file(at.join("blob.txt")).unwrap();
    // Twenty-one names of 200 bytes: past 4,095 bytes, each name within a system's limit.
    let long = (0..21)
        .map(|n| format!("{n:03}{}", "d".repeat(197)))
        .collect::<Vec<_>>()
        .join("/")
        + "/bottom.txt";
    assert!(long.len() > 4095);
    let entry = format!("100644,{},{long}", blob.trim());
    support::git(at, &["update-index", "--add", "--cacheinfo", &entry]);
    support::git(at, &["commit", "-q", "-m", "a long path"]);
    write(at, "zz-new.txt", "a needle\n");
    let plane = f.plane.as_path();
    let branch = Branch::repo(&f.ws, &f.repo);

    let told = reader().ask(plane, branch, Ask::Offered { most: None });
    assert!(matches!(told, Ok(Answer::Unindexed(_))), "{told:?}");
    let listed = files::list(plane, branch).expect("git lists the branch");
    assert!(listed.contains(&long), "{listed:?}");
    assert!(listed.contains(&"zz-new.txt".to_string()), "{listed:?}");

    let reader = reader();
    let root = files::root(&reader, plane, branch).expect("the reader finds the branch");
    let here = files::tree(plane, branch, "").map_err(|e| e.to_string());
    let there = root.tree(&reader, "").map_err(|e| e.to_string());
    assert!(there.is_ok(), "{there:?}");
    assert_eq!(there, here);
    for path in ["README.md", "zz-new.txt", long.as_str()] {
        let here = files::open(plane, branch, path).map_err(|e| e.to_string());
        let there = root.open(&reader, path).map_err(|e| e.to_string());
        assert_eq!(there, here, "the path '{path}'");
    }
    assert!(root.open(&reader, "zz-new.txt").is_ok());

    let scope = [Place { plane, branch }];
    for query in ["new", "bottom"] {
        let here = Finder::default().find(&scope, query, 50);
        let there = Finder::default()
            .reading_with(self::reader())
            .find(&scope, query, 50);
        assert!(there.refused.is_empty(), "{:?}", there.refused);
        assert_eq!(there, here, "the query '{query}'");
    }
    let there = searched(&scope, "needle", true);
    assert_eq!(there, searched(&scope, "needle", false));
    assert!(
        there.iter().any(|one| one.contains("zz-new.txt")),
        "{there:?}"
    );
}
