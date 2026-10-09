//! The session protocol's `Event` cannot drift from the event log's (#1095).
//!
//! ADR 0066's envelope is written twice: once in purlis-core's event log, and once in the
//! session protocol, which mirrors it instead of depending on purlis-core so that it can be
//! published without it. This crate is the one that depends on both, so the check lives here.
//!
//! Two halves, for two ways the copies can drift:
//! - a field added to, or taken out of, either one: the conversions below name every field of
//!   each with no `..`, so either change stops this file compiling;
//! - a key renamed on the wire (a `serde(rename)` on one side only): a fully filled event is
//!   carried through JSON each way and must come back as the same JSON.

use purlis_core::eventlog;
use purlis_session_protocol::session;
use serde_json::{Value, json};

/// The log's event as the protocol carries it, field by field.
fn to_session(event: eventlog::Event) -> session::Event {
    let eventlog::Event {
        v,
        device_id,
        seq,
        ulid,
        chat,
        run,
        parent_run,
        kind,
        body,
    } = event;
    session::Event {
        v,
        device_id,
        seq,
        ulid,
        chat,
        run,
        parent_run,
        kind,
        body,
    }
}

/// The protocol's event as the log holds it, field by field.
fn to_log(event: session::Event) -> eventlog::Event {
    let session::Event {
        v,
        device_id,
        seq,
        ulid,
        chat,
        run,
        parent_run,
        kind,
        body,
    } = event;
    eventlog::Event {
        v,
        device_id,
        seq,
        ulid,
        chat,
        run,
        parent_run,
        kind,
        body,
    }
}

/// An event with every field set, each to a value no other field has, so a key that moved to
/// another field's name could not pass for itself.
fn filled() -> eventlog::Event {
    eventlog::Event {
        v: eventlog::VERSION,
        device_id: "device-a".into(),
        seq: 42,
        ulid: "01J9ZZZZZZZZZZZZZZZZZZZZZZ".into(),
        chat: Some("chat-b".into()),
        run: Some("run-c".into()),
        parent_run: Some("run-d".into()),
        kind: "tool".into(),
        body: json!({ "tool": "Bash", "took_ms": 12 }),
    }
}

#[test]
fn every_field_crosses_from_the_log_to_the_protocol_and_back() {
    let event = filled();
    assert_eq!(to_log(to_session(event.clone())), event);
}

#[test]
fn the_log_s_json_reads_as_the_protocol_s_and_writes_back_the_same() {
    let written = serde_json::to_value(filled()).expect("the log's event as JSON");
    let read: session::Event =
        serde_json::from_value(written.clone()).expect("the protocol reads the log's JSON");
    let again = serde_json::to_value(&read).expect("the protocol's event as JSON");
    assert_eq!(again, written);
    assert_eq!(read, to_session(filled()));
}

#[test]
fn the_protocol_s_json_reads_as_the_log_s_and_writes_back_the_same() {
    let written = serde_json::to_value(to_session(filled())).expect("the protocol's JSON");
    let read: eventlog::Event =
        serde_json::from_value(written.clone()).expect("the log reads the protocol's JSON");
    let again: Value = serde_json::to_value(&read).expect("the log's event as JSON");
    assert_eq!(again, written);
    assert_eq!(read, filled());
}

#[test]
fn an_event_with_no_chat_or_run_crosses_the_same_way_in_json() {
    // The optional fields left empty, so a `skip_serializing_if` or a default on one side only
    // writes a different line than the other reads back.
    let bare = eventlog::Event {
        chat: None,
        run: None,
        parent_run: None,
        ..filled()
    };
    let from_log = serde_json::to_value(&bare).expect("the log's event as JSON");
    let from_protocol =
        serde_json::to_value(to_session(bare.clone())).expect("the protocol's event as JSON");
    assert_eq!(from_log, from_protocol);
    let read: session::Event =
        serde_json::from_value(from_log).expect("the protocol reads the log's JSON");
    assert_eq!(to_log(read), bare);
}
