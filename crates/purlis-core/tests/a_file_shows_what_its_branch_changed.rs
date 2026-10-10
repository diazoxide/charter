//! FM-11 (#1114): **"Show what changed"** — one file of a branch, compared against the branch's
//! base by the one diff engine (RC-2, ADR 0084), committed or not. The window names a branch and
//! a path, never a directory; the path is confined as every other file command confines one,
//! and a file the branch did not change, or one the engine will not draw as lines, is answered
//! with a sentence or a kind, never with an empty diff.

mod support;

use purlis_core::files::{self, Branch, FileDiff, Hunk, Mark, Refused};
use purlis_core::worktree;

/// The bounded reader, as the app starts it — but this test binary run again, picking
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

/// The reader's child: answers the one question it was asked and exits; in any other run it
/// does nothing.
#[test]
fn reader_child() {
    purlis_core::unsteered!();
    if let Some(code) = files::serve_if_asked() {
        std::process::exit(code);
    }
}

fn write(at: &std::path::Path, path: &str, bytes: &[u8]) {
    let file = at.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, bytes).unwrap();
}

fn branch(f: &support::Fixture) -> Branch<'_> {
    Branch::piece(&f.ws, &f.repo, "piece")
}

/// A clone whose `main` holds a few files, and a piece cut from it.
fn cut_with_files(f: &support::Fixture) -> std::path::PathBuf {
    write(&f.clone, "src/lib.rs", b"one\ntwo\nthree\nfour\n");
    write(&f.clone, "src/same.rs", b"untouched\n");
    write(
        &f.clone,
        "src/old.rs",
        b"a file long enough to be found renamed\nand a second line\n",
    );
    write(&f.clone, "logo.bin", b"\0\x01\x02");
    support::git(&f.clone, &["add", "-A"]);
    support::git(&f.clone, &["commit", "-q", "-m", "files"]);
    worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect("a piece is cut")
        .path
}

#[test]
fn a_changed_file_is_compared_against_the_branchs_base_committed_and_not() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut_with_files(&f);
    // One change committed on the branch, one still in the working tree.
    write(&piece, "src/lib.rs", b"one\nTWO\nthree\nfour\n");
    support::git(&piece, &["commit", "-q", "-am", "two"]);
    write(&piece, "src/lib.rs", b"one\nTWO\nthree\nfour\nfive\n");

    let shown = files::what_changed(&reader(), &f.plane, branch(&f), "src/lib.rs").unwrap();

    assert_eq!(shown.change.path, "src/lib.rs");
    assert_eq!(shown.change.mark, Mark::Changed);
    assert!(shown.change.uncommitted);
    assert_eq!(shown.base.as_deref(), Some("main"));
    assert_eq!(
        shown.diff,
        FileDiff::Text {
            base: "one\ntwo\nthree\nfour\n".into(),
            head: "one\nTWO\nthree\nfour\nfive\n".into(),
            hunks: vec![
                Hunk {
                    old_start: 2,
                    old_lines: 1,
                    new_start: 2,
                    new_lines: 1,
                },
                Hunk {
                    old_start: 4,
                    old_lines: 0,
                    new_start: 5,
                    new_lines: 1,
                },
            ],
        }
    );
}

#[test]
fn a_renamed_file_is_compared_with_where_it_came_from() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut_with_files(&f);
    support::git(&piece, &["mv", "src/old.rs", "src/new.rs"]);
    write(
        &piece,
        "src/new.rs",
        b"a file long enough to be found renamed\nand a changed line\n",
    );

    let shown = files::what_changed(&reader(), &f.plane, branch(&f), "src/new.rs").unwrap();

    assert_eq!(shown.change.mark, Mark::Renamed);
    assert_eq!(shown.change.from.as_deref(), Some("src/old.rs"));
    let FileDiff::Text { base, hunks, .. } = shown.diff else {
        panic!("a text diff: {:?}", shown.diff);
    };
    assert_eq!(
        base,
        "a file long enough to be found renamed\nand a second line\n"
    );
    assert_eq!(
        hunks,
        [Hunk {
            old_start: 2,
            old_lines: 1,
            new_start: 2,
            new_lines: 1,
        }]
    );
}

