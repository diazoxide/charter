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

#[test]
fn a_chat_an_older_build_handed_off_owing_a_report_is_its_askers_task_after_an_update() {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (quit, id, steward) = a_steward_chat(&host, &plane);
    let held = quit.held(&id).expect("held");
    let tickets = Tickets::default();
    // How the older build opened it: a handoff that asked for a report.
    let (older, _) =
        hand_off(&held, &id, &tickets, steward, Some("check prod"), true).expect("opened");
    let steward_id = held.chats().chat_at(steward).and_then(|at| at.id);
    let older_id = held.chats().chat_at(older).and_then(|at| at.id);
    // What a quit writes, to the disk, which is what the next launch reads.
    let at_quit = held.chats().record();
    purlis_core::reopen::write(held.root(), &at_quit).expect("written");
    let on_disk = std::fs::read_to_string(purlis_core::reopen::path(held.root())).expect("read");
    assert!(
        on_disk.contains(r#""report": "owed""#) && !on_disk.contains(r#""mode""#),
        "the older build's shape: {on_disk}"
    );
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
