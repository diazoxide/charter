//! A file tool's hook names the path it touched to the app, for the tree's live marker, and
//! nowhere else (FM-6, #1109; D-86a): not in the tool call's own line, which the event log
//! keeps, and not in the spool when no app is listening.
//!
//! One adapter test per harness whose hooks see a file tool: Claude Code's payload, and the
//! payload charter's opencode plugin builds. Codex's armed hook is the Bash guard, which names
//! no file.

use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use purlis_core::hookwire::{
    CHAT_ENV, Hearing, Listener, SOCKET_ENV, TOKEN_ENV, ToolCall, Touching,
};

const CHARTER: &str = env!("CARGO_BIN_EXE_purlis");

const CANARY: &str = "CANARY-touched-4f2a";

/// What the host heard of one `charter hook <word>` run as chat 7 with `payload`.
struct Heard {
    tool: Option<ToolCall>,
    touching: Vec<Touching>,
    /// What the hook printed: its answer to the harness.
    said: String,
}

fn hook(word: &str, payload: &serde_json::Value) -> Heard {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue(7).expect("a token");
    let (tools, heard_tool) = mpsc::channel();
    let tools = Mutex::new(tools);
    let (touches, heard_touch) = mpsc::channel();
    let touches = Mutex::new(touches);
    let _reading = listener.hear(Hearing {
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
        touching: Box::new(move |touching| touches.lock().unwrap().send(touching).unwrap()),
        permission: Box::new(|_| None),
    });
    let mut child = Command::new(CHARTER)
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
        .expect("charter runs");
    stand_in::feed(&mut child, payload.to_string().as_bytes());
    let out = child.wait_with_output().expect("charter finishes");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains(CANARY),
        "the hook's stderr, which goes to the harness's log, names the path: {stderr}"
    );
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    let tool = heard_tool.recv_timeout(Duration::from_secs(5)).ok();
    // The touch is written after the tool call, by the same process, which has exited.
    let touching =
        std::iter::from_fn(|| heard_touch.recv_timeout(Duration::from_millis(300)).ok()).collect();
    Heard {
        tool,
        touching,
        said,
    }
}

fn named(heard: &Heard) -> Vec<&str> {
    heard.touching.iter().map(|t| t.touching.as_str()).collect()
}

/// The tool call's line as the host would record it holds no part of the path.
fn holds_no_path(call: &ToolCall) {
    let line = serde_json::to_string(call).unwrap();
    assert!(
        !line.contains(CANARY),
        "the tool call carries the path: {line}"
    );
}

#[test]
fn a_claude_code_read_names_its_file_to_the_app() {
    let file = format!("/w/branch/src/{CANARY}.rs");
    let heard = hook(
        "pretooluse-read",
        &serde_json::json!({
            "tool_name": "Read",
            "tool_use_id": "toolu_1",
            "tool_input": {"file_path": file},
            "cwd": "/w/branch",
        }),
    );
    assert_eq!(named(&heard), [file.as_str()]);
    assert!(heard.touching.iter().all(|t| t.chat == 7));
    holds_no_path(&heard.tool.expect("the host heard the tool call"));
}

#[test]
fn a_claude_code_edit_names_its_file_before_and_after() {
    let file = format!("/w/branch/{CANARY}.md");
    for word in ["pretooluse-edit", "posttooluse"] {
        let heard = hook(
            word,
            &serde_json::json!({
                "tool_name": "Edit",
                "tool_input": {"file_path": file, "old_string": "a", "new_string": "b"},
                "cwd": "/w/branch",
            }),
        );
        assert_eq!(named(&heard), [file.as_str()], "{word}");
        holds_no_path(&heard.tool.expect("the host heard the tool call"));
    }
}

#[test]
fn an_opencode_edit_names_its_file_to_the_app() {
    // What charter's opencode plugin sends for opencode's `edit`: Claude Code's hook word and
    // tool name, opencode's own argument names.
    let file = format!("/w/branch/{CANARY}.ts");
    let heard = hook(
        "pretooluse-edit",
        &serde_json::json!({
            "hook_event_name": "PreToolUse",
            "session_id": "ses_1",
            "cwd": "/w/branch",
            "tool_name": "Edit",
            "tool_input": {"filePath": file, "oldString": "a", "newString": "b"},
        }),
    );
    assert_eq!(named(&heard), [file.as_str()]);
    holds_no_path(&heard.tool.expect("the host heard the tool call"));
}

#[test]
fn a_file_tool_the_guard_refuses_names_no_file() {
    // A read of a vault's file is refused (`leakguard`): the tool never runs, so it touched
    // nothing, and no marker goes up.
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
    assert_eq!(named(&heard), Vec::<&str>::new());
    let call = heard.tool.expect("the host heard the tool call");
    assert_eq!(call.decision, purlis_core::hookwire::Decision::Deny);
    holds_no_path(&call);
}

#[test]
fn a_shell_command_names_no_file() {
    // All Codex's armed hook sees, and Claude Code's and opencode's Bash.
    let heard = hook(
        "pretooluse",
        &serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": format!("cat /w/branch/{CANARY}.rs")},
            "cwd": "/w/branch",
        }),
    );
    assert!(heard.tool.is_some(), "the host heard the tool call");
    assert_eq!(named(&heard), Vec::<&str>::new());
}

#[test]
fn a_file_tool_no_app_takes_leaves_its_path_in_no_file() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join(".charter/app/hooks.sock");
    let token = {
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        listener.tokens().issue(7).expect("a token")
    };
    let mut child = Command::new(CHARTER)
        .args(["hook", "pretooluse-read"])
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
        .expect("charter runs");
    let payload = serde_json::json!({
        "tool_name": "Read",
        "tool_input": {"file_path": format!("/w/branch/{CANARY}.rs")},
        "cwd": "/w/branch",
    });
    stand_in::feed(&mut child, payload.to_string().as_bytes());
    let out = child.wait_with_output().expect("charter finishes");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains(CANARY), "stderr names the path: {stderr}");

    let mut files = 0;
    let mut stack = vec![dir.path().to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            files += 1;
            let text = std::fs::read(&path).unwrap_or_default();
            assert!(
                !String::from_utf8_lossy(&text).contains(CANARY),
                "{} holds the touched path",
                path.display()
            );
        }
    }
    assert!(
        files > 0,
        "the tool call itself was spooled, so the walk read something"
    );
}
