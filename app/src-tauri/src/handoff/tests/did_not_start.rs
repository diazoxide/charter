//! A task that did not start is a failed row under the chat that asked, and that chat's
//! report (#1497).
//!
//! Against the app's own records, on a pretend session host that a test can have refuse a
//! start, as a host with no pseudo-terminal to give does.

use purlis_core::dispatched::{Answered, Asked, Waited, What};
use purlis_core::dispatchrecord;
use purlis_core::reopen::{Chat, Identity};

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

/// The number chat `asker`'s own list of its tasks gives its finished task `name`.
fn listed_number(held: &Held, asker: u32, name: &str) -> u32 {
    match asks(held, asker, What::List) {
        Answer::Task(answered) => match *answered {
            Answered::Listed { rows } => rows
                .into_iter()
                .find(|row| row.name == name && row.finished)
                .map(|row| row.chat)
                .expect("the task is listed as finished"),
            other => panic!("a list, not {other:?}"),
        },
        other => panic!("a list, not {other:?}"),
    }
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
    let record = dispatchrecord::read(held.root(), &row.id).expect("its record");
    assert!(record.did_not_start && !record.running());
    assert_eq!(record.brief, "# Check the queue\nSay how many are stuck.\n");

    // Cleared as every finished row is: the row goes and the record stays.
    assert_eq!(
        crate::finished::clear(&held, std::slice::from_ref(&row.id)),
        1
    );
    assert!(finished_under(&held, steward).is_empty());
    assert!(dispatchrecord::read(held.root(), &row.id).is_some());
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
    let waited = asks(
        &held,
        steward,
        What::Wait {
            of: number,
            within_secs: 100,
        },
    );
    let Answer::Task(answered) = waited else {
        panic!("a wait's answer, not {waited:?}");
    };
    let Answered::Waited {
        what: Waited::Reported { report },
        ..
    } = *answered
    else {
        panic!("its report, not {answered:?}");
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

/// A chat as a reopen record holds one, on the steward chat's own profile and folder.
fn recorded_like(held: &Held, steward: u32, id: &str, number: u32) -> Chat {
    let like = held
        .chats()
        .recorded_chat(steward)
        .expect("the steward chat");
    Chat {
        name: number.to_string(),
        number: Some(number),
        identity: Identity {
            id: Some(id.to_owned()),
            ..Identity::default()
        },
        from: None,
        label: None,
        resume: None,
        pid: None,
        active: false,
        ..like
    }
}

/// A task chat `asker` asked for, recorded like the open chat `like`.
fn a_task_of_chat(held: &Held, like: u32, asker: u32, id: &str, number: u32, name: &str) -> Chat {
    Chat {
        label: Some(name.to_owned()),
        from: Some(HandedFrom {
            chat: asker,
            name: "steward".to_owned(),
            workspace: Place::Workspace("alpha".to_owned()),
            report: Owed::Due,
            mode: Mode::Task,
            depth: 1,
            root: None,
            by_person: false,
        }),
        ..recorded_like(held, like, id, number)
    }
}

const NOT_THE_HARNESS: &str = "this project runs every chat sandboxed, and this profile's \
                               program does not answer as Claude Code, whose sandbox it was given";

#[test]
fn a_task_a_launch_could_not_put_back_is_a_failed_row_and_no_longer_waits_to_start() {
    let plane = a_plane_with_personas();
    let (_host, _planes, _id, held, steward) = a_steward(&plane);
    // What a launch found it could not start: a task of the steward chat's, a chat the
    // person opened, and a task whose asking chat is not open.
    let task = "01K6TASK000000000000000001";
    let own = "01K6R00T000000000000000002";
    let orphan = "01K6TASK000000000000000003";
    held.chats().was_not_put_back(
        a_task_of_chat(&held, steward, steward, task, 41, "check prod"),
        NOT_THE_HARNESS,
    );
    held.chats()
        .was_not_put_back(recorded_like(&held, steward, own, 42), NOT_THE_HARNESS);
    held.chats().was_not_put_back(
        a_task_of_chat(&held, steward, 9000, orphan, 43, "check staging"),
        NOT_THE_HARNESS,
    );

    crate::unstarted::not_put_back(&held);

    // The window's banner is left with the two that have nobody to be a row under.
    let waiting: Vec<String> = held
        .chats()
        .would_not_start()
        .into_iter()
        .map(|one| one.id)
        .collect();
    assert_eq!(waiting, vec![own.to_owned(), orphan.to_owned()]);

    // The task is a failed row under the chat that asked, with the reason in full.
    let rows = finished_under(&held, steward);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        (
            rows[0].name.as_str(),
            rows[0].outcome.as_str(),
            rows[0].folds
        ),
        ("check prod", "failed", false)
    );
    assert_eq!(
        rows[0].report,
        format!("it did not start: {NOT_THE_HARNESS}")
    );

    // And that chat is told once, as a failed report, under the number the task had.
    assert_eq!(listed_number(&held, steward, "check prod"), 41);
    let told = left_for(&held, steward);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].summary, rows[0].report);

    // Looked at again: nothing more is written or said.
    crate::unstarted::not_put_back(&held);
    assert_eq!(finished_under(&held, steward).len(), 1);
    assert!(left_for(&held, steward).is_empty());
}

#[test]
fn a_task_that_had_reported_before_the_quit_just_leaves_the_chats_waiting_to_start() {
    let plane = a_plane_with_personas();
    let (_host, _planes, _id, held, steward) = a_steward(&plane);
    let task = "01K6TASK000000000000000004";
    let mut reported = a_task_of_chat(&held, steward, steward, task, 44, "check prod");
    if let Some(from) = reported.from.as_mut() {
        from.report = Owed::Sent;
    }
    held.chats().was_not_put_back(reported, NOT_THE_HARNESS);

    crate::unstarted::not_put_back(&held);

    assert!(held.chats().would_not_start().is_empty());
    // It finished before the quit: nothing is failed for it now, and nobody is told again.
    assert!(finished_under(&held, steward).is_empty());
    assert!(left_for(&held, steward).is_empty());
}
