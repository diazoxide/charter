//! A sandbox block in what a Bash command came back with reaches the app as an operation and a
//! kind, and nothing more (#1338): never the path, the command or what it printed — not on the
//! block's line, not on the tool call's, and not on the hook's stderr, which goes to the
//! harness's log. The chat is told, in fixed words, that the person has a Notice and it should
//! say what was blocked and wait (#1631).
//!
//! Fed recorded violation lines, as Claude Code hands them to `PostToolUse` and
//! `PostToolUseFailure`.

use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use purlis_core::hookwire::{
    CHAT_ENV, HARNESS_ENV, Hearing, Listener, SANDBOXED_ENV, SOCKET_ENV, SandboxBlocked, TOKEN_ENV,
    ToolCall,
};
use purlis_core::sandboxblock::{Block, CHAT_DIR_ENV, Kind, Operation};

const CHARTER: &str = env!("CARGO_BIN_EXE_purlis");

const CANARY: &str = "CANARY-blocked-91c3";

/// What the host heard of one `purlis hook <word>` run as chat 7 with `payload`.
struct Heard {
    tool: Option<ToolCall>,
    blocked: Vec<SandboxBlocked>,
    said: String,
}

fn hook(word: &str, payload: &serde_json::Value) -> Heard {
    hook_in(word, payload, true)
}

/// [`hook`], in a chat the app started sandboxed or not.
fn hook_in(word: &str, payload: &serde_json::Value, sandboxed: bool) -> Heard {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue_to_this_process(7).expect("a token");
    let (tools, heard_tool) = mpsc::channel();
    let tools = Mutex::new(tools);
    let (blocks, heard_block) = mpsc::channel();
    let blocks = Mutex::new(blocks);
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
        doing: Box::new(|_| {}),
        touching: Box::new(|_| {}),
        blocked: Box::new(move |blocked| blocks.lock().unwrap().send(blocked).unwrap()),
        permission: Box::new(|_| None),
    });
    let mut command = Command::new(CHARTER);
    command.env_clear();
    if sandboxed {
        command.env(SANDBOXED_ENV, "1");
    }
    let mut child = command
        .args(["hook", word])
        .current_dir(dir.path())
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", "/Users/dev")
        .env(SOCKET_ENV, &path)
        .env(CHAT_ENV, "7")
        .env(TOKEN_ENV, token.expose())
        .env(HARNESS_ENV, "claude")
        .env(CHAT_DIR_ENV, "/Users/dev/plane/workspaces/a/repo")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("purlis runs");
    stand_in::feed(&mut child, payload.to_string().as_bytes());
    let out = child.wait_with_output().expect("purlis finishes");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains(CANARY),
        "the hook's stderr, which goes to the harness's log, names the path: {stderr}"
    );
    let tool = heard_tool.recv_timeout(Duration::from_secs(5)).ok();
    let blocked =
        std::iter::from_fn(|| heard_block.recv_timeout(Duration::from_millis(300)).ok()).collect();
    Heard {
        tool,
        blocked,
        said: String::from_utf8_lossy(&out.stdout).into_owned(),
    }
}

