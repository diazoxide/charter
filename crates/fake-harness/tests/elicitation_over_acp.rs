//! ACP elicitation (#1377, ADR 0080 §2) against the fake harness as a scripted ACP agent: the
//! agent asks for values, the ask elicits a secret, and only the person answers it, from the
//! window.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use purlis_core::acp::{self, Chat, Elicitation, Elicited, Event, Given, Launch, Stop, TurnFailed};
use purlis_core::harness::asks::{Admitted, Answerer, Asks, Refused};
use purlis_core::harness::model::Deadline;

const PATIENCE: Duration = Duration::from_secs(20);

fn launch(dir: &Path) -> Launch {
    static LEFT: std::sync::Once = std::sync::Once::new();
    LEFT.call_once(|| {
        use purlis_core::noterminal::{Left, leave};
        if let Left::Relaunched(code) = leave().expect("the tests leave their terminal") {
            std::process::exit(code);
        }
    });
    Launch {
        chat: "chat-1".to_owned(),
        argv: vec![
            env!("CARGO_BIN_EXE_fake-harness").to_owned(),
            "--acp".to_owned(),
        ],
        cwd: dir.to_path_buf(),
        env: Vec::new(),
        charter_mcp: None,
        patience: PATIENCE,
    }
}

fn start(dir: &Path) -> (Arc<Chat>, acp::Events, Arc<Asks>) {
    let asks = Arc::new(Asks::new());
    let (chat, events) = Chat::start(launch(dir), Arc::clone(&asks)).expect("the agent starts");
    (Arc::new(chat), events, asks)
}

fn events_until(events: &acp::Events, until: impl Fn(&Event) -> bool) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        match events.recv_timeout(PATIENCE) {
            Ok(event) => {
                let done = until(&event);
                seen.push(event);
                if done {
                    return seen;
                }
            }
            Err(err) => panic!("no more events ({err}) after {seen:#?}"),
        }
    }
}

fn text(event: &Event) -> Option<&str> {
    match event {
        Event::Text(text) => Some(text),
        _ => None,
    }
}

fn prompt_aside(chat: &Arc<Chat>, text: &str) -> Receiver<Result<Stop, TurnFailed>> {
    let chat = Arc::clone(chat);
    let text = text.to_owned();
    let (ended, turn) = std::sync::mpsc::channel();
    std::thread::spawn(move || ended.send(chat.prompt(&text)));
    turn
}

fn ended(turn: &Receiver<Result<Stop, TurnFailed>>) -> Result<Stop, TurnFailed> {
    turn.recv_timeout(PATIENCE).expect("the turn ended")
}

/// What the agent said it was answered, once its turn ended.
fn said(turn: &Receiver<Result<Stop, TurnFailed>>, events: &acp::Events) -> (String, Vec<Event>) {
    assert_eq!(ended(turn), Ok(Stop::EndTurn));
    let seen = events_until(events, |event| text(event).is_some());
    let said = seen.iter().find_map(text).unwrap_or_default().to_owned();
    (said, seen)
}

fn elicited(events: &acp::Events) -> Elicitation {
    let seen = events_until(events, |event| {
        matches!(event, Event::Elicited(_) | Event::Raised(_))
    });
    match seen.into_iter().last() {
        Some(Event::Elicited(elicitation)) => elicitation,
        other => panic!("not an elicitation: {other:?}"),
    }
}

fn the_window() -> Answerer {
    Answerer::admitted(Admitted::LocalUi).expect("the window answers")
}

fn the_inbox() -> Answerer {
    Answerer::admitted(Admitted::Approval).expect("the inbox answers")
}

fn values(pairs: &[(&str, Given)]) -> BTreeMap<String, Given> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

