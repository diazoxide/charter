//! Tasks keep their place across a restart, and a task whose asker has gone finishes and keeps
//! its report (#1513, V100-63, V100-64).
//!
//! Against the app's own records, on a pretend session host. **A restart is two apps, one
//! after the other**: the first is let go of before the second opens the project, as in life,
//! because one project is never open twice in one process.

use purlis_core::dispatchrecord;
use purlis_core::dispatchrestart::CARRY_ON;
use purlis_core::reopen::{Choice, Record};

use super::*;

const A_SIZE: purlis_core::engine::Size = purlis_core::engine::Size {
    columns: 80,
    rows: 24,
};

/// A steward chat in `alpha` and the task it dispatched, at work, on a pretend host.
fn a_steward_and_a_working_task() -> (Plane, Pretend, Planes, PlaneId, Arc<Held>, u32, u32) {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    let task = a_task_of(&held, &id, steward, "check prod");
    works(&held, task);
    (plane, host, planes, id, held, steward, task)
}

/// What a quit leaves: the record of every open chat, written where the next launch reads it.
fn quits(held: &Held) -> Record {
    let at_quit = held.chats().record();
    purlis_core::reopen::write(held.root(), &at_quit).expect("the record is written");
    at_quit
}

/// The app that quit is let go of, and the next one opens the project and puts `at_quit` back
/// the way a launch does (`Held::reopen`). Answers the new host, the project, and the chats
/// that came back.
fn launches_again(
    plane: &Plane,
    quit: (Pretend, Planes, Arc<Held>),
    at_quit: &Record,
) -> (Pretend, Planes, PlaneId, Arc<Held>, Vec<crate::chats::Open>) {
    let relaunched = Pretend::default();
    let planes = planes_on(&relaunched);
    let (again, held, opened) =
        reopens(plane, quit, &planes, Ok(at_quit.clone()), Choice::ReopenAll);
    (relaunched, planes, again, held, opened)
}

/// The app that quit is let go of, and `planes` opens the project and puts `read` back as
/// `choice` asks, through the launch's own path.
fn reopens(
    plane: &Plane,
    quit: (Pretend, Planes, Arc<Held>),
    planes: &Planes,
    read: Result<Record, std::io::Error>,
    choice: Choice,
) -> (PlaneId, Arc<Held>, Vec<crate::chats::Open>) {
    bounded("the app that quit, closing its project", move || drop(quit));
    let again = planes.open(&plane.root);
    let held = planes.held(&again).expect("held");
    held.reopen(A_SIZE, read, choice);
    let opened = held.chats().open_now();
    (again, held, opened)
}

/// Planes on `host` whose kill switch is kept in `config`, as the app's are.
fn planes_kept_in(host: &Pretend, config: &Path) -> Planes {
    let host = host.clone();
    Planes::telling(
        Arc::new(|_| {}),
        crate::Shipped::default(),
        Some(config.to_path_buf()),
    )
    .running_sessions_on(Arc::new(move |_| Box::new(host.clone())))
}

/// The steward chat is resumed from its session record at `path`, as Resume does: a new chat,
/// with what `restored::resuming` gives it.
fn resumes_the_steward(held: &Held, path: &str) -> Result<u32, String> {
    let alpha = held.root().join("workspaces").join("alpha");
    let ready = purlis_core::start::ready(
        &purlis_core::start::Start {
            profile: Some("work".to_owned()),
            persona: Some("steward".to_owned()),
            name: "1".to_owned(),
            cwd: Some(alpha),
            ..Default::default()
        },
        held.root(),
    )?;
    let chat = Chat {
        program: ready.program.clone(),
        cwd: ready.cwd.clone(),
        name: "1".to_owned(),
        resume: ready.session.clone(),
        profile: Some("work".to_owned()),
        persona: Some("steward".to_owned()),
        ..Default::default()
    };
    crate::restored::resuming(held, path, chat, |chat| {
        held.chats().start_ready(chat, &ready, A_SIZE)
    })
}

