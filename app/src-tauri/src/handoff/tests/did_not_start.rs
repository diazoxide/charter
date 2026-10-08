//! A task that did not start is a failed row under the chat that asked, and that chat's
//! report (#1497). And a launch that could not start a task again ends nothing: the task is
//! drawn under the chat that asked, kept, and ended only by the person (D-1497-14).
//!
//! Against the app's own records, on a pretend session host that a test can have refuse a
//! start, as a host with no pseudo-terminal to give does.

use purlis_core::dispatched::{Answered, Asked, Waited, What};
use purlis_core::dispatchrecord;
use purlis_core::reopen::Choice;

use super::*;

const NO_TERMINAL: &str = "could not open a pseudo-terminal: Operation not permitted";

fn a_steward(plane: &Plane) -> (Pretend, Planes, PlaneId, Arc<Held>, u32) {
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, plane);
    let held = planes.held(&id).expect("held");
    (host, planes, id, held, steward)
}

fn finished_under(held: &Held, asker: u32) -> Vec<crate::finished::FinishedTask> {
    crate::finished::listed(held)
        .into_iter()
        .filter(|row| row.asker == asker)
        .collect()
}

fn left_for(held: &Held, chat: u32) -> Vec<purlis_core::handback::Handback> {
    purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(chat))
}

fn asks(held: &Held, chat: u32, what: What) -> Answer {
    crate::dispatched::answer(held, &Asked { chat, what }, 0)
}

/// Chat `asker`'s own list of its tasks.
fn its_list(held: &Held, asker: u32) -> Vec<purlis_core::dispatched::Row> {
    match asks(held, asker, What::List) {
        Answer::Task(answered) => match *answered {
            Answered::Listed { rows } => rows,
            other => panic!("a list, not {other:?}"),
        },
        other => panic!("a list, not {other:?}"),
    }
}

/// The number chat `asker`'s own list of its tasks gives its finished task `name`.
fn listed_number(held: &Held, asker: u32, name: &str) -> u32 {
    its_list(held, asker)
        .into_iter()
        .find(|row| row.name == name && row.finished)
        .map(|row| row.chat)
        .expect("the task is listed as finished")
}

/// How a wait of chat `asker`'s on task number `of` ends, at once.
fn waited(held: &Held, asker: u32, of: u32) -> Waited {
    let said = asks(
        held,
        asker,
        What::Wait {
            of,
            within_secs: 100,
        },
    );
    let Answer::Task(answered) = said else {
        panic!("a wait's answer, not {said:?}");
    };
    let Answered::Waited { what, .. } = *answered else {
        panic!("a wait's end, not {answered:?}");
    };
    what
}

#[test]
fn a_chat_s_task_whose_start_is_refused_is_a_failed_row_and_is_answered_once_on_its_command() {
    let plane = a_plane_with_personas();
    let (host, _planes, id, held, steward) = a_steward(&plane);
    let before = held.chats().open_now().len();
    host.refuses(Some(NO_TERMINAL));

    let (said, arrived) = dispatch(&held, &id, &Tickets::default(), steward, None, "check prod");

    // Answered on the command, as it always was, with why.
    let Answer::No { why } = &said else {
        panic!("refused, not {said:?}");
    };
    assert!(why.contains(NO_TERMINAL), "{why}");
    assert!(arrived.is_none());
    assert_eq!(held.chats().open_now().len(), before, "nothing started");
    // And never a second time: nothing waits for the asking chat's next turn.
    assert!(left_for(&held, steward).is_empty());

    // A failed row under the chat that asked, with the whole of why, read from its record.
    let rows = finished_under(&held, steward);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(
        (
            row.name.as_str(),
            row.outcome.as_str(),
            row.folds,
            row.reopens
        ),
        ("check prod", "failed", false, false)
    );
    assert_eq!(row.report, format!("it did not start: {why}"));
    assert!(row.did_not_start && row.waits.is_none());
    let record = dispatchrecord::read(held.root(), &row.id).expect("its record");
    assert!(record.did_not_start && !record.running());
    assert_eq!(record.brief, "# Check the queue\nSay how many are stuck.\n");
    // It never ran, so the chat's own list does not say it reported.
    let listed = its_list(&held, steward);
    assert_eq!(
        listed
            .iter()
            .map(|row| (row.name.as_str(), row.state.as_str()))
            .collect::<Vec<_>>(),
        vec![("check prod", "did not start")]
    );

    // Cleared as every finished row is: the row goes and the record stays.
    assert_eq!(
        crate::finished::clear(&held, std::slice::from_ref(&row.id)),
        1
    );
    assert!(finished_under(&held, steward).is_empty());
    assert!(dispatchrecord::read(held.root(), &row.id).is_some());
}

