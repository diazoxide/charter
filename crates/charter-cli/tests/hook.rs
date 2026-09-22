//! `charter hook <event>` as the harness runs it: a real process, a real socket.
//!
//! The one rule behind every test here is that a hook must never cost a harness its turn.
//! Whatever is wrong — no app, a socket that has gone, a payload that will not parse, a word
//! charter does not know — the hook gets out of the way, quickly and quietly.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use charter_core::hookwire::{CHAT_ENV, Conversation, Listener, Report, SOCKET_ENV};
use charter_core::state::{Detail, Event};

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

const CLAUDE_STOP: &str = r#"{"session_id":"11111111-2222-4333-8444-555555555555","cwd":"/tmp","hook_event_name":"Stop","stop_hook_active":false}"#;

/// Runs `charter hook <word>` with `payload` on stdin. Answers its exit code.
fn hook(word: &str, payload: &str, env: &[(&str, &str)]) -> i32 {
    let mut child = Command::new(CHARTER)
        .args(["hook", word])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
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
        .write_all(payload.as_bytes())
        .expect("the payload is written");
    child.wait().expect("charter finishes").code().unwrap_or(-1)
}

#[test]
fn a_hook_tells_the_app_what_the_harness_fired() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let (tx, rx) = mpsc::channel();
    let _reading = Listener::bind(dir.path(), &path)
        .expect("a socket")
        .each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

    let code = hook(
        "stop",
        CLAUDE_STOP,
        &[
            (SOCKET_ENV, path.to_str().expect("a path")),
            (CHAT_ENV, "7"),
            // A Claude Code hook carries both, and they agree (ADR 0024, C7). The report
            // names a conversation only when they do.
            (
                "CLAUDE_CODE_SESSION_ID",
                "11111111-2222-4333-8444-555555555555",
            ),
            ("CLAUDE_PID", "4242"),
        ],
    );

    assert_eq!(code, 0);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Named("11111111-2222-4333-8444-555555555555".to_owned()),
            pid: Some(4242),
            detail: Detail::default(),
        })
    );
}

#[test]
fn every_state_event_reaches_the_app_under_the_word_the_plugin_uses() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let (tx, rx) = mpsc::channel();
    let _reading = Listener::bind(dir.path(), &path)
        .expect("a socket")
        .each(Box::new(move |report| {
            let _ = tx.send(report.event);
        }));

    for event in [
        Event::SessionStart,
        Event::UserPromptSubmit,
        Event::Notification,
        Event::SubagentStop,
        Event::Stop,
        Event::SessionEnd,
    ] {
        let code = hook(
            event.word(),
            "{}",
            &[
                (SOCKET_ENV, path.to_str().expect("a path")),
                (CHAT_ENV, "1"),
            ],
        );

        assert_eq!(code, 0, "`charter hook {}` refused", event.word());
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(event));
    }
}

#[test]
fn no_command_line_mistake_can_make_this_binary_exit_two() {
    // **Exit 2 is the one code that means something.** Claude Code reads it as "block", and
    // on `Stop` that makes the harness unable to end. It is reserved here for a hook word
    // this binary does not own, which is a refusal it means — and nothing else may reach it.
    //
    // This used to test only command lines beginning with the literal `hook`, because the
    // binary only guarded those. A review broke it in one line: `charter hooks stop`, a typo
    // in the SUBCOMMAND rather than the event word, went to clap and exited 2. The shape of
    // the command line was the wrong thing to reason about.
    for argv in [
        vec!["hooks", "stop"],
        vec!["hok", "stop"],
        vec!["HOOK", "stop"],
        vec!["hook.", "stop"],
        vec!["Hook", "stop"],
        vec![],
        vec!["--no-such-flag", "hook", "stop"],
        vec!["hook", "stop", "--no-such-flag"],
        vec!["hook", "stop", "extra"],
        vec!["hook"],
        vec!["root", "--no-such-flag"],
        vec!["bogus"],
    ] {
        let out = Command::new(CHARTER)
            .args(&argv)
            .stdin(Stdio::null())
            .output()
            .expect("charter runs");

        assert_ne!(
            out.status.code(),
            Some(2),
            "`charter {}` exited 2, which a harness reads as `block`",
            argv.join(" ")
        );
    }

    // And asking for help is not a failure. A surviving mutant: mapping every clap error to
    // `FAILURE` left `--help` and `--version` exiting 1, which nothing pinned.
    for argv in [vec!["--help"], vec!["--version"], vec!["help", "hook"]] {
        let out = Command::new(CHARTER)
            .args(&argv)
            .stdin(Stdio::null())
            .output()
            .expect("charter runs");

        assert_eq!(
            out.status.code(),
            Some(0),
            "`charter {}` did not succeed",
            argv.join(" ")
        );
    }
}

