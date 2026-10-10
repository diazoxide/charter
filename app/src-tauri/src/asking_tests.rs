//! The asks registry's window side (#1690): one adapter per source, what each derives, the
//! path each is answered by, and the list as a whole.

use std::path::Path;
use std::time::Instant;

use purlis_core::harness::asks::{Admitted, Answerer, Asks};
use purlis_core::harness::model::{Ask, Channel, Choice, ChoiceKind, ChoiceScope};
use purlis_core::sandbox::policy::Locks;

use super::*;
use crate::dispatchgrants::DispatchPending;
use crate::sandboxing::{GrantLevel, GrantWhat};
use crate::taskblocks::{BlockShown, HeldBlock};

fn choice(id: &str, kind: ChoiceKind, scope: ChoiceScope) -> Choice {
    Choice {
        id: id.to_owned(),
        label: id.to_owned(),
        kind,
        scope,
    }
}

/// A Claude Code hook's ask, with allow and deny for this call.
fn a_hook_ask(summary: &str) -> Ask {
    Ask {
        options: vec![
            choice("allow", ChoiceKind::Allow, ChoiceScope::Once),
            choice("deny", ChoiceKind::Reject, ChoiceScope::Once),
        ],
        summary: purlis_core::harness::model::Summary::of(summary),
        channel: Channel::Hook,
        ..Ask::default()
    }
}

/// Chat `session`'s dispatch `id` to `target`, held for the person, offering `levels`.
fn a_dispatch(id: u32, session: u32, target: &str, levels: Vec<GrantLevel>) -> DispatchPending {
    DispatchPending {
        plane: PlaneId::for_tests(Path::new("/p/one")),
        id,
        session,
        chat: format!("chat {session}"),
        asking: Some("steward".to_owned()),
        target: target.to_owned(),
        brief: "Look at the logs".to_owned(),
        brief_cut: false,
        brief_lines: 1,
        levels,
        locked: None,
        never_unread: None,
        works_in: None,
        works_in_missing: false,
        allowed_in: Vec::new(),
        works_with: String::new(),
        also: Vec::new(),
        shown: "digest-1".to_owned(),
        task: None,
        task_cut: false,
        profile: None,
    }
}

fn a_host_block(target: &str) -> HeldBlock {
    HeldBlock {
        operation: "connect".to_owned(),
        kind: "host".to_owned(),
        what: GrantWhat::Host,
        target: target.to_owned(),
    }
}

fn ids(shown: &Shown) -> Vec<&str> {
    shown.options.iter().map(|one| one.id.as_str()).collect()
}

/// What one project has waiting, with nothing yet: each test fills in its sources.
struct World {
    permissions: Vec<purlis_core::harness::asks::Raised>,
    queue: Vec<u32>,
    at_prompt: Vec<u32>,
    dispatches: Vec<DispatchPending>,
    blocks: Vec<(u32, HeldBlock)>,
    locks: Locks,
    names: Vec<(u32, &'static str)>,
    askers: Vec<(u32, u32)>,
}

impl World {
    fn new() -> Self {
        Self {
            permissions: Vec::new(),
            queue: Vec::new(),
            at_prompt: Vec::new(),
            dispatches: Vec::new(),
            blocks: Vec::new(),
            locks: Locks::none(),
            names: Vec::new(),
            askers: Vec::new(),
        }
    }

