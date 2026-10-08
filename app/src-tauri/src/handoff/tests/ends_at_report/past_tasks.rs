//! Past tasks (#1510): what is read of a task after it has ended, and after the session that
//! asked for it has closed. Against the app's own records, on a pretend session host.

use super::*;

/// The past tasks of the workspace `record`'s task worked in, read whole.
fn past_of(held: &Held, record: &dispatchrecord::Record) -> crate::past::PastTasks {
    crate::past::listed(held, record.place.workspace.as_deref(), None)
}

/// Waits for the mark a closed asking chat's rows are given, on a thread of its own.
fn until_cleared(held: &Held, task: u32) {
    let began = Instant::now();
    while !record_of(held, task).cleared && began.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(record_of(held, task).cleared);
}

#[test]
fn a_task_whose_session_has_closed_is_found_in_its_workspace_s_past_tasks_with_its_report() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);
    // The session that asked closes: its finished rows go with it (V100-10).
    closes(&held, steward).expect("closed");
    until_cleared(&held, task);
    assert_eq!(crate::finished::listed(&held), Vec::new());
    let record = record_of(&held, task);

    let past = past_of(&held, &record);

    assert_eq!(past.rows.len(), 1, "{past:?}");
    let row = &past.rows[0];
    assert_eq!(
        (
            row.id.as_str(),
            row.name.as_str(),
            row.outcome.as_str(),
            row.how
        ),
        (
            record.id.as_str(),
            "check prod",
            "done",
            crate::finished::How::Done
        )
    );
    // Who asked whom, as the record has it: nothing is asked of a chat that has gone.
    assert_eq!(row.asker, record.asker.chat.name);
    assert_eq!(row.asker_persona, record.asker.chat.persona);
    assert_eq!(row.persona, record.persona);
    assert!(!row.by_person);
    assert_eq!(row.says, "Forty are stuck.");
    assert_eq!(row.ended, record.ended);
    assert!(past.whole);
    assert_eq!(
        (past.older, past.unread, past.undrawn, past.waiting.len()),
        (0, 0, 0, 0)
    );
    assert_eq!(
        past.most,
        u32::try_from(dispatchrecord::MOST_PAST).expect("a small bound")
    );

    // Opened, it has its report and its brief as written.
    let read = crate::past::read(&held, &row.id).expect("read");
    assert_eq!(read.report, "Forty are stuck.");
    assert_eq!(read.brief, record.brief);

    // Another workspace's past tasks have none of it.
    assert_eq!(
        crate::past::listed(&held, Some("no-such-workspace"), None).rows,
        Vec::new()
    );
    // A file in the store that does not read is skipped and counted.
    std::fs::write(
        dispatchrecord::dir(held.root()).join(format!("{}.json", dispatchrecord::mint())),
        "{ not a record",
    )
    .expect("written");
    let again = past_of(&held, &record);
    assert_eq!((again.rows.len(), again.unread), (1, 1));
    // And a record that is gone says so when its row is opened.
    assert!(
        crate::past::read(&held, "01K6NOSUCHRECORD0000000000")
            .unwrap_err()
            .starts_with("purlis no longer has a record of that task")
    );
}

#[test]
fn a_task_is_not_past_until_its_chat_has_closed_and_a_later_read_finds_it_then() {
    let (_plane, _host, _planes, id, held, _steward, task) = a_steward_and_its_task();
    let record = record_of(&held, task);
    // Still working: it is in the Chats list, and nowhere here.
    let working = past_of(&held, &record);
    assert_eq!((working.rows.len(), working.waiting.len()), (0, 0));

    // Reported, and open for its moment to settle: named as one to ask about again.
    works(&held, task);
    reports(&held, &id, task);
    let reported = past_of(&held, &record);
    assert_eq!(reported.rows, Vec::new());
    assert_eq!(reported.waiting, vec![record.id.clone()]);

    its_turn_ends(&held, task);
    settles(&held, task);
    // The read that follows is handed the one before, and answers the task that ended.
    let later = crate::past::listed(
        &held,
        record.place.workspace.as_deref(),
        Some(&crate::past::PastSince {
            at: reported.read_at.clone(),
            also: reported.waiting.clone(),
        }),
    );
    assert!(!later.whole);
    assert_eq!(
        later
            .rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec![record.id.as_str()]
    );
    assert!(later.waiting.is_empty());
    // A marker that does not read is no marker: everything is read.
    let whole = crate::past::listed(
        &held,
        record.place.workspace.as_deref(),
        Some(&crate::past::PastSince {
            at: "soon".to_owned(),
            also: Vec::new(),
        }),
    );
    assert!(whole.whole);
    assert_eq!(whole.rows.len(), 1);
}