#[test]
fn a_tool_hook_this_binary_does_not_answer_blocks_rather_than_allowing() {
    // A denylist of the words charter happens to know fails OPEN on everything else:
    // `charter hook pretooluse-notebook` exited 1, which a harness logs and ignores, so the
    // day charter adds a matcher the Rust binary on PATH would allow that whole tool class
    // silently. The namespace covers every matcher there is and every one there will be.
    //
    // **`pretooluse` is NOT here any more (M3.1 stage 6).** It is the one word in the namespace
    // this binary has ported and answers; the tests below are its. Every other word — the eight
    // the charter plugin wires and every one charter has not invented — still blocks, and the
    // argument is unchanged: a program that has checked nothing may not say `allow`.
    for word in [
        // The guard as it stands, minus the one answered.
        "pretooluse-read",
        "pretooluse-edit",
        "pretooluse-dispatch",
        "posttooluse",
        "posttooluse-bash",
        "posttooluse-skill",
        "posttooluse-dispatch",
        "posttooluse-message",
        // And matchers charter has not added yet.
        "pretooluse-notebook",
        "posttooluse-web",
    ] {
        let out = Command::new(CHARTER)
            .args(["hook", word])
            .stdin(Stdio::null())
            .output()
            .expect("charter runs");

        assert_eq!(
            out.status.code(),
            Some(2),
            "`charter hook {word}` did not block; a harness reads anything else as allow"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("PATH"),
            "`charter hook {word}` refused without saying what to check"
        );
    }
}

/// `charter hook pretooluse` with `payload` on stdin, standing in `cwd`. Answers
/// `(exit code, stdout, stderr)`.
///
/// `env_clear`, and `$CHARTER_ROOT` is set positively on every call: a guard reads the plane
/// from the PROCESS's directory, and an inherited `$CHARTER_ROOT` would point this test at the
/// operator's own plane — which is the fence this suite is under (charter-app#132).
fn guard(cwd: &std::path::Path, payload: &str, env: &[(&str, &str)]) -> (i32, String, String) {
    let mut child = Command::new(CHARTER)
        .args(["hook", "pretooluse"])
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("CHARTER_ROOT", cwd)
        .env("CHARTER_HARNESS", "claude-code")
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
        .write_all(payload.as_bytes())
        .expect("the payload is written");
    let out = child.wait_with_output().expect("charter finishes");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A payload for one Bash call, as a harness sends it.
fn bash(command: &str) -> String {
    serde_json::json!({
        "session_id": "11111111-2222-4333-8444-555555555555",
        "cwd": ".",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
    })
    .to_string()
}

/// A plane with a `charter.toml` and a vault in it — enough for the gate to open and for the
/// leak guard to have something real to refuse.
fn a_plane() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").expect("the marker");
    let vaults = dir.path().join(".charter").join("vaults");
    std::fs::create_dir_all(&vaults).expect("the vault directory");
    std::fs::write(vaults.join("db.json"), "{}\n").expect("a vault");
    dir
}

/// The `permissionDecisionReason` this hook printed, or `None`.
fn decision(stdout: &str) -> Option<String> {
    let out: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let out = out.get("hookSpecificOutput")?;
    if out.get("permissionDecision")?.as_str()? != "deny" {
        return None;
    }
    Some(out.get("permissionDecisionReason")?.as_str()?.to_owned())
}

