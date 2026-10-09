//! A task's working time held to its limit, and a session's tokens shown against theirs
//! (#1512, V100-59).
//!
//! Against the app's own records, on a pretend session host, as the tests beside it are. The
//! app's clock is called by hand, at moments the test names.

use super::limits_where_they_bind::{a_steward_under, a_turn_begins, its_turn_ends, row_of};
use super::*;

use purlis_core::dispatchlimits::Reached;
use purlis_core::dispatchrecord::EndedBy;
use purlis_core::handback::{LIMIT_ENDED, PERSON_ENDED};

const ESCAPE: &[u8] = purlis_core::dispatched::INTERRUPT;

/// The app's clock looks at `at`.
fn the_clock_looks_at(held: &Held, at: chrono::DateTime<chrono::Utc>) {
    let held = kept(held);
    bounded("the clock's look", move || {
        crate::overlimit::look_at(&held, at);
    });
}

/// Chat `chat`'s harness says its conversation has used `input` and `output` tokens in all.
fn has_used(held: &Held, chat: u32, input: u64, output: u64) {
    let conversation = held
        .board()
        .conversation(chat)
        .or_else(|| {
            held.chats()
                .recorded_chat(chat)
                .and_then(|one| one.resume)
                .map(|id| id.as_str().to_owned())
        })
        .expect("the chat's conversation");
    let path = purlis_core::usage::spend_file_for(held.root(), &conversation).expect("a path");
    std::fs::create_dir_all(path.parent().expect("its folder")).expect("the folder");
    std::fs::write(
        &path,
        format!("{{\"input_tokens\":{input},\"output_tokens\":{output}}}\n"),
    )
    .expect("the figure");
}

/// Task `task`'s record says it has worked `secs` already: a task put back after a restart.
fn has_worked(held: &Held, task: u32, secs: u64) {
    let record = record_of(held, task);
    purlis_core::dispatchrecord::worked(held.root(), &record.id, secs).expect("kept");
}

/// The moment an interrupted turn is given to stop has passed.
fn its_turn_has_stopped(held: &Held, chat: u32) {
    let held = kept(held);
    bounded("the look after an interrupt", move || {
        crate::stopping::settled_in_a_test(&held, chat);
    });
}

/// The moment of the first look.
fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

fn secs(n: i64) -> chrono::Duration {
    chrono::Duration::seconds(n)
}

#[test]
fn with_neither_limit_set_nothing_is_stopped_however_long_and_however_much() {
    let (_plane, host, _planes, id, held, steward) = a_steward_under("");
    let task = a_task_of(&held, &id, steward, "check prod");
    works(&held, task);
    has_worked(&held, task, 60 * 60 * 24);
    has_used(&held, steward, 50_000_000, 50_000_000);
    let at = now();

    the_clock_looks_at(&held, at);
    the_clock_looks_at(&held, at + secs(30));

    assert!(!held.stopping().is_stopping(task));
    assert!(host.typed(task).is_empty(), "{:?}", host.typed(task));
    assert_eq!(row_of(&held, steward).at_limit, None);
    // The clock read nothing, and so kept nothing: the record's figure is as it was.
    assert_eq!(record_of(&held, task).worked, 60 * 60 * 24);
    let more = a_task_of(&held, &id, steward, "check staging");
    assert!(open_chats(&held).contains(&more));
}

#[test]
fn a_task_past_its_working_time_is_asked_for_its_report_and_its_asker_is_told_why() {
    let (_plane, host, _planes, id, held, steward) = a_steward_under("minutes-per-task = 30");
    let task = a_task_of(&held, &id, steward, "check prod");
    rests(&held, steward);
    works(&held, task);
    has_worked(&held, task, 29 * 60 + 50);
    let at = now();

    // The first look only notes when it looked: inside its time, nothing happens.
    the_clock_looks_at(&held, at);
    assert!(!held.stopping().is_stopping(task));

    // Forty working seconds later it is past its time: its turn is ended, then it is asked
    // for its report in purlis's words.
    the_clock_looks_at(&held, at + secs(40));
    assert!(held.stopping().is_stopping(task));
    assert_eq!(host.typed(task), vec![ESCAPE.to_vec()]);
    its_turn_has_stopped(&held, task);
    let line = crate::stopping::sent_at_a_limit(Reached::Time {
        limit: 30,
        worked: 30,
    });
    assert_eq!(host.typed(task)[1], line.into_bytes());
    // A second look begins no second stop.
    the_clock_looks_at(&held, at + secs(70));
    assert_eq!(host.typed(task).len(), 2);
    // Its asking chat lists it as being stopped at its time limit, never by the person.
    let record = record_of(&held, task);
    assert_eq!(
        record.limit,
        Some(Reached::Time {
            limit: 30,
            worked: 30
        })
    );

    a_turn_begins(&held, task);
    let answer = tasks_report(
        &held,
        &id,
        &Tickets::default(),
        task,
        Outcome::Blocked,
        Some("checked two of five hosts"),
    );
    assert!(matches!(answer, Answer::Reported { .. }), "{answer:?}");
    its_turn_ends(&held, task);

    assert!(!open_chats(&held).contains(&task), "it ended");
    let words = purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(steward));
    assert_eq!(words.len(), 1, "{words:?}");
    let told = purlis_core::handback::context(&words, false).expect("told");
    assert!(told.contains("stopped at its time limit"), "{told}");
    assert!(told.contains("It had worked 30 minutes"), "{told}");
    assert!(told.contains(LIMIT_ENDED), "{told}");
    assert!(!told.contains(PERSON_ENDED), "{told}");
    assert!(!told.contains("by the person"), "{told}");
    assert!(told.contains("> checked two of five hosts"), "{told}");
    assert_eq!(record_of(&held, task).ended_by, Some(EndedBy::Limit));
}

