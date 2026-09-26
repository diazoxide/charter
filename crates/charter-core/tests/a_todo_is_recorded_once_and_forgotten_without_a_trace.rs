//! Recording a todo and forgetting one, as `charter ws todo` and the window's Todos panel both
//! do it (SI-3): one rule in the core, so a terminal and the window refuse the same things.

use charter_core::workspaces::{Plane, RecordRefused, Workspace};

fn alpha(tmp: &tempfile::TempDir) -> Workspace {
    std::fs::write(tmp.path().join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("workspaces/alpha")).unwrap();
    Plane::open(tmp.path()).workspace("alpha").unwrap()
}

fn at() -> chrono::NaiveDateTime {
    "2026-05-04T11:32:17".parse().unwrap()
}

fn titles(ws: &Workspace) -> Vec<String> {
    ws.todos().unwrap().into_iter().map(|t| t.title).collect()
}

#[test]
fn a_todo_about_work_already_on_the_list_is_refused_with_the_one_it_repeats() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.record_todo("Review the rollout plan for staging", at())
        .unwrap();

    let refused = ws
        .record_todo("review the rollout plan for staging today", at())
        .unwrap_err();

    match refused {
        RecordRefused::AlreadyListed(title) => {
            assert_eq!(title, "Review the rollout plan for staging");
        }
        other => panic!("{other}"),
    }
    assert_eq!(titles(&ws), ["Review the rollout plan for staging"]);
}

#[test]
fn a_todo_with_no_words_is_refused_and_nothing_is_written() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);

    let refused = ws.record_todo("   \n ", at()).unwrap_err();

    assert!(matches!(refused, RecordRefused::Empty), "{refused}");
    assert!(titles(&ws).is_empty());
}

#[test]
fn a_forgotten_todo_is_gone_and_the_journal_says_nothing_of_it() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.record_todo("Drop the old importer", at()).unwrap();
    let slug = ws.todos().unwrap()[0].slug.clone();

    ws.forget_todo(&slug).unwrap();

    assert!(titles(&ws).is_empty());
    assert!(ws.memories().unwrap().is_empty(), "a forget was journalled");
}

#[test]
fn a_slug_that_reaches_out_of_the_todo_store_is_refused_and_its_target_survives() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let victim = tmp.path().join("workspaces/alpha/workspace.md");
    std::fs::write(&victim, "# alpha\n").unwrap();

    let refused = ws.forget_todo("../workspace").unwrap_err();

    assert_eq!(
        refused.kind(),
        std::io::ErrorKind::InvalidInput,
        "{refused}"
    );
    assert!(victim.exists());
}
