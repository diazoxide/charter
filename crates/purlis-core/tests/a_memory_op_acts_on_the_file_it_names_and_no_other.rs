//! The memory operations the window and `charter … edit|archive|unarchive` call act on the file
//! they were named, and on no other (SI-9d). An adversarial review of SI-9a/b/c found each of
//! these by reproducing it; every test here failed before its fix.
//!
//! - `open`, `edit`, `archive_one` and `unarchive` take a memory's exact name. A slug that is only
//!   the tail of another file's name (`deploy` of `prod-deploy.md`) names nothing to them.
//! - The short-slug lookup a person types (`ws todo done write-the-migration`) refuses a slug
//!   that more than one file ends in, rather than taking the first.
//! - An index line's title is escaped, and a retitle or a drop touches only the line whose
//!   leading `- [..](file)` links the file — never a line whose title mentions it.
//! - `MEMORY` is the store's index, not a memory.

use purlis_core::memstore::{self, Base, EditRefused};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// A plane with one persona store, `personas/p/memory/`: the plane's root and the store.
fn store() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    let dir = root.join("personas/p/memory");
    fs::create_dir_all(&dir).unwrap();
    (tmp, root, dir)
}

/// A memory `name.md` titled `title`, indexed.
fn mem(root: &Path, dir: &Path, name: &str, title: &str) {
    fs::write(
        dir.join(format!("{name}.md")),
        format!("# {title}\n\n_2026-01-01 10:00 · persistent_\n\nbody of {name}\n"),
    )
    .unwrap();
    memstore::index_append(root, &dir.join("MEMORY.md"), &format!("{name}.md"), title).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn index(dir: &Path) -> String {
    read(&dir.join("MEMORY.md"))
}

// --- 1: the four operations take the exact name ----------------------------------------------

#[test]
fn a_memory_archived_elsewhere_does_not_read_as_the_memory_its_name_ends_with() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "deploy", "Deploy");
    mem(&root, &dir, "prod-deploy", "Prod deploy");
    memstore::archive_one(&root, &dir, "deploy").unwrap();

    let opened = memstore::open(&root, &dir, "deploy");

    assert_eq!(
        opened.map(|(path, _)| path).unwrap_err().kind(),
        ErrorKind::NotFound,
        "a tab for `deploy` must not be handed prod-deploy.md"
    );
}

#[test]
fn a_save_of_a_memory_archived_meanwhile_never_writes_the_memory_its_name_ends_with() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "deploy", "Deploy");
    mem(&root, &dir, "prod-deploy", "Prod deploy");
    let prod = read(&dir.join("prod-deploy.md"));
    memstore::archive_one(&root, &dir, "deploy").unwrap();

    let saved = memstore::edit(
        &root,
        &dir,
        "deploy",
        "Deploy v2",
        "my edit",
        Base::Overwrite,
    );

    assert!(
        matches!(&saved, Err(EditRefused::Io(e)) if e.kind() == ErrorKind::NotFound),
        "{saved:?}"
    );
    assert_eq!(read(&dir.join("prod-deploy.md")), prod);
}

#[test]
fn archiving_twice_leaves_the_memory_its_name_ends_with_in_the_store() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "foo", "Foo");
    mem(&root, &dir, "old-foo", "Old foo");

    let first = memstore::archive_one(&root, &dir, "foo").unwrap();
    let second = memstore::archive_one(&root, &dir, "foo").unwrap();

    assert_eq!(
        second, first,
        "the second is the first's answer: already archived"
    );
    assert!(dir.join("old-foo.md").is_file(), "old-foo was never named");
    assert!(index(&dir).contains("(old-foo.md)"));
}

#[test]
fn undo_of_a_delete_restores_the_memory_deleted_and_renames_no_other() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "deploy", "Deploy");
    mem(&root, &dir, "prod-deploy", "Prod deploy");
    let archived = memstore::archive_one(&root, &dir, "deploy").unwrap();
    // A stale tab's Delete, then its Undo.
    let again = memstore::archive_one(&root, &dir, "deploy").unwrap();
    assert_eq!(again, archived);
    let stem = again.file_stem().unwrap().to_string_lossy().into_owned();

    let back = memstore::unarchive(&root, &dir, &stem, Some("deploy")).unwrap();

    assert_eq!(back, dir.join("deploy.md"));
    assert!(read(&back).contains("body of deploy\n"));
    assert!(dir.join("prod-deploy.md").is_file());
    assert!(read(&dir.join("prod-deploy.md")).contains("body of prod-deploy\n"));
}

