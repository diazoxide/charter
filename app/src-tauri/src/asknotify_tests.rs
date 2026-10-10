//! When an ask raises a system notification (#1694, I-7): only asks, one per chat for asks a
//! few seconds apart, none while the person is looking, and what a click lands on.

use std::path::Path;
use std::time::{Duration, Instant};

use super::*;
use crate::asking::{AnswerPath, AskSource, Shown};

fn plane(root: &str) -> PlaneId {
    PlaneId::for_tests(Path::new(root))
}

fn ask(session: u32, key: &str, says: &str, source: AskSource) -> Shown {
    Shown {
        session,
        ask: key.to_owned(),
        says: says.to_owned(),
        options: Vec::new(),
        source,
        chain: vec!["steward 12".to_owned(), format!("chat {session}")],
        answer: AnswerPath::InItsPane,
    }
}

fn permission(session: u32, key: &str, says: &str) -> Shown {
    ask(session, key, says, AskSource::Permission)
}

const NONE_ARE_UPDATES: &dyn Fn(&Shown) -> bool = &|_| false;
const NOBODY_LOOKS: &dyn Fn(u32) -> bool = &|_| false;

/// What `rules` sends for `asks` of `plane`, heard at `now`, with nobody looking.
fn heard(rules: &mut Rules, plane: &PlaneId, asks: &[Shown], now: Instant) -> Vec<Notice> {
    rules.heard(&Heard {
        plane,
        asks,
        update: NONE_ARE_UPDATES,
        looking: NOBODY_LOOKS,
        now,
    })
}

#[test]
fn a_new_ask_is_one_notification_titled_by_who_asks_and_saying_what_kind() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let sent = heard(
        &mut rules,
        &a,
        &[permission(3, "p1", "Run cargo test")],
        Instant::now(),
    );
    assert_eq!(
        sent,
        vec![Notice {
            plane: a,
            session: 3,
            title: "steward 12 › chat 3".to_owned(),
            body: "Asks your permission".to_owned(),
        }]
    );
}

#[test]
fn an_ask_already_told_is_not_told_again_however_often_the_list_is_read() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let start = Instant::now();
    let asks = [permission(3, "p1", "Run cargo test")];
    assert_eq!(heard(&mut rules, &a, &asks, start).len(), 1);
    let later = start + TOGETHER * 3;
    assert!(heard(&mut rules, &a, &asks, later).is_empty());
}

#[test]
fn asks_of_one_chat_a_few_seconds_apart_share_one_notification() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let start = Instant::now();
    // Two in one read: one notification, which says there is more than one.
    let two = [
        permission(3, "p1", "Run cargo test"),
        permission(3, "p2", "Change src/lib.rs"),
    ];
    let sent = heard(&mut rules, &a, &two, start);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body, "Asks your permission, and 1 more");
    // A third two seconds on, and a fourth two seconds after that: each rides the first.
    let three = [two.to_vec(), vec![permission(3, "p3", "Run ls")]].concat();
    let soon = start + Duration::from_secs(2);
    assert!(heard(&mut rules, &a, &three, soon).is_empty());
    let four = [three.clone(), vec![permission(3, "p4", "Run pwd")]].concat();
    let sooner = soon + Duration::from_secs(2);
    assert!(heard(&mut rules, &a, &four, sooner).is_empty());
    // Once the chat has been quiet past the window, its next ask is a notification again.
    let five = [four.clone(), vec![permission(3, "p5", "Run make")]].concat();
    let sent = heard(&mut rules, &a, &five, sooner + TOGETHER);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body, "Asks your permission");
}

#[test]
fn asks_of_two_chats_are_two_notifications() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let sent = heard(
        &mut rules,
        &a,
        &[
            permission(3, "p1", "Run ls"),
            permission(4, "p2", "Run pwd"),
        ],
        Instant::now(),
    );
    let chats: Vec<u32> = sent.iter().map(|notice| notice.session).collect();
    assert_eq!(chats, vec![3, 4]);
}