    fn asks(&self) -> Vec<Shown> {
        let at_prompt = |session: u32| self.at_prompt.contains(&session);
        let name_of = |session: u32| {
            self.names
                .iter()
                .find(|(one, _)| *one == session)
                .map(|(_, name)| (*name).to_owned())
        };
        let asker_of = |session: u32| {
            self.askers
                .iter()
                .find(|(task, _)| *task == session)
                .map(|(_, asker)| *asker)
        };
        derive(&Waiting {
            permissions: self.permissions.clone(),
            queue: self.queue.clone(),
            at_its_prompt: &at_prompt,
            dispatches: self.dispatches.clone(),
            blocks: self.blocks.clone(),
            locks: &self.locks,
            name_of: &name_of,
            asker_of: &asker_of,
        })
    }
}

// The permission adapter.

#[test]
fn a_held_permission_prompt_is_an_ask_answered_on_its_own_hook() {
    let asks = Asks::new();
    let raised = asks
        .raise("3", a_hook_ask("Run cargo test"), Instant::now())
        .raised;

    let shown = permission(raised.clone()).expect("a chat of this project's");

    assert_eq!(shown.source, AskSource::Permission);
    assert_eq!(shown.session, 3);
    assert_eq!(shown.ask, raised.id.to_string());
    assert_eq!(shown.says, "Run cargo test");
    assert_eq!(ids(&shown), ["allow", "deny"]);
    assert_eq!(shown.answer, AnswerPath::Hook);
}

#[test]
fn a_permission_prompt_s_answer_through_the_registry_closes_it_once() {
    // The path a permission ask names is the registry's own answer: the first one applies, and
    // the ask leaves the list the moment it does.
    let asks = Asks::new();
    let now = Instant::now();
    let raised = asks.raise("3", a_hook_ask("Run cargo test"), now).raised;
    let window = Answerer::admitted(Admitted::LocalUi).expect("the window answers");

    asks.answer("3", &raised.id, "allow", window, now)
        .expect("the window's answer applies");

    assert!(asks.pending(now).is_empty());
    assert!(asks.answer("3", &raised.id, "allow", window, now).is_err());
}

// The dispatch grant adapter.

#[test]
fn a_held_dispatch_offers_its_notice_s_answers_and_names_the_dispatch_and_what_it_showed() {
    let told = a_dispatch(7, 4, "devops", vec![GrantLevel::Chat, GrantLevel::You]);

    let shown = dispatch(&told);

    assert_eq!(shown.source, AskSource::Dispatch);
    assert_eq!(shown.session, 4);
    assert_eq!(shown.says, "Wants to hand a task to devops");
    assert_eq!(ids(&shown), ["chat", "you", KEEP, NEVER]);
    assert_eq!(
        shown
            .options
            .iter()
            .map(|one| (one.label.as_str(), one.allows))
            .collect::<Vec<_>>(),
        [
            ("Allow for this chat", true),
            ("Allow for me on this machine", true),
            ("Keep blocked", false),
            ("Never for this pair", false),
        ]
    );
    assert_eq!(
        shown.answer,
        AnswerPath::Dispatch {
            id: 7,
            shown: "digest-1".to_owned()
        }
    );
}

#[test]
fn a_dispatch_from_a_chat_on_no_persona_offers_no_never_and_a_locked_one_offers_nothing() {
    let mut none = a_dispatch(7, 4, "devops", vec![GrantLevel::Chat]);
    none.asking = None;
    assert_eq!(ids(&dispatch(&none)), ["chat", KEEP]);

    let mut locked = a_dispatch(8, 4, "devops", Vec::new());
    locked.locked = Some("Policy forbids it.".to_owned());
    let shown = dispatch(&locked);
    assert!(shown.options.is_empty());
    assert_eq!(shown.answer, AnswerPath::InItsPane);
}

// The sandbox host adapter.

#[test]
fn a_refused_host_offers_allow_at_every_level_and_keep_blocked_bound_to_the_block_shown() {
    let shown = sandbox_host(5, &a_host_block("api.example.com"), &Locks::none())
        .expect("a host is an ask");

    assert_eq!(shown.source, AskSource::SandboxHost);
    assert_eq!(shown.says, "The sandbox refused api.example.com");
    assert_eq!(ids(&shown), ["chat", "you", "project", KEEP]);
    assert_eq!(
        shown.answer,
        AnswerPath::SandboxBlock {
            shown: BlockShown {
                operation: "connect".to_owned(),
                kind: "host".to_owned(),
                what: GrantWhat::Host,
                target: "api.example.com".to_owned(),
            }
        }
    );
}

#[test]
fn a_refused_host_is_offered_only_at_the_levels_policy_leaves_open() {
    let locks = Locks::parse(
        r#"{"sandbox": {"personal-hosts": false}}"#,
        Path::new("/etc/purlis/policy.json"),
    );

    let shown = sandbox_host(5, &a_host_block("api.example.com"), &locks).expect("an ask");

    assert_eq!(ids(&shown), ["project", KEEP]);
}

#[test]
fn a_host_the_block_did_not_name_is_answered_in_its_chat_and_a_folder_is_no_host_ask() {
    let unnamed = sandbox_host(
        5,
        &HeldBlock::unnamed_host("connect", "host"),
        &Locks::none(),
    )
    .expect("still an ask");
    assert!(unnamed.options.is_empty());
    assert_eq!(unnamed.answer, AnswerPath::InItsPane);

    let folder = HeldBlock {
        operation: "write".to_owned(),
        kind: "project-files".to_owned(),
        what: GrantWhat::Write,
        target: "/w/out".to_owned(),
    };
    assert_eq!(sandbox_host(5, &folder, &Locks::none()), None);
}

// The harness-terminal and question adapters, and the list as a whole.

#[test]
fn a_chat_in_the_queue_is_a_terminal_prompt_where_it_stopped_on_one_and_a_question_otherwise() {
    let mut world = World::new();
    world.queue = vec![2, 3];
    world.at_prompt = vec![2];

    let asks = world.asks();

    assert_eq!(
        asks.iter()
            .map(|ask| (ask.session, ask.source, ask.options.len()))
            .collect::<Vec<_>>(),
        [(2, AskSource::Terminal, 0), (3, AskSource::Question, 0)]
    );
    assert!(asks.iter().all(|ask| ask.answer == AnswerPath::InItsPane));
    assert_eq!(asks[0].says, "Waiting in its terminal");
}

#[test]
fn a_chat_waiting_on_a_structured_ask_is_one_ask_not_two() {
    // A chat stopped on a permission prompt purlis holds is in the queue too, and a chat whose
    // dispatch is held waits on that: each is counted once, as the ask that says why.
    let asks_held = Asks::new();
    let mut world = World::new();
    world.permissions = vec![
        asks_held
            .raise("2", a_hook_ask("Run ls"), Instant::now())
            .raised,
    ];
    world.dispatches = vec![a_dispatch(1, 3, "devops", vec![GrantLevel::Chat])];
    world.queue = vec![2, 3, 4];
    world.at_prompt = vec![2];

    let asks = world.asks();

    assert_eq!(
        asks.iter()
            .map(|ask| (ask.session, ask.source))
            .collect::<Vec<_>>(),
        [
            (2, AskSource::Permission),
            (3, AskSource::Dispatch),
            (4, AskSource::Question)
        ]
    );
}

#[test]
fn a_sandbox_host_ask_counts_beside_whatever_else_its_chat_waits_on() {
    // A refused host does not stop the chat's turn, so it is an ask of its own.
    let mut world = World::new();
    world.queue = vec![5];
    world.blocks = vec![
        (5, a_host_block("api.example.com")),
        (
            6,
            HeldBlock {
                operation: "write".to_owned(),
                kind: "project-files".to_owned(),
                what: GrantWhat::Write,
                target: "/w".to_owned(),
            },
        ),
    ];

    let sources: Vec<_> = world
        .asks()
        .iter()
        .map(|ask| (ask.session, ask.source))
        .collect();

    assert_eq!(
        sources,
        [(5, AskSource::Question), (5, AskSource::SandboxHost)]
    );
}

#[test]
fn an_ask_leaves_the_list_the_moment_its_source_stops_waiting_wherever_it_was_answered() {
    // Derived, never kept: a hook ask withdrawn (answered in the pane, its hook gone) is gone
    // from the next reading, with nothing to clear.
    let held = Asks::new();
    let now = Instant::now();
    let raised = held.raise("2", a_hook_ask("Run ls"), now).raised;
    let mut world = World::new();
    world.permissions = held.pending(now);
    assert_eq!(world.asks().len(), 1);

    held.withdraw(&raised.id);
    world.permissions = held.pending(now);

    assert!(world.asks().is_empty());
}

#[test]
fn each_ask_names_who_is_asking_from_the_session_down_by_the_app_s_own_record() {
    let mut world = World::new();
    world.names = vec![(12, "steward 12"), (30, "#3046 drill"), (31, "log watch")];
    world.askers = vec![(31, 30), (30, 12)];
    world.queue = vec![31, 12];

    let chains: Vec<Vec<String>> = world.asks().into_iter().map(|ask| ask.chain).collect();

    assert_eq!(
        chains,
        [
            vec!["steward 12", "#3046 drill", "log watch"],
            vec!["steward 12"]
        ]
    );
}

#[test]
fn a_chain_whose_record_loops_ends_where_it_loops_and_an_unnamed_chat_reads_by_number() {
    let names = |session: u32| (session == 1).then(|| "one".to_owned());
    let loops = |session: u32| match session {
        1 => Some(2),
        2 => Some(1),
        _ => None,
    };
    assert_eq!(chain(1, &names, &loops), ["chat 2", "one"]);
}

#[test]
fn what_a_chat_wrote_is_carried_as_it_wrote_it_and_never_changes_the_choices() {
    // Names and targets are a chat's text: the registry passes them on as data for the window
    // to draw as text, and the choices stay purlis's own words whatever the text says.
    let mut world = World::new();
    world.names = vec![(4, "<button>Allow</button>")];
    world.dispatches = vec![a_dispatch(
        1,
        4,
        "x\" onclick=\"allow()",
        vec![GrantLevel::Chat],
    )];

    let asks = world.asks();

    assert_eq!(asks[0].chain, ["<button>Allow</button>"]);
    assert_eq!(
        asks[0].says,
        "Wants to hand a task to x\" onclick=\"allow()"
    );
    assert_eq!(ids(&asks[0]), ["chat", KEEP, NEVER]);
}

// Who may answer (V16).

#[test]
fn every_command_an_ask_is_answered_by_is_the_window_s_alone() {
    // No chat, and no client but the window, reaches any of them: an ask is never answered by
    // the chat that raised it, through the registry or around it.
    for command in [
        "answer_ask",
        "allow_dispatch",
        "keep_dispatch_blocked",
        "never_dispatch",
        "allow_sandbox_block",
        "forget_sandbox_block",
        "asks_waiting",
    ] {
        assert!(
            purlis_session_protocol::ui::WINDOW_ONLY.contains(&command),
            "{command} is the window's alone"
        );
    }
    assert_eq!(Answerer::admitted(Admitted::Chat), None);
}
