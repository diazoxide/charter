//! The session protocol's compatibility promise (E5, ADR 0068 §4), held in CI.
//!
//! `fixtures/session-protocol.jsonl` is a frame of every kind version 1 says. Each must read
//! back as it was recorded, by this build and so by every later one, which is what lets a
//! client and a host one version apart talk. A frame kind added without being recorded fails
//! here, so the promise grows on purpose. The UI RPC is not in it: it is private to one build
//! of the app, and excluded from this promise (V7).
//!
//! And a host reads a later minor's frames the way the promise says: a field it does not know
//! is ignored, a command it does not know is refused with `unknown_command` and the link
//! carries on, and a frame kind it does not know is passed over.

use std::collections::BTreeSet;

use bytes::Bytes;
use charter_session_protocol::auth::Scope;
use charter_session_protocol::link::{self, Link};
use charter_session_protocol::session::{
    self, Answer, Command, Frame, Host, Outcome, Peer, Pushed, Refusal, Reply,
};
use serde_json::{Value, json};
use tokio::io::duplex;

mod common;
use common::HELD;

const RECORDED: &str = include_str!("fixtures/session-protocol.jsonl");

fn recorded() -> Vec<Value> {
    RECORDED
        .lines()
        .map(|line| serde_json::from_str(line).expect("each recorded line is JSON"))
        .collect()
}

/// What kind of frame a recorded line is, in words.
fn kind(frame: &Value) -> String {
    if let Some(call) = frame.get("call") {
        return format!("call.{}", call["command"].as_str().unwrap());
    }
    if let Some(reply) = frame.get("reply") {
        return match &reply["ok"] {
            Value::String(word) => format!("reply.ok.{word}"),
            Value::Object(answer) => format!("reply.ok.{}", answer.keys().next().unwrap()),
            _ => "reply.refused".into(),
        };
    }
    let (key, inner) = frame.as_object().unwrap().iter().next().unwrap();
    format!(
        "{key}.{}",
        inner.as_object().unwrap().keys().next().unwrap()
    )
}

#[test]
fn every_recorded_frame_reads_back_as_it_was_recorded() {
    for line in recorded() {
        let read: Frame = serde_json::from_value(line.clone())
            .unwrap_or_else(|e| panic!("{line} no longer reads: {e}"));
        assert_eq!(
            serde_json::to_value(&read).unwrap(),
            line,
            "{line} no longer writes as it was recorded"
        );
    }
}

/// Every kind of frame this build can say, from the words each variant has on the wire. Those
/// come from an exhaustive `match` (`Command::word`, `Answer::word`, `Pushed::word`), so a new
/// variant does not compile until it has a word, and then fails here until it is recorded.
fn promised() -> BTreeSet<String> {
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for frame in Frame::WORDS {
        match *frame {
            "call" => kinds.extend(Command::WORDS.iter().map(|w| format!("call.{w}"))),
            "reply" => {
                for outcome in Outcome::WORDS {
                    match *outcome {
                        "ok" => kinds.extend(Answer::WORDS.iter().map(|w| format!("reply.ok.{w}"))),
                        "refused" => {
                            kinds.insert("reply.refused".into());
                        }
                        other => panic!("a reply outcome `{other}` this test does not place"),
                    }
                }
            }
            "pushed" => kinds.extend(Pushed::WORDS.iter().map(|w| format!("pushed.{w}"))),
            // The UI RPC: private to one build, never in the promise (V7).
            "ui" => {}
            other => panic!("a frame kind `{other}` this test does not place: is it promised?"),
        }
    }
    kinds
}

#[test]
fn every_kind_of_frame_this_build_says_is_recorded() {
    let kinds: BTreeSet<String> = recorded().iter().map(kind).collect();
    assert_eq!(
        kinds,
        promised(),
        "a frame kind this build says is not recorded, or one recorded is no longer said"
    );
    // A few by name, so the derivation itself cannot quietly drop them.
    for named in [
        "call.list",
        "call.subscribe",
        "reply.ok.chats",
        "pushed.missed",
    ] {
        assert!(kinds.contains(named), "{named}");
    }
    assert_eq!(session::COMMANDS, Command::WORDS);
}

#[test]
fn each_variants_word_is_the_word_its_frame_carries() {
    for line in recorded() {
        let frame: Frame = serde_json::from_value(line.clone()).unwrap();
        let said = match &frame {
            Frame::Call(call) => format!("call.{}", call.command.word()),
            Frame::Reply(Reply {
                outcome: Outcome::Ok(answer),
                ..
            }) => format!("reply.ok.{}", answer.word()),
            Frame::Reply(_) => "reply.refused".into(),
            Frame::Pushed(pushed) => format!("pushed.{}", pushed.word()),
            Frame::Ui(_) => panic!("{line} is a UI RPC frame"),
        };
        assert_eq!(said, kind(&line), "{line}");
    }
}

