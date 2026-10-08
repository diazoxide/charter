//! A session waiting on its tasks is not a needs-you item, and is flagged only for what
//! matters (#1491).
//!
//! Against the app's own records, on a pretend session host. Every hook is applied as the
//! socket's listener applies it (`Hooks::hear`): what the chat waits on is read from the
//! project's records first, and the move the window would be told is kept, so a test can ask
//! whether anything would have interrupted the person (`Moved::interrupts`).

use purlis_core::handback::Outcome;
use purlis_core::state::{FailedTask, HowFailed, State};

use super::*;
use crate::hooks::{Moved, Need};

/// Every move the window was told, in the order it was told.
type Told = Arc<Mutex<Vec<Moved>>>;

/// A steward chat on a pretend host, in a project that keeps every move it tells the window.
fn a_steward_chat_telling() -> (Plane, Pretend, Planes, PlaneId, Arc<Held>, u32, Told) {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let told: Told = Arc::default();
    let planes = Planes::telling(
        Arc::new({
            let told = Arc::clone(&told);
            move |moved| told.lock().expect("the moves told").push(moved)
        }),
        crate::Shipped::default(),
        None,
    )
    .running_sessions_on(Arc::new({
        let host = host.clone();
        move |_| Box::new(host.clone())
    }));
    let id = planes.open(&plane.root);
    let held = planes.held(&id).expect("held");
    let alpha = held.root().join("workspaces").join("alpha");
    let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
    (plane, host, planes, id, held, steward, told)
}

/// The hook channel hears `event` from chat `chat`'s own harness, and everything that listens
/// to it is told, as the socket's listener has it: the board takes it with what the chat
/// waits on, the window is told what moved, and whatever waited on the chat looks again.
fn hears(held: &Held, told: &Told, chat: u32, event: Event) {
    use purlis_core::hookwire::Conversation;
    let conversation = held
        .board()
        .conversation(chat)
        .map_or(Conversation::Unknown, Conversation::Named);
    let report = purlis_core::hookwire::Report {
        chat,
        event,
        conversation,
        pid: Some(4000 + chat),
        agent: None,
        detail: purlis_core::state::Detail::default(),
    };
    if let Some(moved) = held.hooks().hear(&report) {
        told.lock().expect("the moves told").push(moved);
    }
    crate::dispatched::heard(held, &report);
}

/// A turn of chat `chat` begins, and ends.
fn has_a_turn(held: &Held, told: &Told, chat: u32) {
    hears(held, told, chat, Event::UserPromptSubmit);
    hears(held, told, chat, Event::Stop);
}

fn queue(held: &Held) -> Vec<u32> {
    held.hooks().board().needs_you()
}

/// How many of the moves told from `since` on would have interrupted the person about `chat`.
fn interruptions(told: &Told, since: usize, chat: u32) -> usize {
    told.lock().expect("the moves told")[since..]
        .iter()
        .filter(|moved| moved.session == chat && moved.interrupts())
        .count()
}

fn told_so_far(told: &Told) -> usize {
    told.lock().expect("the moves told").len()
}

/// The task sends its report, saying how it ended.
fn reports_as(held: &Held, id: &PlaneId, task: u32, outcome: Outcome) {
    let said = tasks_report(held, id, &Tickets::default(), task, outcome, None);
    // A task that purlis will end is told so, in an answer of its own (#1485).
    assert!(
        matches!(said, Answer::Reported { .. } | Answer::Finished { .. }),
        "{said:?}"
    );
}

#[test]
fn a_session_idle_with_a_working_task_is_not_in_the_needs_you_queue() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);

    hears(&held, &told, steward, Event::Stop);

    assert_eq!(held.board().glance(steward).state, State::Waiting);
    assert_eq!(crate::dispatched::waits(&held, steward).tasks, 1);
    assert!(queue(&held).is_empty(), "it waits on its task, not on you");
    assert_eq!(interruptions(&told, 0, steward), 0);
    // The harness's own nudge of a chat left idle raises nothing either.
    hears(&held, &told, steward, Event::Notification);
    assert!(queue(&held).is_empty());
}