#[test]
fn only_working_time_counts_and_a_restored_task_keeps_what_it_had() {
    let (_plane, _host, _planes, id, held, steward) = a_steward_under("minutes-per-task = 30");
    let task = a_task_of(&held, &id, steward, "check prod");
    // Put back after a restart, having worked 25 minutes; dispatched long ago, which counts
    // for nothing.
    has_worked(&held, task, 25 * 60);
    let at = now();

    // Waiting, on the person or on its own tasks: no time is added.
    rests(&held, task);
    the_clock_looks_at(&held, at);
    the_clock_looks_at(&held, at + secs(30));
    assert_eq!(record_of(&held, task).worked, 25 * 60);

    // Working: what the clock saw is added and kept on the record.
    works(&held, task);
    the_clock_looks_at(&held, at + secs(60));
    the_clock_looks_at(&held, at + secs(120));
    // 25 minutes, then 30 and 60 working seconds: kept as each minute is passed.
    assert_eq!(record_of(&held, task).worked, 26 * 60 + 30);

    // Ten hours with nobody looking, as purlis closed overnight: one look's worth at most.
    the_clock_looks_at(&held, at + secs(120 + 10 * 3600));
    assert_eq!(record_of(&held, task).worked, 27 * 60 + 30);
    assert!(!held.stopping().is_stopping(task));
}

#[test]
fn a_task_the_person_is_typing_into_is_left_for_the_next_look() {
    let (_plane, host, _planes, id, held, steward) = a_steward_under("minutes-per-task = 30");
    let task = a_task_of(&held, &id, steward, "check prod");
    works(&held, task);
    has_worked(&held, task, 31 * 60);
    held.operator_input(task, b"use the staging cluster instead\r")
        .expect("sent");
    let at = now();

    the_clock_looks_at(&held, at);
    the_clock_looks_at(&held, at + secs(30));

    // Past its time, and the person is in it: nothing is typed and nothing ends.
    assert!(!held.stopping().is_stopping(task));
    assert_eq!(
        host.typed(task),
        vec![b"use the staging cluster instead\r".to_vec()]
    );
}

#[test]
fn a_task_past_its_time_takes_its_own_tasks_with_it_deepest_first_and_names_them() {
    let (_plane, host, _planes, id, held, steward) = a_steward_under("minutes-per-task = 30");
    let task = a_task_of(&held, &id, steward, "check prod");
    let below = a_task_of(&held, &id, task, "check one host");
    rests(&held, steward);
    rests(&held, below);
    works(&held, task);
    has_worked(&held, task, 31 * 60);
    let at = now();

    the_clock_looks_at(&held, at);
    the_clock_looks_at(&held, at + secs(30));

    // Both are being stopped; the one below is asked at once, as stopped with the task above.
    assert!(held.stopping().is_stopping(task));
    assert!(held.stopping().is_stopping(below));
    assert_eq!(
        host.typed(below),
        vec![crate::stopping::sent_at_a_limit(Reached::Above { limit: 30 }).into_bytes()]
    );
    // The task waits for the one below to end before its own stop begins: deepest first.
    assert!(host.typed(task).is_empty(), "{:?}", host.typed(task));

    // The one below reports and ends first.
    a_turn_begins(&held, below);
    let answer = tasks_report(
        &held,
        &id,
        &Tickets::default(),
        below,
        Outcome::Done,
        Some("host one is fine"),
    );
    assert!(matches!(answer, Answer::Reported { .. }), "{answer:?}");
    its_turn_ends(&held, below);
    assert_eq!(
        record_of(&held, below).limit,
        Some(Reached::Above { limit: 30 })
    );

    // Then the task itself: its turn is ended, then it is asked; the steward is told of it
    // once, naming the one below.
    assert_eq!(host.typed(task), vec![ESCAPE.to_vec()]);
    its_turn_has_stopped(&held, task);
    a_turn_begins(&held, task);
    let answer = tasks_report(
        &held,
        &id,
        &Tickets::default(),
        task,
        Outcome::Blocked,
        Some("half done"),
    );
    assert!(matches!(answer, Answer::Reported { .. }), "{answer:?}");
    its_turn_ends(&held, task);
    let words = purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(steward));
    assert_eq!(words.len(), 1, "{words:?}");
    let told = purlis_core::handback::context(&words, false).expect("told");
    assert!(told.contains("stopped at its time limit"), "{told}");
    assert!(told.contains("`check one host`"), "{told}");
}

