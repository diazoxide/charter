use super::*;
use crate::harness::model::{Ask, Choice, ChoiceKind, ChoiceScope, Deadline};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

fn an_ask(deadline: Deadline) -> Ask {
    Ask {
        options: vec![
            Choice {
                id: "allow".into(),
                label: "Allow".into(),
                kind: ChoiceKind::Allow,
                scope: ChoiceScope::Once,
            },
            Choice {
                id: "deny".into(),
                label: "Deny".into(),
                kind: ChoiceKind::Reject,
                scope: ChoiceScope::Once,
            },
        ],
        deadline,
        channel: Channel::Hook,
        ..Ask::default()
    }
}

/// The operator, answering through a connection the host admitted as `client`.
fn operator(client: &str) -> Answerer {
    let scope = match client {
        "local-ui" => Admitted::LocalUi,
        "approval" => Admitted::Approval,
        other => panic!("{other} is no human scope"),
    };
    Answerer::admitted(scope).expect("a human scope answers")
}

/// `asks.raise`, for a test that needs only the ask it raised.
fn raise(asks: &Asks, chat: &str, ask: Ask, now: Instant) -> Raised {
    asks.raise(chat, ask, now).raised
}

#[test]
fn two_clients_answer_one_ask_and_only_the_first_applies_while_the_other_hears_answered_elsewhere()
{
    let asks = Asks::new();
    let t0 = Instant::now();
    let raised = raise(&asks, "chat-1", an_ask(Deadline::None), t0);

    let first = asks.answer(
        &raised.id,
        "allow",
        operator("local-ui"),
        t0 + Duration::from_secs(3),
    );
    let second = asks.answer(
        &raised.id,
        "deny",
        operator("approval"),
        t0 + Duration::from_secs(4),
    );

    let applied = first.expect("the first answer applies");
    assert_eq!(applied.choice.id, "allow");
    assert_eq!(applied.chat, "chat-1");
    assert_eq!(applied.waited, Duration::from_secs(3));
    assert_eq!(second, Err(Refused::AnsweredElsewhere));
    assert_eq!(
        Refused::AnsweredElsewhere.to_string(),
        "answered elsewhere: another client answered this ask first"
    );
    assert!(asks.pending(t0).is_empty());
}

#[test]
fn clients_racing_to_answer_one_ask_apply_exactly_one_answer() {
    for _ in 0..50 {
        let asks = Arc::new(Asks::new());
        let t0 = Instant::now();
        let raised = raise(&asks, "chat-1", an_ask(Deadline::None), t0);
        let clients = 8;
        let gate = Arc::new(Barrier::new(clients));
        let answers: Vec<_> = (0..clients)
            .map(|n| {
                let (asks, gate, id) = (asks.clone(), gate.clone(), raised.id.clone());
                std::thread::spawn(move || {
                    gate.wait();
                    let choice = if n % 2 == 0 { "allow" } else { "deny" };
                    asks.answer(
                        &id,
                        choice,
                        operator(if n % 2 == 0 { "local-ui" } else { "approval" }),
                        t0,
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().expect("a client"))
            .collect();

        let applied = answers.iter().filter(|answer| answer.is_ok()).count();
        let elsewhere = answers
            .iter()
            .filter(|answer| **answer == Err(Refused::AnsweredElsewhere))
            .count();
        assert_eq!((applied, elsewhere), (1, clients - 1));
    }
}

#[test]
fn the_same_client_answering_twice_is_refused_the_second_time() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let raised = raise(&asks, "chat-1", an_ask(Deadline::None), t0);

    assert!(
        asks.answer(&raised.id, "allow", operator("local-ui"), t0)
            .is_ok()
    );
    assert_eq!(
        asks.answer(&raised.id, "allow", operator("local-ui"), t0),
        Err(Refused::AnsweredElsewhere)
    );
}

#[test]
fn an_answer_at_or_after_the_deadline_is_refused_and_the_source_decides() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let raised = raise(&asks, "chat-1", an_ask(Deadline::Within(5_000)), t0);

    // Not yet expired by anyone: the answer itself finds the deadline passed.
    let late = asks.answer(
        &raised.id,
        "allow",
        operator("local-ui"),
        t0 + Duration::from_secs(5),
    );

    assert_eq!(late, Err(Refused::TimedOut));
    assert!(Refused::TimedOut.to_string().contains("timed out"));
    assert!(asks.pending(t0 + Duration::from_secs(5)).is_empty());
}

#[test]
fn expiring_hands_back_each_ask_past_its_deadline_and_keeps_the_rest() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let short = raise(&asks, "chat-1", an_ask(Deadline::Within(1_000)), t0);
    let long = raise(&asks, "chat-2", an_ask(Deadline::Within(60_000)), t0);
    let none = raise(&asks, "chat-3", an_ask(Deadline::None), t0);

    let expired = asks.expire(t0 + Duration::from_secs(2));

    assert_eq!(expired, std::slice::from_ref(&short.id));
    let left: Vec<AskId> = asks
        .pending(t0)
        .into_iter()
        .map(|raised| raised.id)
        .collect();
    assert_eq!(left, [long.id, none.id]);
    assert_eq!(
        asks.answer(
            &short.id,
            "allow",
            operator("local-ui"),
            t0 + Duration::from_millis(500)
        ),
        Err(Refused::TimedOut),
        "an expired ask stays expired whatever time an answer says it is"
    );
}

#[test]
fn an_answer_just_inside_the_deadline_applies() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let raised = raise(&asks, "chat-1", an_ask(Deadline::Within(5_000)), t0);

    let answer = asks.answer(
        &raised.id,
        "deny",
        operator("local-ui"),
        t0 + Duration::from_millis(4_999),
    );

    assert_eq!(answer.expect("in time").choice.id, "deny");
}

