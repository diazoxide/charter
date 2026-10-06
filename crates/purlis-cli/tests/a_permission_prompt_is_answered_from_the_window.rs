//! HP-6: a Claude Code chat's permission prompt is answered from charter's window, through the
//! harness's own `PermissionRequest` hook, without the chat's pane having focus. The hook
//! prints Claude Code's decision for the option the operator chose, and prints nothing — so
//! the pane asks — when nobody answers.

#![cfg(unix)]

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use purlis_core::harness::asks::{Admitted, Answerer, Asks, Raised, Refused};
use purlis_core::harness::hooked::HookAsks;
use purlis_core::hookwire::permission::held_in;
use purlis_core::hookwire::{CHAT_ENV, Hearing, Listener, Reading, SOCKET_ENV, TOKEN_ENV};

const CHARTER: &str = env!("CARGO_BIN_EXE_purlis");

/// What Claude Code hands its `PermissionRequest` hook for a Bash call (hooks reference).
fn payload() -> serde_json::Value {
    serde_json::json!({
        "session_id": "abc",
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        "tool_input": {"command": "npm test"},
        "permission_suggestions": [
            {"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "npm test"}],
             "behavior": "allow", "destination": "session"}
        ]
    })
}

/// A host listening, holding permission asks in `hooks`, and a chat numbered 7 it issued a
/// token to.
///
/// Its fields drop in this order: the reading stops, which wakes it through the socket, before
/// the directory holding the socket goes.
struct Host {
    _reading: Reading,
    path: std::path::PathBuf,
    token: String,
    hooks: Arc<HookAsks>,
    dir: tempfile::TempDir,
}

fn host() -> Host {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener
        .tokens()
        .issue(7)
        .expect("a token")
        .expose()
        .to_owned();
    let hooks = Arc::new(HookAsks::new(Arc::new(Asks::new())));
    let reading = listener.hear(Hearing {
        each: Box::new(|_| Ok(())),
        answer: Box::new(|_, _| panic!("no ask for a ticket")),
        noticed: Box::new(|_| {}),
        saved: Box::new(|_| {}),
        refused: Box::new(|_| Ok(())),
        tool: Box::new(|_| Ok(())),
        blocked: Box::new(|_| {}),
        touching: Box::new(|_| {}),
        permission: held_in(Arc::clone(&hooks), Arc::new(|| {})),
    });
    Host {
        _reading: reading,
        path,
        token,
        hooks,
        dir,
    }
}

/// `charter hook permissionrequest` as Claude Code runs a command hook: through `/bin/sh -c`,
/// so the hook's host is its grandparent and the ancestry walk takes more than one step.
fn the_hook_through_a_shell() -> Command {
    let mut sh = Command::new("/bin/sh");
    sh.args(["-c", "\"$0\" hook permissionrequest", CHARTER]);
    sh
}

/// `charter hook permissionrequest` as Claude Code runs it in chat 7, `payload` on stdin.
fn hook(host: &Host, chat: &str) -> Child {
    let mut child = the_hook_through_a_shell()
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", host.dir.path())
        .env(SOCKET_ENV, &host.path)
        .env(CHAT_ENV, chat)
        .env(TOKEN_ENV, &host.token)
        // A group of its own, so the shell and the hook under it stop together, as Claude
        // Code stops a hook it no longer waits for.
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    stand_in::feed(&mut child, payload().to_string().as_bytes());
    child
}