#[test]
fn the_bash_guard_refuses_by_printing_and_exits_cleanly() {
    // **A `PreToolUse` hook refuses by PRINTING.** The verdict is the JSON on stdout and the
    // status is 0; exit 2 is the fallback for a denial that could not be delivered, and every
    // other non-zero status is a non-blocking error the harness logs while the tool call goes
    // ahead. Getting this wrong in the safe-looking direction — exit 2 as well — would make
    // the harness show a refusal with no reason attached.
    let plane = a_plane();
    let (code, out, err) = guard(plane.path(), &bash("cat .charter/vaults/db.json"), &[]);

    assert_eq!(code, 0, "a denial is exit 0; the JSON is the refusal");
    assert_eq!(err, "", "nothing on stderr when the verdict was delivered");
    let said = decision(&out).unwrap_or_else(|| panic!("a denial: {out:?}"));
    assert!(said.starts_with("charter guard: "), "{said}");
    assert!(
        said.contains("reads a vault/secret file directly"),
        "{said}"
    );
    assert!(
        said.contains("no config key, environment variable or switch"),
        "every denial carries the override note: {said}"
    );
}

#[test]
fn an_ordinary_command_is_answered_with_nothing_at_all() {
    // Silence is how a `PreToolUse` hook allows. This is the switch's real subject: before it,
    // every one of these exited 2 and the tool call was refused.
    let plane = a_plane();
    for command in [
        "git status",
        "charter handoff beta <<'BRIEF'\nship it\nBRIEF",
        "echo hello",
    ] {
        let (code, out, err) = guard(plane.path(), &bash(command), &[]);
        assert_eq!(code, 0, "{command:?}");
        assert_eq!(out, "", "{command:?} was not allowed: {out}");
        assert_eq!(err, "", "{command:?}");
    }
}

#[test]
fn the_guard_survives_every_payload_a_harness_could_send() {
    // A hook that crashed on one of these would take the tool call with it, and a non-zero
    // exit that is not 2 is a NON-blocking error — so the failure would be silent. `""` is
    // what the deadline in `payload()` produces when a harness opens stdin and never writes.
    let plane = a_plane();
    for payload in [
        "",
        "not json {{{",
        "null",
        "[]",
        "{}",
        r#"{"tool_input": null}"#,
        r#"{"tool_input": {"command": null}}"#,
        r#"{"tool_input": {"command": 7}, "cwd": 7, "permission_mode": 7, "agent_id": 7}"#,
    ] {
        let (code, out, err) = guard(plane.path(), payload, &[]);
        assert_eq!(code, 0, "{payload:?} did not answer cleanly: {err}");
        assert_eq!(out, "", "{payload:?} refused something it cannot have read");
    }
}

#[test]
fn outside_a_plane_the_gated_arms_are_silent_and_the_others_are_not() {
    // charter#852. The plugin is installed per user, so this handler runs in every repository
    // on the machine; the arms that are about a control plane must say nothing where there is
    // none, and the arms that are about the SHELL must still speak.
    //
    // **The gate is the `charter.toml`, not a resolved directory.** `$CHARTER_ROOT` is set on
    // every call here and points at a directory that is not a plane, which is exactly the case
    // the differential caught the first port of this getting wrong.
    let bare = tempfile::tempdir().expect("a directory");
    let vaults = bare.path().join(".charter").join("vaults");
    std::fs::create_dir_all(&vaults).expect("the vault directory");
    std::fs::write(vaults.join("db.json"), "{}\n").expect("a vault");

    for gated in [
        "git clone git@github.com:o/r.git",
        "charter handoff beta",
        "gh release create v1.0.0",
    ] {
        let (code, out, _) = guard(bare.path(), &bash(gated), &[]);
        assert_eq!(code, 0);
        assert_eq!(out, "", "{gated:?} was refused where there is no plane");
    }
    for ungated in [
        "cat .charter/vaults/db.json",
        "charter persona remember d \"$(cat notes)\"",
    ] {
        let (_, out, _) = guard(bare.path(), &bash(ungated), &[]);
        assert!(
            decision(&out).is_some(),
            "{ungated:?} is a fact about the shell and must still be refused: {out:?}"
        );
    }
}