#[test]
fn a_task_that_finishes_as_done_changes_no_queue_and_sends_no_notification() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);
    hears(&held, &told, steward, Event::Stop);
    let before = told_so_far(&told);

    reports_as(&held, &id, task, Outcome::Done);

    // The asking chat was told a move of its own: its row says who reported back. It is a
    // move that only counts, and no notification is sent for it.
    let moves: Vec<Moved> = told.lock().expect("the moves told")[before..]
        .iter()
        .filter(|moved| moved.session == steward)
        .cloned()
        .collect();
    assert!(
        moves.iter().any(|moved| !moved.reports.is_empty()),
        "the report back is told: {moves:?}"
    );
    assert_eq!(interruptions(&told, before, steward), 0, "{moves:?}");
    assert!(held.hooks().board().failed_tasks(steward).is_empty());
    assert!(
        !queue(&held).contains(&steward),
        "a done task is no item on the chat that asked"
    );
}

#[test]
fn a_done_task_interrupts_nobody_even_where_the_session_already_needs_the_person() {
    // The session is an item for a reason of its own (a prompt mid-turn). A task of its that
    // finishes moves its row again, and that move sends nothing: one item, one notification.
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);
    hears(&held, &told, steward, Event::Notification);
    assert_eq!(queue(&held), vec![steward], "a real prompt wins");
    assert_eq!(interruptions(&told, 0, steward), 1);
    let before = told_so_far(&told);

    reports_as(&held, &id, task, Outcome::Done);

    assert_eq!(interruptions(&told, before, steward), 0);
    assert_eq!(queue(&held), vec![steward]);
}

#[test]
fn the_session_is_an_item_once_every_task_has_reported_and_it_has_then_stopped() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);
    hears(&held, &told, steward, Event::Stop);
    reports_as(&held, &id, task, Outcome::Done);
    assert_eq!(crate::dispatched::waits(&held, steward).tasks, 0);
    let before = told_so_far(&told);

    // It reads the report in a turn of its own, and stops with nothing below it.
    has_a_turn(&held, &told, steward);

    assert_eq!(queue(&held), vec![steward]);
    assert_eq!(
        interruptions(&told, before, steward),
        1,
        "the session itself needs the person now, and that is the one notification"
    );
}

#[test]
fn a_task_two_dispatches_down_holds_the_session_too() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);
    let deeper = a_task_of(&held, &id, task, "check one shard");
    hears(&held, &told, deeper, Event::UserPromptSubmit);
    assert_eq!(crate::dispatched::waits(&held, steward).tasks, 2);

    // The task between them reports; the one it dispatched still owes its report.
    reports_as(&held, &id, task, Outcome::Done);
    assert_eq!(
        crate::handoff::owing_below(&held, steward),
        vec![deeper],
        "what a reported task started is still below the session"
    );

    has_a_turn(&held, &told, steward);

    assert!(
        !queue(&held).contains(&steward),
        "a task at any depth holds the session's end of turn"
    );
}

#[test]
fn a_task_paused_on_a_question_to_its_asker_is_not_in_the_queue_and_its_own_prompt_is() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);
    let asked = asks(&held, &id, task, question("Which queue?"));
    assert!(matches!(asked, Answer::Task(_)), "{asked:?}");
    assert!(crate::dispatched::waits(&held, task).its_asker);

    // Its turn ends because it waits on the answer: the asking chat is who can give it.
    hears(&held, &told, task, Event::Stop);
    assert!(
        !queue(&held).contains(&task),
        "it reads asking its asker, never needs you"
    );
    assert_eq!(interruptions(&told, 0, task), 0);

    // A prompt of its own, shown to the person mid-turn, always is one.
    hears(&held, &told, task, Event::UserPromptSubmit);
    hears(&held, &told, task, Event::Notification);
    assert!(queue(&held).contains(&task));
    assert_eq!(interruptions(&told, 0, task), 1);
}

