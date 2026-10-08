//! A task ends at its report, stays as a finished row, and can be reopened (#1485).
//!
//! Against the app's own records, on a pretend session host: the report is delivered and then
//! the program is ended; the finished row is read from the dispatch record; and a reopened
//! chat is an ordinary chat. The clock is the test's: it looks for the end
//! (`dispatched::end_look`) where it means the time to have passed.

use purlis_core::dispatched::{Answered, Looked, Waited, What};
use purlis_core::dispatchrecord;

use super::*;

/// A steward chat and the one task it dispatched, on a pretend host.
fn a_steward_and_its_task() -> (Plane, Pretend, Planes, PlaneId, Arc<Held>, u32, u32) {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    let task = a_task_of(&held, &id, steward, "check prod");
    (plane, host, planes, id, held, steward, task)
}

/// The task's harness says its turn ended, and the app looks at whatever waited on that, as
/// the hook channel has it do (`dispatched::heard`).
fn its_turn_ends(held: &Held, task: u32) {
    the_board_hears(held, task, Event::Stop);
    crate::dispatched::moved(held, task);
}

/// The moment a reported task is given to settle has passed.
fn settles(held: &Held, task: u32) {
    crate::dispatched::end_look(held, task, Looked::Settled);
}

/// The task reports mid-turn, its turn ends, and it has had its moment: purlis ends it.
fn reports_and_ends(held: &Held, id: &PlaneId, task: u32) {
    works(held, task);
    let said = reports(held, id, task);
    assert!(matches!(said, Answer::Reported { .. }), "{said:?}");
    its_turn_ends(held, task);
    settles(held, task);
}

fn finished_under(held: &Held, asker: u32) -> Vec<crate::finished::FinishedTask> {
    crate::finished::listed(held)
        .into_iter()
        .filter(|row| row.asker == asker)
        .collect()
}

#[test]
fn the_report_reaches_the_asking_chat_first_and_then_the_task_s_program_is_ended() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    works(&held, task);

    let said = reports(&held, &id, task);

    // Delivered: it waits for the steward chat's next turn, and the task's record says sent.
    assert!(matches!(said, Answer::Reported { .. }), "{said:?}");
    let dir = purlis_core::handback::dir(held.root()).join(format!("chat-{steward}"));
    assert_eq!(std::fs::read_dir(&dir).expect("kept").count(), 1);
    assert_eq!(
        stands(&held, steward, task),
        Some((PersonaChatState::Reported, "reported".to_owned()))
    );
    // Its turn is not over: its program runs on, and no clock running out early ends it.
    settles(&held, task);
    assert!(open_chats(&held).contains(&task));
    assert_eq!(held.operator_input(task, b"\r"), Ok(()));

    // Its harness says the turn ended. It gets a moment, and is still open in it.
    its_turn_ends(&held, task);
    assert!(open_chats(&held).contains(&task));
    settles(&held, task);

    // Ended by purlis: no longer a chat, and its program takes no more keys.
    assert!(!open_chats(&held).contains(&task));
    assert!(held.operator_input(task, b"\r").is_err());
    assert_eq!(stands(&held, steward, task), None);
    // The report is the one it sent, still there for the steward chat, and nothing says it
    // failed or went unreported.
    assert_eq!(
        waiting(&held, For::Chat(steward)),
        vec![("check prod".to_owned(), false, false)]
    );
    let record = record_of(&held, task);
    assert_eq!(
        record.report.map(|report| report.outcome),
        Some(dispatchrecord::Outcome::Done)
    );
}

#[test]
fn the_exit_that_follows_the_end_marks_nothing_failed_and_tells_nobody_again() {
    let (_plane, host, _planes, id, held, steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);
    // The one report, taken by the steward chat's turn.
    assert_eq!(waiting(&held, For::Chat(steward)).len(), 1);

    // The operating system says the program purlis ended is gone.
    host.program_ends(task, KILLED());

    assert_eq!(
        waiting(&held, For::Chat(steward)),
        Vec::new(),
        "no second word"
    );
    assert_eq!(
        record_of(&held, task).report.map(|report| report.outcome),
        Some(dispatchrecord::Outcome::Done)
    );
    assert!(!held.hooks().board().needs_you().contains(&steward));
}