#[test]
fn a_request_the_source_sends_again_supersedes_the_one_before_it() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let acp = |call: &str| Ask {
        channel: Channel::Acp {
            session: "s1".into(),
            tool_call: call.into(),
        },
        ..an_ask(Deadline::None)
    };
    let old = raise(&asks, "chat-1", acp("call_1"), t0);
    let other = raise(&asks, "chat-1", acp("call_2"), t0);
    let new = raise(&asks, "chat-1", acp("call_1"), t0);

    assert_eq!(
        asks.answer(&old.id, "allow", operator("local-ui"), t0),
        Err(Refused::Superseded)
    );
    assert!(Refused::Superseded.to_string().contains("newer ask"));
    assert!(
        asks.answer(&other.id, "allow", operator("local-ui"), t0)
            .is_ok()
    );
    assert!(
        asks.answer(&new.id, "allow", operator("local-ui"), t0)
            .is_ok()
    );
}

#[test]
fn a_new_nudge_in_the_pane_supersedes_the_last_one_of_the_same_chat_only() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let first = raise(&asks, "chat-1", Ask::default(), t0);
    let elsewhere = raise(&asks, "chat-2", Ask::default(), t0);
    let second = raise(&asks, "chat-1", Ask::default(), t0);

    let left: Vec<AskId> = asks
        .pending(t0)
        .into_iter()
        .map(|raised| raised.id)
        .collect();
    assert_eq!(left, [elsewhere.id, second.id]);
    assert_eq!(
        asks.answer(&first.id, "allow", operator("local-ui"), t0),
        Err(Refused::Superseded)
    );
}

#[test]
fn hook_asks_of_one_chat_never_supersede_each_other() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let one = raise(&asks, "chat-1", an_ask(Deadline::None), t0);
    let two = raise(&asks, "chat-1", an_ask(Deadline::None), t0);

    assert_eq!(asks.pending(t0).len(), 2);
    assert!(
        asks.answer(&one.id, "allow", operator("local-ui"), t0)
            .is_ok()
    );
    assert!(
        asks.answer(&two.id, "deny", operator("local-ui"), t0)
            .is_ok()
    );
}

#[test]
fn a_withdrawn_or_unknown_ask_refuses_its_answer_with_why() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let raised = raise(&asks, "chat-1", an_ask(Deadline::None), t0);

    assert!(asks.withdraw(&raised.id));
    assert!(!asks.withdraw(&raised.id), "withdrawn once");
    assert_eq!(
        asks.answer(&raised.id, "allow", operator("local-ui"), t0),
        Err(Refused::Withdrawn)
    );
    let never = raise(&asks, "chat-9", an_ask(Deadline::None), t0).id;
    let other = Asks::new();
    assert_eq!(
        other.answer(&never, "allow", operator("local-ui"), t0),
        Err(Refused::Unknown)
    );
}

#[test]
fn an_answer_must_be_one_of_the_options_the_source_offered() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let raised = raise(&asks, "chat-1", an_ask(Deadline::None), t0);

    assert_eq!(
        asks.answer(&raised.id, "allow_always", operator("local-ui"), t0),
        Err(Refused::NotAnOption("allow_always".into()))
    );
    assert!(
        asks.answer(&raised.id, "allow", operator("local-ui"), t0)
            .is_ok(),
        "a refused answer leaves the ask open"
    );
}

#[test]
fn an_ask_with_no_options_is_answered_in_the_pane() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let raised = raise(&asks, "chat-1", Ask::default(), t0);

    assert_eq!(
        asks.answer(&raised.id, "allow", operator("local-ui"), t0),
        Err(Refused::InThePane)
    );
}