#[test]
fn a_failed_task_puts_the_hand_on_the_chat_that_asked_and_a_done_one_does_not() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let done = a_task_of(&held, &id, steward, "check the queue");
    let failed = a_task_of(&held, &id, steward, "check prod");
    let blocked = a_task_of(&held, &id, steward, "check staging");
    for task in [done, failed, blocked] {
        hears(&held, &told, task, Event::UserPromptSubmit);
    }
    hears(&held, &told, steward, Event::Stop);
    assert!(queue(&held).is_empty());

    reports_as(&held, &id, done, Outcome::Done);
    assert!(
        !queue(&held).contains(&steward),
        "done changes the count only"
    );
    let before = told_so_far(&told);

    reports_as(&held, &id, failed, Outcome::Failed);

    // An item on the asking chat, though another task of its still works.
    assert!(queue(&held).contains(&steward));
    let (record, name) =
        crate::finished::task_record(&held, failed).expect("the task has a record");
    assert_eq!(
        held.hooks().board().failed_tasks(steward),
        vec![
            FailedTask::new(&record, &name, HowFailed::Failed, "Forty are stuck.")
                .of_open_chat(failed)
        ]
    );
    assert_eq!(
        interruptions(&told, before, steward),
        1,
        "the failure is the session's item, and the one notification"
    );
    // The window is sent which task and why, as a need of the chat's.
    let said = told.lock().expect("the moves told")[before..]
        .iter()
        .filter(|moved| moved.session == steward)
        .find_map(|moved| moved.needs.clone())
        .expect("the move says why");
    assert_eq!(
        said,
        vec![Need::TaskFailed {
            id: record,
            chat: Some(failed),
            task: name,
            how: crate::hooks::HowFailed::Failed,
            why: "Forty are stuck.".to_owned(),
        }]
    );

    // Blocked did not do the work either, and reads failed on its row: flagged the same.
    reports_as(&held, &id, blocked, Outcome::Blocked);
    assert_eq!(held.hooks().board().failed_tasks(steward).len(), 2);
}

#[test]
fn a_task_that_ends_without_a_report_is_flagged_and_one_the_person_stopped_is_not() {
    let (_plane, host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let died = a_task_of(&held, &id, steward, "check prod");
    let stopped = a_task_of(&held, &id, steward, "check staging");
    hears(&held, &told, steward, Event::Stop);

    // The person's stop: purlis's word to the asking chat, and no failure of the work.
    {
        let deciding = held.chats().deciding();
        operator_stopped(&held, stopped, false, true, &deciding);
    }
    assert!(held.hooks().board().failed_tasks(steward).is_empty());
    assert!(!queue(&held).contains(&steward));
    assert_eq!(interruptions(&told, 0, steward), 0);

    host.program_ends(died, KILLED());

    let failed = held.hooks().board().failed_tasks(steward);
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(
        (failed[0].how, failed[0].why.as_str()),
        (HowFailed::Unreported, "")
    );
    assert!(queue(&held).contains(&steward));
}

#[test]
fn a_failed_task_s_item_goes_when_the_person_looks_and_when_its_row_is_cleared() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check prod");
    hears(&held, &told, task, Event::UserPromptSubmit);
    reports_as(&held, &id, task, Outcome::Failed);
    assert!(queue(&held).contains(&steward));

    // Looked at: the item goes, and the window is told.
    let before = told_so_far(&told);
    let (record, _) = crate::finished::task_record(&held, task).expect("its record");
    held.task_failure_cleared(steward, &record);
    assert!(!queue(&held).contains(&steward));
    assert!(told_so_far(&told) > before, "the window is told it went");

    // Raised again by a second failure, and cleared with that task's finished row.
    let again = a_task_of(&held, &id, steward, "check staging");
    hears(&held, &told, again, Event::UserPromptSubmit);
    reports_as(&held, &id, again, Outcome::Failed);
    assert!(queue(&held).contains(&steward));
    // purlis ends a reported task once its turn is over: its row is a finished one.
    hears(&held, &told, again, Event::Stop);
    crate::dispatched::end_look(&held, again, purlis_core::dispatched::Looked::Settled);
    let row = crate::finished::listed(&held)
        .into_iter()
        .find(|row| row.asker == steward && row.outcome == "failed" && row.name.contains("staging"))
        .expect("its finished row");

    assert_eq!(
        crate::finished::clear(&held, std::slice::from_ref(&row.id)),
        1
    );

    assert!(held.hooks().board().failed_tasks(steward).is_empty());
    assert!(!queue(&held).contains(&steward));
}

