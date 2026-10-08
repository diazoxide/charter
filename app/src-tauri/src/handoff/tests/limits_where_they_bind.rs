//! A limit is shown where it binds, and a session's tasks can all be stopped at once (#1498,
//! V100-53, V100-54).
//!
//! Against the app's own records, on a pretend session host, as the tests beside it are.

use super::*;

/// The row the sidebar lists for chat `session`.
fn row_of(held: &Held, session: u32) -> crate::OpenChat {
    let sidebar = crate::sidebar_of(held).expect("the sidebar");
    sidebar
        .workspaces
        .into_iter()
        .flat_map(|workspace| workspace.chats)
        .chain(sidebar.unfiled)
        .find(|chat| chat.session == session)
        .expect("the chat is listed")
}

/// A steward chat, on a pretend host, in a project whose `[dispatch]` table is `table`.
fn a_steward_under(table: &str) -> (Plane, Pretend, Planes, PlaneId, Arc<Held>, u32) {
    let plane = a_plane_with_personas();
    std::fs::write(
        plane.root.join(purlis_core::plane::MANIFEST),
        format!("[persona]\ndefault = \"steward\"\n[dispatch]\n{table}\n"),
    )
    .expect("the manifest");
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    (plane, host, planes, id, held, steward)
}

/// Chat `chat`'s harness says a prompt began a turn, and the app hears it, its stop too.
fn a_turn_begins(held: &Held, chat: u32) {
    use purlis_core::hookwire::Conversation;
    the_board_hears(held, chat, Event::UserPromptSubmit);
    crate::dispatched::heard(
        held,
        &purlis_core::hookwire::Report {
            chat,
            event: Event::UserPromptSubmit,
            conversation: held
                .board()
                .conversation(chat)
                .map_or(Conversation::Unknown, Conversation::Named),
            pid: Some(4000 + chat),
            agent: None,
            detail: purlis_core::state::Detail::default(),
        },
    );
    crate::stopping::reported(&kept(held), chat);
}

/// Chat `chat`'s harness says its turn ended, and the app hears it, its stop too.
fn its_turn_ends(held: &Held, chat: u32) {
    the_board_hears(held, chat, Event::Stop);
    crate::dispatched::moved(held, chat);
    let held = kept(held);
    bounded("the end of a stopped chat's turn", move || {
        crate::stopping::reported(&held, chat);
    });
}

#[test]
fn a_session_refused_for_its_task_limit_says_so_on_its_row_until_a_slot_frees() {
    let (_plane, _host, _planes, id, held, asking) = a_steward_under("running-per-chat = 1");
    let (first, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "tidy up");
    let Answer::Dispatched { chat: task, .. } = first else {
        panic!("dispatched, not {first:?}");
    };
    // At the limit and not refused yet: nothing is said, since nothing was held back.
    assert_eq!(row_of(&held, asking).at_limit, None);

    let (second, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "and more");
    assert!(matches!(second, Answer::No { .. }), "{second:?}");

    // Refused: its row says so, with the number and where it is changed.
    let said = row_of(&held, asking).at_limit.expect("at its task limit");
    assert_eq!(said.limit, 1);
    assert!(
        said.said.contains("Settings › Project › Dispatch"),
        "{}",
        said.said
    );
    assert!(said.said.contains('1'), "{}", said.said);
    // A task's own row says nothing of its asker's limit.
    assert_eq!(row_of(&held, task).at_limit, None);

    // The task reports: a slot is free, and the sentence goes.
    let reported = reports(&held, &id, task);
    assert!(matches!(reported, Answer::Finished { .. }), "{reported:?}");
    assert_eq!(row_of(&held, asking).at_limit, None);
    assert!(!held.at_limits().marked(asking), "the mark is let go of");
}