#[test]
fn a_task_tried_again_and_refused_again_is_one_row_that_counts_the_tries() {
    let plane = a_plane_with_personas();
    let (host, _planes, id, held, steward) = a_steward(&plane);
    host.refuses(Some(NO_TERMINAL));
    for _ in 0..3 {
        let (said, _) = dispatch(&held, &id, &Tickets::default(), steward, None, "check prod");
        assert!(matches!(said, Answer::No { .. }), "{said:?}");
    }

    let rows = finished_under(&held, steward);
    assert_eq!(rows.len(), 1, "one row, not a pile: {rows:?}");
    assert_eq!(rows[0].attempts, 3);
    assert!(rows[0].report.contains(NO_TERMINAL));
}

#[test]
fn a_task_the_person_asked_for_from_a_tab_is_answered_in_its_dialog_and_leaves_no_row() {
    let plane = a_plane_with_personas();
    let (host, _planes, id, held, steward) = a_steward(&plane);
    host.refuses(Some(NO_TERMINAL));

    let said = ask_from_the_tab(&held, &id, steward, "devops", "check prod");

    let why = said.expect_err("nothing starts");
    assert!(why.contains(NO_TERMINAL), "{why}");
    assert!(finished_under(&held, steward).is_empty());
    assert!(left_for(&held, steward).is_empty());
}

#[test]
fn a_dispatch_the_person_allowed_that_does_not_start_is_its_asking_chat_s_failed_report() {
    let plane = a_plane_with_personas();
    let (host, _planes, id, held, steward) = a_steward(&plane);
    let (held_one, _) = dispatch(
        &held,
        &id,
        &Tickets::default(),
        steward,
        Some("devops"),
        "check the cluster",
    );
    assert!(
        matches!(held_one, Answer::NeedsGrant { .. }),
        "{held_one:?}"
    );
    let pending = held.dispatch_grants().waiting(steward)[0].id;
    host.refuses(Some(NO_TERMINAL));

    on_the_ground(&held, |ground| {
        held.dispatch_grants()
            .allow(ground, pending, purlis_core::sandbox::grant::Level::You)
    })
    .expect("allowed");

    // A failed row under the chat that asked.
    let rows = eventually(|| {
        let rows = finished_under(&held, steward);
        (!rows.is_empty()).then_some(rows)
    })
    .expect("its row");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        (
            rows[0].name.as_str(),
            rows[0].outcome.as_str(),
            rows[0].folds
        ),
        ("check the cluster", "failed", false)
    );
    assert!(
        rows[0].report.starts_with("it did not start: ") && rows[0].report.contains(NO_TERMINAL),
        "{}",
        rows[0].report
    );

    // A command waiting on it returns at once with the report: failed, in purlis's words.
    let number = eventually(|| {
        let number = listed_number(&held, steward, "check the cluster");
        (number != 0).then_some(number)
    })
    .expect("listed by a number a wait can name");
    let Waited::Reported { report } = waited(&held, steward, number) else {
        panic!("its report");
    };
    assert_eq!(report.summary, rows[0].report);
    assert_eq!(
        report.task,
        Some(purlis_core::handback::Task::unreported(None, false))
    );

    // Told once: the one report waits for its next turn, and no word on a held dispatch
    // beside it.
    let told = left_for(&held, steward);
    assert_eq!(told, vec![*report]);
    assert_eq!(told[0].answered, None);
}

// ----- a launch that could not start a task again (D-1497-14) -----

const NOT_THE_HARNESS: &str = "this project runs every chat sandboxed, and this profile's \
                               program does not answer as Claude Code, whose sandbox it was given";