/// What chat `chat` was started with, by the host that started it.
fn started_with(host: &Pretend, chat: u32) -> Vec<String> {
    let asked = host.asked();
    let at = asked
        .iter()
        .position(|(number, _)| *number == chat)
        .expect("it was started");
    host.openings()[at].args.clone()
}

/// How many starts, of every host, were given the brief `a_task_of` sends.
fn briefs_sent(hosts: &[&Pretend]) -> usize {
    hosts
        .iter()
        .flat_map(|host| host.openings())
        .filter(|opening| {
            opening
                .args
                .iter()
                .any(|arg| arg.contains("# Check the queue"))
        })
        .count()
}

#[test]
fn after_a_restart_a_working_task_is_still_under_its_session_and_its_report_arrives() {
    let (plane, host, planes, _id, held, steward, task) = a_steward_and_a_working_task();
    let at_quit = quits(&held);

    let (relaunched, _planes, again, held, opened) =
        launches_again(&plane, (host.clone(), planes, held), &at_quit);

    // Both came back under the numbers they had, and the task is still the steward chat's.
    let mut numbers: Vec<u32> = opened.iter().map(|open| open.session).collect();
    numbers.sort_unstable();
    assert_eq!(numbers, [steward, task]);
    let from = held.chats().handed_from(task).expect("still a task");
    assert_eq!(
        (from.chat, from.mode, from.report),
        (
            steward,
            purlis_core::reopen::Mode::Task,
            purlis_core::reopen::Owed::Due
        )
    );
    // It was brought back on its conversation, told to carry on; the steward chat was told
    // nothing.
    assert_eq!(
        started_with(&relaunched, task).last().map(String::as_str),
        Some(CARRY_ON)
    );
    assert!(
        !started_with(&relaunched, steward)
            .iter()
            .any(|arg| arg == CARRY_ON)
    );
    // Its dispatch is still running: nothing settled it as failed.
    assert!(record_of(&held, task).running());

    // Its report reaches the steward chat.
    works(&held, task);
    let said = reports(&held, &again, task);
    assert!(
        matches!(
            said,
            Answer::Reported { kept_for: None, .. } | Answer::Finished { .. }
        ),
        "{said:?}"
    );
    assert_eq!(
        waiting(&held, For::Chat(steward)),
        vec![("check prod".to_owned(), false, false)]
    );
}

#[test]
fn no_brief_is_sent_twice_across_a_restart() {
    let (plane, host, planes, id, held, steward, task) = a_steward_and_a_working_task();
    // In one run, the same brief twice is two tasks on purpose.
    let second = a_task_of(&held, &id, steward, "check prod again");
    assert_ne!(second, task);
    let at_quit = quits(&held);

    let (relaunched, _planes, again, held, _opened) =
        launches_again(&plane, (host.clone(), planes, held), &at_quit);

    // The two tasks were started on their brief once each, before the quit, and never again.
    assert_eq!(briefs_sent(&[&host, &relaunched]), 2);
    // The steward chat, cut off as it dispatched, asks again: refused, with the task's number.
    match dispatch(
        &held,
        &again,
        &Tickets::default(),
        steward,
        None,
        "check prod once more",
    )
    .0
    {
        Answer::No { why } => {
            assert!(
                why.starts_with("purlis did not dispatch this a second time"),
                "{why}"
            );
            assert!(
                why.contains(&format!("(chat {task})"))
                    || why.contains(&format!("(chat {second})")),
                "{why}"
            );
        }
        other => panic!("refused, not {other:?}"),
    }
    assert_eq!(briefs_sent(&[&host, &relaunched]), 2);
}