#[test]
fn the_two_payload_fields_no_command_can_see_are_read() {
    // A7's whole reason for being the hook's and not the command's. Neither `agent_id` nor
    // `permission_mode` is anything `charter handoff` could ask about once it is running.
    let plane = a_plane();
    let canonical = "charter handoff beta <<'BRIEF'\nship it\nBRIEF";
    let with = |field: &str, value: &str| {
        let mut payload: serde_json::Value =
            serde_json::from_str(&bash(canonical)).expect("the payload");
        payload[field] = serde_json::Value::String(value.to_owned());
        payload.to_string()
    };

    let (_, out, _) = guard(plane.path(), &with("agent_id", "sub-1"), &[]);
    assert!(
        decision(&out).is_some_and(|d| d.contains("from inside a sub-agent")),
        "{out:?}"
    );
    let (_, out, _) = guard(
        plane.path(),
        &with("permission_mode", "bypassPermissions"),
        &[],
    );
    assert!(
        decision(&out).is_some_and(|d| d.contains("in an unattended run")),
        "{out:?}"
    );
    // ...and on a harness nobody measured, an `agent_id` means nothing at all.
    let (_, out, _) = guard(
        plane.path(),
        &with("agent_id", "sub-1"),
        &[("CHARTER_HARNESS", "opencode")],
    );
    assert_eq!(out, "", "an unmeasured harness is not read as a sub-agent");
}

#[test]
fn the_guard_answers_the_command_line_the_plugin_actually_writes() {
    // `hooks/hooks.json` calls `charter hook pretooluse --plugin-version 0.62.1`, never the
    // bare word. The flag is taken and ignored here, as it is for every other event: the skew
    // check it feeds is the Python charter's. What must NOT happen is the `sessionstart`
    // notice — a `systemMessage` on every tool call is a line the operator reads once and
    // then learns to ignore, which is the gate `_queue_plugin_notices` exists for.
    let plane = a_plane();
    let mut child = Command::new(CHARTER)
        .args(["hook", "pretooluse", "--plugin-version", "0.62.1"])
        .current_dir(plane.path())
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("CHARTER_ROOT", plane.path())
        .env("CHARTER_HARNESS", "claude-code")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(bash("charter handoff beta").as_bytes())
        .expect("written");
    let out = child.wait_with_output().expect("charter finishes");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();

    assert_eq!(out.status.code(), Some(0));
    assert!(decision(&stdout).is_some(), "{stdout:?}");
    assert!(
        !stdout.contains("systemMessage"),
        "the plugin notice is a session-start line, not a per-tool-call one: {stdout}"
    );
}

#[test]
fn the_denial_is_written_the_way_python_writes_it() {
    // Byte for byte with `json.dumps`: Python's separators and `ensure_ascii`. The denials
    // carry `—` and `…`, and a `serde_json` default would pack the separators and write both
    // characters literally — which the differential compares stdout on.
    let plane = a_plane();
    let (_, out, _) = guard(plane.path(), &bash("charter handoff beta"), &[]);

    assert!(
        out.starts_with(r#"{"hookSpecificOutput": {"hookEventName": "PreToolUse", "#),
        "{out}"
    );
    assert!(out.contains(r#""permissionDecision": "deny", "#), "{out}");
    assert!(out.contains("\\u2014"), "an em dash is escaped: {out}");
    assert!(
        out.ends_with("}}\n"),
        "one line, newline-terminated: {out:?}"
    );
}

#[test]
fn a_typo_in_a_state_event_never_blocks_the_session() {
    // **The hazard a blanket block brought back, and the reason refusing is by namespace.**
    // charter writes the settings file that names these words, so a typo in one is charter's
    // own — and `charter hook stopp` blocking would mean a session that cannot END, which is
    // the exact failure this binary's exit codes exist to prevent. A review found it.
    for word in [
        "stopp",
        "Stop",
        "sessionstartt",
        "notification ",
        "",
        "sessionend2",
        "subagent-stop",
    ] {
        let out = Command::new(CHARTER)
            .args(["hook", word])
            .stdin(Stdio::null())
            .output()
            .expect("charter runs");

        assert_eq!(
            out.status.code(),
            Some(1),
            "`charter hook {word}` did not fail quietly; 2 would wedge the session"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("not one of this binary's events"),
            "`charter hook {word}` failed without saying why"
        );
    }
}

#[test]
fn the_command_line_the_charter_plugin_actually_writes_is_answered() {
    // `hooks/hooks.json` does not call `charter hook sessionstart`. Every one of its twelve
    // entries calls `charter hook <word> --plugin-version 0.62.1`, and this binary rejected
    // the flag outright — an independent review found the test that claimed otherwise.
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let (tx, rx) = mpsc::channel();
    let _reading = Listener::bind(dir.path(), &path)
        .expect("a socket")
        .each(Box::new(move |report| {
            let _ = tx.send(report.event);
        }));

    let mut child = Command::new(CHARTER)
        .args(["hook", "sessionstart", "--plugin-version", "0.62.1"])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env(SOCKET_ENV, &path)
        .env(CHAT_ENV, "7")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"{}")
        .expect("written");
    let out = child.wait_with_output().expect("charter finishes");

    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Event::SessionStart)
    );
}

