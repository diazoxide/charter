//! What a handoff todo is a duplicate of (#372): `todos.duplicate_of(…, by_title=True)`.
//!
//! **First lines against titles, never the whole text.** Every handoff todo ends in the same
//! provenance sentence, and compared over the whole text that boilerplate reads as agreement:
//! `Fix the widget` and `Ship the release` scored 0.750 in Python before the rule was
//! narrowed, and the second handoff into a workspace recorded nothing.

use charter_core::handoff::todo_text;
use charter_core::workspaces::{Plane, Workspace};

fn alpha(tmp: &tempfile::TempDir) -> Workspace {
    std::fs::write(tmp.path().join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("workspaces/alpha")).unwrap();
    Plane::open(tmp.path()).workspace("alpha").unwrap()
}

fn at() -> chrono::NaiveDateTime {
    "2026-05-04T11:32:17".parse().unwrap()
}

fn handed_off(brief: &str) -> String {
    todo_text(brief, "3", "default")
}

#[test]
fn the_same_first_line_is_the_same_work() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.add_todo(&handed_off("Fix the widget\n\nall of it"), at())
        .unwrap();

    assert_eq!(
        ws.todo_for_the_same_work(&handed_off("  fix  THE widget \n\nother words")),
        Some("Fix the widget".to_owned())
    );
}

#[test]
fn two_briefs_that_share_only_the_provenance_sentence_are_two_todos() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.add_todo(&handed_off("Fix the widget"), at()).unwrap();

    assert_eq!(
        ws.todo_for_the_same_work(&handed_off("Ship the release")),
        None
    );
}

#[test]
fn a_title_retitled_by_a_word_is_the_same_work() {
    charter_core::unsteered!();
    // Three words in common is the least overlap that says anything, and half the words
    // between them is the same work.
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.add_todo(&handed_off("Retry failed webhook deliveries"), at())
        .unwrap();

    assert_eq!(
        ws.todo_for_the_same_work(&handed_off("Retry failed webhook deliveries today")),
        Some("Retry failed webhook deliveries".to_owned())
    );
}

#[test]
fn two_short_titles_that_share_a_word_are_not_the_same_work() {
    charter_core::unsteered!();
    // Below three shared words the overlap is no signal, and identity decides.
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.add_todo(&handed_off("Update the README"), at()).unwrap();

    assert_eq!(
        ws.todo_for_the_same_work(&handed_off("Update the README file")),
        None
    );
}

#[test]
fn a_workspace_with_no_todos_has_nothing_it_duplicates() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);

    assert_eq!(
        ws.todo_for_the_same_work(&handed_off("Fix the widget")),
        None
    );
}