#[test]
fn the_order_the_two_come_back_in_changes_nothing() {
    let (plane, host, planes, _id, held, steward, task) = a_steward_and_a_working_task();
    let mut at_quit = quits(&held);
    // The strip had the task's tab first.
    at_quit.chats.reverse();

    let (relaunched, _planes, again, held, _opened) =
        launches_again(&plane, (host.clone(), planes, held), &at_quit);

    assert_eq!(
        held.chats().handed_from(task).map(|from| from.chat),
        Some(steward)
    );
    assert_eq!(
        started_with(&relaunched, task).last().map(String::as_str),
        Some(CARRY_ON)
    );
    works(&held, task);
    reports(&held, &again, task);
    assert_eq!(
        waiting(&held, For::Chat(steward)),
        vec![("check prod".to_owned(), false, false)]
    );
}

#[test]
fn a_task_that_cannot_be_resumed_has_ended_by_itself_and_its_asker_is_told_once() {
    let (plane, host, planes, _id, held, steward, task) = a_steward_and_a_working_task();
    let mut at_quit = quits(&held);
    for chat in &mut at_quit.chats {
        if chat.number == Some(task) {
            // Its harness named no conversation purlis could bring back.
            chat.resume = None;
        }
    }

    let (relaunched, _planes, _again, held, opened) =
        launches_again(&plane, (host.clone(), planes, held), &at_quit);

    // Not started at all: a fresh chat would need its brief a second time.
    assert_eq!(
        opened.iter().map(|open| open.session).collect::<Vec<_>>(),
        [steward]
    );
    assert_eq!(relaunched.asked().len(), 1);
    assert!(held.chats().would_not_start().is_empty());
    // The steward chat is told it ended without a report, once.
    assert_eq!(
        waiting(&held, For::Chat(steward)),
        vec![("check prod".to_owned(), true, false)]
    );
    // And it is a finished row under the steward chat, failed, which does not fold away.
    let rows: Vec<_> = crate::finished::listed(&held)
        .into_iter()
        .filter(|row| row.asker == steward)
        .map(|row| (row.name, row.how, row.folds))
        .collect();
    assert_eq!(
        rows,
        [(
            "check prod".to_owned(),
            crate::finished::How::Unreported,
            false
        )]
    );
}

#[test]
fn an_orphaned_task_s_report_is_not_lost_and_is_delivered_when_its_asker_is_reopened() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_a_working_task();
    let path = writes_its_record(&held, steward);
    closes(&held, steward).expect("closed, and the task kept running");

    // The task finishes with nobody to tell.
    works(&held, task);
    let said = reports(&held, &id, task);
    assert!(
        matches!(
            &said,
            Answer::Reported {
                kept_for: Some(_),
                ..
            }
        ),
        "{said:?}"
    );
    // Kept on its record, where the person reads it.
    let record = record_of(&held, task);
    assert_eq!(
        record.report.as_ref().map(|report| report.text.as_str()),
        Some("Forty are stuck.")
    );
    assert!(record.undelivered.is_some());

    // The person resumes the steward chat from its session record: it is handed the report,
    // once, and the workspace is not handed it as well.
    let resumed = resumes_the_steward(&held, &path).expect("it resumes");
    assert_eq!(
        waiting(&held, For::Chat(resumed)),
        vec![("check prod".to_owned(), false, false)]
    );
    assert!(waiting(&held, For::Place(&Place::Workspace("alpha".to_owned()))).is_empty());
    assert!(record_of(&held, task).undelivered.is_none());

    // A second resume of the same record is handed nothing.
    let again = resumes_the_steward(&held, &path).expect("it resumes");
    assert!(waiting(&held, For::Chat(again)).is_empty());
}

#[test]
fn a_report_its_asker_closed_without_reading_reaches_it_reopened() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_a_working_task();
    let path = writes_its_record(&held, steward);
    reports(&held, &id, task);
    // The steward chat closes before its next turn ever read the report.
    closes(&held, steward).expect("closed");

    let resumed = resumes_the_steward(&held, &path).expect("it resumes");

    assert_eq!(
        waiting(&held, For::Chat(resumed)),
        vec![("check prod".to_owned(), false, false)]
    );
    assert!(waiting(&held, For::Place(&Place::Workspace("alpha".to_owned()))).is_empty());
}