fn waiting(hooks: &HookAsks) -> Raised {
    let began = Instant::now();
    loop {
        if let Some(raised) = hooks.pending(Instant::now()).into_iter().next() {
            return raised;
        }
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "the hook never asked"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn the_window() -> Answerer {
    Answerer::admitted(Admitted::LocalUi).expect("the window answers")
}

#[test]
fn approving_in_the_window_prints_claude_code_s_allow_on_the_hook() {
    let host = host();
    let child = hook(&host, "7");
    let raised = waiting(&host.hooks);
    assert_eq!(raised.chat, "7");
    let offered: Vec<&str> = raised.ask.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(offered, ["allow", "suggestion:0", "deny"]);

    host.hooks
        .answer("7", &raised.id, "allow", the_window(), Instant::now())
        .expect("the first answer applies");
    let out = child.wait_with_output().expect("the hook ends");

    assert!(out.status.success());
    let said: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("one line of JSON on stdout");
    assert_eq!(
        said,
        serde_json::json!({"hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {"behavior": "allow"},
        }})
    );
}

#[test]
fn a_session_rule_chosen_in_the_window_goes_back_as_claude_code_s_permission_update() {
    let host = host();
    let child = hook(&host, "7");
    let raised = waiting(&host.hooks);

    host.hooks
        .answer(
            "7",
            &raised.id,
            "suggestion:0",
            the_window(),
            Instant::now(),
        )
        .expect("applies");
    let out = child.wait_with_output().expect("the hook ends");

    let said: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(
        said["hookSpecificOutput"]["decision"]["updatedPermissions"],
        serde_json::json!([payload()["permission_suggestions"][0].clone()])
    );
}

#[test]
fn an_answer_for_another_chat_is_refused_and_the_hook_still_waits_for_its_own() {
    let host = host();
    let child = hook(&host, "7");
    let raised = waiting(&host.hooks);

    assert_eq!(
        host.hooks
            .answer("8", &raised.id, "allow", the_window(), Instant::now()),
        Err(Refused::Unknown)
    );
    host.hooks
        .answer("7", &raised.id, "deny", the_window(), Instant::now())
        .expect("its own chat's answer applies");
    let out = child.wait_with_output().expect("the hook ends");

    let said: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(said["hookSpecificOutput"]["decision"]["behavior"], "deny");
}

#[test]
fn a_hook_the_harness_stops_withdraws_its_ask_from_the_window() {
    // The operator answered in the pane, and Claude Code stopped the hook.
    let host = host();
    let mut child = hook(&host, "7");
    let raised = waiting(&host.hooks);

    // The shell and the hook below it, by their group (the pid of the group's leader).
    let stopped = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{}", child.id())])
        .status()
        .expect("kill runs");
    assert!(stopped.success());
    let _ = child.wait();
    let began = Instant::now();
    while !host.hooks.pending(Instant::now()).is_empty() {
        assert!(began.elapsed() < Duration::from_secs(10), "never withdrawn");
        std::thread::sleep(Duration::from_millis(20));
    }

    assert_eq!(
        host.hooks
            .answer("7", &raised.id, "allow", the_window(), Instant::now()),
        Err(Refused::Withdrawn)
    );
}

#[test]
fn with_no_app_listening_the_hook_prints_nothing_and_the_pane_asks() {
    let dir = tempfile::tempdir().expect("a directory");
    let mut child = the_hook_through_a_shell()
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    stand_in::feed(&mut child, payload().to_string().as_bytes());
    let out = child.wait_with_output().expect("the hook ends");

    assert!(out.status.success());
    assert!(
        out.stdout.is_empty(),
        "{:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Where [`a_stand_in_at_the_socket`] binds, when this test binary is run as that stand-in.
const STAND_IN_AT: &str = "CHARTER_TEST_STAND_IN_AT";
/// Where it writes whatever a hook told it.
const STAND_IN_HEARD: &str = "CHARTER_TEST_STAND_IN_HEARD";

/// Not a test: this binary, run again as a process beside the hook (as anything a chat runs
/// is), binds the hook socket's path, keeps whatever it is told, and answers "allow".
#[test]
#[ignore = "a helper the stand-in test starts as a process of its own"]
fn a_stand_in_at_the_socket() {
    use std::io::{BufRead, Write};

    let (Some(path), Some(heard)) = (
        std::env::var_os(STAND_IN_AT),
        std::env::var_os(STAND_IN_HEARD),
    ) else {
        return;
    };
    let listener = std::os::unix::net::UnixListener::bind(path).expect("bound");
    println!("up");
    for connection in listener.incoming() {
        let Ok(mut connection) = connection else {
            continue;
        };
        let _ = connection.set_read_timeout(Some(Duration::from_secs(2)));
        let mut line = String::new();
        let _ = std::io::BufReader::new(&connection).read_line(&mut line);
        std::fs::write(&heard, &line).expect("kept");
        let _ = connection.write_all(b"{\"chosen\":\"allow\"}\n");
    }
}

#[test]
fn a_stand_in_the_chat_bound_at_the_socket_is_told_nothing_and_allows_nothing() {
    // D-88n: a process beside the hook, not its ancestor, listening where the host should be.
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let heard = dir.path().join("heard");
    let mut stand_in = Command::new(std::env::current_exe().expect("this test"))
        .args([
            "--exact",
            "a_stand_in_at_the_socket",
            "--ignored",
            "--nocapture",
        ])
        .env(STAND_IN_AT, &path)
        .env(STAND_IN_HEARD, &heard)
        .stdout(Stdio::piped())
        .spawn()
        .expect("the stand-in starts");
    let mut said = std::io::BufReader::new(stand_in.stdout.take().expect("piped"));
    let mut line = String::new();
    while !line.contains("up") {
        line.clear();
        if std::io::BufRead::read_line(&mut said, &mut line).expect("it speaks") == 0 {
            break;
        }
    }

    let mut child = the_hook_through_a_shell()
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", dir.path())
        .env(SOCKET_ENV, &path)
        .env(CHAT_ENV, "7")
        .env(TOKEN_ENV, "a-chat-token")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    stand_in::feed(&mut child, payload().to_string().as_bytes());
    let out = child.wait_with_output().expect("the hook ends");
    let _ = stand_in.kill();
    let _ = stand_in.wait();

    assert!(out.status.success());
    assert!(
        out.stdout.is_empty(),
        "the hook printed a decision: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let told = std::fs::read_to_string(&heard).unwrap_or_default();
    assert!(told.is_empty(), "the stand-in was told: {told}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not the host"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
