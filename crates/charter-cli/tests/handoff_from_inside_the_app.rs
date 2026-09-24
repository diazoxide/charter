//! `charter handoff` from a chat the app started opens the chat IN the app, and from anywhere
//! else says exactly what it always said (charter-app#204).
//!
//! The terminal path is pinned byte for byte, in two ways: against the literal it printed
//! before #204, and against itself with an app socket in the environment that nothing is
//! listening on. The second is the one that matters most. "The app is gone" is the ordinary
//! way this path fails, and it must fall through to the terminal answer without a word
//! changing, a hang, or a lie.
//!
//! The app half is a stand-in: `hookwire`'s own listener with the core's own `Tickets`,
//! answering the way `app/src-tauri/src/handoff.rs` answers. What is tested here is the
//! command's side of the conversation: two lines on one connection, the ticket it was handed
//! spent on the same connection, and the brief arriving as the stamped first message.

#![cfg(unix)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use charter_core::hookwire::{
    Answer, Ask, CHAT_ENV, Listener, OpenChat, Reading, SOCKET_ENV, Tickets,
};

const BRIEF: &str =
    "# Retry the failed webhook deliveries\n\nThe queue is in workspaces/alpha/svc.\n";

/// The chat the handoff is asked from, as the app numbers it.
const ASKING: u32 = 3;

/// A copy of the committed `daily` fixture plane, which the Python charter wrote.
fn daily() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().expect("a directory");
    copy(&fixture, &dir.path().join("plane"));
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("a readable fixture") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).expect("a copy");
        }
    }
}

fn root(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("plane")
}

/// `charter handoff alpha` with the brief on a pipe, as a chat numbered [`ASKING`] runs it,
/// with `app` naming the socket an app would have put in its environment.
fn handoff(root: &Path, app: Option<&Path>) -> Output {
    charter(root, app, &["handoff", "alpha"])
}

