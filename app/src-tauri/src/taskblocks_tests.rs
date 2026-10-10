//! One answer for the tasks of one session that hit the same block (#1508): who it covers,
//! that it is bound to the blocks the app holds, what it keeps and where, and that it covers no
//! chat it did not list. Driven at [`answered`] and [`checked`] with a pretend app, so no chat
//! is started; the tests at the end start real chats, which needs a terminal.

use std::cell::RefCell;
use std::collections::HashMap;

use super::*;
use purlis_core::sandbox::hosts::Host;

const NPM: &str = "registry.npmjs.org";

/// steward 1 asked for talk (4), sweep (5) and probe (6); talk asked for deep (7). steward 2
/// (2) is another session, and its task is 8.
fn asker_of(chat: u32) -> Option<u32> {
    HashMap::from([(4, 1), (5, 1), (6, 1), (7, 4), (8, 2)])
        .get(&chat)
        .copied()
}

fn host() -> What {
    What::Host(Host::parse(NPM).expect("a host"))
}

/// Task `task` seen blocked on reaching `target`.
fn seen(task: u32, target: &str) -> SeenBlock {
    SeenBlock {
        task,
        operation: "connect".to_owned(),
        kind: "host".to_owned(),
        what: GrantWhat::Host,
        target: target.to_owned(),
    }
}

fn on_npm(tasks: &[u32]) -> Vec<SeenBlock> {
    tasks.iter().map(|task| seen(*task, NPM)).collect()
}

/// The block `seen` names, as the app holds it.
fn held(one: &SeenBlock) -> HeldBlock {
    HeldBlock {
        operation: one.operation.clone(),
        kind: one.kind.clone(),
        what: one.what,
        target: normalised(one.what, &one.target).expect("named"),
    }
}

/// What a pretend app holds, and was asked to keep, audit and owe.
#[derive(Default)]
struct App {
    blocks: Blocks,
    kept: RefCell<Vec<(u32, String, &'static str)>>,
    noted: RefCell<Vec<u32>>,
    owed: RefCell<Vec<(u32, String)>>,
}

impl App {
    /// An app holding each of `tasks` on reaching the npm registry.
    fn blocked(tasks: &[u32]) -> Self {
        let app = Self::default();
        for one in on_npm(tasks) {
            app.blocks.heard(one.task, held(&one));
        }
        app
    }

    fn kept(&self) -> Vec<(u32, String, &'static str)> {
        self.kept.borrow().clone()
    }

    fn owed(&self) -> Vec<u32> {
        self.owed.borrow().iter().map(|(task, _)| *task).collect()
    }

    fn check(&self, session: u32, seen: &[SeenBlock]) -> Result<(Vec<u32>, HeldBlock), String> {
        checked(
            session,
            seen,
            &Reading {
                asker_of: &asker_of,
                holds: &|task, block| self.blocks.holds(task, block),
            },
        )
    }