#[test]
fn an_elicitation_waits_for_the_window_and_its_values_reach_the_agent_and_nothing_else() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events, asks) = start(dir.path());
    let turn = prompt_aside(&chat, "elicit form");

    let Elicitation { raised, form } = elicited(&events);
    assert_eq!(raised.chat, "chat-1");
    assert!(
        raised.ask.elicits_secret,
        "every elicitation elicits a secret"
    );
    assert_eq!(raised.ask.deadline, Deadline::None);
    assert_eq!(form.message, "Your registry token, please");
    assert_eq!(
        form.fields
            .iter()
            .map(|field| (field.name.as_str(), field.label.as_str(), field.required))
            .collect::<Vec<_>>(),
        [
            ("region", "region", false),
            ("token", "Registry token", true)
        ]
    );
    assert_eq!(asks.pending(Instant::now()), vec![raised.clone()]);

    // Nothing answers it on a timer, and nobody but the window.
    std::thread::sleep(Duration::from_millis(300));
    assert!(turn.try_recv().is_err(), "a timeout never answers");
    let token = values(&[("token", Given::Text("tok-0001".to_owned()))]);
    assert_eq!(
        chat.answer_elicitation(&raised.id, Elicited::Accept(token.clone()), the_inbox()),
        Err(Refused::NotYours)
    );
    assert_eq!(
        chat.answer(&raised.id, "decline", the_inbox()),
        Err(Refused::NotYours)
    );
    // Values that do not fit leave it open.
    assert!(matches!(
        chat.answer_elicitation(
            &raised.id,
            Elicited::Accept(values(&[("token", Given::Text("t".to_owned()))])),
            the_window()
        ),
        Err(Refused::Unfit(_))
    ));
    assert!(matches!(
        chat.answer(&raised.id, "accept", the_window()),
        Err(Refused::Unfit(_))
    ));
    assert_eq!(asks.pending(Instant::now()).len(), 1);

    let applied = chat
        .answer_elicitation(&raised.id, Elicited::Accept(token), the_window())
        .expect("answered");
    assert_eq!(applied.choice.id, "accept");
    assert_eq!(
        chat.answer(&raised.id, "decline", the_window()),
        Err(Refused::AnsweredElsewhere)
    );
    let (said, seen) = said(&turn, &events);
    assert_eq!(said, r#"accept {"token":"tok-0001"}"#);
    // The value went to the agent and into no event of charter's own: only the agent's own
    // reply, which is the agent's to write, carries it.
    for event in &seen {
        if text(event).is_none() {
            assert!(!format!("{event:?}").contains("tok-0001"), "{event:?}");
        }
    }
    assert!(!format!("{applied:?}").contains("tok-0001"));
}

#[test]
fn the_window_declines_or_dismisses_an_elicitation_by_its_option() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events, _asks) = start(dir.path());
    for (option, answered) in [("decline", "decline"), ("cancel", "cancel")] {
        let turn = prompt_aside(&chat, "elicit form");
        let Elicitation { raised, .. } = elicited(&events);
        assert_eq!(
            chat.answer(&raised.id, "maybe", the_window()),
            Err(Refused::NotAnOption("maybe".to_owned()))
        );
        chat.answer(&raised.id, option, the_window())
            .expect("answered");
        assert_eq!(said(&turn, &events).0, answered);
    }
}

#[test]
fn cancelling_the_turn_answers_a_waiting_elicitation_cancel_and_withdraws_its_ask() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events, asks) = start(dir.path());
    let turn = prompt_aside(&chat, "elicit form");
    let Elicitation { raised, .. } = elicited(&events);

    chat.cancel();
    assert_eq!(said(&turn, &events).0, "cancel");
    assert!(asks.pending(Instant::now()).is_empty());
    assert_eq!(
        chat.answer(&raised.id, "decline", the_window()),
        Err(Refused::Withdrawn)
    );
}

#[test]
fn a_url_mode_elicitation_is_refused_and_reported_and_never_raised() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events, asks) = start(dir.path());
    let turn = prompt_aside(&chat, "elicit url");
    let (said, seen) = said(&turn, &events);
    // JSON-RPC's "invalid params".
    assert_eq!(said, "refused -32602");
    assert!(
        seen.contains(&Event::Refused {
            method: "elicitation/create".to_owned()
        }),
        "{seen:#?}"
    );
    assert!(asks.pending(Instant::now()).is_empty());
}

#[test]
fn an_elicitation_for_another_session_is_refused_and_never_raised() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events, asks) = start(dir.path());
    let turn = prompt_aside(&chat, "elicit foreign");
    let (said, seen) = said(&turn, &events);
    assert_eq!(said, "refused -32602");
    assert!(
        !seen.iter().any(|event| matches!(event, Event::Elicited(_))),
        "{seen:#?}"
    );
    assert!(asks.pending(Instant::now()).is_empty());
}

#[test]
fn an_elicitation_past_its_bound_is_answered_cancel_and_never_raised() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events, asks) = start(dir.path());
    let turn = prompt_aside(&chat, "elicit big");
    assert_eq!(said(&turn, &events).0, "cancel");
    assert_eq!(chat.asks_refused(), 1);
    assert!(asks.pending(Instant::now()).is_empty());
}
