//! What ends, and what does not, once a task has reported (#1485, fix round 1).
//!
//! **On the real clock.** Every test here turns the end's clock on (`Tasks::on_the_clock`), so
//! the settle and the bound are the app's own threads: a flow that would be ended mid-turn is
//! ended in these tests too, and none is green because the clock was off.

use super::*;

/// Longer than a reported task's moment to settle: had it been one to end, it would be gone.
fn past_the_settle() {
    std::thread::sleep(
        purlis_core::dispatched::A_TURN_SETTLES_WITHIN + std::time::Duration::from_millis(900),
    );
}

/// Waits until `task` is no longer an open chat, or fails after a bound.
fn ends_within_moments(held: &Held, task: u32) {
    let began = Instant::now();
    let bound = purlis_core::dispatched::A_TURN_SETTLES_WITHIN + std::time::Duration::from_secs(10);
    while open_chats(held).contains(&task) && began.elapsed() < bound {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!open_chats(held).contains(&task), "chat {task} never ended");
}

/// A steward chat and its one task, with the end's clock on.
fn on_the_clock() -> (Plane, Pretend, Planes, PlaneId, Arc<Held>, u32, u32) {
    let all = a_steward_and_its_task();
    all.4.tasks().on_the_clock();
    all
}

#[test]
fn a_reported_task_the_person_types_into_stays_open_and_working() {
    // The assertion this ticket's first commit took out, put back: typing `one more thing`
    // into a task after its report must leave the chat open and working.
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    works(&held, task);
    let said = reports(&held, &id, task);
    assert!(matches!(said, Answer::Finished { .. }), "{said:?}");

    // Mid-turn, after the report: the harness queues the line.
    assert_eq!(held.operator_input(task, b"one more thing\r"), Ok(()));
    // The reporting turn ends, and the queued line begins the next at once.
    its_turn_ends(&held, task);
    a_turn_begins(&held, task);
    past_the_settle();

    // Open, typeable, and still said to have reported: nothing ended it mid-turn.
    assert!(open_chats(&held).contains(&task));
    assert_eq!(held.operator_input(task, b"and this\r"), Ok(()));
    assert_eq!(
        stands(&held, steward, task).map(|(state, _)| state),
        Some(PersonaChatState::Reported)
    );
    // Nor when that turn ends, nor at the bound.
    its_turn_ends(&held, task);
    past_the_settle();
    crate::dispatched::end_look(&held, task, Looked::WaitedOut);
    assert!(open_chats(&held).contains(&task));
    // Its record says the person took it over, for the next launch.
    assert!(record_of(&held, task).kept_open);
    // It is a finished row once the person closes it.
    assert_eq!(finished_under(&held, steward), Vec::new());
    closes(&held, task).expect("closed");
    assert_eq!(finished_under(&held, steward).len(), 1);
}

#[test]
fn a_key_typed_during_the_settle_keeps_the_chat_too() {
    let (_plane, _host, _planes, id, held, _steward, task) = on_the_clock();
    works(&held, task);
    reports(&held, &id, task);
    its_turn_ends(&held, task);

    // Inside its two seconds.
    assert_eq!(held.operator_input(task, b"wait\r"), Ok(()));
    past_the_settle();

    assert!(open_chats(&held).contains(&task));
}

#[test]
fn a_smart_close_of_a_reported_task_is_not_cut_by_its_pending_end() {
    let (_plane, _host, _planes, id, held, _steward, task) = on_the_clock();
    works(&held, task);
    reports(&held, &id, task);
    its_turn_ends(&held, task);

    // The chat is asked for its session record, which is a turn of its own. Whether the
    // close is offered here or not, the end has stood down before anything is sent.
    let _ = crate::smartclose::begin(&held, task);
    a_turn_begins(&held, task);
    past_the_settle();

    assert!(open_chats(&held).contains(&task));
    assert!(record_of(&held, task).kept_open);
}

