//! A Claude Code chat whose turn ends while background agents of its own still work is not
//! waiting on the person (#1626): it is working, and Claude Code wakes it when they finish.
//!
//! Replayed here as the hooks of such a chat reach the app: each payload read the way a hook
//! reads it ([`Report::read`]) and applied to the board the window draws from. The payloads
//! are shaped as Claude Code 2.1.296's hook input schema says: its `Stop` input carries
//! `background_tasks`, the in-flight background work of the session, each entry with a `type`
//! (`subagent`, `shell`, `monitor`, `workflow`, ...), and an empty array when nothing is in
//! flight.

use purlis_core::harness::Harness;
use purlis_core::hookwire::{
    CHAT_ENV, CLAUDE_CONVERSATION_ENV, CLAUDE_PID_ENV, HARNESS_ENV, Report, SOCKET_ENV,
};
use purlis_core::state::{Board, Event, State};

const SESSION: &str = "11111111-2222-4333-8444-555555555555";
const CHAT: u32 = 7;

/// The environment a hook of chat [`CHAT`]'s own Claude Code runs in.
fn env(name: &str) -> Option<String> {
    match name {
        SOCKET_ENV => Some("/tmp/s.sock".to_owned()),
        CHAT_ENV => Some(CHAT.to_string()),
        CLAUDE_CONVERSATION_ENV => Some(SESSION.to_owned()),
        CLAUDE_PID_ENV => Some("4242".to_owned()),
        HARNESS_ENV => Some("claude-code".to_owned()),
        _ => None,
    }
}

/// A board with chat [`CHAT`] open on Claude Code, in a turn.
fn in_a_turn() -> Board {
    let mut board = Board::new();
    board.opened(CHAT, Some(Harness::ClaudeCode), Some(SESSION.to_owned()));
    hear(
        &mut board,
        Event::SessionStart,
        serde_json::json!({"source": "startup"}),
    );
    hear(
        &mut board,
        Event::UserPromptSubmit,
        serde_json::json!({"prompt": "review it"}),
    );
    assert_eq!(board.state(CHAT), State::Running);
    board
}

/// The hook of `event` heard with `payload`, as Claude Code hands it over.
fn hear(board: &mut Board, event: Event, mut payload: serde_json::Value) {
    payload["session_id"] = SESSION.into();
    payload["hook_event_name"] = event.word().into();
    let report = Report::read(event, &payload.to_string(), &env).expect("a report");
    board.reported(&report);
}

/// A `Stop` whose `background_tasks` lists `tasks`, as `(type, status)`; `None` leaves the
/// field out, as a Claude Code older than it does.
fn stop(board: &mut Board, tasks: Option<&[(&str, &str)]>) {
    let mut payload = serde_json::json!({
        "stop_hook_active": false,
        "last_assistant_message": "Waiting for the background agents to finish.",
    });
    if let Some(tasks) = tasks {
        payload["background_tasks"] = tasks
            .iter()
            .enumerate()
            .map(|(n, (kind, status))| {
                serde_json::json!({
                    "id": format!("t{n}"),
                    "type": kind,
                    "status": status,
                    "description": "Review one module",
                    "agent_type": "general-purpose",
                })
            })
            .collect();
    }
    hear(board, Event::Stop, payload);
}

fn agents(n: usize) -> Vec<(&'static str, &'static str)> {
    vec![("subagent", "running"); n]
}

#[test]
fn a_turn_that_ends_while_four_background_agents_work_is_still_working() {
    purlis_core::unsteered!();
    let mut board = in_a_turn();

    stop(&mut board, Some(&agents(4)));

    assert!(
        board.needs_you().is_empty(),
        "the chat was put in the queue"
    );
    assert_eq!(board.state(CHAT), State::Running);
}

#[test]
fn it_needs_the_person_once_its_agents_are_done_and_it_stops_with_nothing_in_flight() {
    purlis_core::unsteered!();
    let mut board = in_a_turn();
    stop(&mut board, Some(&agents(4)));
    for agent in ["a1", "a2", "a3", "a4"] {
        hear(
            &mut board,
            Event::SubagentStop,
            serde_json::json!({"agent_id": agent, "agent_type": "general-purpose"}),
        );
        assert!(
            board.needs_you().is_empty(),
            "a helper's end was the chat's"
        );
    }

    // Claude Code wakes the chat with what its agents said, and that turn ends with nothing
    // in flight: the next move is the person's.
    stop(&mut board, Some(&[]));

    assert_eq!(board.needs_you(), vec![CHAT]);
    assert_eq!(board.state(CHAT), State::Waiting);
}

#[test]
fn a_question_while_its_agents_work_still_needs_the_person() {
    purlis_core::unsteered!();
    let mut board = in_a_turn();
    stop(&mut board, Some(&agents(2)));

    hear(
        &mut board,
        Event::Notification,
        serde_json::json!({"notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash"}),
    );

    assert_eq!(board.needs_you(), vec![CHAT]);
}

#[test]
fn only_background_work_that_ends_by_itself_holds_the_end_of_a_turn() {
    purlis_core::unsteered!();
    // A background shell or monitor may run for ever (a dev server, a log tail): a chat
    // left with only those has stopped, and the person has the next move.
    for kind in ["shell", "monitor"] {
        let mut board = in_a_turn();
        stop(&mut board, Some(&[(kind, "running")]));
        assert_eq!(
            board.needs_you(),
            vec![CHAT],
            "{kind} held the end of the turn"
        );
    }
    // A workflow is agents, and ends by itself as they do.
    let mut board = in_a_turn();
    stop(
        &mut board,
        Some(&[("shell", "running"), ("workflow", "running")]),
    );
    assert!(board.needs_you().is_empty());
}

#[test]
fn a_harness_that_does_not_say_what_is_in_flight_hands_the_person_its_turn_s_end() {
    purlis_core::unsteered!();
    let mut board = in_a_turn();

    stop(&mut board, None);

    assert_eq!(board.needs_you(), vec![CHAT]);
    assert_eq!(board.state(CHAT), State::Waiting);
}
