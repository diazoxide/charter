//! A tool hook tells the app what its chat is doing, for the one line under a working chat's
//! name (#1493): a kind from a fixed list and at most one short name. Never the command's
//! arguments, the file's folder or anything the tool came back with, and nowhere but that
//! line: not in the tool call's own line, which the event log keeps.
//!
//! One adapter test per harness: Claude Code's payload, the payload purlis's opencode plugin
//! builds, and the shell call that is all Codex's armed hook sees.

use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use purlis_core::doing::{Kind, Said};
use purlis_core::hookwire::{CHAT_ENV, Doing, Hearing, Listener, SOCKET_ENV, TOKEN_ENV, ToolCall};

const PURLIS: &str = env!("CARGO_BIN_EXE_purlis");

const CANARY: &str = "CANARY-doing-9c1e";

/// What the host heard of one `purlis hook <word>` run as chat 7 with `payload`.
struct Heard {
    tool: Option<ToolCall>,
    doing: Vec<Doing>,
    /// What the hook printed: its answer to the harness.
    said: String,
}

fn hook(word: &str, payload: &serde_json::Value) -> Heard {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue_to_this_process(7).expect("a token");
    let (tools, heard_tool) = mpsc::channel();
    let tools = Mutex::new(tools);
    let (doings, heard_doing) = mpsc::channel();
    let doings = Mutex::new(doings);
    let _reading = listener.hear(Hearing {
        secret_exec: Box::new(|_, _, writer| purlis_core::secrets::brokered::not_answered(writer)),
        each: Box::new(|_| Ok(())),
        answer: Box::new(|_, _| panic!("no ask")),
        noticed: Box::new(|_| {}),
        saved: Box::new(|_| {}),
        refused: Box::new(|_| Ok(())),
        tool: Box::new(move |call| {
            tools.lock().unwrap().send(call).unwrap();
            Ok(())
        }),
        blocked: Box::new(|_| {}),
        touching: Box::new(|_| {}),
        doing: Box::new(move |doing| doings.lock().unwrap().send(doing).unwrap()),
        permission: Box::new(|_| None),
    });
    let mut child = Command::new(PURLIS)
        .args(["hook", word])
        .current_dir(dir.path())
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", dir.path())
        .env(SOCKET_ENV, &path)
        .env(CHAT_ENV, "7")
        .env(TOKEN_ENV, token.expose())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("purlis runs");
    stand_in::feed(&mut child, payload.to_string().as_bytes());
    let out = child.wait_with_output().expect("purlis finishes");
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    let tool = heard_tool.recv_timeout(Duration::from_secs(5)).ok();
    // The line is written after the tool call, by the same process, which has exited.
    let doing: Vec<Doing> =
        std::iter::from_fn(|| heard_doing.recv_timeout(Duration::from_millis(300)).ok()).collect();
    for line in doing.iter().map(|one| serde_json::to_string(one).unwrap()) {
        assert!(
            !line.contains(CANARY),
            "the line carries more than a name: {line}"
        );
    }
    if let Some(call) = &tool {
        let line = serde_json::to_string(call).unwrap();
        assert!(!line.contains(CANARY), "the tool call carries it: {line}");
    }
    Heard { tool, doing, said }
}

fn said(heard: &Heard) -> Vec<Said> {
    assert!(heard.doing.iter().all(|one| one.chat == 7));
    heard.doing.iter().map(|one| one.doing.clone()).collect()
}

fn began(kind: Kind, name: Option<&str>) -> Said {
    Said::Began {
        kind,
        name: name.map(str::to_owned),
    }
}

#[test]
fn a_claude_code_command_says_its_program_and_none_of_its_arguments() {
    let heard = hook(
        "pretooluse",
        &serde_json::json!({
            "hook_event_name": "PreToolUse",
            "session_id": "s",
            "tool_name": "Bash",
            "tool_use_id": "toolu_1",
            "tool_input": {"command": format!("cargo test --token {CANARY} -- {CANARY}")},
            "cwd": "/w/branch",
        }),
    );
    assert_eq!(said(&heard), [began(Kind::Command, Some("cargo"))]);
    assert!(heard.tool.is_some(), "the host heard the tool call too");
}