#[test]
fn a_task_on_a_harness_purlis_hears_nothing_from_is_ended_after_its_bounded_wait() {
    let (_plane, _host, _planes, id, held, _steward, task) = a_steward_and_its_task();

    // No hook has ever reported for it: nothing says when its turn ends.
    let said = reports(&held, &id, task);
    assert!(matches!(said, Answer::Reported { .. }), "{said:?}");
    crate::dispatched::end_look(&held, task, Looked::Moved);
    settles(&held, task);
    assert!(open_chats(&held).contains(&task), "not before the bound");

    crate::dispatched::end_look(&held, task, Looked::WaitedOut);

    assert!(!open_chats(&held).contains(&task));
}

#[test]
fn a_task_that_has_not_reported_is_never_ended_by_this() {
    let (_plane, _host, _planes, _id, held, steward, task) = a_steward_and_its_task();
    rests(&held, task);

    for looked in [Looked::Moved, Looked::Settled, Looked::WaitedOut] {
        crate::dispatched::end_look(&held, task, looked);
    }

    assert!(open_chats(&held).contains(&task));
    assert_eq!(
        stands(&held, steward, task).map(|(state, _)| state),
        Some(PersonaChatState::Running)
    );
    assert_eq!(waiting(&held, For::Chat(steward)), Vec::new());
}

#[test]
fn a_finished_task_stays_as_a_row_under_the_chat_that_asked_read_from_its_record() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    works(&held, task);
    reports(&held, &id, task);
    // Reported, and still an open chat: drawn once, as that chat.
    assert_eq!(finished_under(&held, steward), Vec::new());
    its_turn_ends(&held, task);
    settles(&held, task);

    let rows = finished_under(&held, steward);

    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(
        (row.name.as_str(), row.outcome.as_str(), row.folds),
        ("check prod", "done", true)
    );
    assert_eq!(row.report, "Forty are stuck.");
    assert_eq!(row.id, record_of(&held, task).id);
    // It ended in a conversation, which is what a Reopen resumes.
    assert!(row.reopens);
    assert!(record_of(&held, task).conversation.is_some());
    // Nothing in memory is the row: the ledger forgotten, it reads the same from the store,
    // as an app started again reads it.
    crate::dispatched::closed(&held, task, None);
    assert_eq!(finished_under(&held, steward), rows);
}

#[test]
fn a_task_that_failed_is_a_row_of_its_own_and_one_whose_program_died_is_listed_too() {
    let (_plane, host, _planes, id, held, steward, task) = a_steward_and_its_task();
    let died = a_task_of(&held, &id, steward, "check staging");
    works(&held, task);
    tasks_report(&held, &id, &Tickets::default(), task, Outcome::Failed, None);
    its_turn_ends(&held, task);
    settles(&held, task);
    // The other's program ends by itself before it reports: purlis says so in its place, and
    // closes the chat whose program is gone.
    host.program_ends(died, KILLED());
    settles(&held, died);

    let rows = finished_under(&held, steward);

    assert_eq!(
        rows.iter()
            .map(|row| (row.name.as_str(), row.outcome.as_str(), row.folds))
            .collect::<Vec<_>>(),
        [
            ("check prod", "failed", false),
            ("check staging", "ended without a report", false)
        ]
    );
    assert!(!open_chats(&held).contains(&died));
}

#[test]
fn clear_finished_takes_the_rows_and_leaves_the_records() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);
    let row = finished_under(&held, steward).remove(0);
    let before = record_of(&held, task);

    assert_eq!(
        crate::finished::clear(&held, std::slice::from_ref(&row.id)),
        1
    );

    assert_eq!(finished_under(&held, steward), Vec::new());
    let after = record_of(&held, task);
    assert_eq!(
        after,
        dispatchrecord::Record {
            cleared: true,
            ..before
        }
    );
    // And a second press clears nothing.
    assert_eq!(crate::finished::clear(&held, &[row.id]), 0);
}

#[test]
fn the_rows_go_when_the_chat_that_asked_closes_and_the_records_stay() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);
    assert_eq!(finished_under(&held, steward).len(), 1);

    closes(&held, steward).expect("closed");

    assert_eq!(crate::finished::listed(&held), Vec::new());
    let record = record_of(&held, task);
    assert!(record.cleared);
    assert_eq!(
        record.report.map(|report| report.text),
        Some("Forty are stuck.".to_owned())
    );
}