#[test]
fn a_task_reading_its_own_task_s_report_is_ended_only_when_that_turn_is_over() {
    let (_plane, _host, _planes, id, held, steward, first) = on_the_clock();
    let second = a_task_of(&held, &id, first, "read the logs");
    // The first reports while the second, a task of its own, still works.
    works(&held, first);
    works(&held, second);
    reports(&held, &id, first);
    its_turn_ends(&held, first);
    past_the_settle();
    assert!(
        open_chats(&held).contains(&first),
        "held: a task of its own works"
    );

    // The second reports. The first is waiting, so purlis's line begins a turn in it.
    reports(&held, &id, second);
    a_turn_begins(&held, first);
    // The second's turn ends and purlis ends it, which is when the first is looked at again:
    // mid-turn, reading that report.
    its_turn_ends(&held, second);
    ends_within_moments(&held, second);
    past_the_settle();
    assert!(
        open_chats(&held).contains(&first),
        "ended while it read its own task's report"
    );
    // The bound set at its report is spent: it was the reporting turn's.
    crate::dispatched::end_look(&held, first, Looked::WaitedOut);
    assert!(open_chats(&held).contains(&first));

    // It has read it, and its turn is over: now it is ended.
    its_turn_ends(&held, first);
    ends_within_moments(&held, first);
    assert_eq!(
        finished_under(&held, steward)
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["check prod"]
    );
}

#[test]
fn a_task_the_person_has_in_front_is_ended_when_they_move_away_and_not_before() {
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    // They opened its tab and are reading it.
    held.chats().bring_to_front(Some(task));
    works(&held, task);
    reports(&held, &id, task);
    its_turn_ends(&held, task);
    past_the_settle();
    crate::dispatched::end_look(&held, task, Looked::WaitedOut);

    assert!(
        open_chats(&held).contains(&task),
        "ended under the person who was reading it"
    );

    // They bring another chat forward: it is ended then, as it would have been.
    held.chats().bring_to_front(Some(steward));
    crate::dispatched::front_moved(&held);
    ends_within_moments(&held, task);
    assert_eq!(finished_under(&held, steward).len(), 1);
}

#[test]
fn a_task_shown_inside_its_session_s_tab_is_ended_when_the_tab_goes_back_and_not_before() {
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    // The session's tab is in front, switched to its task (#1486): "in front" is the
    // session's own chat, and the task is the one on screen.
    held.chats().bring_to_front(Some(steward));
    held.chats()
        .tab_shows(steward, Some(task), None)
        .expect("shown");
    assert_eq!(held.chats().looked_at(), vec![task]);
    works(&held, task);
    reports(&held, &id, task);
    its_turn_ends(&held, task);
    past_the_settle();
    crate::dispatched::end_look(&held, task, Looked::WaitedOut);

    assert!(
        open_chats(&held).contains(&task),
        "ended under the person who was reading it in its session's tab"
    );

    // They go back to the session's own chat: it is ended then.
    held.chats()
        .tab_shows(steward, None, None)
        .expect("its own chat");
    assert_eq!(held.chats().looked_at(), vec![steward]);
    crate::dispatched::front_moved(&held);
    ends_within_moments(&held, task);
    assert_eq!(finished_under(&held, steward).len(), 1);
}

#[test]
fn a_task_shown_in_a_tab_that_is_no_longer_in_front_is_ended() {
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    held.chats().bring_to_front(Some(steward));
    held.chats()
        .tab_shows(steward, Some(task), None)
        .expect("shown");
    works(&held, task);
    reports(&held, &id, task);
    its_turn_ends(&held, task);
    past_the_settle();
    assert!(open_chats(&held).contains(&task));

    // Its tab still shows it, and is no longer the tab in front: nobody is reading it.
    held.chats().bring_to_front(None);
    assert_eq!(held.chats().looked_at(), Vec::<u32>::new());
    crate::dispatched::front_moved(&held);
    ends_within_moments(&held, task);
}

