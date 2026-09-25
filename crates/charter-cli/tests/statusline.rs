//! `charter statusline` as Claude Code runs it: a real process, a real payload on stdin.
//!
//! **The test this file exists for is
//! [`the_turns_tokens_are_recorded_even_when_the_footer_is_blank`].** ADR 0019 keeps the
//! command wired up for one reason — Claude Code's per-turn JSON is the only place the
//! session's token usage exists — and says in as many words that a "cleanup" which unwires it
//! "deletes the record, silently, and nothing notices until somebody goes looking for a
//! history that stopped being written months earlier".
//!
//! A comment cannot stop that. This can: take the record out of either branch and a test goes
//! red.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use charter_core::hookwire::{CHAT_ENV, Listener, SOCKET_ENV};

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

/// One turn's payload, in the shape Claude Code sends it.
const A_TURN: &str = r#"{"session_id":"s-1","context_window":{"used_percentage":42,
    "current_usage":{"cache_read_input_tokens":90,"cache_creation_input_tokens":10}}}"#;

struct Ran {
    out: String,
    code: i32,
}

/// `charter statusline` with `payload` on stdin, standing in `plane`.
fn statusline(plane: &Path, payload: &str, env: &[(&str, &str)]) -> Ran {
    let mut child = Command::new(CHARTER)
        .arg("statusline")
        .current_dir(plane)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", plane)
        .env("CHARTER_ROOT", plane)
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
    let done = child.wait_with_output().expect("charter finishes");
    Ran {
        out: String::from_utf8_lossy(&done.stdout).into_owned(),
        code: done.status.code().unwrap_or(-1),
    }
}

fn plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a directory");
    let at = std::fs::canonicalize(dir.path()).expect("a real path");
    std::fs::write(at.join("charter.toml"), "schema = 1\n").expect("a plane");
    (dir, at)
}

fn recorded(plane: &Path) -> String {
    std::fs::read_to_string(plane.join(".charter/sessions/s-1.usage")).unwrap_or_default()
}

#[test]
fn the_turns_tokens_are_recorded_even_when_the_footer_is_blank() {
    let (_keep, at) = plane();
    // The app: a socket something is accepting on, a chat number it issued, and Claude Code.
    let socket = at.join(".charter/app/hooks.sock");
    std::fs::create_dir_all(socket.parent().expect("a parent")).expect("the app's directory");
    let _listening = Listener::bind(&at, &socket).expect("a socket");

    let ran = statusline(
        &at,
        A_TURN,
        &[
            (SOCKET_ENV, socket.to_str().expect("a path")),
            (CHAT_ENV, "7"),
            ("CHARTER_HARNESS", "claude-code"),
        ],
    );

    assert_eq!(ran.code, 0);
    // ADR 0019: an EMPTY line, not no line. Claude Code reads a line from this command, and
    // an empty one is how it is told there is nothing to show.
    assert_eq!(ran.out, "\n", "the app's panels are the surface");
    // And the whole reason the command still runs.
    assert_eq!(recorded(&at), "90,10,90,42\n");
}

#[test]
fn the_turns_tokens_are_recorded_when_the_line_is_drawn_too() {
    // The other branch. Both write, because the record is not a consolation prize for the
    // suppressed one — it is what this command is for.
    let (_keep, at) = plane();

    let ran = statusline(&at, A_TURN, &[]);

    assert_eq!(ran.code, 0);
    assert!(ran.out.ends_with('\n'));
    assert_ne!(ran.out, "\n", "outside the app the command says something");
    assert_eq!(recorded(&at), "90,10,90,42\n");
}

/// An approved extension beside `plane` that declares one footer badge and fills it with
/// `value`, true `age` seconds ago — a facts file and no program, so nothing could be started
/// to draw it. Its config home is `<plane>/config`.
fn a_badge_in_the_footer(plane: &Path, value: &str, age: i64) -> PathBuf {
    use charter_core::extension;
    let ext = plane.join("ext");
    std::fs::create_dir_all(ext.join("state")).expect("the extension's state directory");
    std::fs::write(
        ext.join(extension::MANIFEST),
        r#"{"version": 1, "id": "prs", "name": "Pull requests", "state": "state",
            "capabilities": ["badges"],
            "contributes": {"badges": [{"id": "open", "label": "PRs", "surfaces": ["footer"],
                                        "fresh_seconds": 600}]}}"#,
    )
    .expect("a manifest");
    let at = chrono::Utc::now().timestamp() - age;
    std::fs::write(
        ext.join("state").join(extension::facts::FILE),
        format!(r#"{{"badges": {{"open": {{"value": "{value}", "at": {at}}}}}}}"#),
    )
    .expect("a facts file");
    let config = plane.join("config");
    let found = extension::install(&config, &ext).expect("installed");
    extension::approve(&config, found.id(), &found.path, &found.fingerprint).expect("approved");
    config
}

