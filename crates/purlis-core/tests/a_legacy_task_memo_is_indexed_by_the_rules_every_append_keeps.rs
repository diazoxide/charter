//! A pre-v2 workspace's `memory/notes.md` is grandfathered into its index as
//! `- [Task memo (legacy)](notes.md)` (SI-9f), by the rules every other index append keeps since
//! SI-9d and SI-9e — each reproduced by a test that failed before its fix:
//!
//! - whether the index already lists `notes.md` is read by the leading link
//!   (`memstore::listed`), so a title that mentions `(notes.md)` does not stand in for the
//!   memo's own line;
//! - the line is appended under the store's `rewrite::Lock`, as `write`, `unarchive` and
//!   `optimize --apply`'s repair append theirs.

use purlis_core::workspaces::{Plane, Workspace};
use std::fs;
use std::path::{Path, PathBuf};

const LEGACY_LINE: &str = "- [Task memo (legacy)](notes.md)";

/// A plane with one workspace, `alpha`, whose journal holds a legacy `notes.md`.
fn legacy(tmp: &tempfile::TempDir) -> (Workspace, PathBuf) {
    fs::write(tmp.path().join("charter.toml"), "schema = 1\n").unwrap();
    let memory = tmp.path().join("workspaces/alpha/memory");
    fs::create_dir_all(&memory).unwrap();
    fs::write(
        memory.join("notes.md"),
        "# alpha\n\n- the old single-log memo\n",
    )
    .unwrap();
    (Plane::open(tmp.path()).workspace("alpha").unwrap(), memory)
}

fn at() -> chrono::NaiveDateTime {
    "2026-03-02T09:14:00".parse().unwrap()
}

fn index(memory: &Path) -> String {
    fs::read_to_string(memory.join("MEMORY.md")).unwrap_or_default()
}

/// An index whose only line is a memory titled after the legacy memo.
fn mentioned(memory: &Path) {
    fs::write(memory.join("a.md"), "# see (notes.md) first\n\nbody\n").unwrap();
    fs::write(
        memory.join("MEMORY.md"),
        "# alpha — task memory\n\n- [see (notes.md) first](a.md)\n",
    )
    .unwrap();
}

// --- 1: a title that mentions `(notes.md)` is not the memo's line ----------------------------

#[test]
fn scaffolding_indexes_the_legacy_memo_when_only_a_title_mentions_it() {
    purlis_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let (ws, memory) = legacy(&tmp);
    mentioned(&memory);

    ws.scaffold_memory().unwrap();

    let index = index(&memory);
    assert!(index.contains(LEGACY_LINE), "{index}");
    assert!(index.contains("- [see (notes.md) first](a.md)"), "{index}");
}

#[test]
fn remembering_indexes_the_legacy_memo_when_only_a_title_mentions_it() {
    purlis_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let (ws, memory) = legacy(&tmp);
    mentioned(&memory);

    ws.remember("A fact", at()).unwrap();

    let index = index(&memory);
    assert!(index.contains(LEGACY_LINE), "{index}");
}

#[test]
fn the_legacy_memo_is_indexed_once_however_often_it_is_asked() {
    purlis_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let (ws, memory) = legacy(&tmp);

    ws.scaffold_memory().unwrap();
    ws.remember("A fact", at()).unwrap();
    ws.scaffold_memory().unwrap();
    ws.remember("Another fact", at()).unwrap();

    assert_eq!(
        index(&memory).matches("](notes.md)").count(),
        1,
        "{}",
        index(&memory)
    );
}

#[test]
fn a_hand_written_line_that_links_the_memo_still_lists_it() {
    purlis_core::unsteered!();
    // A line of another shape is read by charter's pattern, as `listed` reads it.
    let tmp = tempfile::tempdir().unwrap();
    let (ws, memory) = legacy(&tmp);
    fs::write(
        memory.join("MEMORY.md"),
        "# alpha — task memory\n\nThe old memo is (notes.md).\n",
    )
    .unwrap();

    ws.scaffold_memory().unwrap();

    assert!(!index(&memory).contains(LEGACY_LINE), "{}", index(&memory));
}

// --- 2: the legacy line is appended under the store's lock ------------------------------------

/// Run `op` on another thread while this one holds the journal's lock, and answer the index as
/// it stood while the lock was held and once `op` finished.
#[cfg(unix)]
fn while_locked(
    ws: Workspace,
    memory: &Path,
    op: impl FnOnce(&Workspace) + Send + 'static,
) -> (String, String) {
    let held = purlis_core::rewrite::Lock::on(memory);
    let running = std::thread::spawn(move || op(&ws));
    std::thread::sleep(std::time::Duration::from_millis(300));
    let while_held = index(memory);
    drop(held);
    running.join().unwrap();
    (while_held, index(memory))
}

#[cfg(unix)]
#[test]
fn scaffolding_waits_for_another_writer_of_the_journal_before_indexing_the_memo() {
    purlis_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let (ws, memory) = legacy(&tmp);

    let (while_held, after) = while_locked(ws, &memory, |ws| {
        ws.scaffold_memory().unwrap();
    });

    assert!(
        !while_held.contains("(notes.md)"),
        "the memo's line was appended while another writer held the journal:\n{while_held}"
    );
    assert!(after.contains(LEGACY_LINE), "{after}");
}

#[cfg(unix)]
#[test]
fn remembering_waits_for_another_writer_of_the_journal_before_indexing_the_memo() {
    purlis_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let (ws, memory) = legacy(&tmp);

    let (while_held, after) = while_locked(ws, &memory, |ws| {
        ws.remember("A fact", at()).unwrap();
    });

    assert!(
        !while_held.contains("(notes.md)"),
        "the memo's line was appended while another writer held the journal:\n{while_held}"
    );
    assert!(after.contains(LEGACY_LINE), "{after}");
    assert!(after.contains("- [A fact]("), "{after}");
}