#[test]
fn a_session_with_a_task_open_is_listed_with_the_limit_in_force_for_it() {
    let (_plane, _host, _planes, id, held, steward, _told) = a_steward_chat_telling();
    let limit_of = |session: u32| {
        let sidebar = crate::sidebar_of(&held).expect("the sidebar");
        sidebar
            .workspaces
            .into_iter()
            .flat_map(|workspace| workspace.chats)
            .chain(sidebar.unfiled)
            .find(|chat| chat.session == session)
            .expect("the chat is listed")
            .tasks_limit
    };
    assert_eq!(limit_of(steward), None, "no task open: nothing is read");

    let task = a_task_of(&held, &id, steward, "check the queue");

    assert_eq!(
        limit_of(steward),
        Some(
            purlis_core::dispatchlimits::Limit::RunningPerChat
                .when_unset()
                .expect("six")
        ),
        "the limit a dispatch from it is decided by"
    );
    assert_eq!(limit_of(task), None, "the task has none of its own open");
}

// ----- fix round 1: a chat that needs the person is never left idle with no hand -----

/// A steward chat whose turn has ended while the one task it asked for works: held.
fn a_steward_held_for_its_task() -> (Plane, Pretend, Planes, PlaneId, Arc<Held>, u32, u32, Told) {
    let (plane, host, planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);
    hears(&held, &told, steward, Event::Stop);
    assert!(held.hooks().board().held(steward));
    assert!(queue(&held).is_empty());
    (plane, host, planes, id, held, steward, task, told)
}

/// The person has a key in chat `chat`'s pane, so purlis types it no line: what a harness
/// that takes no line, or one with a picker open, comes to.
fn the_person_has_a_key_in(held: &Held, chat: u32) {
    held.operator_input(chat, b"/model\r").expect("sent");
}

/// The person stops chat `chat` and it writes no last report: the one word, then the close,
/// as the stop engine ends a chat whose last turn ran out.
fn the_person_stops(held: &Held, chat: u32) {
    {
        let deciding = held.chats().deciding();
        operator_stopped(held, chat, false, true, &deciding);
    }
    held.close_chat(chat).expect("closed");
}

#[test]
fn the_last_task_reporting_where_no_line_is_typed_makes_the_session_an_item() {
    let (_plane, _host, _planes, id, held, steward, task, told) = a_steward_held_for_its_task();
    the_person_has_a_key_in(&held, steward);

    reports_as(&held, &id, task, Outcome::Done);

    assert_eq!(queue(&held), vec![steward]);
    assert_eq!(interruptions(&told, 0, steward), 1);
}

#[test]
fn a_line_typed_that_the_harness_never_takes_makes_the_session_an_item() {
    let (_plane, _host, _planes, id, held, steward, task, _told) = a_steward_held_for_its_task();
    reports_as(&held, &id, task, Outcome::Done);
    // The line was typed: the session is about to work, and is not the person's yet.
    assert!(crate::dispatched::waits(&held, steward).line_coming);
    assert!(queue(&held).is_empty());

    // Its time passes and no turn began on it.
    crate::dispatched::line_not_taken(&held, steward);

    assert_eq!(queue(&held), vec![steward]);
    // Said once: a second look finds nothing held.
    crate::dispatched::line_not_taken(&held, steward);
    assert_eq!(queue(&held), vec![steward]);
}

#[test]
fn a_line_that_was_taken_is_not_given_up_on() {
    let (_plane, _host, _planes, id, held, steward, task, told) = a_steward_held_for_its_task();
    reports_as(&held, &id, task, Outcome::Done);
    hears(&held, &told, steward, Event::UserPromptSubmit);

    crate::dispatched::line_not_taken(&held, steward);

    assert!(queue(&held).is_empty(), "it is working");
}

#[test]
fn the_last_task_stopped_by_the_person_with_no_last_report_makes_the_session_an_item() {
    let (_plane, _host, _planes, _id, held, steward, task, _told) = a_steward_held_for_its_task();
    the_person_has_a_key_in(&held, steward);

    the_person_stops(&held, task);

    assert_eq!(queue(&held), vec![steward]);
    assert!(
        held.hooks().board().failed_tasks(steward).is_empty(),
        "a stop is no failure of the work"
    );
}

#[test]
fn the_last_task_s_tab_closed_makes_the_session_an_item() {
    // Close now, or the task's tab closed: the close alone, with no stop before it.
    let (_plane, _host, _planes, _id, held, steward, task, _told) = a_steward_held_for_its_task();
    the_person_has_a_key_in(&held, steward);

    held.close_chat(task).expect("closed");

    assert_eq!(queue(&held), vec![steward]);
}