#[test]
fn what_the_asking_chat_asks_of_a_finished_task_is_answered_in_plain_words() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);

    // A wait is answered with its report.
    let said = asks(
        &held,
        &id,
        steward,
        What::Wait {
            of: task,
            within_secs: 1,
        },
    );
    assert!(
        matches!(&said, Answer::Task(answered)
            if matches!(&**answered, Answered::Waited { what: Waited::Reported { .. }, .. })),
        "{said:?}"
    );
    // The waiting command took it: the steward chat's next turn is not handed it again.
    assert_eq!(
        asks(&held, &id, steward, What::Read { of: task }),
        Answer::Task(Box::new(Answered::Noted))
    );
    assert_eq!(waiting(&held, For::Chat(steward)), Vec::new());

    // A cancel, a follow-up and an answer each say it has finished.
    for (what, asked) in [
        (What::Cancel { of: task }, "nothing to cancel"),
        (
            What::Tell {
                to: task,
                text: "One more thing.".to_owned(),
            },
            "no turn left to read a message in",
        ),
        (
            What::Answer {
                to: task,
                text: "The second one.".to_owned(),
            },
            "no question left to answer",
        ),
    ] {
        let said = asks(&held, &id, steward, what);
        let Answer::No { why } = &said else {
            panic!("refused, not {said:?}")
        };
        assert_eq!(
            why,
            &format!(
                "'check prod' (chat {task}) has finished: it reported (done), and its program \
                 has ended, so there is {asked}. `purlis dispatch wait {task}` reads its report \
                 again. Dispatch a new task for more."
            )
        );
    }

    // And its list still has it, as finished, until its row is cleared.
    let listed = |held: &Held| match asks(held, &id, steward, What::List) {
        Answer::Task(answered) => match *answered {
            Answered::Listed { rows } => rows,
            other => panic!("a list, not {other:?}"),
        },
        other => panic!("a list, not {other:?}"),
    };
    let rows = listed(&held);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            rows[0].chat,
            rows[0].name.as_str(),
            rows[0].state.as_str(),
            rows[0].finished
        ),
        (task, "check prod", "reported: done", true)
    );
    let row = finished_under(&held, steward).remove(0);
    crate::finished::clear(&held, &[row.id]);
    assert_eq!(listed(&held), Vec::new());
}

#[test]
fn reopen_resumes_the_conversation_as_an_ordinary_chat_that_reports_to_nobody() {
    let (_plane, host, _planes, id, held, steward, task) = a_steward_and_its_task();
    reports_and_ends(&held, &id, task);
    // The steward chat's turn takes the task's one report.
    assert_eq!(waiting(&held, For::Chat(steward)).len(), 1);
    let record = record_of(&held, task);
    let conversation = record.conversation.clone().expect("it ended in one");
    let row = finished_under(&held, steward).remove(0);

    let reopened = crate::finished::reopen(
        &held,
        &row.id,
        purlis_core::engine::Size {
            columns: 80,
            rows: 24,
        },
    )
    .expect("reopened");

    // A new chat, on the same conversation, as the persona the task ran as.
    assert_ne!(reopened, task);
    let chat = held.chats().recorded_chat(reopened).expect("open");
    assert_eq!(
        chat.resume.as_ref().map(|id| id.as_str()),
        Some(conversation.as_str())
    );
    assert_eq!(chat.persona, record.persona);
    assert_eq!(chat.label.as_deref(), Some("check prod"));
    let started = host.openings().pop().expect("started");
    assert!(
        started.args.iter().any(|arg| arg == &conversation),
        "its harness is given the conversation to resume: {:?}",
        started.args
    );
    // An ordinary chat with a tab: nobody asked for it, and it is nobody's task.
    assert_eq!(held.chats().handed_from(reopened), None);
    assert!(chat.has_tab());
    assert_eq!(stands(&held, steward, reopened), None);
    assert_eq!(held.chats().lineage(steward, None, &|_| true).running, 0);
    // It carries on from the task's chat, as a chat of its own.
    assert_ne!(chat.identity.id, record.worker.chat.id);
    assert_eq!(chat.identity.resumed_from, record.worker.chat.id);
    // Its row has gone; its record stays.
    assert_eq!(finished_under(&held, steward), Vec::new());
    assert!(record_of(&held, task).cleared);

    // A report from it is refused in plain words, by either command, and the chat that asked
    // is told nothing.
    let said = reports(&held, &id, reopened);
    let Answer::No { why } = &said else {
        panic!("refused, not {said:?}")
    };
    assert_eq!(
        why,
        "this chat was reopened from the finished task 'check prod'. That task has already \
         reported, and a reopened chat is an ordinary chat: no chat is waiting on a report from \
         it, and none is told. Say what you found to the person instead"
    );
    assert_eq!(waiting(&held, For::Chat(steward)), Vec::new());
    assert_eq!(
        held.hooks().board().reports(steward),
        vec!["check prod".to_owned()]
    );
    // And the task's record is as its one report left it.
    assert_eq!(
        record_of(&held, task).report.map(|report| report.text),
        Some("Forty are stuck.".to_owned())
    );
}