#[test]
fn a_resume_that_does_not_start_leaves_the_report_where_it_was() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_a_working_task();
    let path = writes_its_record(&held, steward);
    closes(&held, steward).expect("closed");
    works(&held, task);
    reports(&held, &id, task);

    let refused = crate::restored::resuming(&held, &path, Chat::default(), |_| {
        Err("no profile".to_owned())
    });

    assert_eq!(refused, Err("no profile".to_owned()));
    assert!(record_of(&held, task).undelivered.is_some());
    assert_eq!(
        waiting(&held, For::Place(&Place::Workspace("alpha".to_owned()))),
        vec![("check prod".to_owned(), false, false)]
    );
}

#[test]
fn a_dispatch_record_nothing_brings_back_has_ended_after_the_launch() {
    let (plane, host, planes, _id, held, steward, task) = a_steward_and_a_working_task();
    // The record of open chats lost the task, as a crash between its start and the record's
    // next write would: its dispatch is still running on disk.
    let mut at_quit = quits(&held);
    at_quit.chats.retain(|chat| chat.number != Some(task));
    let record = record_of(&held, task);

    let (_relaunched, _planes, _again, held, _opened) =
        launches_again(&plane, (host.clone(), planes, held), &at_quit);

    let after = dispatchrecord::read(held.root(), &record.id).expect("its record");
    assert!(!after.running());
    assert_eq!(
        dispatchrecord::Finished::of(&after),
        Some(dispatchrecord::Finished::EndedWithoutAReport)
    );
    let _ = steward;
}

#[test]
fn a_task_s_report_after_a_restart_names_the_session_record_it_wrote_before_it() {
    // #1456: the app kept the path of a chat's session record in memory, so a report sent after
    // a relaunch said the chat had written none.
    let (plane, host, planes, _id, held, steward, task) = a_steward_and_a_working_task();
    let path = writes_its_record(&held, task);
    let at_quit = quits(&held);

    let (_relaunched, _planes, again, held, _opened) =
        launches_again(&plane, (host.clone(), planes, held), &at_quit);
    works(&held, task);
    reports(&held, &again, task);

    let told = purlis_core::handback::take(held.root(), For::Chat(steward));
    assert_eq!(told.len(), 1);
    assert_eq!(
        told[0].task.as_ref().and_then(|task| task.record.clone()),
        Some(path)
    );
}

// ---- review of #1513: an unread record, a stop of every agent, resume then report ---------------

#[test]
fn a_record_that_could_not_be_read_ends_no_dispatch_and_start_fresh_ends_them() {
    let (plane, host, planes, _id, held, _steward, task) = a_steward_and_a_working_task();
    quits(&held);
    let record = record_of(&held, task);

    // M1: the launch could not read the record of open chats. It says nothing about which
    // chats are gone, so nothing is ended.
    let relaunched = Pretend::default();
    let next = planes_on(&relaunched);
    let unread = Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "garbled",
    ));
    let (_again, held, opened) = reopens(
        &plane,
        (host.clone(), planes, held),
        &next,
        unread,
        Choice::ReopenAll,
    );
    assert!(opened.is_empty());
    let still = dispatchrecord::read(held.root(), &record.id).expect("its record");
    assert!(still.running(), "an unreadable record ended a dispatch");
    assert!(still.undelivered.is_none());

    // The person chose to start fresh: that is a word on which chats are gone.
    let at_quit = held.chats().record();
    let again = Pretend::default();
    let fresh = planes_on(&again);
    let mut whole = at_quit;
    whole.chats = Vec::new();
    let (_again, held, _opened) = reopens(
        &plane,
        (relaunched, next, held),
        &fresh,
        Ok(whole),
        Choice::StartFresh,
    );
    let ended = dispatchrecord::read(held.root(), &record.id).expect("its record");
    assert!(!ended.running());
    assert!(ended.undelivered.is_some(), "kept for the chat that asked");
}

