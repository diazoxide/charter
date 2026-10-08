//! A chat an older build opened by a handoff that asked for a report, still open after an
//! update, is the task of the chat that asked (#1519).
//!
//! Against the app's own records, on a pretend session host: the older build's chat is opened
//! the way that build opened it, the project is quit and its records are written to disk as
//! that build wrote them, and the next launch reads them back. From then the chat is listed by
//! the chat that asked, its report is a task's and can be waited on, and its dispatch record
//! ends as a task's.

use purlis_core::dispatched::{Answered, Waited, What};
use purlis_core::dispatchrecord;

use super::*;

const A_SIZE: purlis_core::engine::Size = purlis_core::engine::Size {
    columns: 80,
    rows: 24,
};

/// The number a chat put back at a launch has now, found among `started` by the id it had
/// when the app quit.
fn now_numbered(held: &Held, started: &[u32], id: Option<&str>) -> u32 {
    *started
        .iter()
        .find(|&&session| {
            held.chats()
                .chat_at(session)
                .and_then(|at| at.id)
                .as_deref()
                == id
        })
        .expect("it is back")
}

/// The records of the project at `root` as the older build wrote them for a handoff that
/// asked for a report: no `mode` on the chat's `from`, and dispatch `id` a running handoff
/// that owes a report.
fn in_the_older_shape(root: &std::path::Path, id: &str) {
    let edit = |path: std::path::PathBuf, change: &dyn Fn(&mut serde_json::Value)| {
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        change(&mut value);
        std::fs::write(&path, serde_json::to_string_pretty(&value).expect("json")).expect("write");
    };
    edit(purlis_core::reopen::path(root), &|record| {
        for chat in record["chats"].as_array_mut().expect("chats") {
            if let Some(from) = chat.get_mut("from").and_then(|from| from.as_object_mut()) {
                from.remove("mode");
                assert_eq!(from["report"], "owed");
            }
        }
    });
    edit(
        dispatchrecord::dir(root).join(format!("{id}.json")),
        &|record| {
            record["mode"] = "handoff".into();
            assert_eq!(record["report_owed"], true);
        },
    );
    let on_disk = std::fs::read_to_string(purlis_core::reopen::path(root)).expect("read");
    assert!(!on_disk.contains(r#""mode""#), "{on_disk}");
}

#[test]
fn a_chat_an_older_build_handed_off_owing_a_report_is_its_askers_task_after_an_update() {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (quit, id, steward) = a_steward_chat(&host, &plane);
    let held = quit.held(&id).expect("held");
    // This build opens no handoff that owes a report, so the chat is a task here, and the
    // records a quit writes are put in the older build's shape by hand below.
    let older = a_task_of(&held, &id, steward, "check prod");
    let steward_id = held.chats().chat_at(steward).and_then(|at| at.id);
    let older_id = held.chats().chat_at(older).and_then(|at| at.id);
    let at_quit = held.chats().record();
    purlis_core::reopen::write(held.root(), &at_quit).expect("written");
    in_the_older_shape(held.root(), &record_of(&held, older).id);
    bounded("the app that quit, closing its project", move || {
        drop(held);
        drop(quit);
        drop(host);
    });

    // The next launch, on a host of its own: the project is opened, then its chats put back.
    let relaunched = Pretend::default();
    let planes = planes_on(&relaunched);
    let again = planes.open(&plane.root);
    let held = planes.held(&again).expect("held");
    let back = purlis_core::reopen::read_or_refusal(held.root()).expect("it reads");
    let started: Vec<u32> = held
        .chats()
        .put_back(&back, A_SIZE)
        .iter()
        .map(|open| open.session)
        .collect();
    let steward = now_numbered(&held, &started, steward_id.as_deref());
    let older = now_numbered(&held, &started, older_id.as_deref());

    // Listed by the chat that asked, as its task.
    let Answer::Task(listed) = asks(&held, &again, steward, What::List) else {
        panic!("a list")
    };
    let Answered::Listed { rows } = *listed else {
        panic!("rows, not {listed:?}")
    };
    assert_eq!(
        rows.iter().map(|row| row.chat).collect::<Vec<_>>(),
        [older],
        "{rows:?}"
    );
    // And it still has the tab the older build gave it.
    assert!(
        held.chats()
            .recorded_chat(older)
            .is_some_and(|chat| chat.has_tab()),
        "its tab"
    );

    // The command the older build taught it is answered with the one that sends a task's.
    let said = report(&held, &again, &Tickets::default(), older, "Healthy.");
    assert!(
        matches!(&said, Answer::No { why } if why.contains("purlis dispatch report --outcome")),
        "{said:?}"
    );
    // Sent as a task's, it is one: a wait has it, and its record ends as a task's.
    let said = tasks_report(
        &held,
        &again,
        &Tickets::default(),
        older,
        purlis_core::handback::Outcome::Done,
        None,
    );
    assert!(
        matches!(said, Answer::Reported { .. } | Answer::Finished { .. }),
        "{said:?}"
    );
    let said = asks(
        &held,
        &again,
        steward,
        What::Wait {
            of: older,
            within_secs: 30,
        },
    );
    let Answer::Task(answered) = said else {
        panic!("an answer, not {said:?}")
    };
    assert!(
        matches!(
            &*answered,
            Answered::Waited { what: Waited::Reported { .. }, of, .. } if *of == older
        ),
        "{answered:?}"
    );
    let record = record_of(&held, older);
    assert_eq!(record.mode, dispatchrecord::Mode::Task);
    assert_eq!(
        dispatchrecord::Finished::of(&record),
        Some(dispatchrecord::Finished::Done)
    );
}

/// The open an older `purlis handoff --report` sends, from chat `asking` into `alpha`.
fn a_reporting_open(held: &Held, id: &PlaneId, asking: u32, name: &str) -> Answer {
    let tickets = Tickets::default();
    let ticket = ticket(held, id, &tickets, asking);
    answer(
        held,
        id,
        &tickets,
        1,
        Ask::Open(Box::new(OpenChat {
            chat: asking,
            workspace: "alpha".to_owned(),
            create_vision: None,
            persona: None,
            message: stamped(asking),
            ticket,
            name: Some(name.to_owned()),
            report: true,
        })),
        &nobody,
    )
}

/// F1/F2 of the review: an open that asks for a report is a task from the moment it is asked
/// for, so no record of the older shape is ever made again, and it is counted against the
/// same limit `purlis dispatch` is.
#[test]
fn an_open_that_asks_for_a_report_is_a_task_counted_and_held_as_one() {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    let tickets = Tickets::default();
    for n in 0..5 {
        let (said, _) = dispatch(&held, &id, &tickets, steward, None, &format!("task {n}"));
        assert!(matches!(said, Answer::Dispatched { .. }), "{n}: {said:?}");
    }

    // The sixth is an open that asks for a report: dispatched as a task.
    let said = a_reporting_open(&held, &id, steward, "check prod");
    let Answer::Dispatched { chat, .. } = said else {
        panic!("dispatched as a task, not {said:?}")
    };
    let from = held.chats().handed_from(chat).expect("its lineage");
    assert_eq!(
        (from.chat, from.mode, from.report),
        (steward, purlis_core::reopen::Mode::Task, Owed::Due)
    );
    let record = record_of(&held, chat);
    assert_eq!(
        (record.mode, record.report_owed),
        (dispatchrecord::Mode::Task, true)
    );
    assert_eq!(record.place.workspace.as_deref(), Some("alpha"));

    // It counts: a seventh task is refused, by either route, with the same sentence.
    let full = Answer::No {
        why: purlis_core::dispatchdecision::Refused::Limit(
            purlis_core::dispatchlimits::Refused::TooManyRunning {
                limit: 6,
                running: 6,
            },
        )
        .say(),
    };
    let (said, _) = dispatch(&held, &id, &tickets, steward, None, "one more");
    assert_eq!(said, full);
    assert_eq!(a_reporting_open(&held, &id, steward, "and another"), full);
}

#[test]
fn an_open_that_asks_for_a_report_and_would_create_its_workspace_is_refused_creating_nothing() {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    let tickets = Tickets::default();
    let ticket = ticket(&held, &id, &tickets, steward);

    let said = answer(
        &held,
        &id,
        &tickets,
        1,
        Ask::Open(Box::new(OpenChat {
            chat: steward,
            workspace: "gamma".to_owned(),
            create_vision: Some("a new one".to_owned()),
            persona: None,
            message: stamped(steward),
            ticket,
            name: None,
            report: true,
        })),
        &nobody,
    );

    assert!(
        matches!(&said, Answer::No { why } if why.contains("purlis workspace create gamma")),
        "{said:?}"
    );
    assert!(!held.root().join("workspaces").join("gamma").exists());
}