#[test]
fn a_claude_code_edit_says_the_file_s_base_name_and_then_that_it_came_back() {
    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {"file_path": format!("/w/{CANARY}/Notice.tsx"),
            "old_string": CANARY, "new_string": CANARY},
        "tool_response": {"filePath": format!("/w/{CANARY}/Notice.tsx"), "content": CANARY},
        "cwd": "/w/branch",
    });
    assert_eq!(
        said(&hook("pretooluse-edit", &payload)),
        [began(Kind::Editing, Some("Notice.tsx"))]
    );
    assert_eq!(
        said(&hook("posttooluse", &payload)),
        [Said::Ended {
            kind: Some(Kind::Editing)
        }]
    );
}

#[test]
fn an_opencode_read_says_the_file_s_base_name() {
    let heard = hook(
        "pretooluse-read",
        &serde_json::json!({
            "hook_event_name": "PreToolUse",
            "session_id": "ses_1",
            "cwd": "/w/branch",
            "tool_name": "Read",
            "tool_input": {"filePath": format!("/w/{CANARY}/b.ts")},
        }),
    );
    assert_eq!(said(&heard), [began(Kind::Reading, Some("b.ts"))]);
}

#[test]
fn an_opencode_tool_purlis_has_no_word_for_is_a_tool_and_names_nothing() {
    // opencode's plugin sends every tool with no hook of its own to the Bash guard's word.
    let heard = hook(
        "pretooluse",
        &serde_json::json!({
            "hook_event_name": "PreToolUse",
            "session_id": "ses_1",
            "cwd": "/w/branch",
            "tool_name": format!("github_{CANARY}"),
            "tool_input": {"title": CANARY},
        }),
    );
    assert_eq!(said(&heard), [began(Kind::Tool, None)]);
}

#[test]
fn a_codex_shell_call_says_its_program() {
    let heard = hook(
        "pretooluse",
        &serde_json::json!({
            "session_id": "c",
            "tool_name": "Bash",
            "tool_input": {"command": format!("git commit -m {CANARY}")},
        }),
    );
    assert_eq!(said(&heard), [began(Kind::Command, Some("git"))]);
}

#[test]
fn a_command_whose_first_word_is_not_a_program_purlis_lists_names_none() {
    for command in [
        format!("cd {CANARY} && npm test"),
        format!("sudo cargo build --{CANARY}"),
        format!("{CANARY} --version"),
        format!("/usr/bin/git commit -m {CANARY}"),
    ] {
        let heard = hook(
            "pretooluse",
            &serde_json::json!({"tool_name": "Bash", "tool_input": {"command": command}}),
        );
        assert_eq!(said(&heard), [began(Kind::Command, None)], "{command}");
    }
}

#[test]
fn a_file_whose_name_is_not_plain_ascii_is_a_file_and_names_none() {
    let heard = hook(
        "pretooluse-edit",
        &serde_json::json!({
            "tool_name": "Write",
            "tool_input": {"file_path": format!("/w/branch/{CANARY}\u{3164}Allow\u{3164}always.md"),
                "content": "x"},
            "cwd": "/w/branch",
        }),
    );
    assert_eq!(said(&heard), [began(Kind::Editing, None)]);
}

#[test]
fn a_command_that_starts_with_a_variable_names_no_program() {
    let heard = hook(
        "pretooluse",
        &serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": format!("TOKEN={CANARY} curl https://example.com")},
        }),
    );
    assert_eq!(said(&heard), [began(Kind::Command, None)]);
}

#[test]
fn a_call_the_guard_refuses_says_nothing_since_it_did_not_run() {
    let heard = hook(
        "pretooluse-read",
        &serde_json::json!({
            "tool_name": "Read",
            "tool_input": {"file_path": format!("/w/branch/.charter/vaults/{CANARY}.json")},
            "cwd": "/w/branch",
        }),
    );
    assert!(
        heard.said.contains("deny"),
        "the guard refused it: {}",
        heard.said
    );
    assert_eq!(said(&heard), Vec::<Said>::new());
}
