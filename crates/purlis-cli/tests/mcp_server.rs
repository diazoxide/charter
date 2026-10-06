//! `charter mcp`, spoken to as a harness speaks to it: JSON-RPC lines on its stdin and stdout
//! (HP-7). What each tool does is `purlis_core::chattools`'s to test; this holds the protocol,
//! the chat's place, and the question that only the person answers.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

/// A project with one workspace, `alpha`, and another, `beta`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(dir.path().join("charter.toml"), "").expect("charter.toml");
    for ws in ["alpha", "beta"] {
        std::fs::create_dir_all(dir.path().join("workspaces").join(ws)).expect("a workspace");
    }
    dir
}

/// A running `charter mcp` for a chat in `workspace`, with nothing of this process's own
/// environment but what a program needs to run.
struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Server {
    fn start(root: &Path, workspace: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_purlis"))
            .arg("mcp")
            .current_dir(root)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", root)
            .env("CHARTER_ROOT", root)
            .env("CHARTER_WORKSPACE", workspace)
            .env("CHARTER_CHAT_TOKEN", "the-chat-token")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("charter mcp starts");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
            next: 1,
        }
    }

    /// `initialize` at a protocol version the harnesses speak today, offering elicitation or not.
    fn initialize(&mut self, elicitation: bool) -> Value {
        let capabilities = if elicitation {
            json!({ "elicitation": {} })
        } else {
            json!({})
        };
        let answer = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": capabilities,
                "clientInfo": { "name": "test-harness", "version": "1" },
            }),
        );
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        answer
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("write");
        self.stdin.flush().expect("flush");
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read");
        assert!(!line.is_empty(), "the server closed its stdout");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC ({e}): {line:?}"))
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let answer = self.read();
        assert_eq!(answer["id"], id, "{answer}");
        answer
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )["result"]
            .clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn a_harness_lists_charter_s_tools() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    let hello = server.initialize(false);
    assert_eq!(hello["result"]["serverInfo"]["name"], "purlis", "{hello}");

    let listed = server.request("tools/list", json!({}));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(
        names,
        [
            "todo_list",
            "todo_add",
            "todo_done",
            "memory_search",
            "memory_add",
            "persona_remember",
            "session_record_list",
            "session_record_read",
            "session_record",
            "change_status",
            "ask_operator",
        ]
    );
}

const RECORD_BODY: &str = "## Goal\n\ng\n\n## Done\n\nd\n\n## Decisions\n\nx\n\n## Open\n\no\n\n\
                           ## How to resume\n\nr\n";

#[test]
fn a_session_record_written_over_mcp_with_no_app_lands_where_the_chat_works() {
    // No hook socket in the server's environment: no app to hand it to, so the server writes
    // it itself, as `purlis session record` would (#1332).
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(false);

    let written = server.call(
        "session_record",
        json!({ "title": "Over MCP", "body": RECORD_BODY }),
    );

    assert_ne!(written["isError"], true, "{written}");
    assert!(
        text(&written).contains("Session record → workspaces/alpha/sessions/"),
        "{written}"
    );
    assert!(
        text(&written).contains("tab will not close by itself"),
        "{written}"
    );
    let records: Vec<String> = std::fs::read_dir(p.path().join("workspaces/alpha/sessions"))
        .expect("alpha's records")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with("-over-mcp.md"))
        .collect();
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(!p.path().join("workspaces/beta/sessions").exists());
}

#[test]
fn a_session_record_that_is_not_one_is_a_tool_error_and_nothing_is_written() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(false);

    let refused = server.call(
        "session_record",
        json!({ "title": "Half", "body": "## Goal\n\ng\n" }),
    );

    assert_eq!(refused["isError"], true, "{refused}");
    assert!(text(&refused).contains("nothing was written"), "{refused}");
    assert!(!p.path().join("workspaces/alpha/sessions").exists());
}

#[test]
fn a_todo_recorded_over_mcp_lands_in_the_chat_s_own_workspace() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(false);

    let added = server.call("todo_add", json!({ "text": "Wire the MCP server" }));
    assert_ne!(added["isError"], true, "{added}");
    let listed = server.call("todo_list", json!({}));
    assert!(text(&listed).contains("Wire the MCP server"), "{listed}");

    assert!(
        std::fs::read_dir(p.path().join("workspaces/alpha/todos"))
            .expect("alpha's todos")
            .count()
            > 0
    );
    assert!(!p.path().join("workspaces/beta/todos").exists());
}

#[test]
fn a_refusal_is_a_tool_error_the_model_reads_and_the_session_goes_on() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(false);

    let refused = server.call("todo_done", json!({ "slug": "no-such-todo" }));
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(text(&refused).contains("no-such-todo"), "{refused}");

    let listed = server.call("todo_list", json!({}));
    assert_ne!(listed["isError"], true, "{listed}");
}

#[test]
fn a_tool_charter_does_not_have_is_a_protocol_error() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(false);
    let answer = server.request(
        "tools/call",
        json!({ "name": "vault_read", "arguments": {} }),
    );
    assert!(answer["error"].is_object(), "{answer}");
}

