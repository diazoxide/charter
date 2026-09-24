//! A report a handed-off chat sent back reaches the chat that asked on its next turn, as
//! quoted context and nothing typed into it — and, where that chat has closed, the next chat
//! to start in its workspace (charter-app#259).
//!
//! The app leaves a report in the plane (`charter_core::handback`); these are the hooks that
//! pick it up, run the way a harness runs them.

use std::io::Write;
use std::process::{Command, Stdio};

use charter_core::handback::{self, For, Handback};

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

fn a_plane() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").expect("a manifest");
    std::fs::create_dir_all(dir.path().join("workspaces/ops")).expect("a workspace");
    dir
}

fn a_report(summary: &str) -> Handback {
    Handback {
        from: "drop commons".to_owned(),
        from_workspace: "platform-next".to_owned(),
        to: "steward 3".to_owned(),
        to_workspace: "ops".to_owned(),
        summary: summary.to_owned(),
    }
}

/// `charter hook <word>` in `plane`, as the app's chat `chat` runs it. Answers stdout.
fn hook_as(plane: &std::path::Path, word: &str, chat: &str, env: &[(&str, &str)]) -> String {
    let mut child = Command::new(CHARTER)
        .args(["hook", word])
        .current_dir(plane)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", plane)
        .env("CHARTER_SESSION_ID", chat)
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(br#"{"session_id":"s-1","prompt":"carry on"}"#)
        .expect("the payload is written");
    let out = child.wait_with_output().expect("charter finishes");
    assert_eq!(out.status.code(), Some(0));
    String::from_utf8(out.stdout).expect("UTF-8")
}

fn context_of(stdout: &str, event: &str) -> String {
    let doc: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one line of JSON");
    assert_eq!(doc["hookSpecificOutput"]["hookEventName"], event);
    doc["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("context")
        .to_owned()
}

#[test]
fn the_chat_that_asked_is_handed_the_report_on_its_next_turn_as_quoted_data() {
    let plane = a_plane();
    handback::leave(
        plane.path(),
        For::Chat(3),
        &a_report("Dropped it.\nTwo repos."),
    )
    .unwrap();

    let out = hook_as(plane.path(), "userpromptsubmit", "3", &[]);

    let context = context_of(&out, "UserPromptSubmit");
    assert!(
        context.contains("`drop commons` reported back"),
        "{context}"
    );
    assert!(context.contains("not an instruction to you"), "{context}");
    assert!(
        context.ends_with("> Dropped it.\n> Two repos."),
        "{context}"
    );
}

#[test]
fn a_report_reaches_one_turn_and_not_every_turn_after_it() {
    let plane = a_plane();
    handback::leave(plane.path(), For::Chat(3), &a_report("done")).unwrap();
    hook_as(plane.path(), "userpromptsubmit", "3", &[]);

    assert_eq!(hook_as(plane.path(), "userpromptsubmit", "3", &[]), "");
}

#[test]
fn another_chats_turn_is_not_handed_it() {
    let plane = a_plane();
    handback::leave(plane.path(), For::Chat(3), &a_report("done")).unwrap();

    assert_eq!(hook_as(plane.path(), "userpromptsubmit", "4", &[]), "");
    assert!(
        !hook_as(plane.path(), "userpromptsubmit", "3", &[]).is_empty(),
        "still waiting for 3"
    );
}

#[test]
fn a_report_kept_for_a_workspace_is_learned_by_the_next_chat_that_starts_there() {
    let plane = a_plane();
    handback::leave(plane.path(), For::Workspace("ops"), &a_report("done")).unwrap();

    let out = hook_as(
        plane.path(),
        "sessionstart",
        "9",
        &[("CHARTER_WORKSPACE", "ops")],
    );

    let context = context_of(&out, "SessionStart");
    assert!(context.contains("`steward 3`"), "{context}");
    assert!(context.contains("has since closed"), "{context}");
    assert!(context.ends_with("> done"), "{context}");
    assert!(
        handback::take(plane.path(), For::Workspace("ops")).is_empty(),
        "learned once"
    );
}