#[test]
fn a_refusal_a_slot_does_not_free_says_nothing_on_the_row() {
    let (_plane, _host, _planes, id, held, asking) = a_steward_under("depth = 1");
    let task = a_task_of(&held, &id, asking, "tidy up");

    // The task is as deep as a chain goes: its dispatch is refused, and no slot frees that.
    let (deeper, _) = dispatch(&held, &id, &Tickets::default(), task, None, "deeper");
    assert!(matches!(deeper, Answer::No { .. }), "{deeper:?}");
    assert_eq!(row_of(&held, task).at_limit, None);
    assert!(!held.at_limits().marked(task));
}

#[test]
fn stop_all_tasks_stops_every_task_keeps_the_session_and_wakes_it_once() {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    let first = a_task_of(&held, &id, steward, "check prod");
    let second = a_task_of(&held, &id, steward, "check staging");
    rests(&held, steward);
    rests(&held, first);
    rests(&held, second);

    // The question names both.
    let ending = crate::stopping::all_tasks_ending_of(&held, steward).expect("asked");
    let mut named = ending.tasks.clone();
    named.sort_unstable();
    assert_eq!(named, vec![first, second]);

    let stopped = crate::stopping::stop_all_tasks_in_a_test(&kept(&held), steward, &ending.tasks)
        .expect("stopped");
    assert_eq!(stopped, 2);
    // Each is asked for its one short turn, as Stop and get its report asks.
    for task in [first, second] {
        assert!(held.stopping().is_stopping(task));
        assert_eq!(
            host.typed(task),
            vec![crate::stopping::sent_as(true).into_bytes()]
        );
    }
    // The session keeps running, and is no part of the stop.
    assert!(!held.stopping().is_stopping(steward));

    // The first reports and ends: its word waits, and the session is typed nothing yet.
    for (task, said) in [(first, "prod: fine"), (second, "staging: 2 stuck")] {
        a_turn_begins(&held, task);
        let answer = tasks_report(
            &held,
            &id,
            &Tickets::default(),
            task,
            Outcome::Blocked,
            Some(said),
        );
        assert!(matches!(answer, Answer::Reported { .. }), "{answer:?}");
        its_turn_ends(&held, task);
        if task == first {
            assert!(
                host.typed(steward).is_empty(),
                "nothing typed while one is stopping"
            );
        }
    }

    // Both have ended, the session has not, and it was typed one line for both words.
    let open = open_chats(&held);
    assert!(!open.contains(&first) && !open.contains(&second));
    assert!(open.contains(&steward));
    assert_eq!(host.typed(steward).len(), 1, "{:?}", host.typed(steward));
    let words = purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(steward));
    assert_eq!(words.len(), 2, "{words:?}");
    assert!(words.iter().all(|word| word.stopped.is_some()), "{words:?}");
    for task in [first, second] {
        assert_eq!(
            record_of(&held, task).ended_by,
            Some(purlis_core::dispatchrecord::EndedBy::Person)
        );
    }
}

#[test]
fn stop_all_tasks_ends_no_more_than_the_question_named() {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, steward) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    let first = a_task_of(&held, &id, steward, "check prod");
    rests(&held, first);
    let ending = crate::stopping::all_tasks_ending_of(&held, steward).expect("asked");
    assert_eq!(ending.tasks, vec![first]);
    // A task that starts after the question was asked is not stopped by its answer.
    let later = a_task_of(&held, &id, steward, "check staging");

    let stopped = crate::stopping::stop_all_tasks_in_a_test(&kept(&held), steward, &ending.tasks)
        .expect("stopped");

    assert_eq!(stopped, 1);
    assert!(held.stopping().is_stopping(first));
    assert!(!held.stopping().is_stopping(later));
    // And a session with nothing at work below it is told so, and nothing is stopped.
    let none = crate::stopping::stop_all_tasks_in_a_test(&kept(&held), later, &[later]);
    assert_eq!(none, Err(crate::stopping::NO_TASK_AT_WORK.to_owned()));
}
