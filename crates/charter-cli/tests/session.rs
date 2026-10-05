//! `charter session record|list|show` through the binary, on a copy of the committed `daily`
//! fixture plane (workspaces `alpha` and `beta`), with a real hook socket where the app would
//! have one (SI-8, ADR 0064). The record's rules are held by charter-core's `sessionrecord`
//! tests; these hold the command line around them: what it reads from the chat's environment,
//! what it writes, and the line it sends the app.
#![cfg(unix)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use charter_core::hookwire::{self, CHAT_ENV, Listener, SOCKET_ENV, SessionSaved, TOKEN_ENV};

const BODY: &str = "## Goal\n\nShip the record.\n\n## Done\n\n- wrote it\n\n## Decisions\n\n- one \
file per record\n\n## Open\n\nNothing.\n\n## How to resume\n\nRead this, then run the tests.\n";

fn daily() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().unwrap();
    copy(&fixture, &dir.path().join("plane"));
    std::fs::create_dir_all(dir.path().join("home")).unwrap();
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

fn root(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("plane")
}

/// `charter <args>` in the plane with nothing of the caller's environment but what is named:
/// the chat this test runs in has a hook socket of its own, and nothing here may reach it.
fn charter(tmp: &tempfile::TempDir, args: &[&str], env: &[(&str, &str)], stdin: &str) -> Output {
    let root = root(tmp);
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(args)
        .current_dir(&root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("CHARTER_ROOT", &root)
        .env("HOME", tmp.path().join("home"))
        .env("NO_COLOR", "1")
        .env("CHARTER_NO_BACKGROUND_CHECKS", "1")
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("the binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn out(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const IN_ALPHA: (&str, &str) = ("CHARTER_WORKSPACE", "alpha");

fn record(tmp: &tempfile::TempDir, title: &str, now: &str, env: &[(&str, &str)]) -> Output {
    charter(
        tmp,
        &["session", "record", "--title", title, "--now", now],
        env,
        BODY,
    )
}

#[test]
fn a_record_is_written_indexed_and_pointed_at_and_the_app_is_told_which_chat_saved_it() {
    let tmp = daily();
    let socket = tmp.path().join("app").join("hooks.sock");
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let listener = Listener::bind(tmp.path(), &socket).expect("a socket");
    let token = listener.tokens().issue(3).expect("a token");
    let _reading = listener.each_answering_noticing_and_saving(
        Box::new(|_| panic!("a saved record is not a report")),
        Box::new(|_, _| panic!("a saved record is not an ask")),
        Box::new(|_| panic!("a saved record is not a harness started by hand")),
        Box::new(move |saved| tx.lock().unwrap().send(saved).unwrap()),
    );

    let ran = record(
        &tmp,
        "Ship the record",
        "2026-09-28T14:03:12",
        &[
            IN_ALPHA,
            (SOCKET_ENV, socket.to_str().unwrap()),
            (CHAT_ENV, "3"),
            (TOKEN_ENV, token.expose()),
        ],
    );

    assert!(ran.status.success(), "{}", err(&ran));
    let shown = "workspaces/alpha/sessions/20260928-140312-ship-the-record.md";
    assert_eq!(out(&ran).trim(), shown);
    let path = root(&tmp).join(shown);
    let text = std::fs::read_to_string(&path).expect("the record");
    assert!(text.contains("\nchat: 3\n"), "{text}");
    assert!(text.contains("\nworkspace: alpha\n"), "{text}");
    let index =
        std::fs::read_to_string(root(&tmp).join("workspaces/alpha/sessions/index.md")).unwrap();
    assert!(index.contains("[Ship the record](20260928-140312-ship-the-record.md)"));
    let charter_md =
        std::fs::read_to_string(root(&tmp).join("workspaces/alpha/workspace.md")).unwrap();
    assert!(
        charter_md.contains("## Sessions\n\n1 session record — "),
        "{charter_md}"
    );
    let saved: SessionSaved = rx.recv_timeout(Duration::from_secs(5)).expect("the line");
    assert_eq!(saved.chat, 3);
    assert_eq!(
        saved.session_saved.canonicalize().unwrap(),
        path.canonicalize().unwrap()
    );
    assert!(
        !marker(&tmp, "3").exists(),
        "a line the app heard was left for the Stop hook as well"
    );
}

/// Where chat `chat`'s saved-record marker is for a chat in `alpha` (`docs/plane-format.md`).
fn marker(tmp: &tempfile::TempDir, chat: &str) -> PathBuf {
    root(tmp).join(format!("workspaces/alpha/.charter/sessions/{chat}.saved"))
}

/// `charter hook stop` in chat `chat`, as the harness runs it at the end of a turn, with the
/// token the app gave that chat.
fn stop(
    tmp: &tempfile::TempDir,
    socket: &Path,
    chat: &str,
    tokens: &hookwire::ChatTokens,
) -> Output {
    let token = tokens.issue(chat.parse().unwrap()).expect("a token");
    charter(
        tmp,
        &["hook", "stop"],
        &[
            IN_ALPHA,
            (SOCKET_ENV, socket.to_str().unwrap()),
            (CHAT_ENV, chat),
            (TOKEN_ENV, token.expose()),
        ],
        r#"{"session_id":"0f6c2a1e-aaaa-4bbb-8ccc-123456789abc","hook_event_name":"Stop"}"#,
    )
}

/// A socket the app listens on from now, and the saved-record lines it hears. Reports are
/// taken and dropped: a `Stop` hook sends one of those too.
fn listening(
    tmp: &tempfile::TempDir,
    socket: &Path,
) -> (
    hookwire::Reading,
    mpsc::Receiver<SessionSaved>,
    std::sync::Arc<hookwire::ChatTokens>,
) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let listener = Listener::bind(tmp.path(), socket).expect("a socket");
    let tokens = listener.tokens();
    let reading = listener.each_answering_noticing_and_saving(
        Box::new(|_| {}),
        Box::new(|_, _| panic!("nothing here asks")),
        Box::new(|_| panic!("no harness was started by hand")),
        Box::new(move |saved| tx.lock().unwrap().send(saved).unwrap()),
    );
    (reading, rx, tokens)
}

#[test]
fn a_line_the_socket_refused_is_passed_on_by_the_chats_next_stop_once() {
    // Codex's sandbox refuses the command's connect and runs the hooks outside it (#517): here,
    // the app is not listening yet when the record is written, and is when the turn ends.
    let tmp = daily();
    let socket = tmp.path().join("app").join("hooks.sock");
    let ran = record(
        &tmp,
        "Sandboxed",
        "2026-09-28T16:00:00",
        &[
            IN_ALPHA,
            (SOCKET_ENV, socket.to_str().unwrap()),
            (CHAT_ENV, "3"),
        ],
    );
    assert!(ran.status.success(), "{}", err(&ran));
    assert!(marker(&tmp, "3").is_file(), "no marker for the Stop hook");
    assert!(
        err(&ran).contains("when this turn ends"),
        "the command does not say what happens next"
    );

    let (_reading, rx, tokens) = listening(&tmp, &socket);
    // Another chat's Stop passes on nothing of chat 3's.
    assert!(stop(&tmp, &socket, "4", &tokens).status.success());
    assert!(marker(&tmp, "3").is_file(), "chat 4 took chat 3's marker");

    let stopped = stop(&tmp, &socket, "3", &tokens);

    assert!(stopped.status.success(), "{}", err(&stopped));
    let saved = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the line, passed on");
    assert_eq!(saved.chat, 3);
    assert_eq!(
        saved.session_saved.canonicalize().unwrap(),
        root(&tmp)
            .join("workspaces/alpha/sessions/20260928-160000-sandboxed.md")
            .canonicalize()
            .unwrap()
    );
    assert!(!marker(&tmp, "3").exists(), "the marker outlived its Stop");
    assert!(stop(&tmp, &socket, "3", &tokens).status.success());
    assert!(
        rx.recv_timeout(Duration::from_millis(500)).is_err(),
        "the next Stop sent the line again"
    );
}

#[test]
fn with_no_app_to_tell_the_record_is_still_written_and_the_operator_is_told_what_becomes_of_the_tab()
 {
    let tmp = daily();
    let gone = tmp.path().join("gone.sock");

    for env in [
        vec![IN_ALPHA],
        vec![
            IN_ALPHA,
            (SOCKET_ENV, gone.to_str().unwrap()),
            (CHAT_ENV, "3"),
        ],
    ] {
        let now = if env.len() == 1 {
            "2026-09-28T09:00:00"
        } else {
            "2026-09-28T10:00:00"
        };
        let ran = record(&tmp, "Alone", now, &env);
        assert!(ran.status.success(), "{}", err(&ran));
        // With no socket at all this is no chat the app started; with one that did not answer,
        // the line waits for the chat's Stop hook, and the operator is told either way.
        let said = if env.len() == 1 {
            "will not close by itself"
        } else {
            "close it yourself"
        };
        assert!(err(&ran).contains(said), "{}", err(&ran));
    }
    assert!(marker(&tmp, "3").is_file(), "the refused line was not left");
    let listed = charter(&tmp, &["session", "list", "-w", "alpha"], &[], "");
    assert_eq!(out(&listed).lines().count(), 2, "{}", out(&listed));
}

#[test]
fn a_body_that_is_not_a_record_is_refused_with_exit_2_and_nothing_is_written() {
    let tmp = daily();
    let ran = charter(
        &tmp,
        &["session", "record", "--title", "Half"],
        &[IN_ALPHA],
        "## Goal\n\nx\n",
    );
    assert_eq!(ran.status.code(), Some(2), "{}", err(&ran));
    assert!(err(&ran).contains("## Done"), "{}", err(&ran));
    assert!(!root(&tmp).join("workspaces/alpha/sessions").exists());
}

#[test]
fn a_plane_root_chat_records_into_the_planes_own_sessions_directory() {
    let tmp = daily();
    let ran = record(
        &tmp,
        "Tidy personas",
        "2026-09-28T08:01:02",
        &[("CHARTER_PLANE_ROOT_SESSION", "1")],
    );
    assert!(ran.status.success(), "{}", err(&ran));
    assert_eq!(
        out(&ran).trim(),
        "sessions/20260928-080102-tidy-personas.md"
    );
    let listed = charter(
        &tmp,
        &["session", "list"],
        &[("CHARTER_PLANE_ROOT_SESSION", "1")],
        "",
    );
    assert!(out(&listed).contains("Tidy personas"), "{}", out(&listed));
}

#[test]
fn a_piece_the_workspace_does_not_have_is_refused_rather_than_recorded() {
    let tmp = daily();
    let ran = charter(
        &tmp,
        &[
            "session",
            "record",
            "--title",
            "Claims",
            "--piece",
            "nowhere/nothing",
        ],
        &[IN_ALPHA],
        BODY,
    );
    assert_eq!(ran.status.code(), Some(2), "{}", err(&ran));
    assert!(!root(&tmp).join("workspaces/alpha/sessions").exists());
}

#[test]
fn list_is_newest_first_and_show_prints_one_record_by_its_file_name() {
    let tmp = daily();
    record(&tmp, "Older", "2026-09-28T09:00:00", &[IN_ALPHA]);
    record(&tmp, "Newer", "2026-09-28T10:00:00", &[IN_ALPHA]);

    let listed = charter(&tmp, &["session", "list"], &[IN_ALPHA], "");
    assert!(listed.status.success(), "{}", err(&listed));
    let said = out(&listed);
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains("Newer"), "{lines:?}");
    assert!(
        lines[0].contains("workspaces/alpha/sessions/20260928-100000-newer.md"),
        "{lines:?}"
    );
    assert!(lines[1].contains("Older"), "{lines:?}");

    let shown = charter(
        &tmp,
        &["session", "show", "20260928-090000-older.md"],
        &[IN_ALPHA],
        "",
    );
    assert!(shown.status.success(), "{}", err(&shown));
    assert!(out(&shown).contains("# Older\n"), "{}", out(&shown));

    // A plane-relative path names its own place, whatever the chat's is.
    let by_path = charter(
        &tmp,
        &[
            "session",
            "show",
            "workspaces/alpha/sessions/20260928-100000-newer.md",
        ],
        &[("CHARTER_PLANE_ROOT_SESSION", "1")],
        "",
    );
    assert!(by_path.status.success(), "{}", err(&by_path));
    assert!(out(&by_path).contains("# Newer\n"), "{}", out(&by_path));

    let escape = charter(
        &tmp,
        &["session", "show", "../workspace.md"],
        &[IN_ALPHA],
        "",
    );
    assert!(!escape.status.success());
    assert!(out(&escape).is_empty());
}

#[test]
fn the_profile_and_directory_are_the_apps_record_of_the_chat_never_the_commands_words() {
    let tmp = daily();
    let root = root(&tmp);
    let piece = root.join("workspaces/alpha/charter-app/si-8e");
    std::fs::create_dir_all(&piece).unwrap();
    let app_record = charter_core::reopen::path(&root);
    std::fs::create_dir_all(app_record.parent().unwrap()).unwrap();
    std::fs::write(
        &app_record,
        serde_json::json!({
            "version": 1, "at": 1, "dealt": 3,
            "chats": [{
                "program": "claude", "name": "3", "number": 3, "profile": "work",
                "cwd": piece.display().to_string(),
                "resume": "0f6c2a1e-aaaa-4bbb-8ccc-123456789abc"
            }]
        })
        .to_string(),
    )
    .unwrap();

    // What the chat's own environment claims about a profile is not asked.
    let ran = record(
        &tmp,
        "From a piece",
        "2026-09-28T15:00:00",
        &[
            IN_ALPHA,
            (CHAT_ENV, "3"),
            ("CHARTER_PROFILE", "someone-else"),
        ],
    );

    assert!(ran.status.success(), "{}", err(&ran));
    let text = std::fs::read_to_string(
        root.join("workspaces/alpha/sessions/20260928-150000-from-a-piece.md"),
    )
    .expect("the record");
    assert!(text.contains("\nprofile: work\n"), "the app's profile");
    assert!(
        text.contains("\ncwd: workspaces/alpha/charter-app/si-8e\n"),
        "the chat's directory, plane-relative"
    );
}