#[test]
fn only_a_connection_the_host_admitted_as_a_human_scope_makes_an_answerer() {
    // V16 and ADR 0080 §5.3: the window and `charter inbox` answer; a chat, whose hook socket
    // is its only way in, and every other client scope never do.
    for scope in [
        Admitted::Chat,
        Admitted::Terminal,
        Admitted::FleetMcp,
        Admitted::Editor,
    ] {
        assert_eq!(Answerer::admitted(scope), None, "{scope:?}");
    }
    assert_eq!(
        Answerer::admitted(Admitted::LocalUi).map(|by| by.scope()),
        Some(HumanScope::LocalUi)
    );
    assert_eq!(
        Answerer::admitted(Admitted::Approval).map(|by| by.scope()),
        Some(HumanScope::Approval)
    );
}

#[test]
fn an_ask_that_elicits_a_secret_is_answered_only_from_the_window() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let secret = raise(
        &asks,
        "chat-1",
        Ask {
            elicits_secret: true,
            ..an_ask(Deadline::None)
        },
        t0,
    );

    assert_eq!(
        asks.answer(&secret.id, "allow", operator("approval"), t0),
        Err(Refused::NotYours)
    );
    assert!(
        asks.answer(&secret.id, "allow", operator("local-ui"), t0)
            .is_ok()
    );
}

#[test]
fn raising_says_which_asks_it_superseded() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let first = raise(&asks, "chat-1", Ask::default(), t0);

    let again = asks.raise("chat-1", Ask::default(), t0);
    let other = asks.raise("chat-2", Ask::default(), t0);

    assert_eq!(again.superseded, [first.id]);
    assert!(other.superseded.is_empty());
}

#[test]
fn a_source_request_with_no_id_never_supersedes_another() {
    let asks = Asks::new();
    let t0 = Instant::now();
    let unnamed = || Ask {
        channel: Channel::Acp {
            session: String::new(),
            tool_call: String::new(),
        },
        ..an_ask(Deadline::None)
    };
    let one = raise(&asks, "chat-1", unnamed(), t0);
    let two = asks.raise("chat-1", unnamed(), t0);

    assert!(two.superseded.is_empty());
    assert!(
        asks.answer(&one.id, "allow", operator("local-ui"), t0)
            .is_ok()
    );
    assert!(
        asks.answer(&two.raised.id, "allow", operator("local-ui"), t0)
            .is_ok()
    );
}

#[test]
fn the_median_time_to_answer_is_read_from_the_answers_that_applied() {
    let asks = Asks::new();
    let t0 = Instant::now();
    assert_eq!(asks.median_time_to_answer(), None);

    for (chat, secs) in [("a", 9), ("b", 1), ("c", 4), ("d", 30)] {
        let raised = raise(&asks, chat, an_ask(Deadline::None), t0);
        asks.answer(
            &raised.id,
            "allow",
            operator("local-ui"),
            t0 + Duration::from_secs(secs),
        )
        .expect("applies");
    }
    // A refused answer is no time to answer.
    let timed_out = raise(&asks, "e", an_ask(Deadline::Within(1)), t0);
    let _ = asks.answer(
        &timed_out.id,
        "allow",
        operator("local-ui"),
        t0 + Duration::from_secs(99),
    );

    // 1, 4, 9, 30: the middle two, averaged.
    assert_eq!(
        asks.median_time_to_answer(),
        Some(Duration::from_millis(6_500))
    );
}

#[test]
fn an_ask_id_reads_back_from_its_text() {
    let id = Asks::new()
        .raise("chat-1", Ask::default(), Instant::now())
        .raised
        .id;

    assert_eq!(id.to_string().parse::<AskId>(), Ok(id.clone()));
    assert_eq!(
        serde_json::from_value::<AskId>(serde_json::to_value(&id).unwrap()).unwrap(),
        id
    );
}

#[test]
fn an_open_ask_names_its_chat_even_past_its_deadline_and_a_closed_one_names_none() {
    // What a chat checks before answering, so that it never closes another chat's ask: past
    // its deadline an ask is no longer pending, but until it is expired it is still that chat's.
    let asks = Asks::new();
    let t0 = Instant::now();
    let late = raise(&asks, "chat-2", an_ask(Deadline::Within(1_000)), t0);
    assert!(asks.pending(t0 + Duration::from_secs(5)).is_empty());
    assert_eq!(asks.chat_of(&late.id).as_deref(), Some("chat-2"));
    asks.withdraw(&late.id);
    assert_eq!(asks.chat_of(&late.id), None);
}
