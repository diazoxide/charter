//! A session's tokens and a task's time, held to the limits the person set (#1512, V100-59).
//!
//! Against the app's own records, on a pretend session host, as the tests beside it are. The
//! app's clock is called by hand, at a moment the test names.

use super::limits_where_they_bind::{a_steward_under, a_turn_begins, its_turn_ends, row_of};
use super::*;

use purlis_core::dispatchrecord::EndedBy;
use purlis_core::handback::{LIMIT_ENDED, PERSON_ENDED};

/// The app's clock looks, `minutes` from now.
fn the_clock_looks(held: &Held, minutes: i64) {
    let held = kept(held);
    bounded("the clock's look", move || {
        crate::overlimit::look_at(
            &held,
            chrono::Utc::now() + chrono::Duration::minutes(minutes),
        );
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

#[test]
fn with_neither_limit_set_nothing_is_stopped_however_long_and_however_much() {
    let (_plane, host, _planes, id, held, steward) = a_steward_under("");
    let task = a_task_of(&held, &id, steward, "check prod");
    rests(&held, task);
    has_used(&held, steward, 50_000_000, 50_000_000);

    the_clock_looks(&held, 60 * 24 * 365);

    assert!(!held.stopping().is_stopping(task));
    assert!(host.typed(task).is_empty(), "{:?}", host.typed(task));
    assert_eq!(row_of(&held, steward).at_limit, None);
    // And a dispatch goes ahead.
    let more = a_task_of(&held, &id, steward, "check staging");
    assert!(open_chats(&held).contains(&more));
}

#[test]
fn a_task_past_its_time_is_asked_for_its_report_and_its_asker_is_told_why() {
    let (_plane, host, _planes, id, held, steward) = a_steward_under("minutes-per-task = 30");
    let task = a_task_of(&held, &id, steward, "check prod");
    rests(&held, steward);
    rests(&held, task);

    // Inside its time: nothing happens.
    the_clock_looks(&held, 29);
    assert!(!held.stopping().is_stopping(task));

    // Past it: asked for its report as Stop and get its report asks, in purlis's words.
    the_clock_looks(&held, 31);
    assert!(held.stopping().is_stopping(task));
    assert_eq!(
        host.typed(task),
        vec![crate::stopping::sent_at_a_limit().into_bytes()]
    );
    // A second look begins no second stop.
    the_clock_looks(&held, 32);
    assert_eq!(host.typed(task).len(), 1);

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
    assert!(told.contains("a task may work 30 minutes here"), "{told}");
    assert!(told.contains(LIMIT_ENDED), "{told}");
    assert!(!told.contains(PERSON_ENDED), "{told}");
    assert!(told.contains("> checked two of five hosts"), "{told}");
    assert_eq!(record_of(&held, task).ended_by, Some(EndedBy::Limit));
}

#[test]
fn at_the_token_limit_a_dispatch_is_refused_with_the_figure_and_the_tasks_at_work_report() {
    let (_plane, host, _planes, id, held, steward) = a_steward_under("tokens-per-session = 100000");
    let task = a_task_of(&held, &id, steward, "check prod");
    rests(&held, steward);
    rests(&held, task);
    // The session's own chat and its task, together, as their harnesses reported them.
    has_used(&held, steward, 60_000, 10_000);
    has_used(&held, task, 30_000, 5_000);

    // A new dispatch is refused with the figure and where the limit is changed.
    let (refused, _) = dispatch(&held, &id, &Tickets::default(), steward, None, "and more");
    let Answer::No { why } = &refused else {
        panic!("refused, not {refused:?}");
    };
    assert!(why.contains("105k tokens"), "{why}");
    assert!(why.contains("may use 100k here"), "{why}");
    assert!(why.contains("Settings › Project › Dispatch"), "{why}");
    // The session's row says why.
    let said = row_of(&held, steward).at_limit.expect("at its token limit");
    assert_eq!(said.row, "at its token limit (100k)");

    // The clock asks the task at work for its report.
    the_clock_looks(&held, 1);
    assert!(held.stopping().is_stopping(task));
    assert_eq!(
        host.typed(task),
        vec![crate::stopping::sent_at_a_limit().into_bytes()]
    );
    assert!(
        !held.stopping().is_stopping(steward),
        "the session keeps running"
    );
    assert!(row_of(&held, steward).at_limit.is_some());
}

#[test]
fn a_harness_that_reports_no_tokens_never_reaches_the_token_limit() {
    let (_plane, _host, _planes, id, held, steward) = a_steward_under("tokens-per-session = 1000");
    let task = a_task_of(&held, &id, steward, "check prod");
    rests(&held, task);
    // Nothing reported by either chat: nothing is counted, and nothing guessed.
    the_clock_looks(&held, 1);
    assert!(!held.stopping().is_stopping(task));
    let more = a_task_of(&held, &id, steward, "check staging");
    assert!(open_chats(&held).contains(&more));
}
