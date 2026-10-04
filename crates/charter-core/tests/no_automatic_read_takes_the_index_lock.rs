//! FD-11 (#651): **charter's automatic reads never take `index.lock`**, so they never fight an
//! agent's or the operator's `git add` in the same repo.
//!
//! git takes `index.lock` only to write the index: a plain `git status` that finds a file's
//! stat data stale rewrites the index through the lock (the refresh), which is the fight. Each
//! read here — the shared standings of a plane and a clone, the alerts, a clone's tree state and
//! the explorer's branch status — runs on a tree whose stat data is stale on purpose, and the
//! index must come out byte for byte and inode for inode as it went in. The control shows the
//! same tree does get its index rewritten by a status that may write it, so the test can see
//! a read that takes the lock.
//!
//! And the other side of the fight: a lock someone else holds (a `git add` part-way) leaves
//! every read answering, and is left where it is.

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use charter_core::files::{self, Branch};

/// The bounded reader, as the app starts it — this test binary run again, picking
/// [`reader_child`].
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

/// The index as the file system describes it: what a rewrite through `index.lock` changes.
#[derive(Debug, PartialEq, Eq)]
struct Index {
    bytes: Vec<u8>,
    inode: u64,
    modified: std::time::SystemTime,
}

fn index_of(repo: &Path) -> Index {
    let path = repo.join(".git/index");
    let meta = std::fs::metadata(&path).unwrap();
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&meta);
    #[cfg(not(unix))]
    let inode = 0;
    Index {
        bytes: std::fs::read(&path).unwrap(),
        inode,
        modified: meta.modified().unwrap(),
    }
}

/// A plane that is a repo of its own, ignoring its workspaces, with a clone in one; both with
/// committed files whose stat data the index no longer matches, one tracked edit and one new
/// file.
fn stale_plane() -> support::Fixture {
    let f = support::plane_with_clone("thing");
    std::fs::write(f.plane.join(".gitignore"), "/workspaces/\n").unwrap();
    // A mode, so the briefing asks git what memory is uncommitted.
    std::fs::write(
        f.plane.join("charter.toml"),
        "schema = 1\n\n[plane]\nmode = \"commit\"\n",
    )
    .unwrap();
    for repo in [&f.plane, &f.clone] {
        if !repo.join(".git").exists() {
            support::git(repo, &["init", "-q", "-b", "main", "."]);
        }
        for n in 0..20 {
            std::fs::write(repo.join(format!("kept-{n}.txt")), "same\n").unwrap();
        }
        support::git(repo, &["add", "-A"]);
        support::git(repo, &["commit", "-q", "-m", "files"]);
    }
    // Past a coarse file system clock, then the same bytes written again: content git already
    // has, stat data it does not.
    std::thread::sleep(Duration::from_millis(1100));
    for repo in [&f.plane, &f.clone] {
        for n in 0..20 {
            std::fs::write(repo.join(format!("kept-{n}.txt")), "same\n").unwrap();
        }
        std::fs::write(repo.join("kept-0.txt"), "edited\n").unwrap();
        std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    }
    f
}

fn clone_of(f: &support::Fixture) -> charter_core::repos::Repo {
    charter_core::repos::Repo {
        name: f.repo.clone(),
        path: f.clone.clone(),
    }
}

/// Every read charter makes of a repo by itself, as the app makes them.
fn every_automatic_read(f: &support::Fixture) {
    let plane = charter_core::planegit::shared_standing(&f.plane);
    assert_eq!(plane.changed, ["kept-0.txt", "new.txt"], "{plane:?}");
    assert!(plane.tracked);
    charter_core::planegit::touch(&f.plane);
    let clone = charter_core::reposave::shared_standing(&f.plane, &f.ws, &clone_of(f));
    assert_eq!(clone.changed, 2, "{clone:?}");
    let alerts = charter_core::alerts::read(&charter_core::alerts::Asking {
        root: &f.plane,
        active: None,
        standing: &f.plane,
        shared: true,
    });
    assert!(alerts.stopped.is_none(), "{alerts:?}");
    let state = charter_core::repos::state_of(&f.clone).expect("the clone's state");
    assert_eq!(state.tracked + state.untracked, 2, "{state:?}");
    let status = files::status(&reader(), &f.plane, Branch::repo(&f.ws, &f.repo))
        .expect("the branch's status");
    assert!(!status.changes.is_empty(), "{status:?}");
    // A chat's start: its briefing, which asks what memory is uncommitted.
    let no_env = |_: &str| None;
    let payload = serde_json::json!({});
    let _ = charter_core::briefing::parts(
        &charter_core::briefing::Ask {
            root: &f.plane,
            cwd: &f.plane,
            payload: &payload,
            env: &no_env,
            now: chrono::Utc::now(),
        },
        None,
    );
    // The status line, on every render.
    let footer = charter_core::footer::render(
        &f.plane,
        &payload,
        &charter_core::footer::Ambient {
            env: &no_env,
            cwd: &f.plane,
            now: chrono::Utc::now(),
            config: None,
        },
    );
    assert!(footer.contains("dirty"), "{footer}");
}

#[test]
fn no_automatic_read_rewrites_the_index_where_a_status_that_may_write_it_does() {
    charter_core::unsteered!();
    let f = stale_plane();
    let repos: [PathBuf; 2] = [f.plane.clone(), f.clone.clone()];
    let before: Vec<Index> = repos.iter().map(|repo| index_of(repo)).collect();

    every_automatic_read(&f);

    for (repo, was) in repos.iter().zip(&before) {
        assert_eq!(
            &index_of(repo),
            was,
            "{} had its index written",
            repo.display()
        );
        assert!(!repo.join(".git/index.lock").exists());
    }
    // The control: the same stale tree, read by a `status` allowed to refresh, has its index
    // rewritten through the lock — so a read above that took it would have shown.
    support::git(&f.clone, &["status", "--porcelain"]);
    assert_ne!(index_of(&f.clone), before[1], "the control saw no rewrite");
}

#[test]
fn a_lock_someone_else_holds_leaves_every_read_answering_and_is_left_in_place() {
    charter_core::unsteered!();
    let f = stale_plane();
    for repo in [&f.plane, &f.clone] {
        // An agent's `git add` part-way through.
        std::fs::write(repo.join(".git/index.lock"), b"").unwrap();
    }

    every_automatic_read(&f);

    for repo in [&f.plane, &f.clone] {
        assert!(repo.join(".git/index.lock").exists(), "{}", repo.display());
    }
}
