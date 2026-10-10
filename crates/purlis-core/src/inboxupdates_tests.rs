//! The Inbox's updates (#1693): what is kept, for how long, what a dismissal holds and what
//! Mark all read changes. Driven through [`Store`] alone, on a data home of the test's own.

use super::*;

const HOUR: u64 = 60 * 60;
const NOW: u64 = 1_800_000_000;

fn project() -> (tempfile::TempDir, std::path::PathBuf, Store) {
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).expect("a project");
    let store = Store::in_data(&dir.path().join("data"));
    (dir, root, store)
}

fn update(key: &str, kind: Kind, at: u64) -> Update {
    Update {
        key: key.to_owned(),
        kind,
        at,
        session: Some(3),
        chain: vec!["steward 12".to_owned(), "drill".to_owned()],
        says: format!("{key} happened"),
        read: false,
        dismissed: false,
    }
}

fn keys(updates: &[Update]) -> Vec<&str> {
    updates.iter().map(|one| one.key.as_str()).collect()
}

#[test]
fn what_is_noted_is_read_back_newest_first_and_unread() {
    let (_dir, root, store) = project();
    store
        .note(
            &root,
            vec![
                update("task:a", Kind::TaskDone, NOW - 3 * HOUR),
                update("task:b", Kind::TaskFailed, NOW - HOUR),
                update("doctor:git", Kind::Doctor, NOW - 2 * HOUR),
            ],
            NOW,
        )
        .expect("noted");

    let read = store.read(&root, NOW);
    assert_eq!(keys(&read), ["task:b", "doctor:git", "task:a"]);
    assert!(read.iter().all(|one| !one.read && !one.dismissed));
    assert_eq!(read[0].chain, ["steward 12", "drill"]);
}

#[test]
fn it_survives_a_new_store_on_the_same_data_home_as_a_relaunch_does() {
    let (dir, root, store) = project();
    store
        .note(&root, vec![update("resumed:4", Kind::Resumed, NOW)], NOW)
        .expect("noted");

    let again = Store::in_data(&dir.path().join("data"));
    assert_eq!(keys(&again.read(&root, NOW + HOUR)), ["resumed:4"]);
}

#[test]
fn an_update_is_kept_for_a_day_and_no_longer() {
    let (_dir, root, store) = project();
    store
        .note(
            &root,
            vec![
                update("old", Kind::TaskDone, NOW - KEPT_FOR_SECS - 1),
                update("young", Kind::TaskDone, NOW - KEPT_FOR_SECS + HOUR),
            ],
            NOW,
        )
        .expect("noted");

    assert_eq!(
        keys(&store.read(&root, NOW)),
        ["young"],
        "older than a day is never kept"
    );
    assert!(
        store.read(&root, NOW + 2 * HOUR).is_empty(),
        "and what was kept goes once its day is over"
    );
}

#[test]
fn noting_the_same_update_again_keeps_the_first_and_its_marks() {
    let (_dir, root, store) = project();
    store
        .note(
            &root,
            vec![update("task:a", Kind::TaskDone, NOW - HOUR)],
            NOW,
        )
        .expect("noted");
    store
        .settle(&root, None, Settle::Read, NOW)
        .expect("marked read");

    let mut again = update("task:a", Kind::TaskFailed, NOW);
    again.says = "something else".to_owned();
    store.note(&root, vec![again], NOW).expect("noted again");

    let read = store.read(&root, NOW);
    assert_eq!(read.len(), 1);
    assert_eq!(
        (read[0].kind, read[0].at, read[0].read),
        (Kind::TaskDone, NOW - HOUR, true)
    );
    assert_eq!(read[0].says, "task:a happened");
}

#[test]
fn a_dismissed_update_is_not_brought_back_by_its_source_saying_it_again() {
    let (_dir, root, store) = project();
    store
        .note(
            &root,
            vec![
                update("doctor:git", Kind::Doctor, NOW - HOUR),
                update("task:a", Kind::TaskDone, NOW - HOUR),
            ],
            NOW,
        )
        .expect("noted");
    store
        .settle(
            &root,
            Some(&["doctor:git".to_owned()]),
            Settle::Dismissed,
            NOW,
        )
        .expect("dismissed");

    store
        .note(&root, vec![update("doctor:git", Kind::Doctor, NOW)], NOW)
        .expect("noted again");
    assert_eq!(keys(&store.read(&root, NOW)), ["task:a"]);
}