#[test]
fn a_stop_of_every_agent_then_a_relaunch_keeps_every_task_and_retry_carries_it_on() {
    // M2: a refused start is not an end.
    let plane = a_plane_with_personas();
    let config = tempfile::tempdir().expect("a config home");
    let host = Pretend::default();
    let planes = planes_kept_in(&host, config.path());
    let id = planes.open(&plane.root);
    let held = planes.held(&id).expect("held");
    let alpha = plane.root.join("workspaces").join("alpha");
    let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
    let task = a_task_of(&held, &id, steward, "check prod");
    works(&held, task);
    let at_quit = quits(&held);
    let record = record_of(&held, task);
    let task_id = held
        .chats()
        .chat_at(task)
        .and_then(|at| at.id)
        .expect("an id");
    let steward_id = held
        .chats()
        .chat_at(steward)
        .and_then(|at| at.id)
        .expect("an id");
    planes
        .stop_every_agent(purlis_core::halt::Actor::Window)
        .expect("kept");

    let relaunched = Pretend::default();
    let next = planes_kept_in(&relaunched, config.path());
    let (_again, held, opened) = reopens(
        &plane,
        (host.clone(), planes, held),
        &next,
        Ok(at_quit),
        Choice::ReopenAll,
    );

    // Nothing started, both wait, the task's dispatch runs on, and nobody was told it failed.
    assert!(opened.is_empty());
    assert!(relaunched.asked().is_empty());
    assert_eq!(held.chats().would_not_start().len(), 2);
    assert!(
        dispatchrecord::read(held.root(), &record.id)
            .unwrap()
            .running()
    );
    assert!(waiting(&held, For::Chat(steward)).is_empty());
    assert!(waiting(&held, For::Place(&Place::Workspace("alpha".to_owned()))).is_empty());

    // The person lets agents run again and retries both: the task is told to carry on.
    next.rearm().expect("re-armed");
    for id in [&steward_id, &task_id] {
        crate::restored::retrying(&held, id, || held.chats().retry(id, A_SIZE)).expect("it starts");
    }
    assert_eq!(
        started_with(&relaunched, task).last().map(String::as_str),
        Some(CARRY_ON)
    );
    assert!(
        !started_with(&relaunched, steward)
            .iter()
            .any(|arg| arg == CARRY_ON)
    );
    assert_eq!(
        held.chats().handed_from(task).map(|from| from.chat),
        Some(steward)
    );
}

#[test]
fn forgetting_a_task_that_did_not_start_ends_it_and_its_asker_retried_is_told() {
    let plane = a_plane_with_personas();
    let config = tempfile::tempdir().expect("a config home");
    let host = Pretend::default();
    let planes = planes_kept_in(&host, config.path());
    let id = planes.open(&plane.root);
    let held = planes.held(&id).expect("held");
    let alpha = plane.root.join("workspaces").join("alpha");
    let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
    let task = a_task_of(&held, &id, steward, "check prod");
    let at_quit = quits(&held);
    let record = record_of(&held, task);
    let task_id = held
        .chats()
        .chat_at(task)
        .and_then(|at| at.id)
        .expect("an id");
    let steward_id = held
        .chats()
        .chat_at(steward)
        .and_then(|at| at.id)
        .expect("an id");
    planes
        .stop_every_agent(purlis_core::halt::Actor::Window)
        .expect("kept");
    let relaunched = Pretend::default();
    let next = planes_kept_in(&relaunched, config.path());
    let (_again, held, _opened) = reopens(
        &plane,
        (host.clone(), planes, held),
        &next,
        Ok(at_quit),
        Choice::ReopenAll,
    );

    crate::restored::forgetting(&held, &task_id, || held.chats().forget(&task_id))
        .expect("forgotten");

    let ended = dispatchrecord::read(held.root(), &record.id).unwrap();
    assert_eq!(
        dispatchrecord::Finished::of(&ended),
        Some(dispatchrecord::Finished::EndedWithoutAReport)
    );
    // The steward chat waits too, so the word is kept for it, and handed over when it starts.
    next.rearm().expect("re-armed");
    let back = crate::restored::retrying(&held, &steward_id, || {
        held.chats().retry(&steward_id, A_SIZE)
    })
    .expect("it starts");
    assert_eq!(
        waiting(&held, For::Chat(back)),
        vec![("check prod".to_owned(), true, false)]
    );
    assert!(waiting(&held, For::Place(&Place::Workspace("alpha".to_owned()))).is_empty());
}

