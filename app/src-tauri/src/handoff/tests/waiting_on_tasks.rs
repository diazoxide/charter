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
    let name = crate::finished::task_name(&held, failed).expect("the task has a record");
    assert_eq!(
        held.hooks().board().failed_tasks(steward),
        vec![FailedTask::new(
            &name,
            HowFailed::Failed,
            "Forty are stuck."
        )]
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
            task: name,
            how: "failed".to_owned(),
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
    held.task_failures_seen(steward);
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
