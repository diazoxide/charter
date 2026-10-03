//! Every tool hook `charter hook` answers tells the host it ran (FD-9, #649): the tool, a hash
//! of its arguments and never the arguments, what charter answered, and how long it took. The
//! answer the harness gets is the one it got before.

use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use charter_core::hookwire::{
    CHAT_ENV, Decision, Hearing, Listener, SOCKET_ENV, TOKEN_ENV, ToolCall, spool,
};

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

/// Runs `charter hook <word>` with `payload`, a host listening. Answers its exit code, its
/// stdout, and the tool call the host heard.
fn hook(word: &str, payload: &serde_json::Value) -> (i32, String, Option<ToolCall>) {
    hook_with(word, payload, &[])
}

/// [`hook`], with more of the environment a chat's hook is given.
fn hook_with(
    word: &str,
    payload: &serde_json::Value,
    env: &[(&str, &str)],
) -> (i32, String, Option<ToolCall>) {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue(7).expect("a token");
    let (tx, heard) = mpsc::channel();
    let tx = Mutex::new(tx);
    let _reading = listener.hear(Hearing {
        each: Box::new(|_| Ok(())),
        answer: Box::new(|_, _| panic!("no ask")),
        noticed: Box::new(|_| {}),
        saved: Box::new(|_| {}),
        refused: Box::new(|_| Ok(())),
        tool: Box::new(move |call| {
            tx.lock().unwrap().send(call).unwrap();
            Ok(())
        }),
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
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    stand_in::feed(&mut child, payload.to_string().as_bytes());
    let out = child.wait_with_output().expect("charter finishes");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        heard.recv_timeout(Duration::from_secs(5)).ok(),
    )
}

#[test]
fn a_refused_tool_call_tells_the_host_what_was_refused_and_by_which_rule() {
    let input = serde_json::json!({"command": "GIT_CONFIG_COUNT=1 git commit -m x"});
    let (code, said, heard) = hook(
        "pretooluse",
        &serde_json::json!({
            "tool_name": "Bash",
            "tool_use_id": "toolu_9",
            "tool_input": input,
            "cwd": "/tmp",
        }),
    );

    assert_eq!(code, 0);
    assert!(
        said.contains("deny"),
        "the harness still gets the refusal: {said}"
    );
    let heard = heard.expect("the host heard the tool call");
    assert_eq!(heard.chat, 7);
    assert_eq!(heard.tool_hook, "pretooluse");
    assert_eq!(heard.tool.as_deref(), Some("Bash"));
    assert_eq!(heard.call.as_deref(), Some("toolu_9"));
    assert_eq!(heard.decision, Decision::Deny);
    assert_eq!(heard.rule.as_deref(), Some("git-hook-skip"));
    assert_eq!(
        heard.args.as_deref(),
        Some(charter_core::eventlog::args_hash(&input).as_str()),
        "the arguments' hash, never the arguments"
    );
}

#[test]
fn a_tool_hook_that_decides_nothing_still_tells_the_host_it_ran() {
    let (code, _, heard) = hook(
        "posttooluse",
        &serde_json::json!({"tool_name": "Read", "tool_use_id": "toolu_3", "tool_input": {}}),
    );

    assert_eq!(code, 0);
    let heard = heard.expect("the host heard the tool call");
    assert_eq!(heard.tool_hook, "posttooluse");
    assert_eq!(heard.tool.as_deref(), Some("Read"));
    assert_eq!(heard.decision, Decision::None);
    assert_eq!(heard.rule, None);
}

#[test]
fn a_guard_that_crashed_tells_the_host_it_refused() {
    let (code, _, heard) = hook_with(
        "pretooluse",
        &serde_json::json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
        &[("CHARTER_TEST_HOOK_PANICS", "1")],
    );

    assert_eq!(code, 2, "a crashed guard refuses");
    let heard = heard.expect("the host heard the refusal");
    assert_eq!(heard.decision, Decision::Deny);
    assert_eq!(heard.rule.as_deref(), Some("guard-crashed"));
}

#[test]
fn a_guard_that_did_not_answer_in_time_tells_the_host_it_refused_for_that_reason() {
    let slow = format!("{}1{}", "$((".repeat(3000), "))".repeat(3000));
    let (code, _, heard) = hook_with(
        "pretooluse",
        &serde_json::json!({"tool_name": "Bash", "tool_input": {"command": slow}}),
        &[("CHARTER_TEST_GUARD_DEADLINE_MS", "200")],
    );

    assert_eq!(code, 2, "a guard out of time refuses");
    let heard = heard.expect("the host heard the refusal");
    assert_eq!(heard.decision, Decision::Deny);
    assert_eq!(heard.rule.as_deref(), Some("guard-unanswered"));
}

#[test]
fn a_tool_hook_word_this_binary_does_not_answer_tells_the_host_it_refused() {
    let (code, _, heard) = hook(
        "pretooluse-nonesuch",
        &serde_json::json!({"tool_name": "Bash"}),
    );

    assert_eq!(code, 2);
    let heard = heard.expect("the host heard the refusal");
    assert_eq!(heard.tool_hook, "pretooluse-nonesuch");
    assert_eq!(heard.decision, Decision::Deny);
    assert_eq!(heard.rule.as_deref(), Some("unknown-hook"));
}

#[test]
fn a_hook_an_older_plugin_still_wires_is_still_one_call_the_host_hears() {
    let (code, _, heard) = hook(
        "posttooluse-bash",
        &serde_json::json!({"tool_name": "Bash"}),
    );

    assert_eq!(code, 0);
    let heard = heard.expect("the host heard it");
    assert_eq!(heard.tool_hook, "posttooluse-bash");
    assert_eq!(heard.decision, Decision::None);
}

#[test]
fn a_sub_agents_call_names_its_agent_only_on_a_harness_where_that_was_measured() {
    let payload = serde_json::json!({"tool_name": "Read", "agent_id": "agent-7", "tool_input": {}});

    let (_, _, measured) = hook_with(
        "posttooluse",
        &payload,
        &[("CHARTER_HARNESS", "claude-code")],
    );
    let (_, _, unmeasured) = hook_with("posttooluse", &payload, &[("CHARTER_HARNESS", "opencode")]);

    assert_eq!(measured.unwrap().agent.as_deref(), Some("agent-7"));
    assert_eq!(
        unmeasured.unwrap().agent,
        None,
        "ADR 0066: opencode has no child runs"
    );
}

/// A crashed guard whose harness has closed stderr, with a host socket that is gone, or none.
fn crash_with_stderr_closed(socket: Option<&std::path::Path>) -> std::process::ExitStatus {
    let dir = tempfile::tempdir().expect("a directory");
    let mut command = Command::new(CHARTER);
    command
        .args(["hook", "pretooluse"])
        .current_dir(dir.path())
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", dir.path())
        .env(CHAT_ENV, "7")
        .env("CHARTER_TEST_HOOK_PANICS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(socket) = socket {
        command.env(SOCKET_ENV, socket);
    }
    let mut child = command.spawn().expect("charter runs");
    drop(child.stderr.take());
    drop(child.stdin.take());
    child.wait_with_output().expect("charter finishes").status
}

#[test]
fn a_crashed_guard_with_its_stderr_closed_still_refuses_with_exit_2() {
    let dir = tempfile::tempdir().expect("a directory");
    let gone = dir.path().join("gone.sock");

    assert_eq!(
        crash_with_stderr_closed(Some(&gone)).code(),
        Some(2),
        "a host that cannot be told and a stderr that cannot be written still refuse"
    );
    assert_eq!(crash_with_stderr_closed(None).code(), Some(2));
}

/// Runs `charter hook <word>` as chat 7 with `payload`, against a socket nobody listens on: the
/// host issued the chat its token and went away.
fn hook_with_no_host(word: &str, payload: &serde_json::Value) -> (i32, String, Vec<ToolCall>) {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join(".charter/app/hooks.sock");
    let token = {
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        listener.tokens().issue(7).expect("a token")
    };
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
    let mut spooled = Vec::new();
    spool::drain(&spool::dir_for(&path), &mut |item| {
        if let spool::Drained::Line {
            line: spool::Spooled::Tool(call),
            ..
        } = item
        {
            spooled.push(call);
        }
        Ok(())
    })
    .expect("the spool drains");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        spooled,
    )
}