#[test]
fn an_inflated_token_figure_refuses_nothing_and_stops_nothing() {
    // #1512, #1457: the figure is one a chat can alter, for its own conversation or another's.
    let (_plane, host, _planes, id, held, steward) = a_steward_under("tokens-per-session = 100000");
    let task = a_task_of(&held, &id, steward, "check prod");
    rests(&held, steward);
    rests(&held, task);
    has_used(&held, steward, 1_000_000_000, 0);
    has_used(&held, task, 1_000_000_000, 0);

    the_clock_looks_at(&held, now());

    assert!(!held.stopping().is_stopping(task));
    assert!(host.typed(task).is_empty());
    let more = a_task_of(&held, &id, steward, "check staging");
    assert!(
        open_chats(&held).contains(&more),
        "a dispatch is not refused"
    );
    // The row shows the figure against the limit, and says it is not enforced yet.
    let said = row_of(&held, steward).at_limit.expect("shown");
    assert!(said.row.contains("of 100k"), "{}", said.row);
    assert!(said.row.contains("not enforced yet"), "{}", said.row);
}

#[test]
fn a_session_s_tokens_count_its_ended_tasks_from_what_their_records_kept() {
    let (_plane, _host, _planes, id, held, steward) =
        a_steward_under("tokens-per-session = 100000");
    let task = a_task_of(&held, &id, steward, "check prod");
    has_used(&held, task, 30_000, 5_000);
    let reported = reports(&held, &id, task);
    assert!(matches!(reported, Answer::Finished { .. }), "{reported:?}");
    let _ = held.close_chat(task);
    assert!(!open_chats(&held).contains(&task), "the task has ended");
    assert!(
        record_of(&held, task).usage.is_some(),
        "its figure was kept"
    );
    // Under the limit with its own chat alone: shown only with the ended task's figure.
    has_used(&held, steward, 60_000, 10_000);

    the_clock_looks_at(&held, now());

    let said = row_of(&held, steward).at_limit.expect("past it");
    assert_eq!(
        said.row,
        "past its token limit (105k of 100k) · not enforced yet"
    );
}

#[test]
fn a_session_past_its_token_limit_says_so_as_soon_as_its_chats_are_put_back() {
    // #1545: a restart puts the chats back after the plane is open; the clock is told then,
    // and looks at once, not at its next round.
    let (plane, _host, planes, _id, held, steward) = a_steward_under("tokens-per-session = 100000");
    rests(&held, steward);
    has_used(&held, steward, 150_000, 0);
    // The quit: the last record is written, and the app that wrote it is kept, as a quit ends
    // nothing.
    held.chats().write_last(|record| {
        purlis_core::reopen::write(held.root(), record).expect("the last record is written");
    });
    let _before = (planes, held);

    let host = Pretend::default();
    let planes = planes_on(&host);
    let id = planes.open(&plane.root);
    let held = planes.held(&id).expect("held");
    held.reopen(
        STARTING,
        purlis_core::reopen::read_or_refusal(&plane.root),
        purlis_core::reopen::Choice::ReopenAll,
    );
    assert!(
        held.chats().recorded_chat(steward).is_some(),
        "the session is back, under its number"
    );

    // The clock hears of it with no wait, and looks.
    let round_due =
        crate::overlimit::looks_at_what_was_put_back(&planes, std::time::Duration::ZERO);
    assert!(!round_due, "told of the put back, not timed out");

    let said = row_of(&held, steward).at_limit.expect("shown at once");
    assert_eq!(
        said.row,
        "past its token limit (150k of 100k) · not enforced yet"
    );
}