/// A steward chat dispatched a real task, the app was quit, and the launch after it could
/// not start `refused` of the two again, through the launch's own road (`Held::reopen`).
struct Relaunched {
    _plane: Plane,
    /// The app before the quit, kept so nothing of it is closed.
    _before: (Planes, Arc<Held>),
    host: Pretend,
    _planes: Planes,
    held: Arc<Held>,
    /// The numbers the two chats had, which a launch keeps.
    steward: u32,
    task: u32,
    /// The task's chat id and its dispatch record's, from before the quit.
    task_id: String,
    record: String,
    /// The conversation the task was in, where its harness named one.
    conversation: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Refused {
    TheTask,
    Both,
}

fn relaunched(refused: Refused) -> Relaunched {
    let plane = a_plane_with_personas();
    let (_host, planes, id, held, steward) = a_steward(&plane);
    let task = a_task_of(&held, &id, steward, "check prod");
    let was = held.chats().recorded_chat(task).expect("the task's chat");
    let task_id = was.identity.id.clone().expect("a chat has an id");
    let record = dispatchrecord::running_for(
        held.root(),
        &crate::dispatches::chat_ref(&held, task).expect("the task"),
    )
    .expect("a dispatched task has a running record")
    .id;
    let conversation = was.resume.as_ref().map(|one| one.as_str().to_owned());
    // The quit: the last record is written, with both chats in it, and nothing after it. The
    // app that wrote it is kept and never let go of, as a quit ends no dispatch and closes
    // no chat: its programs go with the process.
    held.chats().write_last(|record| {
        purlis_core::reopen::write(held.root(), record).expect("the last record is written");
    });
    let before = (planes, held);

    let host = Pretend::default();
    host.refuses_number(task, NOT_THE_HARNESS);
    if refused == Refused::Both {
        host.refuses_number(steward, NOT_THE_HARNESS);
    }
    let planes = planes_on(&host);
    let id = planes.open(&plane.root);
    let held = planes.held(&id).expect("held");
    held.reopen(
        STARTING,
        purlis_core::reopen::read_or_refusal(&plane.root),
        Choice::ReopenAll,
    );
    Relaunched {
        _plane: plane,
        _before: before,
        host,
        _planes: planes,
        held,
        steward,
        task,
        task_id,
        record,
        conversation,
    }
}

#[test]
fn a_task_a_launch_could_not_start_again_is_drawn_under_its_asker_and_nothing_is_ended() {
    let it = relaunched(Refused::TheTask);
    let held = &it.held;
    assert!(
        held.chats().recorded_chat(it.steward).is_some(),
        "the chat that asked is back, under its number"
    );

    // Under the chat that asked, in the failed shape, with the reason in full...
    let rows = finished_under(held, it.steward);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(
        (row.id.as_str(), row.name.as_str(), row.outcome.as_str()),
        (it.task_id.as_str(), "check prod", "failed")
    );
    assert_eq!(row.report, format!("it did not start: {NOT_THE_HARNESS}"));
    assert!(row.did_not_start && !row.folds && !row.reopens);
    assert!(row.waits.is_some(), "still a task waiting to start");
    // ...and not in the line across the window.
    assert!(crate::unstarted::across_the_window(held).is_empty());

    // Nothing is ended: it is still recorded to be tried again, and its dispatch still runs.
    let waiting = held.chats().would_not_start();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].id, it.task_id);
    let on_disk = purlis_core::reopen::read_or_refusal(held.root()).expect("the record");
    assert!(
        on_disk
            .chats
            .iter()
            .any(|chat| chat.identity.id.as_deref() == Some(it.task_id.as_str())),
        "kept in the reopen record"
    );
    let record = dispatchrecord::read(held.root(), &it.record).expect("its record");
    assert!(record.running() && !record.did_not_start);

    // The chat that asked is sent nothing at a launch.
    assert!(left_for(held, it.steward).is_empty());
    // A wait on the task is answered at once with where it stands: not a report, and not
    // "not this chat's".
    let Waited::Running { state } = waited(held, it.steward, it.task) else {
        panic!("a standing");
    };
    assert_eq!(
        state,
        format!("waiting on the operator: it did not start again ({NOT_THE_HARNESS})")
    );
    let listed = its_list(held, it.steward);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(
        (listed[0].chat, listed[0].state.as_str(), listed[0].finished),
        (it.task, state.as_str(), false)
    );
}

#[test]
fn trying_again_starts_it_as_the_task_it_was() {
    let it = relaunched(Refused::TheTask);
    let held = &it.held;
    it.host.starts_number(it.task);

    let session = held
        .chats()
        .retry(&it.task_id, STARTING)
        .expect("it starts now");

    // The same task: the same chat, asked for by the same chat, its report still owed, and
    // its dispatch still the one that was running.
    let from = held.chats().handed_from(session).expect("still a task");
    assert_eq!(
        (from.chat, from.mode, from.report),
        (it.steward, Mode::Task, Owed::Due)
    );
    assert_eq!(
        held.chats()
            .recorded_chat(session)
            .and_then(|chat| chat.identity.id),
        Some(it.task_id.clone())
    );
    assert!(
        dispatchrecord::read(held.root(), &it.record)
            .expect("its record")
            .running()
    );
    assert!(finished_under(held, it.steward).is_empty());
    assert!(held.chats().would_not_start().is_empty());
    assert!(left_for(held, it.steward).is_empty());
}

