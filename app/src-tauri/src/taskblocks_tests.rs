//! One answer for the tasks of one session that hit the same block (#1508): who it covers,
//! what it keeps and where, and that it covers no chat it did not list. Driven at
//! [`answered`] with a pretend app, so no chat is started; the last test starts real chats,
//! which needs a terminal.

use std::cell::RefCell;
use std::collections::HashMap;

use super::*;
use purlis_core::sandbox::hosts::Host;

/// steward 1 asked for talk (4), sweep (5) and probe (6); talk asked for deep (7). steward 2
/// (2) is another session, and its task is 8.
fn asker_of(chat: u32) -> Option<u32> {
    HashMap::from([(4, 1), (5, 1), (6, 1), (7, 4), (8, 2)])
        .get(&chat)
        .copied()
}

fn host() -> What {
    What::Host(Host::parse("registry.npmjs.org").expect("a host"))
}

/// What a pretend app was asked to keep, and to owe.
#[derive(Default)]
struct App {
    kept: RefCell<Vec<(u32, String, &'static str)>>,
    owed: RefCell<Vec<(u32, String)>>,
}

impl App {
    fn kept(&self) -> Vec<(u32, String, &'static str)> {
        self.kept.borrow().clone()
    }

    fn owed(&self) -> Vec<u32> {
        self.owed.borrow().iter().map(|(task, _)| *task).collect()
    }
}

/// Answers `tasks` of `session` at `level` on a pretend app that judges every task to `judge`.
fn answer(
    app: &App,
    session: u32,
    tasks: &[u32],
    level: GrantLevel,
    judge: &dyn Fn(u32) -> Result<(What, Level), String>,
) -> Result<Allowed, String> {
    let mut keep = |task: u32, what: &What, level: Level| {
        app.kept
            .borrow_mut()
            .push((task, what.target(), level.word()));
        Ok(())
    };
    answered(
        Asked {
            session,
            tasks,
            level,
        },
        Doing {
            asker_of: &asker_of,
            judge,
            keep: &mut keep,
            owe: &|task, told| app.owed.borrow_mut().push((task, told)),
        },
    )
}

fn to_host(level: Level) -> impl Fn(u32) -> Result<(What, Level), String> {
    move |_| Ok((host(), level))
}

#[test]
fn a_task_is_below_the_session_that_asked_for_it_at_any_depth_and_nothing_else_is() {
    assert!(below(&asker_of, 1, 4));
    assert!(below(&asker_of, 1, 7), "a task of a task");
    assert!(below(&asker_of, 4, 7));
    assert!(!below(&asker_of, 1, 1), "a session is not its own task");
    assert!(!below(&asker_of, 4, 1), "nor is the chat that asked");
    assert!(!below(&asker_of, 4, 5), "nor a sibling");
    assert!(!below(&asker_of, 1, 8), "nor another session's task");
    assert!(!below(&asker_of, 1, 99), "nor a chat with no record");
    // A record that loops reaches nothing.
    let looping = |chat: u32| match chat {
        10 => Some(11),
        11 => Some(10),
        _ => None,
    };
    assert!(!below(&looping, 1, 10));
}

#[test]
fn three_tasks_blocked_on_one_host_are_each_allowed_on_their_own_by_one_answer() {
    let app = App::default();
    let allowed =
        answer(&app, 1, &[4, 5, 7], GrantLevel::Chat, &to_host(Level::Chat)).expect("allowed");
    // Each task's own grant, and none for the session that asked them.
    assert_eq!(
        app.kept(),
        [4, 5, 7].map(|task| (task, "registry.npmjs.org".to_owned(), "chat"))
    );
    assert!(app.kept().iter().all(|(chat, ..)| *chat != 1));
    assert!(
        allowed.said.starts_with(
            "Allowed for each of these 3 tasks on its own, not for the chat that asked them."
        ),
        "{}",
        allowed.said
    );
}

#[test]
fn an_answer_for_everyone_is_kept_once_and_owes_every_listed_task_its_restart() {
    let app = App::default();
    let allowed =
        answer(&app, 1, &[4, 5, 6], GrantLevel::You, &to_host(Level::You)).expect("allowed");
    assert_eq!(
        app.kept(),
        [(4, "registry.npmjs.org".to_owned(), "you")],
        "one grant"
    );
    // The first was owed its restart by keeping it; the others are owed theirs here.
    assert_eq!(app.owed(), [5, 6]);
    assert!(
        app.owed
            .borrow()
            .iter()
            .all(|(_, told)| told.contains("registry.npmjs.org"))
    );
    assert!(
        allowed
            .said
            .starts_with("Allowed for me on this machine. The 3 tasks restart"),
        "{}",
        allowed.said
    );
}

#[test]
fn a_listed_chat_purlis_has_no_record_of_as_a_task_of_the_session_allows_nothing_for_any() {
    for tasks in [
        // Another session's task.
        &[4, 8][..],
        // The session itself: its permission and its tasks' are apart.
        &[1, 4][..],
        // Another session.
        &[4, 2][..],
        // A chat with no record.
        &[4, 99][..],
        // One named twice.
        &[4, 4][..],
        // None.
        &[][..],
    ] {
        let app = App::default();
        let refused =
            answer(&app, 1, tasks, GrantLevel::Chat, &to_host(Level::Chat)).expect_err("refused");
        assert!(refused.contains("allowed"), "{tasks:?}: {refused}");
        assert!(app.kept().is_empty(), "{tasks:?}");
        assert!(app.owed().is_empty(), "{tasks:?}");
    }
}

#[test]
fn a_task_that_cannot_be_allowed_allows_none_of_them() {
    let app = App::default();
    let judge = |task: u32| {
        if task == 5 {
            Err("its folder is not on the allowlist".to_owned())
        } else {
            Ok((host(), Level::Chat))
        }
    };
    let refused = answer(&app, 1, &[4, 5, 6], GrantLevel::Chat, &judge).expect_err("refused");
    assert_eq!(
        refused,
        "Nothing was allowed: for chat 5, its folder is not on the allowlist"
    );
    assert!(app.kept().is_empty());
}

#[test]
fn tasks_judged_to_different_grants_are_not_one_answer_for_everyone() {
    let app = App::default();
    let judge = |task: u32| Ok((What::Write(format!("/work/{task}").into()), Level::You));
    let refused = answer(&app, 1, &[4, 5], GrantLevel::You, &judge).expect_err("refused");
    assert!(refused.contains("do not want the same thing"), "{refused}");
    assert!(app.kept().is_empty());
    assert!(app.owed().is_empty());
}

#[test]
fn a_keep_that_fails_part_way_says_which_tasks_were_allowed() {
    let app = App::default();
    let mut keep = |task: u32, what: &What, level: Level| {
        if task == 6 {
            return Err("the event log is not open".to_owned());
        }
        app.kept
            .borrow_mut()
            .push((task, what.target(), level.word()));
        Ok(())
    };
    let refused = answered(
        Asked {
            session: 1,
            tasks: &[4, 5, 6],
            level: GrantLevel::Chat,
        },
        Doing {
            asker_of: &asker_of,
            judge: &to_host(Level::Chat),
            keep: &mut keep,
            owe: &|_, _| {},
        },
    )
    .expect_err("refused");
    assert_eq!(
        refused,
        "It was allowed for chat 4 and 5 only, and not for the rest: the event log is not open"
    );
}

/// Needs a terminal: real chats are started. A session's own grant, made by a block's Allow
/// on its tab, does not reach its task, and one made by this answer for a task reaches neither
/// the session nor the task's sibling (V100-57).
#[test]
fn a_grant_for_a_session_reaches_none_of_its_tasks_and_one_for_a_task_reaches_no_other_chat() {
    use crate::chats::Chats;
    use purlis_core::engine::Size;
    use purlis_core::reopen::{Chat, HandedFrom, Mode, Owed};
    let chats = Chats::new();
    let size = Size {
        columns: 80,
        rows: 24,
    };
    let plain = Chat {
        program: "/bin/sleep".to_owned(),
        args: vec!["5".to_owned()],
        name: "steward 1".to_owned(),
        ..Default::default()
    };
    let session = chats.start(&plain, size).expect("started");
    let task_of = |name: &str| Chat {
        name: name.to_owned(),
        from: Some(HandedFrom {
            chat: session,
            name: "steward 1".to_owned(),
            workspace: purlis_core::active::Place::Workspace("alpha".to_owned()),
            report: Owed::Due,
            mode: Mode::Task,
            depth: 1,
            root: None,
            by_person: false,
        }),
        ..plain.clone()
    };
    let talk = chats.start(&task_of("talk"), size).expect("started");
    let sweep = chats.start(&task_of("sweep"), size).expect("started");
    let id = |n: u32| {
        chats
            .recorded_chat(n)
            .and_then(|chat| chat.identity.id)
            .expect("an id")
    };
    let other = What::Host(Host::parse("api.example.com").expect("a host"));
    chats
        .grant(session, other.clone(), 1, "told".to_owned())
        .expect("granted to the session");
    assert!(!chats.holds(&id(talk), &other));
    assert!(!chats.holds(&id(sweep), &other));

    // The record is what says talk is the session's task.
    assert!(below(&|chat| asker_in(&chats, chat), session, talk));
    assert!(!below(&|chat| asker_in(&chats, chat), talk, sweep));
    let mut keep =
        |task: u32, what: &What, _: Level| chats.grant(task, what.clone(), 2, "told".to_owned());
    answered(
        Asked {
            session,
            tasks: &[talk],
            level: GrantLevel::Chat,
        },
        Doing {
            asker_of: &|chat| asker_in(&chats, chat),
            judge: &to_host(Level::Chat),
            keep: &mut keep,
            owe: &|task, told| chats.owe_restart(task, told),
        },
    )
    .expect("allowed");
    assert!(chats.holds(&id(talk), &host()));
    assert!(!chats.holds(&id(session), &host()), "not the session");
    assert!(!chats.holds(&id(sweep), &host()), "not a sibling");
    for chat in [sweep, talk, session] {
        chats.close(chat).ok();
    }
}

/// V100-56: a task asking for another persona with no grant is held on its own record, and
/// "Allow for this chat" on its question covers that task alone: its session and its sibling,
/// running as the same persona, are still asked.
#[test]
fn allowing_a_tasks_dispatch_for_this_chat_allows_nothing_for_its_session_or_siblings() {
    use crate::dispatchgrants::{Asking, Ground, Requested, Store, Uncovered};
    let project = tempfile::tempdir().expect("a project");
    let locks = purlis_core::sandbox::policy::Locks::none();
    let as_chat = |session: u32| Asking {
        session,
        id: Some(format!("chat-{session}")),
        name: format!("chat {session}"),
        persona: Some("steward".to_owned()),
        held: false,
    };
    let store = Store::default();
    let ask = |session: u32| {
        store
            .request(
                &Ground {
                    root: project.path(),
                    locks: &locks,
                    is_open: &|_| true,
                    sandboxed: &|_| true,
                    audit: &|_, _| Ok(()),
                    at: 100,
                },
                as_chat(session),
                "devops",
                "Check the deploy.",
                Uncovered::AskThePerson,
            )
            .0
    };
    let Requested::NeedsGrant { pending } = ask(4) else {
        panic!("the task's dispatch is held for the person");
    };
    store
        .allow(
            &Ground {
                root: project.path(),
                locks: &locks,
                is_open: &|_| true,
                sandboxed: &|_| true,
                audit: &|_, _| Ok(()),
                at: 101,
            },
            pending,
            Level::Chat,
        )
        .expect("allowed for the task");
    assert!(
        matches!(ask(4), Requested::Covered(_)),
        "the task is covered"
    );
    assert!(
        matches!(ask(1), Requested::NeedsGrant { .. }),
        "its session is asked"
    );
    assert!(
        matches!(ask(5), Requested::NeedsGrant { .. }),
        "its sibling is asked"
    );
}