/// Every key, and every word a program reads (a state, a refusal code), is snake_case on the
/// wire: kebab-case is for UI text only (V76). An event's `kind` is not checked: it keeps
/// ADR 0066's dotted names (`run.started`), the event log's own word.
#[test]
fn every_word_on_the_wire_is_snake_case() {
    fn snake(word: &str) -> bool {
        !word.is_empty()
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }
    fn walk(value: &Value, at: &str) {
        match value {
            Value::Object(fields) => {
                for (key, inner) in fields {
                    assert!(snake(key), "`{key}` at {at} is not snake_case");
                    if ["command", "state", "code", "ok"].contains(&key.as_str())
                        && let Value::String(word) = inner
                    {
                        assert!(snake(word), "`{word}` ({key} at {at}) is not snake_case");
                    }
                    // A cursor's keys are device ids, not words.
                    if key != "since" {
                        walk(inner, &format!("{at}.{key}"));
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, at)),
            _ => {}
        }
    }
    for line in recorded() {
        // A pushed event's body is the event's own, not the protocol's.
        let mut line = line;
        if let Some(event) = line.pointer_mut("/pushed/event") {
            event["body"] = Value::Null;
        }
        walk(&line, "");
    }
    for code in [
        session::UNKNOWN_COMMAND,
        session::MALFORMED,
        session::NO_SUCH_CHAT,
        session::NO_SUCH_PROJECT,
        session::NOT_ALLOWED,
    ] {
        assert!(snake(code), "{code}");
    }
}

#[test]
fn the_ui_rpc_is_not_part_of_the_promise() {
    for line in recorded() {
        assert!(
            line.get("ui").is_none(),
            "{line} is a UI RPC frame, which is private to one build"
        );
    }
}

#[test]
fn the_version_negotiated_is_the_session_protocols() {
    assert_eq!(session::speaks().majors(), vec![session::VERSION.major]);
    assert_eq!(session::VERSION.major, 1);
}

/// A host that answers a stop with what it heard, and everything else with done.
struct Echo;

impl Host for Echo {
    async fn call(&self, command: Command, _peer: &Peer) -> Result<Answer, Refusal> {
        match command {
            Command::Stop { chat } => Err(Refusal {
                code: "heard".into(),
                why: chat,
            }),
            _ => Ok(Answer::Done),
        }
    }
}

async fn raw_client() -> Link {
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::Terminal,
            HELD.of(Scope::Terminal)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    tokio::spawn(session::serve(served.unwrap(), Echo, None));
    client.unwrap()
}

async fn exchange(link: &mut Link, frame: Value) -> Value {
    link.control()
        .send(Bytes::from(frame.to_string()))
        .await
        .unwrap();
    let answer = tokio::time::timeout(std::time::Duration::from_secs(5), link.control().next())
        .await
        .expect("the host answers")
        .unwrap()
        .unwrap();
    serde_json::from_slice(&answer).unwrap()
}

#[tokio::test]
async fn a_command_from_a_later_minor_is_refused_by_name_and_the_link_carries_on() {
    let mut link = raw_client().await;
    let answer = exchange(
        &mut link,
        json!({"call": {"id": 41, "command": "fork", "chat": "c"}}),
    )
    .await;
    assert_eq!(answer["reply"]["re"], 41);
    assert_eq!(answer["reply"]["refused"]["code"], session::UNKNOWN_COMMAND);

    let answer = exchange(&mut link, json!({"call": {"id": 42, "command": "list"}})).await;
    assert_eq!(answer, json!({"reply": {"re": 42, "ok": "done"}}));
}

#[tokio::test]
async fn a_field_from_a_later_minor_is_ignored() {
    let mut link = raw_client().await;
    let answer = exchange(
        &mut link,
        json!({"call": {"id": 7, "command": "stop", "chat": "c", "signal": "TERM"}}),
    )
    .await;
    assert_eq!(
        answer,
        json!({"reply": {"re": 7, "refused": {"code": "heard", "why": "c"}}})
    );
}

#[tokio::test]
async fn a_known_command_that_cannot_be_read_is_refused_as_malformed() {
    let mut link = raw_client().await;
    let answer = exchange(
        &mut link,
        json!({"call": {"id": 8, "command": "resize", "chat": "c", "cols": "wide"}}),
    )
    .await;
    assert_eq!(answer["reply"]["refused"]["code"], session::MALFORMED);
}

#[tokio::test]
async fn a_frame_kind_from_a_later_version_is_passed_over() {
    let mut link = raw_client().await;
    link.control()
        .send(Bytes::from(json!({"notice": {"text": "hi"}}).to_string()))
        .await
        .unwrap();
    let answer = exchange(&mut link, json!({"call": {"id": 9, "command": "list"}})).await;
    assert_eq!(answer, json!({"reply": {"re": 9, "ok": "done"}}));
}