#[test]
fn a_tool_call_no_host_takes_is_spooled_and_the_harness_gets_the_same_answer() {
    let (code, said, spooled) = hook_with_no_host(
        "pretooluse",
        &serde_json::json!({
            "tool_name": "Bash",
            "tool_use_id": "toolu_5",
            "tool_input": {"command": "GIT_CONFIG_COUNT=1 git commit -m x"},
            "cwd": "/tmp",
        }),
    );

    assert_eq!(code, 0);
    assert!(
        said.contains("deny"),
        "the harness still gets the refusal: {said}"
    );
    assert_eq!(spooled.len(), 1, "{spooled:?}");
    assert_eq!(spooled[0].call.as_deref(), Some("toolu_5"));
    assert_eq!(spooled[0].decision, Decision::Deny);
}

#[test]
fn a_tool_hook_answers_only_once_the_host_has_recorded_its_call() {
    use std::io::BufRead;
    use std::time::Instant;
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue(7).expect("a token");
    let recorded_at = std::sync::Arc::new(Mutex::new(None::<Instant>));
    let _reading = listener.hear(Hearing {
        each: Box::new(|_| Ok(())),
        answer: Box::new(|_, _| panic!("no ask")),
        noticed: Box::new(|_| {}),
        saved: Box::new(|_| {}),
        refused: Box::new(|_| Ok(())),
        tool: Box::new({
            let recorded_at = std::sync::Arc::clone(&recorded_at);
            move |_| {
                std::thread::sleep(Duration::from_millis(120));
                *recorded_at.lock().unwrap() = Some(Instant::now());
                Ok(())
            }
        }),
        permission: Box::new(|_| None),
    });
    let mut child = Command::new(CHARTER)
        .args(["hook", "pretooluse"])
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
    stand_in::feed(
        &mut child,
        serde_json::json!({
            "tool_name": "Bash",
            "tool_use_id": "t",
            "tool_input": {"command": "GIT_CONFIG_COUNT=1 git commit -m x"},
            "cwd": "/tmp",
        })
        .to_string()
        .as_bytes(),
    );
    let mut said = String::new();
    std::io::BufReader::new(child.stdout.take().expect("stdout"))
        .read_line(&mut said)
        .expect("the answer");
    let answered_at = Instant::now();
    let _ = child.wait();

    assert!(said.contains("deny"), "{said}");
    let recorded_at = recorded_at
        .lock()
        .unwrap()
        .expect("the host recorded the call");
    assert!(
        answered_at >= recorded_at,
        "the hook answered before the host had recorded its call"
    );
}