#[test]
fn a_session_typed_the_word_that_its_last_task_was_stopped_is_an_item_at_its_next_stop() {
    // It takes a line: purlis types that a task of its was stopped, it reads the word in a
    // turn of its own, and that turn's end is the item. If it never takes the line, the
    // clock's look is.
    for takes_it in [true, false] {
        let (_plane, _host, _planes, _id, held, steward, task, told) =
            a_steward_held_for_its_task();

        the_person_stops(&held, task);
        assert!(queue(&held).is_empty(), "about to work");
        assert!(crate::dispatched::waits(&held, steward).line_coming);

        if takes_it {
            has_a_turn(&held, &told, steward);
        } else {
            crate::dispatched::line_not_taken(&held, steward);
        }

        assert_eq!(queue(&held), vec![steward], "takes it: {takes_it}");
    }
}

#[test]
fn one_of_two_tasks_stopped_leaves_the_session_held_for_the_other() {
    let (_plane, _host, _planes, id, held, steward, task, told) = a_steward_held_for_its_task();
    let other = a_task_of(&held, &id, steward, "check prod");
    hears(&held, &told, other, Event::UserPromptSubmit);

    the_person_stops(&held, task);

    assert!(queue(&held).is_empty(), "one still works");
    assert!(held.hooks().board().held(steward));
}

#[test]
fn stopping_a_task_in_the_middle_and_the_one_below_it_makes_the_chat_above_an_item() {
    let (_plane, _host, _planes, id, held, steward, task, told) = a_steward_held_for_its_task();
    let deeper = a_task_of(&held, &id, task, "check one shard");
    hears(&held, &told, deeper, Event::UserPromptSubmit);
    the_person_has_a_key_in(&held, steward);

    // "Stop with everything below", on the task in the middle: the deepest ends first.
    the_person_stops(&held, deeper);
    assert!(queue(&held).is_empty(), "the middle task still owes");
    the_person_stops(&held, task);

    assert_eq!(queue(&held), vec![steward]);
}

#[test]
fn a_grandchild_reporting_after_the_middle_task_reported_makes_the_session_an_item() {
    let (_plane, _host, _planes, id, held, steward, task, told) = a_steward_held_for_its_task();
    let deeper = a_task_of(&held, &id, task, "check one shard");
    hears(&held, &told, deeper, Event::UserPromptSubmit);
    // The middle task reports; the session reads it and stops, held on the grandchild.
    reports_as(&held, &id, task, Outcome::Done);
    has_a_turn(&held, &told, steward);
    assert!(queue(&held).is_empty());
    assert!(held.hooks().board().held(steward));

    // The grandchild reports to the middle task. Nothing is typed into the session for it.
    reports_as(&held, &id, deeper, Outcome::Done);

    assert!(
        queue(&held).contains(&steward),
        "every chat above the one that reported is looked at"
    );
}

#[test]
fn a_task_in_the_middle_closed_while_one_below_it_works_makes_the_session_an_item() {
    let (_plane, _host, _planes, id, held, steward, task, told) = a_steward_held_for_its_task();
    let deeper = a_task_of(&held, &id, task, "check one shard");
    hears(&held, &told, deeper, Event::UserPromptSubmit);
    the_person_has_a_key_in(&held, steward);

    held.close_chat(task).expect("closed");

    // Nothing below the session reports to it any more: its count dropped, and it was looked
    // at as it did.
    assert_eq!(crate::dispatched::waits(&held, steward).tasks, 0);
    assert!(queue(&held).contains(&steward));
}

#[test]
fn the_session_s_own_stop_racing_the_last_report_is_looked_at_once_it_is_applied() {
    // What the session waits on was read before the board was taken (one task), the report
    // landed in between, and the stop is then applied held. Its harness takes no line.
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);
    the_person_has_a_key_in(&held, steward);
    reports_as(&held, &id, task, Outcome::Done);
    assert!(
        queue(&held).is_empty(),
        "mid-turn: nothing held, nothing raised"
    );
    let conversation = held
        .board()
        .conversation(steward)
        .map_or(purlis_core::hookwire::Conversation::Unknown, |said| {
            purlis_core::hookwire::Conversation::Named(said)
        });
    let stop = purlis_core::hookwire::Report {
        chat: steward,
        event: Event::Stop,
        conversation,
        pid: Some(4000 + steward),
        agent: None,
        detail: purlis_core::state::Detail::default(),
    };
    held.hooks().board().reported_while(
        &stop,
        purlis_core::state::Waits {
            tasks: 1,
            its_asker: false,
            line_coming: false,
        },
    );
    assert!(held.hooks().board().held(steward));

    // The end is applied: whatever waited on the chat moving looks again.
    crate::dispatched::moved(&held, steward);

    assert_eq!(queue(&held), vec![steward]);
}