#[test]
fn the_chat_looked_at_is_the_one_a_tab_shows_and_not_the_session_hidden_behind_it() {
    // What the notification's "already looking at it" asks (`already_looking_at`): a task on
    // screen is not notified about, and the session's own chat behind it is.
    let (_plane, _host, _planes, _id, held, steward, task) = on_the_clock();
    held.chats().bring_to_front(Some(steward));
    assert_eq!(held.chats().looked_at(), vec![steward]);

    held.chats()
        .tab_shows(steward, Some(task), None)
        .expect("shown");
    assert_eq!(held.chats().looked_at(), vec![task]);
    assert!(!held.chats().looks_at(steward));
    // "In front" is still the session's own chat: the tab is its.
    assert_eq!(held.chats().front(), Some(steward));

    // A task brought in front in a tab of its own is looked at as any chat in front is.
    held.chats().bring_to_front(Some(task));
    assert_eq!(held.chats().looked_at(), vec![task]);
}

#[test]
fn a_task_opened_beside_its_session_is_ended_when_its_pane_goes_and_not_before() {
    // #1489, closing J1's limit: the task has a pane of its own in the session's tab, which is
    // the tab in front. "In front" is the session; the task is on screen beside it.
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    held.chats().bring_to_front(Some(steward));
    held.chats()
        .tab_shows(task, None, Some(steward))
        .expect("beside");
    assert_eq!(held.chats().looked_at(), vec![steward, task]);
    works(&held, task);
    reports(&held, &id, task);
    its_turn_ends(&held, task);
    past_the_settle();
    crate::dispatched::end_look(&held, task, Looked::WaitedOut);

    assert!(
        open_chats(&held).contains(&task),
        "ended under the person who was reading it beside its session"
    );

    // Its pane is sent back to the list: nobody is reading it, and it is ended then.
    held.chats().tab_shows(task, None, None).expect("no pane");
    assert_eq!(held.chats().looked_at(), vec![steward]);
    crate::dispatched::front_moved(&held);
    ends_within_moments(&held, task);
    assert_eq!(finished_under(&held, steward).len(), 1);
}

#[test]
fn a_task_beside_its_session_is_not_held_once_another_tab_is_in_front() {
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    held.chats().bring_to_front(Some(steward));
    held.chats()
        .tab_shows(task, None, Some(steward))
        .expect("beside");
    works(&held, task);
    reports(&held, &id, task);
    its_turn_ends(&held, task);
    past_the_settle();
    assert!(open_chats(&held).contains(&task));

    // The split is still as it was, behind another tab: no pane of it is on screen.
    held.chats().bring_to_front(None);
    assert!(!held.chats().looks_at(task));
    crate::dispatched::front_moved(&held);
    ends_within_moments(&held, task);
}

#[test]
fn a_task_in_a_tab_of_its_own_is_held_while_that_tab_is_in_front_and_ended_when_sent_back() {
    // #1489: Move to its own tab, then the minimise. The minimise ends nothing by itself: it
    // only stops the person looking, and a task that had reported is ended as any other is.
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    held.chats().open_tab(task, true).expect("its own tab");
    held.chats().bring_to_front(Some(task));
    assert_eq!(held.chats().looked_at(), vec![task]);
    works(&held, task);
    reports(&held, &id, task);
    its_turn_ends(&held, task);
    past_the_settle();
    crate::dispatched::end_look(&held, task, Looked::WaitedOut);
    assert!(open_chats(&held).contains(&task), "ended under the person");

    held.chats().open_tab(task, false).expect("sent back");
    held.chats().bring_to_front(Some(steward));
    crate::dispatched::front_moved(&held);
    ends_within_moments(&held, task);
}

#[test]
fn a_working_task_sent_back_out_of_its_tab_goes_on_working() {
    // The minimise never ends a task: one that has not reported is as open after it as before.
    let (_plane, _host, _planes, _id, held, steward, task) = on_the_clock();
    held.chats().open_tab(task, true).expect("its own tab");
    held.chats().bring_to_front(Some(task));
    works(&held, task);

    held.chats().open_tab(task, false).expect("sent back");
    held.chats().tab_shows(task, None, None).expect("no pane");
    held.chats().bring_to_front(Some(steward));
    crate::dispatched::front_moved(&held);
    past_the_settle();

    assert!(open_chats(&held).contains(&task));
    assert_eq!(held.operator_input(task, b"still here\r"), Ok(()));
}

