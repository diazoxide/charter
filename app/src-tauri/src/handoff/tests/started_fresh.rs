//! A task started fresh is handed its brief again, or not started at all (#1609, #1489).
//!
//! A fresh start is a new conversation, and a task's brief was the whole of what it was asked.
//! It is still its asker's task and still owes its report, so it starts fresh only where the
//! brief its dispatch was sent can be handed to it again; anywhere else nothing starts and
//! nothing ends.

use purlis_core::dispatchrecord;

use super::*;

/// The first line of what a dispatched chat started again is told when its brief is handed.
const HANDED: &str = "⟨purlis started this chat again with no conversation. It was dispatched, \
and below is the brief it was handed then";

/// What the task was briefed with, as `a_dispatch` sends it.
const ITS_BRIEF: &str = "# Check the queue\nSay how many are stuck.";

/// A steward chat and the one task it dispatched, on a pretend host.
fn a_steward_and_its_task() -> (Plane, Pretend, Planes, PlaneId, Arc<Held>, u32, u32) {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    let task = a_task_of(&held, &id, steward, "check prod");
    (plane, host, planes, id, held, steward, task)
}

/// It is still `steward`'s task, owing its report, and the only one running under it.
fn still_its_askers_task(held: &Held, steward: u32, task: u32) {
    let from = held
        .chats()
        .handed_from(task)
        .expect("it was handed from its asker");
    assert_eq!(from.chat, steward);
    assert_eq!(from.mode, purlis_core::reopen::Mode::Task);
    assert_eq!(from.report, Owed::Due);
    assert_eq!(held.chats().lineage(steward, None, &|_| true).running, 1);
    assert!(purlis_core::handback::take(held.root(), For::Chat(steward)).is_empty());
}

#[test]
fn a_task_started_fresh_is_handed_its_brief_and_goes_on_owing_its_report() {
    let (_plane, host, _planes, _id, held, steward, task) = a_steward_and_its_task();

    let again = held
        .start_chat_fresh(task, STARTING)
        .expect("a task whose brief can be handed starts fresh");

    assert_ne!(again, task);
    let told = host
        .openings()
        .pop()
        .expect("opened")
        .args
        .last()
        .cloned()
        .unwrap_or_default();
    assert!(told.starts_with(HANDED), "{told:?}");
    assert!(told.contains(purlis_core::handoff::TASK_NOTE), "{told:?}");
    assert!(told.contains(ITS_BRIEF), "the brief, verbatim: {told:?}");
    // In the old one's place: the old run has ended, and the new one carries the task on.
    assert_eq!(open_chats(&held), vec![steward, again]);
    still_its_askers_task(&held, steward, again);
}

#[test]
fn a_task_whose_brief_cannot_be_confirmed_is_not_started_fresh() {
    // The record on the disk is not the brief the dispatch was sent: the task is not started
    // on it, and is not started with nothing to say what it was asked either.
    let (_plane, host, _planes, _id, held, steward, task) = a_steward_and_its_task();
    let record = dispatchrecord::list(held.root())
        .into_iter()
        .find(|record| record.worker.chat.chat == task)
        .expect("the task's record");
    let path = dispatchrecord::dir(held.root()).join(format!("{}.json", record.id));
    let text = std::fs::read_to_string(&path).expect("the record");
    let rewritten = text.replace("Say how many are stuck.", "Empty the queue.");
    assert_ne!(text, rewritten, "the brief is in the record");
    std::fs::write(&path, rewritten).expect("rewritten");
    let opened = host.openings().len();

    assert_eq!(
        held.start_chat_fresh(task, STARTING),
        Err(crate::planes::A_TASK_IS_NOT_STARTED_FRESH.to_owned())
    );

    assert_eq!(host.openings().len(), opened, "nothing is started");
    assert_eq!(open_chats(&held), vec![steward, task], "nothing ends");
    still_its_askers_task(&held, steward, task);
}

#[test]
fn a_task_this_launch_did_not_dispatch_is_not_started_fresh() {
    // Its brief's digest is kept in this app's memory only: a task whose dispatch record names
    // it but that this app never noted (as after a relaunch) is refused, as before #1609.
    let (_plane, host, _planes, _id, held, steward, task) = a_steward_and_its_task();
    let record = dispatchrecord::list(held.root())
        .into_iter()
        .find(|record| record.worker.chat.chat == task)
        .expect("the task's record");
    let path = dispatchrecord::dir(held.root()).join(format!("{}.json", record.id));
    // The same dispatch under an id this launch never sent a brief for.
    let text = std::fs::read_to_string(&path).expect("the record");
    let elsewhere = "01K6NEVERSENT0000000000000";
    std::fs::write(
        dispatchrecord::dir(held.root()).join(format!("{elsewhere}.json")),
        text.replace(&record.id, elsewhere),
    )
    .expect("another record");
    std::fs::remove_file(&path).expect("the noted one gone");
    let opened = host.openings().len();

    assert_eq!(
        held.start_chat_fresh(task, STARTING),
        Err(crate::planes::A_TASK_IS_NOT_STARTED_FRESH.to_owned())
    );

    assert_eq!(host.openings().len(), opened, "nothing is started");
    assert_eq!(open_chats(&held), vec![steward, task], "nothing ends");
    still_its_askers_task(&held, steward, task);
}