#[test]
fn the_plugin_notice_is_said_once_a_session_and_not_once_a_turn() {
    // The plugin wires `sessionstart`, `userpromptsubmit` and `stop`, so an ungated notice
    // reaches the operator on every prompt and every turn end. `charter/hooks.py` carries
    // "the gate that keeps them to sessionstart" for the same reason.
    for word in [
        "userpromptsubmit",
        "stop",
        "notification",
        "sessionend",
        "subagentstop",
    ] {
        let out = Command::new(CHARTER)
            .args(["hook", word, "--plugin-version", "0.62.1"])
            .stdin(Stdio::null())
            .output()
            .expect("charter runs");

        assert_eq!(out.status.code(), Some(0));
        assert!(
            out.stdout.is_empty(),
            "`charter hook {word}` spoke up: {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

#[test]
fn a_hook_with_no_app_behind_it_says_nothing_and_succeeds() {
    // The operator's own `claude` in a terminal, with the plugin's hooks pointed here: no
    // socket in the environment, so there is nothing to tell and nothing to complain about.
    let out = Command::new(CHARTER)
        .args(["hook", "stop"])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .stdin(Stdio::null())
        .output()
        .expect("charter runs");

    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "it wrote to the transcript");
    assert!(
        out.stderr.is_empty(),
        "it wrote {:?} to the debug log on every hook",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_hook_whose_app_has_gone_succeeds_anyway() {
    // The app quit while a session was still running. The hook finds a socket path that
    // leads nowhere, and the turn must not notice.
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("gone.sock");

    let out = Command::new(CHARTER)
        .args(["hook", "stop"])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env(SOCKET_ENV, &path)
        .env(CHAT_ENV, "7")
        .stdin(Stdio::null())
        .output()
        .expect("charter runs");

    // Succeeds, because the turn is not the place to complain about a spinner.
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "it wrote to the transcript");
    // But says so somewhere: a report that never arrives is otherwise invisible — the chat
    // just stops changing, with nothing anywhere to look at. A zero-exit hook's stderr goes
    // to the harness's debug log, which is where somebody debugging this would look.
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("did not take") && said.contains("stop"),
        "{said:?}"
    );
}

#[test]
fn a_hook_reading_a_payload_that_never_ends_still_gets_out_of_the_way() {
    // A harness that opens the hook's stdin and does not write is not a shape anything is
    // known to produce — but it is the shape that would hang a turn forever, and the hook
    // has a deadline of its own rather than trusting the harness to close.
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("hooks.sock");
    let (tx, rx) = mpsc::channel();
    let _reading = Listener::bind(dir.path(), &path)
        .expect("a socket")
        .each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

    let mut child = Command::new(CHARTER)
        .args(["hook", "stop"])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env(SOCKET_ENV, &path)
        .env(CHAT_ENV, "7")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("charter runs");
    // Held open, never written to, never closed until the child is gone.
    let held = child.stdin.take().expect("stdin");

    let report = rx.recv_timeout(Duration::from_secs(10));
    drop(held);
    let code = child.wait().expect("charter finishes").code();

    assert_eq!(
        report.map(|report| report.event),
        Ok(Event::Stop),
        "the event never reached the app"
    );
    assert_eq!(code, Some(0));
}