#[test]
fn a_task_asking_the_session_does_not_hold_the_session_s_end() {
    // The session is the one who must act: where it stops without answering, the person is
    // the one who can see that, and the task stays held on its answer.
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let task = a_task_of(&held, &id, steward, "check the queue");
    hears(&held, &told, task, Event::UserPromptSubmit);
    let asked = asks(&held, &id, task, question("Which queue?"));
    assert!(matches!(asked, Answer::Task(_)), "{asked:?}");
    hears(&held, &told, task, Event::Stop);
    assert_eq!(crate::dispatched::waits(&held, steward).tasks, 0);

    // The session's turn ends with the question unanswered. purlis types it the line that a
    // question waits; it does not take it, and no turn of its begins.
    hears(&held, &told, steward, Event::Stop);
    assert!(queue(&held).is_empty(), "the line is on its way");
    crate::dispatched::line_not_taken(&held, steward);

    assert_eq!(queue(&held), vec![steward], "one of the two is shown");
    assert!(
        held.hooks().board().held(task),
        "the task waits on the answer"
    );
}

#[test]
fn a_question_that_opens_while_the_session_is_held_for_that_task_releases_the_session() {
    let (_plane, _host, _planes, id, held, steward, task, _told) = a_steward_held_for_its_task();
    the_person_has_a_key_in(&held, steward);

    let asked = asks(&held, &id, task, question("Which queue?"));

    assert!(matches!(asked, Answer::Task(_)), "{asked:?}");
    assert_eq!(
        queue(&held),
        vec![steward],
        "it cannot be typed the question"
    );
}

#[test]
fn a_task_paused_on_an_asker_that_closes_or_dies_is_an_item_itself() {
    for dies in [false, true] {
        let (_plane, host, _planes, id, held, steward, told) = a_steward_chat_telling();
        hears(&held, &told, steward, Event::UserPromptSubmit);
        let task = a_task_of(&held, &id, steward, "check the queue");
        hears(&held, &told, task, Event::UserPromptSubmit);
        let asked = asks(&held, &id, task, question("Which queue?"));
        assert!(matches!(asked, Answer::Task(_)), "{asked:?}");
        hears(&held, &told, task, Event::Stop);
        assert!(held.hooks().board().held(task));
        assert!(queue(&held).is_empty());

        if dies {
            host.program_ends(steward, KILLED());
        } else {
            // Closed, keeping what it started running.
            held.close_chat(steward).expect("closed");
        }

        assert!(
            !crate::dispatched::waits(&held, task).its_asker,
            "nobody is left to answer"
        );
        assert!(queue(&held).contains(&task), "dies: {dies}");
    }
}

#[test]
fn a_task_the_person_asked_for_does_not_hold_the_session_s_end() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let theirs = ask_from_the_tab(&held, &id, steward, "devops", "check prod")
        .expect("the person's task starts")
        .session;
    hears(&held, &told, theirs, Event::UserPromptSubmit);
    assert!(
        held.chats()
            .handed_from(theirs)
            .is_some_and(|from| from.by_person),
        "started by the person"
    );

    hears(&held, &told, steward, Event::Stop);

    assert_eq!(crate::dispatched::waits(&held, steward).tasks, 0);
    assert_eq!(queue(&held), vec![steward], "the session asked for nothing");
    assert_eq!(interruptions(&told, 0, steward), 1);
}