#[test]
fn ending_it_is_the_person_s_word_and_only_then_is_the_asking_chat_told_it_failed() {
    let it = relaunched(Refused::TheTask);
    let held = &it.held;

    crate::unstarted::end(held, &it.task_id).expect("ended");

    // The dispatch that was running is the one that ends: failed, marked, with why and the
    // conversation it was in.
    let record = dispatchrecord::read(held.root(), &it.record).expect("its record");
    assert!(record.did_not_start && !record.running());
    assert_eq!(
        record.report.as_ref().map(|report| report.text.as_str()),
        Some(format!("it did not start: {NOT_THE_HARNESS}").as_str())
    );
    assert_eq!(record.conversation, it.conversation);
    // No longer a chat waiting to start, here or in the reopen record.
    assert!(held.chats().would_not_start().is_empty());
    let on_disk = purlis_core::reopen::read_or_refusal(held.root()).expect("the record");
    assert!(
        !on_disk
            .chats
            .iter()
            .any(|chat| chat.identity.id.as_deref() == Some(it.task_id.as_str()))
    );

    // Its row is a finished one now, with Reopen where there is a conversation to resume.
    let rows = finished_under(held, it.steward);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].id, it.record);
    assert!(rows[0].waits.is_none() && rows[0].did_not_start);
    assert_eq!(rows[0].reopens, it.conversation.is_some());

    // The chat that asked is told once, as a failed report, under the number the task had.
    let Waited::Reported { report } = waited(held, it.steward, it.task) else {
        panic!("its report");
    };
    assert_eq!(report.summary, rows[0].report);
    assert_eq!(left_for(held, it.steward), vec![*report]);
    assert_eq!(listed_number(held, it.steward, "check prod"), it.task);

    // And it ends once.
    assert!(crate::unstarted::end(held, &it.task_id).is_err());
    assert!(left_for(held, it.steward).is_empty());
}

#[test]
fn a_task_whose_record_does_not_end_is_not_forgotten_and_nobody_is_told() {
    let it = relaunched(Refused::TheTask);
    let held = &it.held;
    // Its dispatch's record ended by another hand: there is nothing for End task to end.
    assert!(
        dispatchrecord::close(
            held.root(),
            &it.record,
            dispatchrecord::Ending::default(),
            chrono::Utc::now()
        )
        .expect("closed")
    );

    let said = crate::unstarted::end(held, &it.task_id).expect_err("it did not end");

    assert!(said.contains("could not be ended"), "{said}");
    // Still recorded and still drawn: nothing is lost, and the asking chat hears nothing.
    let waiting = held.chats().would_not_start();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].id, it.task_id);
    let on_disk = purlis_core::reopen::read_or_refusal(held.root()).expect("the record");
    assert!(
        on_disk
            .chats
            .iter()
            .any(|chat| chat.identity.id.as_deref() == Some(it.task_id.as_str()))
    );
    assert!(left_for(held, it.steward).is_empty());
}

#[test]
fn a_task_from_before_dispatches_kept_records_is_ended_under_the_chat_it_was() {
    let it = relaunched(Refused::TheTask);
    let held = &it.held;
    // As an older purlis left it: the chat's own record of who asked, and no dispatch record.
    std::fs::remove_file(dispatchrecord::dir(held.root()).join(format!("{}.json", it.record)))
        .expect("its record is removed");
    let rows = finished_under(held, it.steward);
    assert_eq!(rows.len(), 1, "found by the number its own record has");
    assert!(rows[0].waits.is_some());

    crate::unstarted::end(held, &it.task_id).expect("ended");

    let rows = finished_under(held, it.steward);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let record = dispatchrecord::read(held.root(), &rows[0].id).expect("a record written now");
    // Under the chat it was, with its conversation: never a number that is nobody's.
    assert_eq!(record.worker.chat.id.as_deref(), Some(it.task_id.as_str()));
    assert_eq!(record.conversation, it.conversation);
    assert!(record.did_not_start && !dispatchrecord::never_a_chat(&record));
    assert_eq!(left_for(held, it.steward).len(), 1);
}

#[test]
fn a_task_whose_asking_chat_did_not_come_back_either_stays_the_window_s_own_line() {
    let it = relaunched(Refused::Both);
    let held = &it.held;

    // Nobody to be a row under: both are the line across the window, as before.
    assert_eq!(crate::unstarted::across_the_window(held).len(), 2);
    assert!(crate::unstarted::waiting(held).is_empty());
    assert!(crate::finished::listed(held).is_empty());
    assert!(crate::unstarted::end(held, &it.task_id).is_err());
    assert_eq!(held.chats().would_not_start().len(), 2);
}