#[test]
fn the_same_chat_number_in_two_projects_is_two_chats() {
    let mut rules = Rules::default();
    let now = Instant::now();
    assert_eq!(
        heard(
            &mut rules,
            &plane("/a"),
            &[permission(3, "p1", "Run ls")],
            now
        )
        .len(),
        1
    );
    assert_eq!(
        heard(
            &mut rules,
            &plane("/b"),
            &[permission(3, "p1", "Run ls")],
            now
        )
        .len(),
        1
    );
}

#[test]
fn an_update_never_notifies() {
    let mut rules = Rules::default();
    let a = plane("/a");
    // A chat in the queue only because a task of its failed: no reply is asked of anyone.
    let failed = ask(
        5,
        "question:5",
        "Waiting on your reply",
        AskSource::Question,
    );
    let sent = rules.heard(&Heard {
        plane: &a,
        asks: &[failed],
        update: &|ask| an_update(ask, 1),
        looking: NOBODY_LOOKS,
        now: Instant::now(),
    });
    assert!(sent.is_empty());
}

#[test]
fn a_question_is_an_update_only_where_the_app_says_why_the_chat_waits() {
    let question = ask(
        5,
        "question:5",
        "Waiting on your reply",
        AskSource::Question,
    );
    assert!(!an_update(&question, 0), "a turn that ended on the person");
    assert!(an_update(&question, 1), "a failed task, a refused commit");
    let held = permission(5, "p1", "Run ls");
    assert!(
        !an_update(&held, 2),
        "a decision is an ask whatever else the chat waits for"
    );
    let terminal = ask(5, "terminal:5", "Permission", AskSource::Terminal);
    assert!(!an_update(&terminal, 1));
}

#[test]
fn nothing_is_sent_while_the_person_is_looking_and_looking_away_does_not_hold_the_next_one() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let start = Instant::now();
    let first = [permission(3, "p1", "Run ls")];
    let sent = rules.heard(&Heard {
        plane: &a,
        asks: &first,
        update: NONE_ARE_UPDATES,
        looking: &|_| true,
        now: start,
    });
    assert!(sent.is_empty());
    // Seen where it was asked: it is not sent later either.
    assert!(heard(&mut rules, &a, &first, start + Duration::from_secs(1)).is_empty());
    // The person looked away: the chat's next ask, a second on, is sent.
    let second = [first.to_vec(), vec![permission(3, "p2", "Run pwd")]].concat();
    let sent = heard(&mut rules, &a, &second, start + Duration::from_secs(2));
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body, "Asks your permission");
}

#[test]
fn an_ask_answered_and_asked_again_later_is_sent_again() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let start = Instant::now();
    let waiting = [ask(
        3,
        "question:3",
        "Waiting on your reply",
        AskSource::Question,
    )];
    assert_eq!(heard(&mut rules, &a, &waiting, start).len(), 1);
    // Replied to: it leaves the list.
    assert!(heard(&mut rules, &a, &[], start + Duration::from_secs(1)).is_empty());
    // Its next turn ends on the person, past the window.
    let again = start + Duration::from_secs(1) + TOGETHER;
    assert_eq!(heard(&mut rules, &a, &waiting, again).len(), 1);
}

#[test]
fn a_project_let_go_of_is_forgotten() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let asks = [permission(3, "p1", "Run ls")];
    let now = Instant::now();
    assert_eq!(heard(&mut rules, &a, &asks, now).len(), 1);
    rules.forget(&a);
    assert_eq!(heard(&mut rules, &a, &asks, now + TOGETHER).len(), 1);
}

#[test]
fn a_chat_without_names_is_titled_by_its_number() {
    let mut rules = Rules::default();
    let mut nameless = permission(7, "p1", "Run ls");
    nameless.chain.clear();
    let sent = heard(&mut rules, &plane("/a"), &[nameless], Instant::now());
    assert_eq!(sent[0].title, "chat 7");
}

#[test]
fn the_person_is_looking_only_from_the_window_in_front_at_the_inbox_or_the_chat() {
    let in_front = Looking {
        window_in_front: true,
        inbox_open: true,
        chat_on_screen: false,
    };
    assert!(
        in_front.is_looking(),
        "the Inbox open in the focused window"
    );
    assert!(
        Looking {
            inbox_open: false,
            chat_on_screen: true,
            ..in_front
        }
        .is_looking(),
        "the chat itself on screen"
    );
    assert!(
        !Looking {
            inbox_open: false,
            ..in_front
        }
        .is_looking(),
        "focused, but on something else"
    );
    assert!(
        !Looking {
            window_in_front: false,
            ..in_front
        }
        .is_looking(),
        "the Inbox open in a window behind another app"
    );
}

