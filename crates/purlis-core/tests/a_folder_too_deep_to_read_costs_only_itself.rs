//! #1130: a folder chain deeper than the system's path limit costs the branch's markers only
//! that chain. Before, the walk for untracked files read each folder by its whole path, failed
//! on the first one too long to name, and the whole status failed with it: a chat could blank
//! its own branch's markers this way. Now the chain is marked once, as a folder not read past,
//! and every other change is still marked.

mod support;

use purlis_core::files::{self, Branch, Mark};
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

/// A chain of folders under `at/top`, each named by 200 bytes, longer in all than `longer_than`
/// bytes, with a file at its bottom. Built by renames, so no call along the way is handed a
/// path longer than a few hundred bytes: the folder made so far is moved into a new one, again
/// and again.
fn deep_chain(at: &std::path::Path, longer_than: usize) -> usize {
    let top = at.join("top");
    std::fs::create_dir(&top).unwrap();
    std::fs::write(top.join("bottom.txt"), "at the bottom\n").unwrap();
    let mut length = 0;
    let mut n = 0;
    while length <= longer_than {
        let wrapper = at.join("wrapper");
        std::fs::create_dir(&wrapper).unwrap();
        let name = format!("{n:03}{}", "d".repeat(197));
        std::fs::rename(&top, wrapper.join(&name)).unwrap();
        std::fs::rename(&wrapper, &top).unwrap();
        length += name.len() + 1;
        n += 1;
    }
    length
}

#[test]
fn a_chain_deeper_than_the_path_limit_is_marked_once_and_a_later_file_still_is() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    // A cut branch: its git directory is the clone's, apart from its folder.
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect("a piece is cut")
        .path;
    // Past Linux's 4,096 bytes, and so past macOS's 1,024 too.
    let built = deep_chain(&piece, 4096 + piece.as_os_str().len());
    assert!(built > 4096);
    // Walked after `top`, so the walk has to come back from the chain to find it.
    std::fs::write(piece.join("zz-new.txt"), "new\n").unwrap();

    let status = files::status(&reader(), &f.plane, Branch::piece(&f.ws, &f.repo, "piece"))
        .expect("the status degrades rather than fails");

    let marked: Vec<(&str, Mark)> = status
        .changes
        .iter()
        .map(|one| (one.path.as_str(), one.mark))
        .collect();
    assert!(
        marked.contains(&("zz-new.txt", Mark::Added)),
        "a file past the chain is still marked: {marked:?}"
    );
    // The chain is one mark, a folder somewhere along it, and nothing below that is named.
    let in_chain: Vec<&str> = marked
        .iter()
        .map(|(path, _)| *path)
        .filter(|path| path.starts_with("top/"))
        .collect();
    assert_eq!(in_chain.len(), 1, "{marked:?}");
    assert!(!in_chain[0].ends_with("bottom.txt"), "{marked:?}");
    assert_eq!(marked.len(), 2, "{marked:?}");
}

/// The path [`deep_chain`] gave its bottom file, from `at`, when it was built `levels` deep.
fn bottom_of(levels: usize) -> String {
    let mut path = String::from("top");
    for n in (0..levels).rev() {
        path.push('/');
        path.push_str(&format!("{n:03}{}", "d".repeat(197)));
    }
    path.push_str("/bottom.txt");
    path
}

/// #1130 (train 20, item 2): a folder chain deeper than the system's path limit that git
/// **tracks** costs the branch's markers only that chain too. git is handed the bottom file by
/// its blob and its path (`update-index --cacheinfo`), so no call names the long path on the
/// disk; the status then has to look at an entry it cannot name.
///
/// **Ignored until gitoxide reads such an index** (#1130): a path of 4,095 bytes or more is
/// written to the index with its length unsaid and its name ended by padding, and gitoxide's
/// index decoder takes the name up to its first NUL without skipping the rest of the padding,
/// so the whole index fails to read ("Index trailer should have been 20 bytes long"), before the
/// status gets to the entry. Run with `--ignored` to see it.
#[test]
#[ignore = "#1130: gitoxide cannot read an index holding a path of 4,095 bytes or more"]
fn a_tracked_chain_deeper_than_the_path_limit_still_lets_a_later_file_be_marked() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let at = &f.clone;
    let built = deep_chain(at, 4096 + at.as_os_str().len());
    assert!(built > 4096);
    let levels = built / 201;
    std::fs::write(at.join("blob.txt"), "at the bottom\n").unwrap();
    let blob = support::git(at, &["hash-object", "-w", "blob.txt"]);
    std::fs::remove_file(at.join("blob.txt")).unwrap();
    let entry = format!(
        "100644,{},{}",
        String::from_utf8(blob.stdout).unwrap().trim(),
        bottom_of(levels)
    );
    support::git(at, &["update-index", "--add", "--cacheinfo", &entry]);
    support::git(at, &["commit", "-q", "-m", "a tracked chain"]);
    std::fs::write(at.join("zz-new.txt"), "new\n").unwrap();

    let status = files::status(&reader(), &f.plane, Branch::repo(&f.ws, &f.repo))
        .expect("the status degrades rather than fails");

    let marked: Vec<(&str, Mark)> = status
        .changes
        .iter()
        .map(|one| (one.path.as_str(), one.mark))
        .collect();
    assert!(
        marked.contains(&("zz-new.txt", Mark::Added)),
        "a file past the chain is still marked: {marked:?}"
    );
    let in_chain = marked
        .iter()
        .filter(|(path, _)| path.starts_with("top/"))
        .count();
    assert!(in_chain <= 1, "{marked:?}");
}