#[test]
fn a_file_the_branch_did_not_change_is_refused_with_a_sentence_not_an_empty_diff() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut_with_files(&f);
    write(&piece, "src/lib.rs", b"changed\n");

    let refused = files::what_changed(&reader(), &f.plane, branch(&f), "src/same.rs").unwrap_err();

    assert!(
        matches!(&refused, Refused::NotChanged(path) if path == "src/same.rs"),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "'src/same.rs' is not a file this branch changed against its base"
    );
}

#[test]
fn a_binary_file_is_answered_as_binary_and_a_large_one_by_its_size() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut_with_files(&f);
    write(&piece, "logo.bin", b"\0\x03\x04");
    let large = vec![b'x'; usize::try_from(files::LARGEST).unwrap() + 1];
    write(&piece, "big.txt", &large);

    let binary = files::what_changed(&reader(), &f.plane, branch(&f), "logo.bin").unwrap();
    let too_large = files::what_changed(&reader(), &f.plane, branch(&f), "big.txt").unwrap();

    assert_eq!(binary.diff, FileDiff::Binary);
    assert_eq!(
        too_large.diff,
        FileDiff::TooLarge {
            bytes: files::LARGEST + 1
        }
    );
    assert_eq!(too_large.change.mark, Mark::Added);
}

#[test]
fn a_path_that_leaves_the_branch_or_names_gits_folder_is_refused_before_anything_is_read() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut_with_files(&f);
    write(&piece, "src/lib.rs", b"changed\n");

    for path in [
        "../thing/src/lib.rs",
        "src/../../x",
        "/etc/passwd",
        "",
        ".git/config",
        ".GIT/config",
        "src/.git/HEAD",
    ] {
        let refused = files::what_changed(&reader(), &f.plane, branch(&f), path).unwrap_err();
        assert!(
            matches!(&refused, Refused::NotInPiece(said) if said == path),
            "{path}: {refused:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_path_through_a_link_is_refused_and_nothing_past_it_is_read() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut_with_files(&f);
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "secret.txt", b"not the branch's\n");
    std::os::unix::fs::symlink(outside.path(), piece.join("out")).unwrap();

    let refused =
        files::what_changed(&reader(), &f.plane, branch(&f), "out/secret.txt").unwrap_err();

    // Refused by the reader's child, which confines the path in the folder it found (#1189),
    // in the sentence every file command says it with.
    assert_eq!(
        refused.to_string(),
        "'out/secret.txt' is not a path inside the branch's folder"
    );
}

#[test]
fn a_branch_that_is_not_there_is_refused_by_name() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut_with_files(&f);

    let refused = files::what_changed(
        &reader(),
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "nope"),
        "src/lib.rs",
    )
    .unwrap_err();

    // The reader's child refuses it, in the sentence the app's own lookup said it with (#1189).
    assert_eq!(
        refused.to_string(),
        format!(
            "{} in workspace '{}' has no branch folder called 'nope'",
            f.repo, f.ws
        )
    );
}

/// #1189: "Show what changed" finds the branch's folder in the reader's child, never in the
/// process that asks. With a reader that cannot start, nothing of the branch is looked up here:
/// a branch that is not there is answered with the reader's failure, not refused by name, and
/// a path refused by its spelling alone is refused before any child is asked.
#[test]
fn the_branchs_folder_is_found_by_the_reader_and_not_in_this_process() {
    purlis_core::unsteered!();
    let plane = tempfile::tempdir().unwrap();
    let nobody = files::Reader::new(
        plane.path().join("no-such-program"),
        [files::READ_ARG].map(std::ffi::OsString::from),
    );

    for branch in [
        Branch::piece("alpha", "widget", "nope"),
        Branch::repo("alpha", "widget"),
    ] {
        let refused = files::what_changed(&nobody, plane.path(), branch, "src/lib.rs").unwrap_err();
        assert!(
            refused.to_string().starts_with(files::READ_FAILED),
            "{branch:?}: {refused}"
        );
    }

    let refused = files::what_changed(
        &nobody,
        plane.path(),
        Branch::repo("alpha", "widget"),
        "../x",
    )
    .unwrap_err();
    assert!(
        matches!(&refused, Refused::NotInPiece(said) if said == "../x"),
        "{refused:?}"
    );
}