#[test]
fn the_inbox_is_open_where_its_window_said_and_until_it_says_otherwise() {
    let open = InboxOpen::default();
    let a = plane("/a");
    assert!(!open.is_open("main", &a), "a window that never said");
    open.shown("main", &a, true);
    assert!(open.is_open("main", &a));
    assert!(
        !open.is_open("main", &plane("/b")),
        "another project's Inbox"
    );
    assert!(!open.is_open("split-1", &a), "another window");
    open.shown("main", &a, false);
    assert!(!open.is_open("main", &a));
    open.shown("main", &a, true);
    open.forget_window("main");
    assert!(!open.is_open("main", &a), "a window gone");
}

#[test]
fn a_chat_that_asks_anew_is_sent_again_though_its_ask_reads_the_same() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let start = Instant::now();
    let held = permission(3, "p1", "Run ls");
    let waiting = ask(
        3,
        "question:3",
        "Waiting on your reply",
        AskSource::Question,
    );
    let both = [held.clone(), waiting.clone()];
    assert_eq!(heard(&mut rules, &a, &both, start).len(), 1);
    // Replied to and ended on the person again between two reads: the board says so.
    rules.renew(&a, 3);
    let later = start + TOGETHER * 2;
    let sent = heard(&mut rules, &a, &both, later);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].body, "Waiting on your reply",
        "the held permission prompt is the same ask, and is not sent again"
    );
    // Another chat's renewal sends nothing about this one.
    rules.renew(&a, 4);
    assert!(heard(&mut rules, &a, &both, later + TOGETHER * 2).is_empty());
}

#[test]
fn a_notification_says_what_kind_of_ask_and_never_what_it_asks() {
    let mut rules = Rules::default();
    let a = plane("/a");
    let now = Instant::now();
    let said = |ask: Shown| {
        heard(&mut Rules::default(), &a, &[ask], now)[0]
            .body
            .clone()
    };
    let host = ask(
        4,
        "block:4:connect:host:internal.example",
        "The sandbox refused internal.example",
        AskSource::SandboxHost,
    );
    assert_eq!(said(host), "Asks to reach a host");
    let dispatch = ask(
        4,
        "dispatch:7",
        "Wants to hand a task to devops",
        AskSource::Dispatch,
    );
    assert_eq!(said(dispatch), "Asks to hand a task on");
    let terminal = ask(
        4,
        "terminal:4",
        "Waiting in its terminal",
        AskSource::Terminal,
    );
    assert_eq!(said(terminal), "Waiting in its terminal");
    // A command line, a host, a path stay in the window, never on a lock screen.
    let sent = heard(
        &mut rules,
        &a,
        &[permission(3, "p1", "Run deploy --to prod.internal")],
        now,
    );
    assert!(!sent[0].body.contains("prod.internal"));
}

#[test]
fn a_task_that_failed_never_makes_a_chat_s_own_wait_an_update() {
    use crate::hooks::{HowFailed, Need};
    let failed = Need::TaskFailed {
        id: "d1".to_owned(),
        chat: None,
        task: "drill".to_owned(),
        how: HowFailed::Failed,
        why: "it said it was blocked".to_owned(),
    };
    let undelivered = Need::ReportUndelivered {
        asker: "steward 12".to_owned(),
    };
    // A failure left unlooked-at beside a turn that ended on the person: still an ask.
    assert_eq!(
        reasons_besides_failures(Some(std::slice::from_ref(&failed)), 0),
        0
    );
    assert_eq!(reasons_besides_failures(None, 0), 0);
    // A report with nowhere to go, or a refused commit: the app's reasons, an update.
    assert_eq!(
        reasons_besides_failures(Some(&[failed.clone(), undelivered]), 0),
        1
    );
    assert_eq!(reasons_besides_failures(Some(&[failed]), 2), 2);
}