#[test]
fn unarchive_of_a_name_in_neither_place_is_not_found_whatever_ends_with_it() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "old-foo", "Old foo");
    mem(&root, &dir, "old-bar", "Old bar");
    memstore::archive_one(&root, &dir, "old-bar").unwrap();

    // "Already back" must not be read off a store file whose name only ends in the slug …
    assert_eq!(
        memstore::unarchive(&root, &dir, "foo", None)
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
    // … and the archive is not searched by tail either.
    assert_eq!(
        memstore::unarchive(&root, &dir, "bar", None)
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
    assert!(dir.join("archive/old-bar.md").is_file());
}

#[test]
fn the_short_slug_a_person_types_is_refused_when_two_files_end_in_it() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "20260101-100000-the-migration", "One");
    mem(&root, &dir, "20260102-100000-the-migration", "Two");
    mem(&root, &dir, "20260103-100000-the-rollback", "Three");

    let refused = memstore::resolve(&root, &dir, "the-migration").unwrap_err();
    assert_eq!(refused.kind(), ErrorKind::InvalidInput, "{refused}");
    assert!(
        refused
            .to_string()
            .contains("20260101-100000-the-migration")
            && refused
                .to_string()
                .contains("20260102-100000-the-migration"),
        "the refusal names both: {refused}"
    );
    assert_eq!(
        memstore::resolve(&root, &dir, "the-rollback").unwrap(),
        dir.join("20260103-100000-the-rollback.md")
    );
    assert_eq!(
        memstore::resolve(&root, &dir, "20260102-100000-the-migration").unwrap(),
        dir.join("20260102-100000-the-migration.md"),
        "a full name is exact, whatever else ends in it"
    );
}

#[test]
fn forget_of_a_slug_two_files_end_in_deletes_neither() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a-x", "A");
    mem(&root, &dir, "b-x", "B");

    let refused = memstore::forget(&root, &dir, "x").unwrap_err();

    assert_eq!(refused.kind(), ErrorKind::InvalidInput, "{refused}");
    assert!(dir.join("a-x.md").is_file() && dir.join("b-x.md").is_file());
}

// --- 2: an index line is escaped, and only its own line is rewritten -------------------------

#[test]
fn a_title_holding_brackets_is_written_to_the_index_escaped() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();

    memstore::index_append(
        &root,
        &dir.join("MEMORY.md"),
        "a.md",
        r"see [x](b.md) \ here",
    )
    .unwrap();

    assert!(
        index(&dir).contains(r"- [see \[x\](b.md) \\ here](a.md)"),
        "{}",
        index(&dir)
    );
}

#[test]
fn a_retitle_rewrites_only_the_line_that_links_the_file() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    mem(&root, &dir, "b", "B");
    let (_, text) = memstore::open(&root, &dir, "a").unwrap();
    memstore::edit(
        &root,
        &dir,
        "a",
        "see [x](b.md) here",
        "body",
        Base::Read(&text),
    )
    .unwrap();
    let (_, text) = memstore::open(&root, &dir, "b").unwrap();

    memstore::edit(&root, &dir, "b", "B2", "body", Base::Read(&text)).unwrap();

    let index = index(&dir);
    assert!(index.contains(r"- [see \[x\](b.md) here](a.md)"), "{index}");
    assert!(index.contains("- [B2](b.md)"), "{index}");
    assert_eq!(
        memstore::listed(&root, &dir).len(),
        2,
        "{:?}",
        memstore::listed(&root, &dir)
    );
}

#[test]
fn a_retitle_reads_a_line_written_before_titles_were_escaped() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    mem(&root, &dir, "b", "B");
    // The shape an older charter wrote: the title as it was, brackets and all.
    fs::write(
        dir.join("MEMORY.md"),
        "# Memory Index\n\n- [see [x](b.md) here](a.md) — kept\n- [B](b.md)\n",
    )
    .unwrap();

    let (_, text) = memstore::open(&root, &dir, "b").unwrap();
    memstore::edit(&root, &dir, "b", "B2", "body", Base::Read(&text)).unwrap();
    assert_eq!(
        index(&dir),
        "# Memory Index\n\n- [see [x](b.md) here](a.md) — kept\n- [B2](b.md)\n"
    );
    let (_, text) = memstore::open(&root, &dir, "a").unwrap();
    memstore::edit(&root, &dir, "a", "A2", "body", Base::Read(&text)).unwrap();

    assert_eq!(
        index(&dir),
        "# Memory Index\n\n- [A2](a.md) — kept\n- [B2](b.md)\n"
    );
}