#[test]
fn ask_operator_puts_the_question_to_the_person_through_the_harness() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(true);

    server.send(&json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "tools/call",
        "params": { "name": "ask_operator", "arguments": { "question": "Merge the train?" } },
    }));
    // The harness is asked, and shows it to the person: an elicitation, never an answer the
    // model gives itself.
    let asked = server.read();
    assert_eq!(asked["method"], "elicitation/create", "{asked}");
    assert_eq!(
        asked["params"]["message"], "This chat asks: Merge the train?",
        "{asked}"
    );
    assert_eq!(
        asked["params"]["requestedSchema"]["properties"]["answer"]["description"],
        "Don't type a password or secret here.",
        "{asked}"
    );
    server.send(&json!({
        "jsonrpc": "2.0",
        "id": asked["id"],
        "result": { "action": "accept", "content": { "answer": "yes, after CI" } },
    }));

    let answer = server.read();
    assert_eq!(answer["id"], 99, "{answer}");
    assert_eq!(
        text(&answer["result"]),
        "The operator answered: yes, after CI",
        "{answer}"
    );
}

#[test]
fn ask_operator_on_a_harness_that_cannot_ask_says_so() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(false);
    let said = server.call("ask_operator", json!({ "question": "Merge?" }));
    assert_eq!(said["isError"], true, "{said}");
    assert!(text(&said).contains("cannot ask"), "{said}");
}

#[test]
fn no_answer_carries_the_chat_s_token() {
    // The server is handed the chat's whole environment by Claude Code and opencode; nothing
    // it answers repeats it.
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(false);
    for (tool, args) in [
        ("todo_list", json!({})),
        ("memory_search", json!({})),
        ("session_record_list", json!({})),
        ("change_status", json!({})),
    ] {
        let said = server.call(tool, args);
        assert!(
            !said.to_string().contains("the-chat-token"),
            "{tool}: {said}"
        );
    }
}

/// What a 2026-07-28 harness (Claude Code 2.1.288, measured) puts on every request instead of
/// an `initialize`: its version, itself and what it can do.
fn stateless_meta(elicitation: bool) -> Value {
    let capabilities = if elicitation {
        json!({ "elicitation": { "form": {} } })
    } else {
        json!({})
    };
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": { "name": "test-harness", "version": "1" },
        "io.modelcontextprotocol/clientCapabilities": capabilities,
    })
}

#[test]
fn at_2026_07_28_ask_operator_asks_by_returning_input_required_and_reads_the_retry() {
    // SEP-2322: the server asks by returning `input_required`, and the harness retries the
    // call with the person's answer.
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    let asked = server.request(
        "tools/call",
        json!({
            "_meta": stateless_meta(true),
            "name": "ask_operator",
            "arguments": { "question": "Merge the train?" },
        }),
    )["result"]
        .clone();
    assert_eq!(asked["resultType"], "input_required", "{asked}");
    let request = &asked["inputRequests"]["answer"];
    assert_eq!(request["method"], "elicitation/create", "{asked}");
    assert_eq!(
        request["params"]["message"], "This chat asks: Merge the train?",
        "{asked}"
    );

    let answered = server.request(
        "tools/call",
        json!({
            "_meta": stateless_meta(true),
            "name": "ask_operator",
            "arguments": { "question": "Merge the train?" },
            "inputResponses": { "answer": { "action": "decline" } },
            "requestState": asked["requestState"],
        }),
    )["result"]
        .clone();
    assert_eq!(
        text(&answered),
        "The operator declined to answer.",
        "{answered}"
    );
}

#[test]
fn at_2026_07_28_a_harness_that_cannot_ask_is_told_so() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    let said = server.request(
        "tools/call",
        json!({
            "_meta": stateless_meta(false),
            "name": "ask_operator",
            "arguments": { "question": "Merge?" },
        }),
    )["result"]
        .clone();
    assert_eq!(said["isError"], true, "{said}");
    assert!(text(&said).contains("cannot ask"), "{said}");
}

#[test]
fn at_2026_07_28_an_answer_retried_for_another_question_is_not_read() {
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    let asked = server.request(
        "tools/call",
        json!({
            "_meta": stateless_meta(true),
            "name": "ask_operator",
            "arguments": { "question": "Merge the train?" },
        }),
    )["result"]
        .clone();
    let retried = server.request(
        "tools/call",
        json!({
            "_meta": stateless_meta(true),
            "name": "ask_operator",
            "arguments": { "question": "Delete the branch?" },
            "inputResponses": { "answer": { "action": "accept", "content": { "answer": "yes" } } },
            "requestState": asked["requestState"],
        }),
    )["result"]
        .clone();
    assert_eq!(retried["isError"], true, "{retried}");
    assert!(!text(&retried).contains("yes"), "{retried}");
}

#[test]
fn a_write_is_refused_once_the_project_turns_read_only_mid_session() {
    // FR-24: an update or a pull can move the project to a format this charter cannot write
    // while the chat, and its server, keep running.
    let p = project();
    let mut server = Server::start(p.path(), "alpha");
    server.initialize(false);
    let before = server.call("todo_add", json!({ "text": "Before" }));
    assert_ne!(before["isError"], true, "{before}");

    std::fs::write(p.path().join("charter.toml"), "schema = 999\n").expect("charter.toml");
    let after = server.call("memory_add", json!({ "text": "After" }));
    assert_eq!(after["isError"], true, "{after}");
    assert!(!p.path().join("workspaces/alpha/memory").exists());
}