#[test]
fn a_report_that_lands_after_its_asker_was_resumed_reaches_the_resumed_chat() {
    // M3: the person closes the session while a long task runs, resumes it, and the task
    // finishes afterwards.
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_a_working_task();
    let path = writes_its_record(&held, steward);
    closes(&held, steward).expect("closed, and the task kept running");
    let resumed = resumes_the_steward(&held, &path).expect("it resumes");
    assert!(waiting(&held, For::Chat(resumed)).is_empty(), "nothing yet");

    works(&held, task);
    reports(&held, &id, task);

    assert_eq!(
        waiting(&held, For::Chat(resumed)),
        vec![("check prod".to_owned(), false, false)]
    );
    assert!(waiting(&held, For::Place(&Place::Workspace("alpha".to_owned()))).is_empty());
    assert!(record_of(&held, task).undelivered.is_none());
}

#[test]
fn a_later_session_record_of_the_asker_resumes_it_too() {
    // F2: any record the asking chat wrote, not only the first one after the dispatch.
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_a_working_task();
    writes_its_record(&held, steward);
    let later = writes_its_record(&held, steward);
    closes(&held, steward).expect("closed");
    works(&held, task);
    reports(&held, &id, task);

    let resumed = resumes_the_steward(&held, &later).expect("it resumes");

    assert_eq!(
        waiting(&held, For::Chat(resumed)),
        vec![("check prod".to_owned(), false, false)]
    );
}

// ---- #1513 follow-ups (#1546) -----------------------------------------------------------------

#[test]
fn try_to_start_again_on_a_task_whose_dispatch_ended_meanwhile_tells_it_nothing() {
    // Train 66 join review, U1.
    let plane = a_plane_with_personas();
    let config = tempfile::tempdir().expect("a config home");
    let host = Pretend::default();
    let planes = planes_kept_in(&host, config.path());
    let id = planes.open(&plane.root);
    let held = planes.held(&id).expect("held");
    let alpha = plane.root.join("workspaces").join("alpha");
    let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
    let task = a_task_of(&held, &id, steward, "check prod");
    works(&held, task);
    let at_quit = quits(&held);
    let record = record_of(&held, task);
    let task_id = held
        .chats()
        .chat_at(task)
        .and_then(|at| at.id)
        .expect("an id");
    planes
        .stop_every_agent(purlis_core::halt::Actor::Window)
        .expect("kept");
    let relaunched = Pretend::default();
    let next = planes_kept_in(&relaunched, config.path());
    let (_again, held, _opened) = reopens(
        &plane,
        (host.clone(), planes, held),
        &next,
        Ok(at_quit),
        Choice::ReopenAll,
    );
    // Another hand ended its dispatch while it waited to start.
    dispatchrecord::close(
        held.root(),
        &record.id,
        dispatchrecord::Ending::default(),
        chrono::Utc::now(),
    )
    .expect("closed");

    next.rearm().expect("re-armed");
    crate::restored::retrying(&held, &task_id, || held.chats().retry(&task_id, A_SIZE))
        .expect("it starts");

    assert!(
        !started_with(&relaunched, task)
            .iter()
            .any(|arg| arg == CARRY_ON),
        "a dispatch no longer running is not carried on"
    );
}
