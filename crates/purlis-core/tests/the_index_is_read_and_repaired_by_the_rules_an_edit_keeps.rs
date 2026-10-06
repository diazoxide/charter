//! Two loose ends SI-9d left in the memory index, each reproduced by a test that failed before
//! its fix (SI-9e):
//!
//! - `optimize --apply`'s index repair (`curate::apply_safe`) appends under the store's
//!   `rewrite::Lock`, as `write` does since SI-9d: an append between an edit's read of the index
//!   and its replace went with the old file.
//! - `memstore::listed` reads a line by the link that closes its leading `- [..]` element — the
//!   rule a retitle and a drop read it by — so a title that mentions `(b.md)` does not list
//!   `b.md`, and unarchiving `b` puts its line back.

use purlis_core::memstore::{self, Base};
use std::collections::BTreeSet;
use std::fs;
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

/// A memory `name.md` titled `title`, not indexed.
fn file(dir: &Path, name: &str, title: &str) {
    fs::write(
        dir.join(format!("{name}.md")),
        format!("# {title}\n\n_2026-01-01 10:00 · persistent_\n\nbody of {name}\n"),
    )
    .unwrap();
}

/// A memory `name.md` titled `title`, indexed.
fn mem(root: &Path, dir: &Path, name: &str, title: &str) {
    file(dir, name, title);
    memstore::index_append(root, &dir.join("MEMORY.md"), &format!("{name}.md"), title).unwrap();
}

fn index(dir: &Path) -> String {
    fs::read_to_string(dir.join("MEMORY.md")).unwrap()
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn today() -> chrono::NaiveDate {
    "2026-01-02".parse().unwrap()
}

// --- 1: the index repair holds the store's lock ----------------------------------------------

#[cfg(unix)]
#[test]
fn an_index_repair_waits_for_another_writer_of_the_store() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    file(&dir, "c", "C");
    let held = purlis_core::rewrite::Lock::on(&dir);

    let repairing = {
        let (root, dir) = (root.clone(), dir.clone());
        std::thread::spawn(move || purlis_core::curate::apply_safe(&root, &dir, today()).unwrap())
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    let while_held = index(&dir);
    drop(held);
    let actions = repairing.join().unwrap();

    assert!(
        !while_held.contains("(c.md)"),
        "the repair appended while another writer held the store:\n{while_held}"
    );
    assert!(index(&dir).contains("- [C](c.md)"), "{actions:?}");
}

#[cfg(unix)]
#[test]
fn repairing_the_index_while_an_edit_retitles_loses_no_index_line() {
    purlis_core::unsteered!();
    // Before the fix the repair's append went to the index's old inode whenever it fell between
    // the retitle's read and its replace, and the retitle put back what it had read.
    const N: usize = 60;
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "A");
    let editor = {
        let (root, dir) = (root.clone(), dir.clone());
        std::thread::spawn(move || {
            for i in 0..N * 4 {
                memstore::edit(&root, &dir, "a", &format!("A{i}"), "body", Base::Overwrite)
                    .unwrap();
            }
        })
    };
    // A repair relinks every line lost before it, so what shows a loss is a repair that had to
    // link more than the one file made for it.
    let mut relinked = Vec::new();
    for i in 0..N {
        file(&dir, &format!("f{i}"), &format!("F{i}"));
        let actions = purlis_core::curate::apply_safe(&root, &dir, today()).unwrap();
        if actions != ["repaired index: linked 1 unindexed memory(ies)"] {
            relinked.push((i, actions));
        }
    }
    editor.join().unwrap();

    assert_eq!(relinked, Vec::<(usize, Vec<String>)>::new());
    assert_eq!(memstore::listed(&root, &dir).len(), N + 1);
}

// --- 2: `listed` reads a line by its leading link --------------------------------------------

#[test]
fn a_title_that_mentions_another_memory_does_not_list_it() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    fs::write(
        dir.join("MEMORY.md"),
        "# Memory Index\n\n- [see (b.md) first](a.md)\n- [see \\[x\\](c.md)](d.md)\n",
    )
    .unwrap();

    assert_eq!(memstore::listed(&root, &dir), names(&["a.md", "d.md"]));
}

#[test]
fn a_line_written_before_titles_were_escaped_lists_only_its_own_file() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    fs::write(
        dir.join("MEMORY.md"),
        "# Memory Index\n\n- [see [x](b.md) here](a.md) — see also (c.md)\n",
    )
    .unwrap();

    assert_eq!(memstore::listed(&root, &dir), names(&["a.md"]));
}

#[test]
fn a_line_of_another_shape_is_read_as_charter_always_read_it() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    fs::write(
        dir.join("MEMORY.md"),
        "# Memory Index\n\n* [B](b.md)\nSee (c.md) and (d.md).\n- [unclosed (e.md)\n- [site](https://example.com/f.md)\n",
    )
    .unwrap();

    assert_eq!(
        memstore::listed(&root, &dir),
        names(&["b.md", "c.md", "d.md", "e.md"])
    );
}

#[test]
fn unarchiving_a_memory_another_title_mentions_puts_its_index_line_back() {
    purlis_core::unsteered!();
    let (_tmp, root, dir) = store();
    mem(&root, &dir, "a", "see (b.md) first");
    mem(&root, &dir, "b", "B");
    memstore::archive_one(&root, &dir, "b").unwrap();
    assert!(!index(&dir).contains("](b.md)"), "{}", index(&dir));

    memstore::unarchive(&root, &dir, "b", None).unwrap();

    let index = index(&dir);
    assert!(index.contains("- [B](b.md)"), "{index}");
    assert!(index.contains("- [see (b.md) first](a.md)"), "{index}");
    assert_eq!(
        memstore::index_drift(&root, &dir).unwrap(),
        (vec![], vec![])
    );
}
