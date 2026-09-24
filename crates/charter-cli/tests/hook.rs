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
///
/// Outside every plane: `sessionstart` and `userpromptsubmit` now read the plane they stand in,
/// and the test process stands in a checkout that may itself sit inside the operator's plane.
fn hook(word: &str, payload: &str, env: &[(&str, &str)]) -> i32 {
    let mut child = Command::new(CHARTER)
        .args(["hook", word])
        .current_dir(std::env::temp_dir())
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
    // **Every word charter has today is answered now** (the registry, `hookreg`), so what is
    // left here is only the word nobody has invented yet — and the argument is unchanged: a
    // program that has checked nothing may not say `allow`.
    for word in ["pretooluse-notebook", "posttooluse-web", "pretooluse-"] {
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
            String::from_utf8_lossy(&out.stderr).contains("charter hook --list"),
            "`charter hook {word}` refused without saying where the answered words are"
        );
    }
}

/// `charter <args>` with `payload` on stdin, standing in `cwd` with nothing but `PATH` and a
/// `HOME` in the environment. Answers the exit code, stdout and stderr.
fn run_hook(cwd: &std::path::Path, args: &[&str], payload: &str) -> (i32, String, String) {
    let mut child = Command::new(CHARTER)
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", cwd)
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

#[test]
fn every_word_the_python_plugin_wires_is_answered_and_none_of_them_blocks_on_nothing() {
    // The standalone ruling: with no Python charter installed, every command the Python
    // plugin's `hooks.json` runs lands on this binary. Each must answer — and on a payload
    // with nothing in it, answer "no opinion": exit 0, nothing printed. Exit 2 would block the
    // tool call (or keep a session from ending), so none may reach it by accident.
    let nowhere = tempfile::tempdir().expect("a directory that is no plane");
    let mut words: Vec<&str> = charter_core::hookreg::HANDLERS
        .iter()
        .map(|h| h.name)
        .collect();
    words.extend(charter_core::hookreg::NO_OPS);
    for word in &words {
        for payload in [
            "",
            "{}",
            "not json",
            r#"{"tool_name": 7, "tool_input": null}"#,
        ] {
            let (code, stdout, stderr) = run_hook(
                nowhere.path(),
                &["hook", word, "--plugin-version", "0.62.1"],
                payload,
            );
            assert_eq!(code, 0, "`charter hook {word}` on {payload:?}: {stderr}");
            assert!(
                stdout.is_empty(),
                "`charter hook {word}` on {payload:?} spoke: {stdout}"
            );
        }
    }
    // And the three internal words the plugin wires beside its hooks.
    for args in [
        &["workspace", "_reconcile"][..],
        &["persona", "_gc", "--detach"][..],
        &["workspace", "_autosave"][..],
        &["ws", "_autosave"][..],
    ] {
        let (code, stdout, stderr) = run_hook(nowhere.path(), args, "{}");
        assert_eq!(code, 0, "`charter {}`: {stderr}", args.join(" "));
        assert!(
            stdout.is_empty() && stderr.is_empty(),
            "`charter {}` spoke",
            args.join(" ")
        );
    }
}

#[test]
fn the_registry_is_printed_for_whatever_generates_a_plugin() {
    let nowhere = tempfile::tempdir().expect("a directory");
    let (code, stdout, _) = run_hook(nowhere.path(), &["hook", "--list", "--json"], "");
    assert_eq!(code, 0);
    let doc: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one line of JSON");
    assert_eq!(doc["schema"], 1);
    let names: Vec<&str> = doc["handlers"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|h| h["name"].as_str().expect("a name"))
        .collect();
    assert!(names.contains(&"pretooluse-read") && names.contains(&"sessionstart"));
    assert!(
        !names.contains(&"posttooluse-bash"),
        "a no-op is never wired"
    );
    // `--json` alone is a usage error, never a block.
    let (code, _, _) = run_hook(nowhere.path(), &["hook", "--json"], "");
    assert_eq!(code, 1);
    let (code, table, _) = run_hook(nowhere.path(), &["hook", "--list"], "");
    assert_eq!(code, 0);
    assert!(
        table
            .lines()
            .any(|l| l.starts_with("pretooluse-edit") && l.ends_with("Write|Edit|MultiEdit"))
    );
}

/// The vault file the fixtures below hold, spelled once.
const VAULT_FILE: &str = concat!(".charter/", "vaults/db.json");

/// A plane with one persona the app might pin, and a vault holding a file.
fn a_plane_with_a_persona() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(
        dir.path().join("charter.toml"),
        "schema = 1\n\n[persona]\ndefault = \"ops\"\n",
    )
    .expect("a manifest");
    let persona = dir.path().join("personas/ops");
    std::fs::create_dir_all(persona.join("memory")).expect("a persona");
    std::fs::write(
        persona.join("persona.md"),
        "---\nrole: Operations\ntools: gh\n---\n\n# ops\n",
    )
    .expect("its charter");
    let vault = dir.path().join(VAULT_FILE);
    std::fs::create_dir_all(vault.parent().expect("a parent")).expect("a vault directory");
    std::fs::write(vault, "{}").expect("a vault");
    dir
}