/// `charter <args>` with the brief on a pipe, as a chat numbered [`ASKING`] runs it.
fn charter(root: &Path, app: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(args)
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        // The app sets both to the same number (`sessions.rs`), which is what makes the
        // stamp name the chat the app knows.
        .env("CHARTER_SESSION_ID", ASKING.to_string())
        .env("CHARTER_HARNESS", "claude-code")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in [
        "CLAUDE_CODE_SESSION_ID",
        "CHARTER_WORKSPACE",
        "CHARTER_PERSONA",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
        SOCKET_ENV,
        CHAT_ENV,
    ] {
        command.env_remove(name);
    }
    if let Some(socket) = app {
        command
            .env(SOCKET_ENV, socket)
            .env(CHAT_ENV, ASKING.to_string());
    }
    let mut child = command.spawn().expect("the binary runs");
    // A refusal can come before charter reads the brief, and then the pipe is closed under
    // this write. That is the refusal's business, which the caller asserts on; only a
    // different write error is this helper's.
    match child
        .stdin
        .take()
        .expect("a pipe")
        .write_all(BRIEF.as_bytes())
    {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(error) => panic!("the brief is written: {error}"),
    }
    child.wait_with_output().expect("the binary finishes")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

/// `text` with the stamp's minute replaced by `<when>`: the stamp is local wall-clock time
/// and the command takes no `--now`.
fn minute_masked(text: &str) -> String {
    let Some(end) = text.find('⟩') else {
        return text.to_owned();
    };
    let Some(start) = text[..end].rfind(" · ") else {
        return text.to_owned();
    };
    format!("{}{}<when>{}", &text[..start], " · ", &text[end..])
}

/// What `charter handoff alpha` prints from a chat with no app behind it. Written out rather
/// than derived, so that a change to any word of it has to be made here too, on purpose.
const WHAT_A_TERMINAL_IS_TOLD: &str = "✗ charter handoff: no charter app answered this call, so \
nothing was opened. Open charter, then run this handoff again from a chat the app started — or \
start a chat in workspace 'alpha' from the window and give it the brief.\n";

#[test]
fn a_chat_with_no_app_behind_it_is_told_exactly_what_it_was_told_before() {
    let tmp = daily();

    let out = handoff(&root(&tmp), None);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert_eq!(minute_masked(&text(&out.stderr)), WHAT_A_TERMINAL_IS_TOLD);
}

#[test]
fn an_app_that_is_not_listening_is_the_terminal_path_byte_for_byte() {
    // The app quit while the chat kept running: the socket path is in the environment and
    // nothing is behind it. That is the ordinary way the app path fails, and it must not
    // change a word of the terminal answer.
    let tmp = daily();
    let gone = tmp.path().join("gone.sock");

    let out = handoff(&root(&tmp), Some(&gone));

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert_eq!(minute_masked(&text(&out.stderr)), WHAT_A_TERMINAL_IS_TOLD);
}

/// Everything a stand-in app was asked, with the connection each ask came on.
type Asked = Arc<Mutex<Vec<(u64, Ask)>>>;

/// An app on `socket` that answers with `answer`, and everything it was asked, by connection.
fn an_app(
    tmp: &tempfile::TempDir,
    answer: impl Fn(&Tickets, u64, Ask) -> Answer + Send + Sync + 'static,
) -> (PathBuf, Reading, Asked) {
    let within = tmp.path().join("app");
    std::fs::create_dir_all(&within).expect("a directory");
    let socket = within.join("s").join("hooks.sock");
    let listener = Listener::bind(&within, &socket).expect("a socket");
    let asked = Arc::new(Mutex::new(Vec::new()));
    let tickets = Tickets::default();
    let reading = listener.each_answering(Box::new(|_| {}), {
        let asked = Arc::clone(&asked);
        Box::new(move |connection, ask| {
            asked.lock().unwrap().push((connection, ask.clone()));
            answer(&tickets, connection, ask)
        })
    });
    (socket, reading, asked)
}

/// `app/src-tauri/src/handoff.rs`'s ticket half, and an open that always succeeds as chat 9.
fn opens_as_nine(tickets: &Tickets, connection: u64, ask: Ask) -> Answer {
    match ask {
        Ask::Ticket { chat } => match tickets.mint(chat, connection, Instant::now()) {
            Ok(ticket) => Answer::Ticket { ticket },
            Err(why) => Answer::No { why },
        },
        Ask::Open(open) => {
            match tickets.spend(open.chat, connection, &open.ticket, Instant::now()) {
                Ok(()) => Answer::Opened { chat: 9 },
                Err(why) => Answer::No { why },
            }
        }
        Ask::Report(back) => {
            match tickets.spend(back.chat, connection, &back.ticket, Instant::now()) {
                Ok(()) => Answer::Reported {
                    to: "steward 1".to_owned(),
                    kept_for: None,
                },
                Err(why) => Answer::No { why },
            }
        }
    }
}

#[test]
fn a_chat_the_app_started_is_opened_in_the_app_on_one_ticket() {
    let tmp = daily();
    let (socket, _reading, asked) = an_app(&tmp, opens_as_nine);

    let out = handoff(&root(&tmp), Some(&socket));

    assert_eq!(text(&out.stderr), "", "an opened handoff refuses nothing");
    assert_eq!(out.status.code(), Some(0));
    // `commands_handoff.OPENED`, word for word.
    assert_eq!(
        text(&out.stdout),
        "charter handoff: opened chat 9 in workspace 'alpha', started on the brief\n"
    );
    let asked = asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 2, "a ticket, then the open: {asked:?}");
    let (minted_on, Ask::Ticket { chat }) = &asked[0] else {
        panic!("the first line asks for a ticket: {asked:?}")
    };
    assert_eq!(*chat, ASKING, "the ticket is for the chat that is asking");
    let (spent_on, Ask::Open(open)) = &asked[1] else {
        panic!("the second line opens: {asked:?}")
    };
    assert_eq!(spent_on, minted_on, "minted and spent on ONE connection");
    let OpenChat {
        chat,
        workspace,
        create_vision,
        persona,
        message,
        ticket,
        name,
        report,
    } = &**open;
    assert_eq!(name, &None, "no --name, no name");
    assert!(!report, "no --report, no report owed");
    assert_eq!(*chat, ASKING);
    assert_eq!(workspace, "alpha");
    assert_eq!(create_vision, &None);
    assert_eq!(persona, &None);
    assert_eq!(
        ticket.len(),
        64,
        "the ticket the app minted, spent as it was handed"
    );
    // The brief travels as the brief: stamped, a blank line, then verbatim.
    assert!(
        charter_core::handoff::is_stamped_from(message, &ASKING.to_string()),
        "{message:?}"
    );
    assert!(message.ends_with(&format!("\n\n{BRIEF}")), "{message:?}");
}

#[test]
fn an_app_that_refuses_gets_the_printed_command_and_its_reason() {
    // A refusal from the app is not a silence: the operator is handed the command, as a
    // chat with no app would be, and told in one more line why the app would not.
    let tmp = daily();
    let (socket, _reading, _asked) = an_app(&tmp, |_, _, _| Answer::No {
        why: "chat 3 is not on a harness profile".to_owned(),
    });

    let out = handoff(&root(&tmp), Some(&socket));

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert_eq!(
        minute_masked(&text(&out.stderr)),
        "✗ charter handoff: the charter app that started this chat was asked, and would not \
         open one: chat 3 is not on a harness profile — nothing was opened.\n"
    );
}