    /// Answers `seen` of `session` at `level`, every task judged to `judge`, keeping refused
    /// for the tasks of `failing`.
    fn answer(
        &self,
        session: u32,
        seen: &[SeenBlock],
        level: GrantLevel,
        judge: Judge<'_>,
        failing: &[u32],
    ) -> Result<TasksAnswered, String> {
        let mut keep = |task: u32, what: &What, level: Level| {
            if failing.contains(&task) {
                return Err("the event log is not open".to_owned());
            }
            self.kept
                .borrow_mut()
                .push((task, what.target(), level.word()));
            Ok(())
        };
        answered(
            session,
            seen,
            level,
            &Reading {
                asker_of: &asker_of,
                holds: &|task, block| self.blocks.holds(task, block),
            },
            Doing {
                judge,
                keep: &mut keep,
                note: &|task, _, _| {
                    self.noted.borrow_mut().push(task);
                    Ok(())
                },
                owe: &|task, told| self.owed.borrow_mut().push((task, told)),
            },
        )
    }
}

fn to_host(level: Level) -> impl Fn(u32, &HeldBlock) -> Result<(What, Level), String> {
    move |_, _| Ok((host(), level))
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
    let looping = |chat: u32| match chat {
        10 => Some(11),
        11 => Some(10),
        _ => None,
    };
    assert!(!below(&looping, 1, 10));
}

#[test]
fn a_host_is_matched_as_a_grant_matches_it() {
    let host = |typed| normalised(GrantWhat::Host, typed);
    assert_eq!(host("Registry.NPMjs.org.").as_deref(), Some(NPM));
    assert_eq!(host("registry.npmjs.org:443").as_deref(), Some(NPM));
    assert_eq!(host("registry.npmjs.org:80").as_deref(), Some(NPM));
    assert_eq!(
        host("registry.npmjs.org:8443").as_deref(),
        Some("registry.npmjs.org:8443")
    );
    assert_eq!(host("not a host"), None);
    assert_eq!(normalised(GrantWhat::Vault, "prod"), None);
}

#[test]
fn the_app_holds_one_block_per_operation_and_kind_and_each_host_until_it_is_answered_or_the_chat_ends()
 {
    let blocks = Blocks::default();
    let npm = held(&seen(4, NPM));
    let other = held(&seen(4, "api.example.com"));
    blocks.heard(4, npm.clone());
    assert!(blocks.holds(4, &npm));
    // Another host refused beside it is asked about with it (#1637): both are held, so the
    // one Notice's Allow can answer each.
    blocks.heard(4, other.clone());
    assert!(blocks.holds(4, &npm));
    assert!(blocks.holds(4, &other));
    blocks.answered(4, &other);
    assert!(!blocks.holds(4, &other));
    assert!(
        blocks.holds(4, &npm),
        "answering one host leaves the other up"
    );
    // A newer folder of the same operation and kind is the one the Notice shows now.
    let write = |folder: &str| HeldBlock {
        operation: "write".to_owned(),
        kind: "home".to_owned(),
        what: GrantWhat::Write,
        target: folder.to_owned(),
    };
    blocks.heard(4, write("/Users/dev/a"));
    blocks.heard(4, write("/Users/dev/b"));
    assert!(!blocks.holds(4, &write("/Users/dev/a")));
    assert!(blocks.holds(4, &write("/Users/dev/b")));
    // A host the report did not name stands beside the named ones: neither replaces the other.
    let unnamed = HeldBlock::unnamed_host("connect", "host");
    blocks.heard(4, unnamed.clone());
    assert!(blocks.holds(4, &unnamed) && blocks.holds(4, &npm));
    blocks.heard(6, npm.clone());
    blocks.ended(6);
    assert!(!blocks.holds(6, &npm));
}

/// The app holds as much as the window shows (#1637): at most [`MOST_HOSTS_PER_BLOCK`] hosts of
/// one block, the oldest going first, and at most [`MOST_HELD_PER_CHAT`] blocks, however many
/// hosts each names.
#[test]
fn the_hosts_and_blocks_held_for_one_chat_are_bounded_oldest_first() {
    let blocks = Blocks::default();
    let named: Vec<HeldBlock> = (0..=MOST_HOSTS_PER_BLOCK)
        .map(|at| held(&seen(4, &format!("h{at}.example.com"))))
        .collect();
    for one in &named {
        blocks.heard(4, one.clone());
    }
    assert!(!blocks.holds(4, &named[0]), "the oldest host went first");
    assert!(named[1..].iter().all(|one| blocks.holds(4, one)));
    // Other blocks push out the oldest block, all of its hosts at once.
    for kind in ["home", "system", "toolchain-cache", "project-files", "temp"] {
        blocks.heard(
            4,
            HeldBlock {
                operation: "write".to_owned(),
                kind: kind.to_owned(),
                what: GrantWhat::Write,
                target: format!("/x/{kind}"),
            },
        );
    }
    assert!(named.iter().all(|one| !blocks.holds(4, one)));
}

/// What one chat's own Notice sends for its block (#1538): chat 4, its block's operation and
/// kind, and what Allow names.
fn one(
    blocks: &Blocks,
    chat: u32,
    (operation, kind): (&str, &str),
    target: &str,
) -> Result<HeldBlock, String> {
    shown_one(
        blocks,
        chat,
        &shown(operation, kind, GrantWhat::Host, target),
    )
}

fn shown(operation: &str, kind: &str, what: GrantWhat, target: &str) -> BlockShown {
    BlockShown {
        operation: operation.to_owned(),
        kind: kind.to_owned(),
        what,
        target: target.to_owned(),
    }
}

const CONNECT: (&str, &str) = ("connect", "host");

#[test]
fn one_chats_allow_is_refused_unless_it_is_held_on_the_block_its_notice_showed() {
    let blocks = Blocks::default();
    let npm = held(&seen(4, NPM));
    // Never blocked: nothing to answer.
    let never = one(&blocks, 4, CONNECT, NPM).expect_err("nothing held");
    assert!(never.starts_with("Nothing was allowed"), "{never}");
    blocks.heard(4, npm.clone());
    // As a grant matches it, whatever spelling the Notice showed.
    assert_eq!(
        one(&blocks, 4, CONNECT, "REGISTRY.npmjs.org.:443"),
        Ok(npm.clone())
    );
    // Another host, another chat, or another block of the same host is not what was shown.
    assert!(one(&blocks, 4, CONNECT, "api.example.com").is_err());
    assert!(one(&blocks, 5, CONNECT, NPM).is_err(), "another chat");
    assert!(
        one(&blocks, 4, ("connect", "proxy"), NPM).is_err(),
        "another kind"
    );
    assert!(
        shown_one(&blocks, 4, &shown("connect", "host", GrantWhat::Write, NPM)).is_err(),
        "another offer"
    );
    // Answered, it is held no longer: a second Allow for it answers nothing.
    blocks.answered(4, &npm);
    assert!(one(&blocks, 4, CONNECT, NPM).is_err(), "answered once");
}

#[test]
fn a_host_the_block_did_not_name_is_the_one_the_person_types_and_no_question_for_tasks_takes_it() {
    let blocks = Blocks::default();
    let unnamed = HeldBlock::unnamed_host("connect", "host");
    blocks.heard(4, unnamed.clone());
    // The person types the host on the chat's own Notice; the core judges it as any host.
    assert_eq!(
        one(&blocks, 4, CONNECT, "api.example.com"),
        Ok(unnamed.clone())
    );
    assert!(
        one(&blocks, 5, CONNECT, "api.example.com").is_err(),
        "another chat"
    );
    // A question for several tasks names one host for each: none matches an unnamed block.
    let reading = Reading {
        asker_of: &asker_of,
        holds: &|task, block| blocks.holds(task, block),
    };
    assert!(checked(1, &[seen(4, "api.example.com")], &reading).is_err());
    assert!(checked(1, &[seen(4, "")], &reading).is_err());
    // A block that names its host is never answered with another typed in its place.
    let named = Blocks::default();
    named.heard(4, held(&seen(4, NPM)));
    assert!(one(&named, 4, CONNECT, "api.example.com").is_err());
}

#[test]
fn three_tasks_blocked_on_one_host_are_each_allowed_on_their_own_by_one_answer() {
    let app = App::blocked(&[4, 5, 7]);
    let allowed = app
        .answer(
            1,
            &on_npm(&[4, 5, 7]),
            GrantLevel::Chat,
            &to_host(Level::Chat),
            &[],
        )
        .expect("allowed");
    assert_eq!(
        app.kept(),
        [4, 5, 7].map(|task| (task, NPM.to_owned(), "chat"))
    );
    assert_eq!(allowed.answered, [4, 5, 7]);
    assert!(
        allowed.said.starts_with(
            "Allowed for each of these 3 tasks on its own, not for the chat that asked them."
        ),
        "{}",
        allowed.said
    );
}

#[test]
fn an_answer_for_everyone_is_kept_once_and_audited_and_owed_for_every_listed_task() {
    let app = App::blocked(&[4, 5, 6]);
    let allowed = app
        .answer(
            1,
            &on_npm(&[4, 5, 6]),
            GrantLevel::You,
            &to_host(Level::You),
            &[],
        )
        .expect("allowed");
    assert_eq!(app.kept(), [(4, NPM.to_owned(), "you")], "one grant");
    assert_eq!(
        *app.noted.borrow(),
        [5, 6],
        "an audit line for each other task"
    );
    assert_eq!(app.owed(), [5, 6]);
    assert_eq!(allowed.answered, [4, 5, 6]);
    assert!(
        allowed
            .said
            .starts_with("Allowed for me on this machine. The 3 tasks restart"),
        "{}",
        allowed.said
    );
}

#[test]
fn an_answer_is_refused_whole_unless_every_task_is_held_on_the_block_it_showed() {
    // D-1508-9: sweep (5) was never blocked, or has moved on to another host.
    for app in [App::blocked(&[4, 6]), {
        let app = App::blocked(&[4, 6]);
        app.blocks.heard(5, held(&seen(5, "api.example.com")));
        app
    }] {
        let refused = app
            .answer(
                1,
                &on_npm(&[4, 5, 6]),
                GrantLevel::Chat,
                &to_host(Level::Chat),
                &[],
            )
            .expect_err("refused");
        assert!(refused.contains("chat 5 is not blocked"), "{refused}");
        assert!(app.kept().is_empty());
        assert!(app.owed().is_empty());
    }
    // Answered already: the same press again grants nothing.
    let app = App::blocked(&[4, 5]);
    app.blocks.answered(5, &held(&seen(5, NPM)));
    assert!(app.check(1, &on_npm(&[4, 5])).is_err());
}

#[test]
fn a_press_meant_for_one_task_never_grants_a_second() {
    // A task is answered only on a block the app holds for it, named as it holds it.
    let app = App::blocked(&[4]);
    assert!(
        app.check(1, &on_npm(&[4, 5])).is_err(),
        "sweep was not shown blocked"
    );
    let mixed = [seen(4, NPM), seen(5, "api.example.com")];
    let app = App::blocked(&[4, 5]);
    let refused = app.check(1, &mixed).expect_err("refused");
    assert!(refused.contains("not on one block"), "{refused}");
    // The same host spelled as a grant would match it is one block.
    let spelled = [seen(4, NPM), seen(5, "Registry.npmjs.org.:443")];
    assert_eq!(app.check(1, &spelled).expect("one block").0, [4, 5]);
}

#[test]
fn a_listed_chat_purlis_has_no_record_of_as_a_task_of_the_session_allows_nothing_for_any() {
    for tasks in [
        &[4, 8][..],
        &[1, 4][..],
        &[4, 2][..],
        &[4, 99][..],
        &[4, 4][..],
        &[][..],
    ] {
        let app = App::blocked(&[1, 2, 4, 8, 99]);
        let refused = app
            .answer(
                1,
                &on_npm(tasks),
                GrantLevel::Chat,
                &to_host(Level::Chat),
                &[],
            )
            .expect_err("refused");
        assert!(
            refused.starts_with("Nothing was answered"),
            "{tasks:?}: {refused}"
        );
        assert!(app.kept().is_empty(), "{tasks:?}");
        assert!(app.owed().is_empty(), "{tasks:?}");
    }
}

#[test]
fn a_task_that_cannot_be_allowed_allows_none_of_them() {
    let app = App::blocked(&[4, 5, 6]);
    let judge = |task: u32, _: &HeldBlock| {
        if task == 5 {
            Err("its folder is not on the allowlist".to_owned())
        } else {
            Ok((host(), Level::Chat))
        }
    };
    let refused = app
        .answer(1, &on_npm(&[4, 5, 6]), GrantLevel::Chat, &judge, &[])
        .expect_err("refused");
    assert_eq!(
        refused,
        "Nothing was allowed: for chat 5, its folder is not on the allowlist"
    );
    assert!(app.kept().is_empty());
}

#[test]
fn tasks_judged_to_different_grants_are_not_one_answer_for_everyone() {
    let app = App::blocked(&[4, 5]);
    let judge =
        |task: u32, _: &HeldBlock| Ok((What::Write(format!("/work/{task}").into()), Level::You));
    let refused = app
        .answer(1, &on_npm(&[4, 5]), GrantLevel::You, &judge, &[])
        .expect_err("refused");
    assert!(refused.contains("do not want the same thing"), "{refused}");
    assert!(app.kept().is_empty());
    assert!(app.owed().is_empty());
}

#[test]
fn a_keep_that_fails_part_way_answers_the_tasks_it_allowed_and_no_other() {
    let app = App::blocked(&[4, 5, 6]);
    let done = app
        .answer(
            1,
            &on_npm(&[4, 5, 6]),
            GrantLevel::Chat,
            &to_host(Level::Chat),
            &[6],
        )
        .expect("answered in part");
    assert_eq!(done.answered, [4, 5]);
    assert_eq!(
        done.said,
        "It was allowed for chat 4 and 5 only, and not for the rest: the event log is not open"
    );
    // Failing at the first allows nothing.
    let app = App::blocked(&[4, 5]);
    assert!(
        app.answer(
            1,
            &on_npm(&[4, 5]),
            GrantLevel::Chat,
            &to_host(Level::Chat),
            &[4]
        )
        .expect_err("refused")
        .starts_with("Nothing was allowed")
    );
}

/// V100-56: a task asking for another persona with no grant is held on its own record, and
/// "Allow for this chat" on its question covers that task alone: its session and its sibling,
/// running as the same persona, are still asked.
#[test]
fn allowing_a_tasks_dispatch_for_this_chat_allows_nothing_for_its_session_or_siblings() {
    use crate::dispatchgrants::{Asking, Ground, Requested, Store, Uncovered};
    let project = tempfile::tempdir().expect("a project");
    let locks = purlis_core::sandbox::policy::Locks::none();
    let ground = |at| Ground {
        root: project.path(),
        locks: &locks,
        is_open: &|_| true,
        sandboxed: &|_| true,
        audit: &|_, _| Ok(()),
        at,
    };
    let as_chat = |session: u32| Asking {
        session,
        id: Some(format!("chat-{session}")),
        name: format!("chat {session}"),
        persona: Some("steward".to_owned()),
        held: false,
        above: purlis_core::dispatchchain::Above::Known(Vec::new()),
    };
    let store = Store::default();
    let ask = |session: u32| {
        store
            .request(
                &ground(100),
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
        .allow(&ground(101), pending, Level::Chat)
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

// ---- real chats: these need a terminal ------------------------------------------------------

/// A session (steward 1), and `names` started as its tasks, on `chats`, each in its own folder
/// under `dir`.
fn started(chats: &crate::chats::Chats, dir: &std::path::Path, names: &[&str]) -> (u32, Vec<u32>) {
    use purlis_core::engine::Size;
    use purlis_core::reopen::{Chat, HandedFrom, Mode, Owed};
    let size = Size {
        columns: 80,
        rows: 24,
    };
    let plain = Chat {
        program: "/bin/sleep".to_owned(),
        args: vec!["5".to_owned()],
        name: "steward 1".to_owned(),
        cwd: Some(dir.to_path_buf()),
        ..Default::default()
    };
    let session = chats.start(&plain, size).expect("started");
    let tasks = names
        .iter()
        .map(|name| {
            let folder = dir.join(name);
            std::fs::create_dir_all(&folder).expect("a folder");
            let task = Chat {
                name: (*name).to_owned(),
                cwd: Some(folder),
                from: Some(HandedFrom {
                    chat: session,
                    name: "steward 1".to_owned(),
                    workspace: purlis_core::active::Place::Workspace("alpha".to_owned()),
                    report: Owed::Due,
                    mode: Mode::Task,
                    depth: 1,
                    root: None,
                    above: None,
                    by_person: false,
                }),
                ..plain.clone()
            };
            chats.start(&task, size).expect("started")
        })
        .collect();
    (session, tasks)
}

fn id_of(chats: &crate::chats::Chats, n: u32) -> String {
    chats
        .recorded_chat(n)
        .and_then(|chat| chat.identity.id)
        .expect("an id")
}

/// Needs a terminal. A session's own grant does not reach its tasks, those running and one
/// started after it; one made by this answer for a task reaches neither the session nor the
/// task's sibling (V100-57).
#[test]
fn a_grant_for_a_session_reaches_none_of_its_tasks_and_one_for_a_task_reaches_no_other_chat() {
    let dir = tempfile::tempdir().expect("a project");
    let chats = crate::chats::Chats::new();
    let (session, tasks) = started(&chats, dir.path(), &["talk"]);
    let talk = tasks[0];
    let other = What::Host(Host::parse("api.example.com").expect("a host"));
    chats
        .grant(session, other.clone(), 1, "told".to_owned())
        .expect("granted to the session");
    // A task started after the session's grant starts with none of it.
    let (_, later) = started(&chats, dir.path(), &["sweep"]);
    let sweep = later[0];
    assert!(!chats.holds(&id_of(&chats, talk), &other));
    assert!(!chats.holds(&id_of(&chats, sweep), &other));

    let block = seen(talk, NPM);
    chats.blocks().heard(talk, held(&block));
    allow_for_tasks(
        dir.path(),
        &purlis_core::sandbox::Machine::this(),
        &chats,
        (session, &[block], GrantLevel::Chat),
        &|_, _| Ok(()),
        2,
    )
    .expect("allowed");
    assert!(chats.holds(&id_of(&chats, talk), &host()));
    assert!(
        !chats.holds(&id_of(&chats, session), &host()),
        "not the session"
    );
    assert!(
        !chats.holds(&id_of(&chats, sweep), &host()),
        "not a sibling"
    );
    assert!(
        !chats.blocks().holds(talk, &held(&seen(talk, NPM))),
        "answered"
    );
    for chat in [sweep, talk, session] {
        chats.close(chat).ok();
    }
}

/// Needs a terminal. The command's own judging and keeping: a write for each task at "this
/// chat" is judged against that task's own folder, and a folder outside one task's allowlist
/// refuses the whole answer.
#[test]
fn a_write_for_this_chat_is_judged_against_each_tasks_own_folder() {
    let dir = tempfile::tempdir().expect("a project");
    let chats = crate::chats::Chats::new();
    let (session, tasks) = started(&chats, dir.path(), &["talk", "sweep"]);
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).expect("a folder");
    let target = out.display().to_string();
    let seen: Vec<SeenBlock> = tasks
        .iter()
        .map(|task| SeenBlock {
            task: *task,
            operation: "write".to_owned(),
            kind: "project-files".to_owned(),
            what: GrantWhat::Write,
            target: target.clone(),
        })
        .collect();
    for one in &seen {
        chats.blocks().heard(one.task, held(one));
    }
    let audited = RefCell::new(Vec::new());
    let done = allow_for_tasks(
        dir.path(),
        &purlis_core::sandbox::Machine::this(),
        &chats,
        (session, &seen, GrantLevel::Chat),
        &|number, _| {
            audited.borrow_mut().push(number);
            Ok(())
        },
        3,
    )
    .expect("allowed");
    assert_eq!(done.answered, tasks);
    assert_eq!(
        *audited.borrow(),
        tasks.iter().map(|t| Some(*t)).collect::<Vec<_>>()
    );
    assert_eq!(chats.owed_restarts().len(), 2);
    for chat in tasks.iter().chain([&session]) {
        chats.close(*chat).ok();
    }
}

/// Needs a terminal. Keep blocked is checked as an Allow is, and lets the blocks go.
#[test]
fn keep_blocked_is_checked_against_the_held_blocks_and_lets_them_go() {
    let dir = tempfile::tempdir().expect("a project");
    let chats = crate::chats::Chats::new();
    let (session, tasks) = started(&chats, dir.path(), &["talk", "sweep"]);
    let seen = on_npm(&tasks);
    assert!(
        keep_blocked(&chats, session, &seen).is_err(),
        "nothing held"
    );
    for one in &seen {
        chats.blocks().heard(one.task, held(one));
    }
    assert_eq!(keep_blocked(&chats, session, &seen).expect("kept"), tasks);
    assert!(
        keep_blocked(&chats, session, &seen).is_err(),
        "answered once"
    );
    for chat in tasks.iter().chain([&session]) {
        chats.close(*chat).ok();
    }
}