#[test]
fn a_session_in_a_plane_is_briefed_on_its_persona_through_additional_context() {
    // The gap this whole port closes: the app set `$CHARTER_PERSONA` and nothing read it.
    let plane = a_plane_with_a_persona();
    let (code, stdout, stderr) = run_hook(
        plane.path(),
        &["hook", "sessionstart", "--plugin-version", "0.62.1"],
        r#"{"session_id": "s-1", "hook_event_name": "SessionStart"}"#,
    );
    assert_eq!(code, 0, "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one line of JSON");
    let out = &doc["hookSpecificOutput"];
    assert_eq!(out["hookEventName"], "SessionStart");
    let context = out["additionalContext"].as_str().expect("context");
    assert!(
        context.contains("You are the `ops` persona for this session"),
        "{context}"
    );
    assert!(context.contains("> role: Operations"));
    // And no word about a Python charter: this binary is the whole of charter now.
    assert!(!stdout.contains("Python"), "{stdout}");
    assert!(doc.get("systemMessage").is_none(), "{stdout}");
    // The persona tool gate's ceiling was frozen for the session.
    assert!(plane.path().join(".charter/sessions/s-1.tools").is_file());
}

#[test]
fn the_tool_guards_refuse_by_printing_and_exit_cleanly() {
    let plane = a_plane_with_a_persona();
    let read = serde_json::json!({"tool_name": "Read", "tool_input": {"file_path": VAULT_FILE}});
    let edit = serde_json::json!({"tool_name": "Write", "cwd": ".",
        "tool_input": {"file_path": ".charter/active-persona"}});
    for (word, payload, sentence) in [
        (
            "pretooluse-read",
            read,
            "reads a vault/secret file directly",
        ),
        (
            "pretooluse-edit",
            edit,
            "writes charter's own state directly",
        ),
    ] {
        let (code, stdout, stderr) = run_hook(plane.path(), &["hook", word], &payload.to_string());
        assert_eq!(code, 0, "{word}: {stderr}");
        let said = decision(&stdout).unwrap_or_default();
        assert!(said.contains(sentence), "{word} said {stdout:?}");
    }
}