/// What a block word told the chat, from the one line it printed for `event`.
fn context_of(said: &str, event: &str) -> String {
    let line: serde_json::Value = serde_json::from_str(said.trim()).expect("one JSON line");
    assert_eq!(line["hookSpecificOutput"]["hookEventName"], event, "{said}");
    line["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("a context")
        .to_owned()
}

fn holds_nothing_of_the_call(heard: &Heard) {
    let mut lines: Vec<String> = heard
        .blocked
        .iter()
        // What a grant would name is the one thing a block carries of the call (#1342), and
        // only for the Notice's Allow: everything else on the line names nothing.
        .map(|one| {
            serde_json::to_string(&SandboxBlocked {
                target: None,
                ..one.clone()
            })
            .unwrap()
        })
        .collect();
    lines.extend(
        heard
            .tool
            .iter()
            .map(|call| serde_json::to_string(call).unwrap()),
    );
    for line in lines {
        for leak in [CANARY, "/Users/dev", "secret-title", "sessions"] {
            assert!(!line.contains(leak), "{leak} is on the line: {line}");
        }
    }
}

#[test]
fn a_violation_of_the_purlis_process_is_purlis_own_and_one_of_gh_is_not() {
    let error = format!(
        "Exit code 1\npurlis: could not save /Users/dev/plane/workspaces/a/sessions/{CANARY}.md\n\
         <sandbox_violations>\n\
         purlis(48211) deny(1) file-write-create /Users/dev/plane/workspaces/a/sessions/{CANARY}.md\n\
         gh(48212) deny(1) mach-lookup com.apple.trustd.agent\n\
         </sandbox_violations>"
    );
    let heard = hook(
        "posttoolusefailure-blocked",
        &serde_json::json!({
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_use_id": "toolu_1",
            "tool_input": {"command": format!("purlis session record --title secret-title {CANARY}")},
            "error": error,
            "cwd": "/Users/dev/plane/workspaces/a/repo",
        }),
    );
    let told = context_of(&heard.said, "PostToolUseFailure");
    assert!(
        told.contains("Report"),
        "purlis's own is a bug to report: {told}"
    );
    for leak in [CANARY, "/Users/dev", "secret-title", "sessions"] {
        assert!(
            !told.contains(leak),
            "{leak} is in what the chat is told: {told}"
        );
    }
    let tool = heard.tool.as_ref().expect("the host heard the tool call");
    assert_eq!(tool.tool_hook, "posttoolusefailure-blocked");
    assert_eq!(
        heard.blocked,
        vec![
            SandboxBlocked {
                chat: 7,
                sandbox_blocked: Block {
                    operation: Operation::Write,
                    // Not inside a project this hook can see, so outside it: under the home.
                    kind: Kind::Home,
                    ours: true,
                },
                harness: Some("claude".to_owned()),
                target: None,
            },
            SandboxBlocked {
                chat: 7,
                sandbox_blocked: Block {
                    operation: Operation::Lookup,
                    kind: Kind::CertificateCheck,
                    // gh's own refusal, in the same command: never purlis's.
                    ours: false,
                },
                harness: Some("claude".to_owned()),
                target: None,
            },
        ]
    );
    holds_nothing_of_the_call(&heard);
}

#[test]
fn a_command_that_came_back_with_a_refusal_on_stderr_is_a_block_of_the_chats_own() {
    let heard = hook(
        "posttooluse-blocked",
        &serde_json::json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": format!("cargo build {CANARY}")},
            "tool_response": {
                "stdout": "",
                "stderr": format!("touch: /Users/dev/.cargo/registry/{CANARY}: Operation not permitted"),
                "interrupted": false,
            },
        }),
    );
    assert_eq!(
        heard
            .blocked
            .iter()
            .map(|one| one.sandbox_blocked)
            .collect::<Vec<_>>(),
        vec![Block {
            operation: Operation::Write,
            kind: Kind::ToolchainCache,
            ours: false,
        }]
    );
    // A write a person could grant carries its path, for the Notice's Allow (#1342).
    assert_eq!(
        heard.blocked[0].target.as_deref(),
        Some(format!("/Users/dev/.cargo/registry/{CANARY}").as_str())
    );
    holds_nothing_of_the_call(&heard);
}

#[test]
fn what_a_command_printed_on_standard_output_sends_no_block() {
    let heard = hook(
        "posttooluse-blocked",
        &serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": "cat notes.txt"},
            "tool_response": {
                "stdout": format!("touch: /opt/{CANARY}: Operation not permitted"),
                "stderr": "",
            },
        }),
    );
    assert!(heard.blocked.is_empty(), "{:?}", heard.blocked);
    assert!(heard.tool.is_some(), "the call itself is still heard");
    assert_eq!(heard.said, "", "no block, so the chat is told nothing");
}

#[test]
fn a_chat_the_app_did_not_start_sandboxed_sends_no_block() {
    // A refusal there is macOS's or a file's own, not the sandbox's.
    let heard = hook_in(
        "posttoolusefailure-blocked",
        &serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": "purlis todo add x"},
            "error": format!(
                "Exit code 1\n<sandbox_violations>\npurlis(1) deny(1) file-write-create \
                 /Users/dev/plane/{CANARY}\n</sandbox_violations>"
            ),
        }),
        false,
    );
    assert!(heard.blocked.is_empty(), "{:?}", heard.blocked);
    assert!(heard.tool.is_some(), "the call itself is still heard");
    assert_eq!(heard.said, "", "no block, so the chat is told nothing");
}

/// #1631, replayed: a command that reached three hosts came back with exit status 0, and Claude
/// Code's proxy said on its standard error that it refused each. No Notice was raised and the
/// chat, told nothing, went around the block.
#[test]
fn each_host_claude_codes_proxy_refused_reaches_the_app_and_the_chat_is_told_to_wait() {
    let heard = hook(
        "posttooluse-blocked",
        &serde_json::json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": format!("curl -s -o /dev/null https://api.example.com/{CANARY}")},
            "tool_response": {
                "stdout": format!("https://api.example.com/{CANARY} 000\nhttps://console.example.com/ready 000\n"),
                "stderr": "\nShell cwd was reset to /Users/dev/plane/workspaces/a/repo\n\
                    <sandbox_violations>\n\
                    deny network-outbound api.example.com:443 (host is not on the allow list)\n\
                    deny network-outbound console.example.com:443 (host is not on the allow list)\n\
                    </sandbox_violations>",
                "interrupted": false,
            },
            "cwd": "/Users/dev/plane/workspaces/a/repo",
        }),
    );
    let host = Block {
        operation: Operation::Connect,
        kind: Kind::Host,
        ours: false,
    };
    assert_eq!(
        heard
            .blocked
            .iter()
            .map(|one| (one.sandbox_blocked, one.target.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            (host, Some("api.example.com:443")),
            (host, Some("console.example.com:443")),
        ]
    );
    let told = context_of(&heard.said, "PostToolUse");
    for words in [
        "a connection to an internet host",
        "Notice",
        "wait for their answer",
    ] {
        assert!(told.contains(words), "{words}: {told}");
    }
    assert!(!told.contains(CANARY), "{told}");
    holds_nothing_of_the_call(&heard);
}
