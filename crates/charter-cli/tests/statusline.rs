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