#[test]
fn the_active_personas_declared_tool_runs_without_a_prompt() {
    let plane = a_plane_with_a_persona();
    let bash = |command: &str| {
        serde_json::json!({"session_id": "s-1", "cwd": ".", "tool_name": "Bash",
            "tool_input": {"command": command}})
        .to_string()
    };
    let (code, stdout, _) = run_hook(plane.path(), &["hook", "pretooluse"], &bash("gh pr list"));
    assert_eq!(code, 0);
    assert!(
        stdout.contains(r#""permissionDecision": "allow""#)
            && stdout.contains("persona 'ops' declares 'gh' in its tools"),
        "{stdout}"
    );
    // A refusal still wins over the grant: reading a vault is denied, never allowed.
    let (_, stdout, _) = run_hook(
        plane.path(),
        &["hook", "pretooluse"],
        &bash(&format!("cat {VAULT_FILE}")),
    );
    assert!(
        stdout.contains(r#""permissionDecision": "deny""#),
        "{stdout}"
    );
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
fn text_that_mentions_a_handoff_is_not_one_and_a_real_one_is_still_refused() {
    // M8.2: a chat writing a test was refused because the TEXT it wrote held the words
    // `charter handoff`. A heredoc body a reader takes, a quoted argument, an `echo`'s words and
    // the later lines of a quoted string are data; a handoff with no brief is still refused.
    let plane = a_plane();
    for command in [
        "cat > tests/t.rs <<'EOF'\nlet c = \"charter handoff beta\";\ncharter handoff beta\nEOF",
        "echo charter handoff beta",
        "grep -rn \"charter handoff\" docs",
        "git commit -m 'fix the guard\n\ncharter handoff beta now asks first'",
    ] {
        let (code, out, err) = guard(plane.path(), &bash(command), &[]);
        assert_eq!(code, 0, "{command:?}");
        assert_eq!(out, "", "{command:?} was refused: {out}");
        assert_eq!(err, "", "{command:?}");
    }
    for command in [
        "charter handoff beta",
        "echo 'a\nb'\ncharter handoff beta",
        "bash -c 'charter handoff beta'",
    ] {
        let (code, out, _) = guard(plane.path(), &bash(command), &[]);
        assert_eq!(code, 0, "{command:?}");
        assert!(decision(&out).is_some(), "{command:?} was allowed");
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
            String::from_utf8_lossy(&out.stderr).contains("is not a hook this binary answers"),
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
        .current_dir(std::env::temp_dir())
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
    // "the gate that keeps them to sessionstart" for the same reason. (This binary no longer
    // says any notice at all — it answers the plugin's words itself — and none of these words
    // may start speaking on every turn.)
    for word in [
        "userpromptsubmit",
        "stop",
        "notification",
        "sessionend",
        "subagentstop",
    ] {
        let out = Command::new(CHARTER)
            .args(["hook", word, "--plugin-version", "0.62.1"])
            .current_dir(std::env::temp_dir())
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
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

// --- the forge cache's trigger on a plane with no app open (charter-app#89) --------------- //

/// A plane with one workspace holding one clone, and nothing fetched for it yet.
///
/// **A bare `git init` and no commit**, deliberately: `glrefresh::trees` asks for directories
/// with a `.git`, and this machine cannot sign a commit (1Password), so a fixture that needed
/// one would be red here for a reason that is not the code's. **And no `origin`**, so the
/// refresh this starts is real but reaches no forge: `glrefresh` has no URL to infer a host
/// from and writes an empty entry. A test that phoned a forge would be a test nobody could run.
fn a_plane_with_a_clone() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").expect("the marker");
    let clone = dir.path().join("workspaces/alpha/svc");
    std::fs::create_dir_all(&clone).expect("the clone's directory");
    let done = Command::new("git")
        .args(["init", "-q", "-b", "main", "."])
        .current_dir(&clone)
        .status()
        .expect("git runs");
    assert!(done.success(), "the fixture clone was not made");
    dir
}

/// `charter hook <word>` standing in that plane's clone, with the environment cleared.
///
/// The directory matters twice: it is the rung the workspace ladder reads — a chat runs inside
/// the checkout it is working on — and `$CHARTER_ROOT` is set positively beside it so no walk
/// up can reach the operator's own plane, which is the fence this suite is under
/// (charter-app#132).
fn hook_in(plane: &std::path::Path, word: &str, env: &[(&str, &str)]) -> i32 {
    let clone = plane.join("workspaces/alpha/svc");
    let out = Command::new(CHARTER)
        .args(["hook", word])
        .current_dir(&clone)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("CHARTER_ROOT", plane)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .expect("charter runs");
    out.status.code().unwrap_or(-1)
}

/// Whether anything decided to refresh: the lock `glstate::maybe_spawn` writes with the pid of
/// the process it started. The hook writes it before it exits, so its absence the moment the
/// hook returns is an answer and not a race.
fn a_refresh_was_started(plane: &std::path::Path) -> bool {
    plane.join(charter_core::glrefresh::LOCK).exists()
}

/// Waits for the refresh the hook started to land its cache, or gives up.
fn the_cache_it_wrote(plane: &std::path::Path) -> Option<String> {
    let cache = plane.join(charter_core::glrefresh::CACHE);
    for _ in 0..200 {
        if let Ok(text) = std::fs::read_to_string(&cache) {
            return Some(text);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn a_session_starting_where_no_app_is_open_refreshes_the_forge_cache() {
    // **charter-app#89, the whole of it.** #69 landed the policy with one trigger — focusing a
    // workspace in the app — which covers a plane with the app open and nothing else. On a
    // plane nobody has an app open on, the operator's own `claude` in a terminal runs these
    // hooks and nothing else of charter's runs at all, so this is the only place a refresh can
    // be decided. Before this, such a plane's CI column said "nothing has fetched this
    // checkout" for ever.
    let plane = a_plane_with_a_clone();

    let code = hook_in(plane.path(), "sessionstart", &[]);

    assert_eq!(code, 0, "a hook must never cost a harness its turn");
    assert!(
        a_refresh_was_started(plane.path()),
        "nothing decided to refresh the cache"
    );
    // The lock alone would only say a process was started. The cache is what the panel reads,
    // and a refresh that wrote one is a refresh that ran — keyed to this plane, because
    // `maybe_spawn` hands the child `$CHARTER_ROOT` rather than letting it walk up from its
    // own directory (charter#527).
    let cache = the_cache_it_wrote(plane.path()).expect("the refresh it started wrote no cache");
    assert!(
        cache.contains("workspaces/alpha/svc"),
        "the refresh was not about this plane's clone: {cache}"
    );
}

#[test]
fn no_other_hook_event_refreshes_it_because_once_a_session_is_not_once_a_turn() {
    // The brakes make a repeat trigger cheap — past the cooldown it is two file reads — but
    // `userpromptsubmit` and `stop` fire on every single turn, and "cheap" per turn is the
    // shape #69's brakes exist to prevent on a path with a budget. With `REFRESH_TTL` at 300 s
    // a per-turn trigger buys no freshness a per-session one does not.
    for word in [
        "userpromptsubmit",
        "stop",
        "notification",
        "sessionend",
        "subagentstop",
    ] {
        let plane = a_plane_with_a_clone();

        let code = hook_in(plane.path(), word, &[]);

        assert_eq!(code, 0);
        assert!(
            !a_refresh_was_started(plane.path()),
            "`charter hook {word}` spawned a refresh, which is one per TURN"
        );
    }
}

#[test]
fn a_session_the_app_started_leaves_the_refresh_to_the_app() {
    // `$CHARTER_HOOK_SOCKET` in the environment means the app started this chat, and the app
    // already decides when a refresh runs (`panels::repo_states`, focusing a workspace). Two
    // deciders on one plane is not unsafe — the lock is what makes it safe — but it is two
    // policies, and #89 is about the plane that has none. The socket leads nowhere on purpose:
    // what is read here is that the variable is SET, not that anything answered.
    let plane = a_plane_with_a_clone();
    let gone = plane.path().join("gone.sock");

    let code = hook_in(
        plane.path(),
        "sessionstart",
        &[(SOCKET_ENV, gone.to_str().expect("a path"))],
    );

    assert_eq!(code, 0);
    assert!(
        !a_refresh_was_started(plane.path()),
        "the hook refreshed on a plane the app is holding"
    );
}

#[test]
fn the_operators_brake_is_honoured_on_this_path_too() {
    // `$CHARTER_NO_BACKGROUND_CHECKS` is a request not to phone home, and a refresh runs `gh`
    // or `glab`. `glstate::decide` reads it before the lock or the cache is touched; this is
    // the check that the new trigger goes THROUGH that decision rather than around it.
    let plane = a_plane_with_a_clone();

    let code = hook_in(
        plane.path(),
        "sessionstart",
        &[(charter_core::glstate::NO_BACKGROUND_CHECKS, "1")],
    );

    assert_eq!(code, 0);
    assert!(
        !a_refresh_was_started(plane.path()),
        "charter phoned a forge for somebody who asked it not to"
    );
}

#[test]
fn a_session_starting_outside_every_plane_refreshes_nothing() {
    // Python's `if not config.HAS_CONTROL_PLANE: return`. Outside a plane `STATE_DIR` is
    // `<cwd>/.charter`, so a spawn here scatters charter's caches into whatever directory the
    // session happened to start in — and the child, handed no plane, goes looking for one of
    // its own. That is charter#527, which is how a render for a throwaway root came to refresh
    // the operator's live plane.
    let outside = tempfile::tempdir().expect("a directory with no charter.toml in it");

    let out = Command::new(CHARTER)
        .args(["hook", "sessionstart"])
        .current_dir(outside.path())
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .stdin(Stdio::null())
        .output()
        .expect("charter runs");

    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "it wrote to the transcript");
    assert!(
        !outside.path().join(".charter").exists(),
        "charter left a cache directory in a plain directory"
    );
}

#[cfg(unix)]
#[test]
fn every_hook_the_bundled_plugin_wires_answers_an_ordinary_call_with_exit_zero() {
    // The registry is the whole of what the app's plugin wires, and a word in it that this
    // binary does not answer would refuse a tool call — or keep a session from ending — in
    // every chat. So each command is run exactly as the generated `hooks.json` spells it,
    // through `/bin/sh -c` as Claude Code runs it, with the variable the app sets.
    let plane = a_plane();
    let doc: serde_json::Value =
        serde_json::from_str(&charter_core::plugin::hooks_json()).expect("the hooks file");
    let mut ran = 0;
    for (event, groups) in doc["hooks"].as_object().expect("events") {
        for group in groups.as_array().expect("groups") {
            for hook in group["hooks"].as_array().expect("hooks") {
                let command = hook["command"].as_str().expect("a command");
                let payload = serde_json::json!({
                    "session_id": "11111111-2222-4333-8444-555555555555",
                    "cwd": plane.path(),
                    "hook_event_name": event,
                    "tool_name": "Bash",
                    "tool_input": {"command": "ls"},
                })
                .to_string();
                let mut child = Command::new("/bin/sh")
                    .args(["-c", command])
                    .current_dir(plane.path())
                    .env_clear()
                    .env("PATH", "/usr/bin:/bin")
                    .env("CHARTER_ROOT", plane.path())
                    .env("CHARTER_HARNESS", "claude-code")
                    .env(charter_core::plugin::BINARY_ENV, CHARTER)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .expect("sh runs");
                // A hook may answer without reading its payload at all (a no-op word exits at
                // once), and then this write meets a closed pipe. That is an answer, not a
                // failure: CI once went red on exactly that race (#228). Any other write error
                // still fails the test.
                if let Err(e) = child
                    .stdin
                    .take()
                    .expect("stdin")
                    .write_all(payload.as_bytes())
                {
                    assert_eq!(
                        e.kind(),
                        std::io::ErrorKind::BrokenPipe,
                        "{event}: writing the payload failed: {e}"
                    );
                }
                let out = child.wait_with_output().expect("it finishes");
                let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                assert_eq!(
                    out.status.code(),
                    Some(0),
                    "{event}: `{command}` did not answer: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert_eq!(decision(&stdout), None, "{event} refused `ls`: {stdout}");
                ran += 1;
            }
        }
    }
    assert_eq!(ran, charter_core::hookreg::HANDLERS.len());
}