#[test]
fn an_approved_extensions_footer_badge_is_drawn_from_its_facts_file() {
    let (_keep, at) = plane();
    let config = a_badge_in_the_footer(&at, "4", 0);
    let config = config.to_str().expect("a path");

    let ran = statusline(
        &at,
        A_TURN,
        &[("COLUMNS", "80"), ("CHARTER_CONFIG_HOME", config)],
    );
    let badge = ran
        .out
        .lines()
        .find(|line| line.contains("PRs"))
        .unwrap_or_else(|| panic!("no badge: {:?}", ran.out));
    assert!(badge.contains("\u{1b}[2mPRs\u{1b}[0m 4"), "{badge:?}");
    assert_eq!(ran.code, 0);
}

#[test]
fn a_footer_badge_older_than_its_freshness_is_dimmed_with_its_age() {
    let (_keep, at) = plane();
    let config = a_badge_in_the_footer(&at, "4", 2 * 3600);
    let config = config.to_str().expect("a path");

    let ran = statusline(
        &at,
        A_TURN,
        &[("COLUMNS", "80"), ("CHARTER_CONFIG_HOME", config)],
    );
    assert!(
        ran.out.contains("\u{1b}[2mPRs 4 · 2h ago\u{1b}[0m"),
        "{:?}",
        ran.out
    );
}

#[test]
fn the_footer_names_the_surfaces_it_does_not_draw() {
    // The rule this build is held to: an omitted section is NAMED, never merely absent. An
    // operator reads a footer to find out whether anything needs them, and one that can only
    // ever say "nothing" is a footer that lies once a week — so the line below is what stands
    // where the repo rows, the persona chips, the alerts and the session strip will be.
    let (_keep, at) = plane();

    let ran = statusline(&at, A_TURN, &[("COLUMNS", "80")]);

    // charter's `render` ends on a newline and `print` adds the second one, so the footer is
    // followed by a blank line. Copied deliberately: it is part of what the two implementations
    // have to agree on for a byte-for-byte comparison to mean anything.
    assert!(ran.out.ends_with("\n\n"), "{:?}", ran.out);
    let lines: Vec<&str> = ran.out.trim_end_matches('\n').lines().collect();
    assert_eq!(
        lines.len(),
        5,
        "frame, row, rule, declaration, frame: {:?}",
        ran.out
    );
    assert!(
        lines[1].contains("default"),
        "the workspace: {:?}",
        lines[1]
    );
    assert!(
        lines[2].contains('\u{251c}'),
        "the zone rule: {:?}",
        lines[2]
    );
    assert!(
        lines[3].contains("not drawn by this build"),
        "the declaration: {:?}",
        lines[3]
    );
    // Every line is the same number of columns, borders included — the frame is a ruler.
    let widths: Vec<usize> = lines.iter().map(|l| charter_core::tui::width(l)).collect();
    assert!(widths.iter().all(|w| *w == 76), "{widths:?}");
}

#[test]
fn a_session_with_no_app_listening_is_not_suppressed() {
    // A socket file left behind by an app that has gone. ADR 0019's frame asked "is the
    // launcher pid still running" for this exact case: without it, a crashed launcher blanks
    // a plane's status line for ever with nothing on screen to say why.
    let (_keep, at) = plane();

    let ran = statusline(
        &at,
        A_TURN,
        &[
            (SOCKET_ENV, at.join("gone.sock").to_str().expect("a path")),
            (CHAT_ENV, "7"),
            ("CHARTER_HARNESS", "claude-code"),
        ],
    );

    assert_ne!(ran.out, "\n", "a dead socket blanked the footer");
    assert_eq!(recorded(&at), "90,10,90,42\n");
}

#[test]
fn a_harness_with_no_footer_of_its_own_is_never_suppressed() {
    let (_keep, at) = plane();
    let socket = at.join(".charter/app/hooks.sock");
    std::fs::create_dir_all(socket.parent().expect("a parent")).expect("the app's directory");
    let _listening = Listener::bind(&at, &socket).expect("a socket");

    let ran = statusline(
        &at,
        A_TURN,
        &[
            (SOCKET_ENV, socket.to_str().expect("a path")),
            (CHAT_ENV, "7"),
            ("CHARTER_HARNESS", "codex"),
        ],
    );

    // Suppression removes a DUPLICATE, and codex has no footer to duplicate.
    assert_ne!(ran.out, "\n");
}