#[test]
fn archiving_a_memory_keeps_the_line_of_another_whose_title_mentions_it() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    mem(&root, &dir, "b", "B");
    fs::write(
        dir.join("MEMORY.md"),
        "# Memory Index\n\n- [see [x](b.md) here](a.md)\n- [B](b.md)\n",
    )
    .unwrap();

    memstore::archive_one(&root, &dir, "b").unwrap();

    assert_eq!(
        index(&dir),
        "# Memory Index\n\n- [see [x](b.md) here](a.md)\n"
    );
}

// --- 3: `MEMORY` is the index, not a memory ---------------------------------------------------

#[test]
fn the_index_is_not_a_memory_to_any_operation() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    let before = index(&dir);

    for name in ["MEMORY", "MEMORY.md"] {
        assert_eq!(
            memstore::open(&root, &dir, name).unwrap_err().kind(),
            ErrorKind::InvalidInput,
            "open {name}"
        );
        assert!(
            matches!(
                memstore::edit(&root, &dir, name, "T", "body", Base::Overwrite),
                Err(EditRefused::Io(e)) if e.kind() == ErrorKind::InvalidInput
            ),
            "edit {name}"
        );
        assert_eq!(
            memstore::archive_one(&root, &dir, name).unwrap_err().kind(),
            ErrorKind::InvalidInput,
            "archive {name}"
        );
        assert_eq!(
            memstore::unarchive(&root, &dir, "a", Some(name))
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidInput,
            "unarchive as {name}"
        );
        assert!(
            memstore::forget(&root, &dir, name).is_err(),
            "forget {name}"
        );
        assert!(
            memstore::resolve(&root, &dir, name).is_err(),
            "resolve {name}"
        );
    }

    assert_eq!(index(&dir), before);
    assert!(!dir.join("archive/MEMORY.md").exists());
}

// --- 5: remember's append and an edit's retitle do not lose each other's line ----------------

#[cfg(unix)]
#[test]
fn a_remember_waits_for_the_store_lock_an_edit_holds() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    let held = purlis_core::rewrite::Lock::on(&dir);

    let writer = {
        let (root, dir) = (root.clone(), dir.clone());
        std::thread::spawn(move || {
            memstore::write(
                &root,
                &dir,
                "a new fact",
                Some("New"),
                false,
                "persistent",
                true,
                "2026-01-01T10:00:00".parse().unwrap(),
            )
            .unwrap()
        })
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    let while_held = index(&dir);
    drop(held);
    writer.join().unwrap();

    assert!(
        !while_held.contains("(new.md)"),
        "the index was appended to while another writer held the store: {while_held}"
    );
    assert!(index(&dir).contains("- [New](new.md)"));
}

#[cfg(unix)]
#[test]
fn a_forget_waits_for_the_store_lock_an_edit_holds() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    mem(&root, &dir, "b", "B");
    let held = purlis_core::rewrite::Lock::on(&dir);

    let forgetting = {
        let (root, dir) = (root.clone(), dir.clone());
        std::thread::spawn(move || memstore::forget(&root, &dir, "b").unwrap())
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    let while_held = dir.join("b.md").exists();
    drop(held);
    forgetting.join().unwrap();

    assert!(
        while_held,
        "forget removed a file while another writer held the store"
    );
    assert!(!dir.join("b.md").exists());
}

// --- 6: `archive` reports a move only when something moved -----------------------------------

#[test]
fn archive_answers_none_for_a_memory_that_was_already_archived() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "dup", "Dup");
    assert!(memstore::archive(&root, &dir, "dup.md").is_some());

    assert_eq!(memstore::archive(&root, &dir, "dup.md"), None);
}

#[cfg(unix)]
#[test]
fn remembering_while_an_edit_retitles_loses_no_index_line() {
    purlis_core::unsteered!();
    // Before the fix this lost about three lines in ten: the append went to the index's old
    // inode, which the retitle then replaced with what it had read.
    const N: usize = 120;
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    let editor = {
        let (root, dir) = (root.clone(), dir.clone());
        std::thread::spawn(move || {
            for i in 0..N {
                memstore::edit(&root, &dir, "a", &format!("A{i}"), "body", Base::Overwrite)
                    .unwrap();
            }
        })
    };
    for i in 0..N {
        memstore::write(
            &root,
            &dir,
            &format!("fact {i}"),
            Some(&format!("F{i}")),
            false,
            "persistent",
            true,
            "2026-01-01T10:00:00".parse().unwrap(),
        )
        .unwrap();
    }
    editor.join().unwrap();

    assert_eq!(memstore::listed(&root, &dir).len(), N + 1);
}