#[test]
fn every_pane_of_the_tab_in_front_is_looked_at_each_as_the_chat_it_shows() {
    // What the notification's "already looking at it" asks, in a split: the session's pane
    // switched to one task, and another task in a pane of its own beside it.
    let (_plane, _host, _planes, _id, held, steward, task) = on_the_clock();
    held.chats().bring_to_front(Some(steward));
    held.chats()
        .tab_shows(task, None, Some(steward))
        .expect("beside");
    assert!(held.chats().looks_at(steward) && held.chats().looks_at(task));

    // The session's own pane shows a chat in place of it: that one is on screen, and the
    // session's own chat, hidden behind it, is not.
    held.chats()
        .tab_shows(steward, Some(900), None)
        .expect("shown");
    assert_eq!(held.chats().looked_at(), vec![task, 900]);
    assert!(!held.chats().looks_at(steward));

    // A chat beside another tab's chat is not on screen with this one.
    held.chats()
        .tab_shows(task, None, Some(901))
        .expect("moved");
    assert!(!held.chats().looks_at(task));
}

#[test]
fn a_blocked_task_stays_open_as_the_chat_it_is() {
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    works(&held, task);

    let said = tasks_report(
        &held,
        &id,
        &Tickets::default(),
        task,
        Outcome::Blocked,
        None,
    );

    // Answered as any report, and told nothing of an end.
    assert!(matches!(said, Answer::Reported { .. }), "{said:?}");
    its_turn_ends(&held, task);
    past_the_settle();
    crate::dispatched::end_look(&held, task, Looked::WaitedOut);
    assert!(open_chats(&held).contains(&task));
    assert_eq!(finished_under(&held, steward), Vec::new());
    // Closed by the person, it is a row of its own that says blocked.
    closes(&held, task).expect("closed");
    let rows = finished_under(&held, steward);
    assert_eq!(
        rows.iter()
            .map(|row| (row.how, row.folds))
            .collect::<Vec<_>>(),
        [(crate::finished::How::Blocked, false)]
    );
}

#[test]
fn a_task_the_person_started_from_a_tab_is_never_ended_by_its_report() {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    held.tasks().on_the_clock();
    let task = ask_from_the_tab(&held, &id, steward, "devops", "check prod")
        .expect("started")
        .session;
    works(&held, task);

    let said = reports(&held, &id, task);

    assert!(matches!(said, Answer::Reported { .. }), "{said:?}");
    its_turn_ends(&held, task);
    past_the_settle();
    crate::dispatched::end_look(&held, task, Looked::WaitedOut);
    // Their conversation: open until they close it, and a finished row from then.
    assert!(open_chats(&held).contains(&task));
    assert_eq!(finished_under(&held, steward), Vec::new());
    closes(&held, task).expect("closed");
    assert_eq!(finished_under(&held, steward).len(), 1);
}

#[test]
fn a_task_the_person_stopped_does_not_fold_whatever_its_last_report_says() {
    let (_plane, _host, _planes, id, held, steward, task) = on_the_clock();
    rests(&held, task);
    stops(&held, task, false).expect("stopping");

    // Its one last turn: it says it is done.
    works(&held, task);
    crate::stopping::reported(&held, task);
    let said = reports(&held, &id, task);
    assert!(
        matches!(said, Answer::Reported { .. } | Answer::Finished { .. }),
        "{said:?}"
    );
    its_turn_ends(&held, task);
    crate::stopping::reported(&held, task);
    ends_within_moments(&held, task);

    let record = record_of(&held, task);
    assert_eq!(
        record.ended_by,
        Some(purlis_core::dispatchrecord::EndedBy::Person)
    );
    let rows = finished_under(&held, steward);
    assert_eq!(
        rows.iter()
            .map(|row| (row.how, row.folds))
            .collect::<Vec<_>>(),
        [(crate::finished::How::StoppedByPerson, false)]
    );
}