#[test]
fn an_app_that_never_answers_does_not_hang_the_handoff() {
    // Something is bound on the socket and takes the connection, and never says a word. A
    // Bash tool call that waited for it would be a turn that never ends, so the command gives
    // up on the ticket's deadline and prints the terminal answer, unchanged.
    let tmp = daily();
    let socket = tmp.path().join("mute.sock");
    let mute = std::os::unix::net::UnixListener::bind(&socket).expect("a socket");
    let held = std::thread::spawn(move || {
        let (connection, _) = mute.accept().expect("a connection");
        std::thread::sleep(Duration::from_secs(8));
        drop(connection);
    });
    let started = Instant::now();

    let out = handoff(&root(&tmp), Some(&socket));

    assert!(
        started.elapsed() < Duration::from_secs(6),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(minute_masked(&text(&out.stderr)), WHAT_A_TERMINAL_IS_TOLD);
    held.join().expect("the mute app");
}

// ----- a name, and a report back (charter-app#258, #259) --------------------------------

/// The open the stand-in app was sent, after its ticket.
fn the_open(asked: &Asked) -> OpenChat {
    let asked = asked.lock().unwrap().clone();
    match asked.get(1) {
        Some((_, Ask::Open(open))) => (**open).clone(),
        other => panic!("an open second, not {other:?}"),
    }
}

#[test]
fn a_handoff_carries_its_task_name_and_whether_it_wants_an_answer() {
    let tmp = daily();
    let (socket, _reading, asked) = an_app(&tmp, opens_as_nine);

    let out = charter(
        &root(&tmp),
        Some(&socket),
        &[
            "handoff",
            "alpha",
            "--name",
            "  retry webhooks ",
            "--report",
        ],
    );

    assert_eq!(text(&out.stderr), "");
    assert_eq!(out.status.code(), Some(0));
    let open = the_open(&asked);
    assert_eq!(open.name.as_deref(), Some("retry webhooks"), "trimmed");
    assert!(open.report);
}

#[test]
fn a_task_name_charter_would_not_draw_is_refused_before_anything_is_asked() {
    let tmp = daily();
    let (socket, _reading, asked) = an_app(&tmp, opens_as_nine);

    let out = charter(
        &root(&tmp),
        Some(&socket),
        &["handoff", "alpha", "--name", "retry\u{200b}webhooks"],
    );

    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("--name"),
        "{}",
        text(&out.stderr)
    );
    assert!(text(&out.stderr).contains("Nothing was opened"));
    assert!(asked.lock().unwrap().is_empty(), "the app was never asked");
}

#[test]
fn a_report_back_goes_to_the_app_on_one_ticket_and_names_no_recipient() {
    let tmp = daily();
    let (socket, _reading, asked) = an_app(&tmp, opens_as_nine);

    let out = charter(
        &root(&tmp),
        Some(&socket),
        &["handoff", "report", "  Dropped it.\nTwo repos changed. "],
    );

    assert_eq!(text(&out.stderr), "");
    assert_eq!(out.status.code(), Some(0));
    assert!(
        text(&out.stdout).contains("sent to 'steward 1'"),
        "{}",
        text(&out.stdout)
    );
    let asked = asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 2, "a ticket, then the report: {asked:?}");
    let (minted_on, Ask::Ticket { chat }) = &asked[0] else {
        panic!("a ticket first: {asked:?}")
    };
    assert_eq!(*chat, ASKING);
    let (spent_on, Ask::Report(back)) = &asked[1] else {
        panic!("the report second: {asked:?}")
    };
    assert_eq!(spent_on, minted_on);
    assert_eq!(back.chat, ASKING, "the chat reporting, and nothing else");
    assert_eq!(back.summary, "Dropped it.\nTwo repos changed.");
}

#[test]
fn a_report_the_app_refuses_says_why_and_that_nothing_was_sent() {
    let tmp = daily();
    let (socket, _reading, _asked) = an_app(&tmp, |_, _, _| Answer::No {
        why: "chat 3 was not opened by a handoff that asked for a report".to_owned(),
    });

    let out = charter(&root(&tmp), Some(&socket), &["handoff", "report", "done"]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stderr),
        "✗ charter handoff report: chat 3 was not opened by a handoff that asked for a report \
         — nothing was sent.\n"
    );
}

#[test]
fn a_report_with_no_app_behind_it_sends_nothing_and_says_so() {
    let tmp = daily();

    let out = charter(&root(&tmp), None, &["handoff", "report", "done"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("nothing was sent"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_report_charter_would_not_hand_back_is_refused_before_anything_is_asked() {
    let tmp = daily();
    let (socket, _reading, asked) = an_app(&tmp, opens_as_nine);

    for bad in ["   ", "done\u{202e}enod", &"x".repeat(5000)] {
        let out = charter(&root(&tmp), Some(&socket), &["handoff", "report", bad]);

        assert_eq!(out.status.code(), Some(1), "{bad:?}");
        assert!(text(&out.stderr).contains("nothing was sent"), "{bad:?}");
    }
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_handoff_into_a_workspace_called_report_is_still_a_handoff() {
    // Without a summary after it, `report` is a workspace name as it always was — here one
    // this plane does not have, which is the ordinary refusal.
    let tmp = daily();

    let out = charter(&root(&tmp), None, &["handoff", "report"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("no workspace 'report'"),
        "{}",
        text(&out.stderr)
    );
}