#[test]
fn a_task_that_cannot_be_reopened_says_why() {
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    let size = purlis_core::engine::Size {
        columns: 80,
        rows: 24,
    };
    // Still working: there is nothing finished to reopen.
    let running = record_of(&held, task).id;
    assert_eq!(
        crate::finished::reopen(&held, &running, size).unwrap_err(),
        "That dispatch is not a finished task, so there is nothing to reopen."
    );
    assert_eq!(
        crate::finished::reopen(&held, "01K6NOSUCHRECORD0000000000", size).unwrap_err(),
        "purlis has no record of that task, so it cannot be reopened."
    );

    // Reported, and its chat still open in its moment to settle.
    works(&held, task);
    reports(&held, &id, task);
    assert_eq!(
        crate::finished::reopen(&held, &running, size).unwrap_err(),
        "'check prod' is still open: its program has not ended yet. Open its chat instead."
    );
    its_turn_ends(&held, task);
    settles(&held, task);
    assert_eq!(finished_under(&held, steward).len(), 1);
}

#[test]
fn on_the_clock_a_task_whose_turn_has_ended_is_ended_within_moments() {
    // The one test that waits on the real clock: the settle is a thread of the app's own.
    let (_plane, _host, _planes, id, held, _steward, task) = a_steward_and_its_task();
    held.tasks().on_the_clock();
    works(&held, task);
    reports(&held, &id, task);
    assert!(open_chats(&held).contains(&task));

    let ended_at = Instant::now();
    its_turn_ends(&held, task);

    // Not at once: the harness has its moment to finish writing the conversation down.
    assert!(open_chats(&held).contains(&task));
    let bound = purlis_core::dispatched::A_TURN_SETTLES_WITHIN + std::time::Duration::from_secs(10);
    while open_chats(&held).contains(&task) && ended_at.elapsed() < bound {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!open_chats(&held).contains(&task), "never ended");
    assert!(ended_at.elapsed() >= purlis_core::dispatched::A_TURN_SETTLES_WITHIN);
}

#[test]
fn a_task_whose_asking_chat_has_gone_is_not_ended_by_its_report() {
    // Its report reaches no chat: it is kept for the workspace, and the chat that wrote it
    // stays open, which is where the person is told the report had nowhere to go.
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    closes(&held, steward).expect("closed, and the task kept running");
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
    its_turn_ends(&held, task);
    settles(&held, task);
    crate::dispatched::end_look(&held, task, Looked::WaitedOut);
    assert!(open_chats(&held).contains(&task));
    assert!(held.hooks().board().needs_you().contains(&task));
}

#[test]
fn a_report_that_lands_when_the_task_s_turn_is_already_over_ends_it_with_no_further_move() {
    // A report written for a task whose turn had ended (a cancelled task that sent none), or
    // sent by a command that outlived its turn: nothing more will be heard from the chat, so
    // the look made as the report is delivered is the one that settles it.
    let (_plane, _host, _planes, id, held, steward, task) = a_steward_and_its_task();
    rests(&held, task);

    let said = reports(&held, &id, task);

    assert!(matches!(said, Answer::Reported { .. }), "{said:?}");
    assert!(open_chats(&held).contains(&task), "it has its moment first");
    settles(&held, task);
    assert!(!open_chats(&held).contains(&task));
    assert_eq!(finished_under(&held, steward).len(), 1);
}