#[test]
fn a_failure_notifies_once_however_the_asking_chat_then_moves() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let failed = a_task_of(&held, &id, steward, "check prod");
    let other = a_task_of(&held, &id, steward, "check staging");
    hears(&held, &told, failed, Event::UserPromptSubmit);
    hears(&held, &told, other, Event::UserPromptSubmit);
    hears(&held, &told, steward, Event::Stop);
    let before = told_so_far(&told);

    reports_as(&held, &id, failed, Outcome::Failed);
    // The session is typed the line and reads the report in a turn of its own, a helper of
    // its comes and goes, and the turn ends with the other task still at work.
    hears(&held, &told, steward, Event::UserPromptSubmit);
    held.hooks().board().child_heard(steward, "a1");
    hears(&held, &told, steward, Event::Stop);
    hears(&held, &told, steward, Event::Notification);

    assert!(queue(&held).contains(&steward), "the item stands");
    let moves = told.lock().expect("the moves told")[before..]
        .iter()
        .filter(|moved| moved.session == steward)
        .count();
    assert!(moves >= 3, "the chat moved while the item stood: {moves}");
    assert_eq!(
        interruptions(&told, before, steward),
        1,
        "one failure, one notification"
    );
}

#[test]
fn reports_that_overlap_send_one_notification_when_the_session_is_done_with_them_all() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    let first = a_task_of(&held, &id, steward, "check prod");
    let second = a_task_of(&held, &id, steward, "check staging");
    hears(&held, &told, first, Event::UserPromptSubmit);
    hears(&held, &told, second, Event::UserPromptSubmit);
    hears(&held, &told, steward, Event::Stop);

    // The first reports: the session is typed the line and reads it in a turn of its own.
    reports_as(&held, &id, first, Outcome::Done);
    hears(&held, &told, steward, Event::UserPromptSubmit);
    // The second reports while that turn runs: nothing is typed into a running turn.
    reports_as(&held, &id, second, Outcome::Done);
    assert_eq!(crate::dispatched::waits(&held, steward).tasks, 0);

    // That turn ends with a report it was not told of: purlis types its line, so the end is
    // not the person's.
    hears(&held, &told, steward, Event::Stop);
    assert!(
        queue(&held).is_empty(),
        "a line is about to start its next turn"
    );
    assert_eq!(interruptions(&told, 0, steward), 0);

    // It reads the second report and stops with nothing left.
    has_a_turn(&held, &told, steward);

    assert_eq!(queue(&held), vec![steward]);
    assert_eq!(interruptions(&told, 0, steward), 1, "one, for the session");
}

#[test]
fn looking_at_one_failure_leaves_the_others_and_clears_by_its_record() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    hears(&held, &told, steward, Event::UserPromptSubmit);
    // Two tasks of one name: two records, two failures.
    let one = a_task_of(&held, &id, steward, "check prod");
    let two = a_task_of(&held, &id, steward, "check prod");
    hears(&held, &told, one, Event::UserPromptSubmit);
    hears(&held, &told, two, Event::UserPromptSubmit);
    reports_as(&held, &id, one, Outcome::Failed);
    reports_as(&held, &id, two, Outcome::Failed);
    let failed = held.hooks().board().failed_tasks(steward);
    assert_eq!(failed.len(), 2, "{failed:?}");
    assert_ne!(failed[0].id, failed[1].id);

    held.task_failure_cleared(steward, &failed[0].id);

    let left = held.hooks().board().failed_tasks(steward);
    assert_eq!(left, vec![failed[1].clone()]);
    assert!(queue(&held).contains(&steward));
}

#[test]
fn a_session_at_its_limit_is_listed_with_the_count_a_dispatch_is_decided_over() {
    let (_plane, _host, _planes, id, held, steward, told) = a_steward_chat_telling();
    let running_of = |session: u32| {
        let sidebar = crate::sidebar_of(&held).expect("the sidebar");
        sidebar
            .workspaces
            .into_iter()
            .flat_map(|workspace| workspace.chats)
            .chain(sidebar.unfiled)
            .find(|chat| chat.session == session)
            .expect("the chat is listed")
            .tasks_running
    };
    assert_eq!(running_of(steward), None);
    let one = a_task_of(&held, &id, steward, "check prod");
    let _two = a_task_of(&held, &id, steward, "check staging");
    assert_eq!(running_of(steward), Some(2));

    // One reports: it owes nothing, and no longer counts against the limit.
    hears(&held, &told, one, Event::UserPromptSubmit);
    reports_as(&held, &id, one, Outcome::Done);

    assert_eq!(running_of(steward), Some(1));
}
