//! The commitment gate on `userpromptsubmit` (charter#369): a prompt that asks for work AND
//! leaves a real fork open is told to scout and then ask before it builds. Everything else is
//! silent, because a nudge that fires on a lookup is one the reader learns to skip.
//!
//! Run the way a harness runs it: `charter hook userpromptsubmit` in a plane, the payload on
//! stdin.

use std::process::{Command, Stdio};

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

fn a_plane() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").expect("a manifest");
    dir
}

/// `charter hook userpromptsubmit` in `dir` with `payload` on stdin, and no app around it.
fn prompt_in(dir: &std::path::Path, payload: &serde_json::Value) -> String {
    prompt_as(dir, payload, &[])
}

/// The same, with `env` set as well — the app's chat number, say.
fn prompt_as(dir: &std::path::Path, payload: &serde_json::Value, env: &[(&str, &str)]) -> String {
    let mut child = Command::new(CHARTER)
        .args(["hook", "userpromptsubmit"])
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", dir)
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    stand_in::feed(&mut child, payload.to_string().as_bytes());
    let out = child.wait_with_output().expect("charter finishes");
    assert_eq!(out.status.code(), Some(0));
    String::from_utf8(out.stdout).expect("UTF-8")
}

fn asking(prompt: &str) -> serde_json::Value {
    serde_json::json!({"session_id": "s-1", "prompt": prompt, "permission_mode": "default"})
}

fn context_of(stdout: &str) -> String {
    let doc: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one line of JSON");
    assert_eq!(
        doc["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
    doc["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("context")
        .to_owned()
}

const FORKED: &str = "maybe refactor the importer so it is cleaner across every repo";

#[test]
fn work_with_a_fork_in_it_is_told_to_scout_and_ask_before_building() {
    let plane = a_plane();

    let context = context_of(&prompt_in(plane.path(), &asking(FORKED)));

    assert!(context.contains("Commitment point"), "{context}");
    assert!(context.contains("open-ended wording"), "{context}");
    assert!(context.contains("broad scope"), "{context}");
    assert!(context.contains("Scout first"), "{context}");
}

#[test]
fn a_lookup_or_work_with_no_fork_says_nothing() {
    let plane = a_plane();

    for quiet in [
        "why is the importer slow? maybe the cache",
        "fix the typo on line 4",
        "what does refactor mean across every repo",
        "/review maybe refactor everything",
    ] {
        assert_eq!(prompt_in(plane.path(), &asking(quiet)), "", "{quiet}");
    }
}

#[test]
fn it_is_quiet_for_the_next_prompts_while_the_answer_is_being_worked_out() {
    let plane = a_plane();
    assert!(!prompt_in(plane.path(), &asking(FORKED)).is_empty());

    for _ in 0..3 {
        assert_eq!(prompt_in(plane.path(), &asking(FORKED)), "");
    }
    assert!(
        !prompt_in(plane.path(), &asking(FORKED)).is_empty(),
        "due again"
    );
}

#[test]
fn the_quiet_is_counted_in_prompts_of_any_kind() {
    // The answers to its own question are what the quiet is for, and they seldom look like a
    // fork: "the second option", "yes". Three of them spend it.
    let plane = a_plane();
    assert!(!prompt_in(plane.path(), &asking(FORKED)).is_empty());

    for answer in ["the second option", "yes", "go on"] {
        assert_eq!(prompt_in(plane.path(), &asking(answer)), "", "{answer}");
    }
    assert!(
        !prompt_in(plane.path(), &asking(FORKED)).is_empty(),
        "a new fork after the quiet is told"
    );
}

#[test]
fn an_unattended_run_is_never_told_to_ask_anyone() {
    let plane = a_plane();
    let mut payload = asking(FORKED);
    payload["permission_mode"] = "bypassPermissions".into();

    assert_eq!(prompt_in(plane.path(), &payload), "");
}

#[test]
fn outside_a_plane_it_says_nothing() {
    let dir = tempfile::tempdir().expect("a directory");

    assert_eq!(prompt_in(dir.path(), &asking(FORKED)), "");
}

#[test]
fn a_symptom_is_told_to_diagnose_rather_than_to_design() {
    let plane = a_plane();

    let context = context_of(&prompt_in(
        plane.path(),
        &asking("fix the importer crash, maybe it's the cache"),
    ));

    assert!(context.contains("symptom to diagnose"), "{context}");
    assert!(context.contains("failing test"), "{context}");
}

#[test]
fn the_gate_and_a_report_back_arrive_as_one_context() {
    use charter_core::handback::{self, For, Handback};
    let plane = a_plane();
    let report = Handback {
        from: "drop commons".to_owned(),
        from_workspace: "platform-next".to_owned(),
        to: "steward 3".to_owned(),
        to_workspace: "ops".to_owned(),
        summary: "Dropped it.".to_owned(),
    };
    handback::leave(plane.path(), For::Chat(3), &report).unwrap();

    let out = prompt_as(
        plane.path(),
        &asking(FORKED),
        &[(charter_core::hookwire::CHAT_ENV, "3")],
    );

    assert_eq!(out.lines().count(), 1, "{out}");
    let context = context_of(&out);
    assert!(context.contains("Commitment point"), "{context}");
    assert!(context.ends_with("> Dropped it."), "{context}");
}