#[test]
fn dismiss_all_and_mark_all_read_act_on_every_update_kept() {
    let (_dir, root, store) = project();
    store
        .note(
            &root,
            vec![
                update("a", Kind::TaskDone, NOW - 2 * HOUR),
                update("b", Kind::Sandbox, NOW - HOUR),
            ],
            NOW,
        )
        .expect("noted");

    let read = store
        .settle(&root, None, Settle::Read, NOW)
        .expect("marked read");
    assert!(read.iter().all(|one| one.read), "{read:?}");
    assert_eq!(keys(&read), ["b", "a"], "nothing is put away by reading it");

    let left = store
        .settle(&root, None, Settle::Dismissed, NOW)
        .expect("dismissed");
    assert!(left.is_empty());
    assert!(store.read(&root, NOW).is_empty());
}

#[test]
fn a_time_ahead_of_the_clock_is_taken_as_now() {
    let (_dir, root, store) = project();
    store
        .note(
            &root,
            vec![update("ahead", Kind::TaskDone, NOW + 10 * HOUR)],
            NOW,
        )
        .expect("noted");
    assert_eq!(store.read(&root, NOW)[0].at, NOW);
}

#[test]
fn what_a_chat_named_is_held_to_its_bounds() {
    let (_dir, root, store) = project();
    let mut long = update(&"k".repeat(MOST_KEY * 2), Kind::TaskDone, NOW);
    long.says = "s".repeat(MOST_SAID * 2);
    long.chain = (0..MOST_CHAIN * 2)
        .map(|n| "n".repeat(MOST_NAME + n))
        .collect();
    store.note(&root, vec![long], NOW).expect("noted");

    let read = store.read(&root, NOW);
    assert_eq!(read.len(), 1);
    assert!(read[0].key.chars().count() <= MOST_KEY);
    assert!(read[0].says.chars().count() <= MOST_SAID);
    assert_eq!(read[0].chain.len(), MOST_CHAIN);
    assert!(
        read[0]
            .chain
            .iter()
            .all(|name| name.chars().count() <= MOST_NAME)
    );
}

#[test]
fn a_project_keeps_at_most_so_many_the_oldest_going_first() {
    let (_dir, root, store) = project();
    let many: Vec<Update> = (0..AT_MOST_KEPT as u64 + 5)
        .map(|n| update(&format!("u{n}"), Kind::TaskDone, NOW - HOUR + n))
        .collect();
    store.note(&root, many, NOW).expect("noted");

    let read = store.read(&root, NOW);
    assert_eq!(read.len(), AT_MOST_KEPT);
    assert!(!keys(&read).contains(&"u0"), "the oldest went first");
}

#[test]
fn each_project_has_its_own_updates() {
    let (dir, root, store) = project();
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).expect("another project");
    store
        .note(&root, vec![update("mine", Kind::TaskDone, NOW)], NOW)
        .expect("noted");

    assert!(store.read(&other, NOW).is_empty());
    assert_eq!(keys(&store.read(&root, NOW)), ["mine"]);
}

#[test]
fn a_record_that_cannot_be_read_reads_as_none_and_is_written_over() {
    let (_dir, root, store) = project();
    std::fs::create_dir_all(store.file(&root).parent().expect("a folder")).expect("folder");
    std::fs::write(store.file(&root), "not json").expect("written");

    assert!(store.read(&root, NOW).is_empty());
    store
        .note(&root, vec![update("a", Kind::TaskDone, NOW)], NOW)
        .expect("noted");
    assert_eq!(keys(&store.read(&root, NOW)), ["a"]);
}

#[cfg(unix)]
#[test]
fn the_record_is_the_person_s_alone_to_read() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_dir, root, store) = project();
    store
        .note(&root, vec![update("a", Kind::TaskDone, NOW)], NOW)
        .expect("noted");
    let mode = std::fs::metadata(store.file(&root))
        .expect("the file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn the_kinds_are_written_in_their_own_words() {
    let words: Vec<String> = [
        Kind::TaskDone,
        Kind::TaskFailed,
        Kind::Doctor,
        Kind::Resumed,
        Kind::Sandbox,
        Kind::RefusedAway,
        Kind::SmartClose,
    ]
    .iter()
    .map(|kind| serde_json::to_string(kind).expect("a word"))
    .collect();
    assert_eq!(
        words,
        [
            "\"task-done\"",
            "\"task-failed\"",
            "\"doctor\"",
            "\"resumed\"",
            "\"sandbox\"",
            "\"refused-away\"",
            "\"smart-close\""
        ]
    );
}