#[test]
fn a_chat_started_asking_for_charters_footer_gets_one_while_the_rest_stay_blank() {
    // Charter ADR 0029: the blanking is the default, and one chat may opt out of it without
    // moving anything for the chats beside it. Both halves are asserted here against one
    // app, because "it can be turned on" and "turning it on for one chat leaves the others
    // alone" are different claims and only the second is the product decision.
    //
    // The variable is spelled out here rather than imported: this file runs the real
    // binary the way Claude Code does, and the name is the contract an operator reads in
    // the ADR. A rename that only edited the constant should fail here.
    let (_keep, at) = plane();
    let socket = at.join(".charter/app/hooks.sock");
    std::fs::create_dir_all(socket.parent().expect("a parent")).expect("the app's directory");
    let _listening = Listener::bind(&at, &socket).expect("a socket");
    let app: [(&str, &str); 3] = [
        (SOCKET_ENV, socket.to_str().expect("a path")),
        (CHAT_ENV, "7"),
        ("CHARTER_HARNESS", "claude-code"),
    ];
    let mut asking: Vec<(&str, &str)> = app.to_vec();
    asking.push(("CHARTER_FOOTER", "show"));

    let asked = statusline(&at, A_TURN, &asking);
    let beside_it = statusline(&at, A_TURN, &app);

    assert_eq!(asked.code, 0);
    assert_ne!(asked.out, "\n", "the chat that asked got a blank line");
    assert_eq!(
        beside_it.out, "\n",
        "one chat's choice moved another chat's footer"
    );
    // The record is still the reason this command runs, on this branch as on the others.
    assert_eq!(recorded(&at), "90,10,90,42\n");
}

#[test]
fn a_footer_variable_charter_did_not_write_leaves_the_default_alone() {
    // The variable is charter's own, set to one word at the exec. Anything else reaching
    // this process — inherited from the shell the app was launched from, or left over —
    // must not turn a surface on that the operator never picked for this chat.
    let (_keep, at) = plane();
    let socket = at.join(".charter/app/hooks.sock");
    std::fs::create_dir_all(socket.parent().expect("a parent")).expect("the app's directory");
    let _listening = Listener::bind(&at, &socket).expect("a socket");

    for said in ["", "1", "true", "SHOW", "blank"] {
        let ran = statusline(
            &at,
            A_TURN,
            &[
                (SOCKET_ENV, socket.to_str().expect("a path")),
                (CHAT_ENV, "7"),
                ("CHARTER_HARNESS", "claude-code"),
                ("CHARTER_FOOTER", said),
            ],
        );

        assert_eq!(ran.out, "\n", "{said:?} drew a footer");
    }
}

#[test]
fn a_turn_that_carries_no_numbers_records_nothing_and_still_answers() {
    // Early in a session and right after `/compact` the payload has no usage at all.
    // Recording a zero there would invent a turn — and a `0/0` divided.
    let (_keep, at) = plane();

    let ran = statusline(&at, r#"{"session_id":"s-1"}"#, &[]);

    assert_eq!(ran.code, 0);
    assert!(ran.out.ends_with('\n'));
    assert_eq!(recorded(&at), "");
}

#[test]
fn a_payload_that_is_not_json_at_all_still_answers_with_a_line() {
    // The outermost boundary of the whole subprocess. A raise here takes the footer down.
    let (_keep, at) = plane();

    let ran = statusline(&at, "not json {{{", &[]);

    assert_eq!(ran.code, 0);
    assert!(ran.out.ends_with('\n'));
}

#[test]
fn a_session_id_that_would_leave_the_state_directory_records_nothing() {
    // The id comes off stdin, and Python joins it straight onto the sessions directory. This
    // build refuses an id that is not one name; the scenario that compares the two uses an
    // ordinary id, where both write the same file.
    let (_keep, at) = plane();
    let payload = r#"{"session_id":"../../escape","context_window":{"used_percentage":42,
        "current_usage":{"cache_read_input_tokens":90,"cache_creation_input_tokens":10}}}"#;

    let ran = statusline(&at, payload, &[]);

    assert_eq!(ran.code, 0);
    // `<plane>/.charter/sessions/../../escape.usage` is `<plane>/escape.usage`, which is the
    // file Python's join would have created.
    assert!(
        !at.join("escape.usage").exists(),
        "a payload chose where charter wrote"
    );
    assert!(!at.join(".charter/sessions").exists());
}