#[test]
fn a_cleared_task_is_reopened_from_past_tasks_by_the_finished_row_s_path_and_only_once() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);
    let record = record_of(&held, task);
    // Clear finished: the row goes from the sidebar, and this is where it went.
    assert_eq!(
        crate::finished::clear(&held, std::slice::from_ref(&record.id)),
        1
    );
    assert_eq!(finished_under(&held, steward), Vec::new());
    let row = past_of(&held, &record).rows.remove(0);
    assert!(row.reopens && !row.reopened);
    assert_eq!(
        crate::past::read(&held, &row.id)
            .expect("read")
            .cannot_reopen,
        None
    );

    // The finished row's own Reopen, on a record whose row is cleared.
    let reopened = crate::finished::reopen(&held, &row.id, A_SIZE).expect("reopened");
    assert_eq!(
        held.chats()
            .recorded_chat(reopened)
            .expect("open")
            .identity
            .resumed_from,
        record.worker.chat.id
    );
    // While its chat starts, no second Reopen is offered.
    let starting = past_of(&held, &record).rows.remove(0);
    assert!(!starting.reopens && !starting.reopened);

    // The new chat is heard from: the task is reopened for good, and still listed.
    a_turn_begins(&held, reopened);
    let after = past_of(&held, &record).rows.remove(0);
    assert!(after.reopened && !after.reopens);
    assert_eq!(
        crate::past::read(&held, &row.id)
            .expect("read")
            .cannot_reopen
            .as_deref(),
        Some(crate::past::REOPENED_ALREADY)
    );
    let chats_before = open_chats(&held).len();
    assert_eq!(
        crate::finished::reopen(&held, &row.id, A_SIZE).unwrap_err(),
        crate::past::REOPENED_ALREADY
    );
    assert_eq!(open_chats(&held).len(), chats_before);
}

#[test]
fn an_opened_past_task_says_why_it_cannot_be_reopened_in_the_finished_row_s_words() {
    let (_plane, _host, _planes, id, held, _steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);
    let record = record_of(&held, task);
    let ran = record.worker.harness.clone().expect("its harness");
    let file = dispatchrecord::dir(held.root()).join(format!("{}.json", record.id));
    let text = std::fs::read_to_string(&file).expect("the record");
    let other = if ran == "codex" { "opencode" } else { "codex" };
    std::fs::write(
        &file,
        text.replace(
            &format!("\"harness\": \"{ran}\""),
            &format!("\"harness\": \"{other}\""),
        ),
    )
    .expect("written");

    let cannot = crate::past::read(&held, &record.id)
        .expect("read")
        .cannot_reopen
        .expect("it cannot");

    assert_eq!(
        cannot,
        crate::finished::reopen(&held, &record.id, A_SIZE).unwrap_err()
    );
    assert!(
        cannot.starts_with(&format!(
            "'check prod' cannot be reopened: it ran on {other}"
        )),
        "{cannot}"
    );
}

#[test]
fn a_second_reopen_after_a_quit_inside_the_settle_brings_the_chat_it_started_forward() {
    let (_plane, host, _planes, id, held, _steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);
    let record = record_of(&held, task);
    let reopened = crate::finished::reopen(&held, &record.id, A_SIZE).expect("reopened");
    // Not heard from yet, so the task is not marked reopened.
    assert!(!record_of(&held, task).reopened);
    // The app quits and is started again: what it remembered of a Reopen under way is gone,
    // and the chat is put back as the open chat it is.
    held.tasks().reopen_failed(&record.id);
    assert!(
        past_of(&held, &record).rows[0].reopens,
        "the list cannot tell, so it still offers Reopen"
    );
    let chats_before = open_chats(&held).len();
    let started_before = host.openings().len();

    // The press finds the chat that carries the task on: that chat, and no second one.
    assert_eq!(
        crate::finished::reopen(&held, &record.id, A_SIZE),
        Ok(reopened)
    );
    assert_eq!(open_chats(&held).len(), chats_before);
    assert_eq!(host.openings().len(), started_before);
    // And the task is marked now, so no row offers it again.
    assert!(record_of(&held, task).reopened);
    let after = past_of(&held, &record).rows.remove(0);
    assert!(after.reopened && !after.reopens);

    // Once that chat is closed, the task was still reopened once.
    closes(&held, reopened).expect("closed");
    assert_eq!(
        crate::finished::reopen(&held, &record.id, A_SIZE).unwrap_err(),
        crate::past::REOPENED_ALREADY
    );
}
