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

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// `command` without any spelling of the variables that choose a project, which a suite run
/// inside a chat inherits under both names (V93k): the test sets its own after.
fn unsteered(mut command: Command) -> Command {
    for rest in purlis_core::envvar::SELECTING {
        for spelling in purlis_core::envvar::spellings(&format!("PURLIS_{rest}")) {
            command.env_remove(spelling);
        }
    }
    command
}

use purlis_core::hookwire::{
    Answer, Ask, CHAT_ENV, ChatToken, Listener, OpenChat, Reading, SOCKET_ENV, TOKEN_ENV, Tickets,
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

/// What an app put in a chat's environment: the socket, and the chat's token where the app
/// gave it one. A bare path is a socket with no token behind it.
trait AppEnv {
    fn socket(&self) -> &Path;
    fn token(&self) -> Option<&ChatToken>;
}

impl AppEnv for PathBuf {
    fn socket(&self) -> &Path {
        self
    }
    fn token(&self) -> Option<&ChatToken> {
        None
    }
}

/// A stand-in app's socket, and the token it gave chat [`ASKING`].
struct App {
    socket: PathBuf,
    token: ChatToken,
}

impl AppEnv for App {
    fn socket(&self) -> &Path {
        &self.socket
    }
    fn token(&self) -> Option<&ChatToken> {
        Some(&self.token)
    }
}

/// `charter handoff alpha` with the brief on a pipe, as a chat numbered [`ASKING`] runs it,
/// with `app` naming the socket an app would have put in its environment.
fn handoff(root: &Path, app: Option<&dyn AppEnv>) -> Output {
    charter(root, app, &["handoff", "alpha"])
}

/// `charter <args>` with the brief on a pipe, as a chat numbered [`ASKING`] runs it.
fn charter(root: &Path, app: Option<&dyn AppEnv>, args: &[&str]) -> Output {
    charter_with(root, app, args, &[])
}

/// [`charter`], with `env` set on top.
fn charter_with(
    root: &Path,
    app: Option<&dyn AppEnv>,
    args: &[&str],
    env: &[(&str, &str)],
) -> Output {
    let mut command = unsteered(Command::new(env!("CARGO_BIN_EXE_purlis")));
    command
        .args(args)
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        // The app sets both to the same number (`sessions.rs`), which is what makes the
        // stamp name the chat the app knows.
        .env("CHARTER_SESSION_ID", ASKING.to_string())
        .env("CHARTER_HARNESS", "claude-code")
        // The machine store it reads for the extensions a handoff tells (charter-app#343): this
        // run's own, which holds none, never the operator's.
        .env("CHARTER_CONFIG_HOME", root.join("no-config-home"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in [
        "CLAUDE_CODE_SESSION_ID",
        "CHARTER_WORKSPACE",
        "CHARTER_PLANE_ROOT_SESSION",
        "CHARTER_PERSONA",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
        SOCKET_ENV,
        CHAT_ENV,
        TOKEN_ENV,
    ] {
        // Under either name (V93k): a suite run in a chat inherits both.
        for spelling in purlis_core::envvar::spellings(name) {
            command.env_remove(spelling);
        }
    }
    if let Some(app) = app {
        command
            .env(SOCKET_ENV, app.socket())
            .env(CHAT_ENV, ASKING.to_string());
        if let Some(token) = app.token() {
            command.env(TOKEN_ENV, token.expose());
        }
    }
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the binary runs");
    // A refusal can come before charter reads the brief; `feed` leaves that to the caller,
    // which asserts on it.
    stand_in::feed(&mut child, BRIEF.as_bytes());
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
const WHAT_A_TERMINAL_IS_TOLD: &str = "✗ purlis handoff: no purlis app answered this call, so \
nothing was opened. Open purlis, then run this handoff again from a chat the app started — or \
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
) -> (App, Reading, Asked) {
    let within = tmp.path().join("app");
    std::fs::create_dir_all(&within).expect("a directory");
    let socket = within.join("s").join("hooks.sock");
    let listener = Listener::bind(&within, &socket).expect("a socket");
    let token = listener
        .tokens()
        .issue_to_this_process(ASKING)
        .expect("a token");
    let asked = Arc::new(Mutex::new(Vec::new()));
    let tickets = Tickets::default();
    let reading = listener.each_answering(Box::new(|_| {}), {
        let asked = Arc::clone(&asked);
        Box::new(move |connection, ask| {
            asked.lock().unwrap().push((connection, ask.clone()));
            answer(&tickets, connection, ask)
        })
    });
    (App { socket, token }, reading, asked)
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
                Ok(()) => Answer::Opened {
                    chat: 9,
                    row: None,
                    note: None,
                },
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
        Ask::SessionRecord(_) | Ask::Write(_) | Ask::Git(_) => Answer::No {
            why: "not a handoff".to_owned(),
        },
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
        "purlis handoff: opened chat 9 in workspace 'alpha', started on the brief\n"
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
        purlis_core::handoff::is_stamped_from(message, &ASKING.to_string()),
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
        "✗ purlis handoff: the purlis app that started this chat was asked, and would not \
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

// ----- what an opened handoff leaves behind (#372) ---------------------------------------

/// `alpha`'s open todos, as `charter workspace todo` lists them.
fn alphas_todos(root: &Path) -> Vec<purlis_core::workspaces::Entry> {
    purlis_core::workspaces::Plane::open(root)
        .workspace("alpha")
        .expect("a workspace name")
        .todos()
        .expect("a readable todo store")
}

#[test]
fn an_opened_handoff_leaves_its_todo_in_the_target_workspace_and_not_the_brief() {
    let tmp = daily();
    let root = root(&tmp);
    // The asking chat chose `beta` (`charter workspace use beta`), so that is where it works.
    std::fs::write(
        root.join(format!(".charter/sessions/{ASKING}.workspace")),
        "beta\n",
    )
    .unwrap();
    let before = alphas_todos(&root).len();
    let (socket, _reading, _asked) = an_app(&tmp, opens_as_nine);

    let out = handoff(&root, Some(&socket));

    assert_eq!(text(&out.stderr), "", "a recorded todo is not news");
    assert_eq!(out.status.code(), Some(0));
    let todos = alphas_todos(&root);
    assert_eq!(todos.len(), before + 1, "{todos:?}");
    let todo = todos
        .iter()
        // The brief's first line as the operator wrote it, heading mark and all
        // (`handoff::title`).
        .find(|t| t.title == "# Retry the failed webhook deliveries")
        .unwrap_or_else(|| panic!("the brief's first line is the todo's title: {todos:?}"));
    // Written out rather than derived from `todo_text`, so a change to a word of it is made
    // here too, on purpose.
    assert_eq!(
        todo.body,
        format!(
            "# Retry the failed webhook deliveries\n\nHanded off from chat {ASKING} · workspace \
             {}. The full brief is private to the chat it opened.",
            source_workspace(&root)
        )
    );
    assert!(
        !todo.body.contains("The queue is in"),
        "the brief never reaches a todo: {todo:?}"
    );
}

/// The workspace `charter handoff` stamps as its source, asked of the binary itself.
fn source_workspace(root: &Path) -> String {
    let out = unsteered(Command::new(env!("CARGO_BIN_EXE_purlis")))
        .args(["workspace", "current"])
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        .env("CHARTER_SESSION_ID", ASKING.to_string())
        .env_remove("CHARTER_WORKSPACE")
        .env_remove("CHARTER_PLANE_ROOT_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("TMUX_PANE")
        .output()
        .expect("the binary runs");
    text(&out.stdout).trim_end().to_owned()
}

#[test]
fn a_second_handoff_of_the_same_work_opens_its_chat_and_does_not_record_the_todo_twice() {
    // Reported and continued, not refused: a second chat on the same brief may be exactly
    // what the operator approved.
    let tmp = daily();
    let root = root(&tmp);
    let before = alphas_todos(&root).len();
    let (socket, _reading, _asked) = an_app(&tmp, opens_as_nine);

    handoff(&root, Some(&socket));
    let again = handoff(&root, Some(&socket));

    assert_eq!(again.status.code(), Some(0));
    assert_eq!(
        text(&again.stdout),
        "purlis handoff: opened chat 9 in workspace 'alpha', started on the brief\n"
    );
    assert_eq!(
        text(&again.stderr),
        "• already on 'alpha's list: # Retry the failed webhook deliveries — not recorded \
         twice\n"
    );
    assert_eq!(alphas_todos(&root).len(), before + 1);
}

/// The handoff rows in the plane's dispatch log.
fn handoff_rows(root: &Path) -> Vec<serde_json::Value> {
    purlis_core::dispatch::rows(root)
        .into_iter()
        .filter(|row| row["event"] == "handoff")
        .collect()
}

#[test]
fn an_opened_handoff_is_one_row_in_the_dispatch_log_that_names_nothing() {
    // Four fields, and what is missing is the design: no workspace name (a LOCAL
    // workspace's name must not reach a committed file), no persona, no brief.
    let tmp = daily();
    let root = root(&tmp);
    let (socket, _reading, _asked) = an_app(&tmp, opens_as_nine);

    let out = charter(
        &root,
        Some(&socket),
        &["handoff", "alpha", "--persona", "devops"],
    );

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let rows = handoff_rows(&root);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = rows[0].as_object().expect("a row is an object");
    let mut keys: Vec<&str> = row.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["created", "event", "placement", "ts"]);
    // The chat asks from `default`, and the work goes to `alpha`.
    assert_eq!(row["placement"], "elsewhere");
    assert_eq!(row["created"], false);
    assert_eq!(purlis_core::dispatch::tally(&root).get("devops"), Some(&1));
}

#[test]
fn a_handoff_the_app_would_not_open_writes_neither_a_todo_nor_a_row() {
    let tmp = daily();
    let root = root(&tmp);
    let before = alphas_todos(&root).len();
    let (socket, _reading, _asked) = an_app(&tmp, |_, _, _| Answer::No {
        why: "chat 3 is not on a harness profile".to_owned(),
    });

    let out = handoff(&root, Some(&socket));

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(alphas_todos(&root).len(), before);
    assert!(handoff_rows(&root).is_empty());
}

#[test]
fn a_todo_that_cannot_be_written_is_said_and_the_chat_stays_open() {
    let tmp = daily();
    let root = root(&tmp);
    let todos = root.join("workspaces/alpha/todos");
    std::fs::remove_dir_all(&todos).expect("the fixture's todos");
    std::fs::write(&todos, "not a directory\n").expect("a file in its place");
    let (socket, _reading, _asked) = an_app(&tmp, opens_as_nine);

    let out = handoff(&root, Some(&socket));

    assert_eq!(out.status.code(), Some(0), "the chat is open");
    assert_eq!(
        text(&out.stdout),
        "purlis handoff: opened chat 9 in workspace 'alpha', started on the brief\n"
    );
    let said = text(&out.stderr);
    assert!(
        said.starts_with(
            "! purlis handoff: chat 9 is open in 'alpha', but its todo could not be recorded \
             there ("
        ),
        "{said:?}"
    );
    assert!(
        said.ends_with(
            "). Record it with: purlis workspace todo -w alpha \"<the brief's first line>\"\n"
        ),
        "{said:?}"
    );
    assert_eq!(handoff_rows(&root).len(), 1, "the row is still written");
}

#[test]
fn a_dispatch_row_that_cannot_be_written_is_said_and_the_chat_stays_open() {
    let tmp = daily();
    let root = root(&tmp);
    let log = root.join("personas/_dispatch");
    std::fs::remove_dir_all(&log).expect("the fixture's log");
    std::fs::write(&log, "not a directory\n").expect("a file in its place");
    let (socket, _reading, _asked) = an_app(&tmp, opens_as_nine);

    let out = handoff(&root, Some(&socket));

    assert_eq!(out.status.code(), Some(0), "the chat is open");
    let said = text(&out.stderr);
    // The chat is open, so nothing is to be run again (#1359): only the row is missing, the
    // log named once, with the OS's reason.
    assert!(
        said.starts_with(
            "! purlis handoff: chat 9 is open in 'alpha'. Only its row in the dispatch log \
             (personas/_dispatch) is missing ("
        ),
        "{said:?}"
    );
    assert!(
        said.ends_with("); there is nothing to run again.\n"),
        "{said:?}"
    );
    assert_eq!(said.matches("_dispatch").count(), 1, "{said:?}");
    assert!(!said.contains("os error"), "{said:?}");
}

/// [`opens_as_nine`], from an app that writes the handoff's row itself and answers `row`.
fn opens_as_nine_and_answers_its_row(
    row: purlis_core::hookwire::Row,
) -> impl Fn(&Tickets, u64, Ask) -> Answer + Send + Sync + 'static {
    move |tickets, connection, ask| match opens_as_nine(tickets, connection, ask) {
        Answer::Opened { chat, .. } => Answer::Opened {
            chat,
            row: Some(row.clone()),
            note: None,
        },
        other => other,
    }
}

#[test]
fn a_row_the_app_wrote_is_not_written_a_second_time() {
    // #1421: a sandboxed chat may not write the project's dispatch log, so the app writes the
    // row where it opens the chat, and the command leaves it to the app.
    let tmp = daily();
    let root = root(&tmp);
    let (socket, _reading, _asked) = an_app(
        &tmp,
        opens_as_nine_and_answers_its_row(purlis_core::hookwire::Row::Written),
    );

    let out = handoff(&root, Some(&socket));

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stderr), "", "nothing to warn about");
    assert!(
        handoff_rows(&root).is_empty(),
        "the app's row, not this one"
    );
}

#[test]
fn a_row_the_app_could_not_write_is_said_with_the_app_s_reason() {
    let tmp = daily();
    let root = root(&tmp);
    let (socket, _reading, _asked) = an_app(
        &tmp,
        opens_as_nine_and_answers_its_row(purlis_core::hookwire::Row::Unwritten {
            why: "No space left on device".to_owned(),
        }),
    );

    let out = handoff(&root, Some(&socket));

    assert_eq!(out.status.code(), Some(0), "the chat is open");
    assert_eq!(
        text(&out.stderr),
        "! purlis handoff: chat 9 is open in 'alpha'. Only its row in the dispatch log \
         (personas/_dispatch) is missing (No space left on device); there is nothing to run \
         again.\n"
    );
    assert!(handoff_rows(&root).is_empty());
}

// ---- SI-1b: a handoff from the plane root ---------------------------------------------------

/// The open the app was asked for, once a handoff has run against [`opens_as_nine`].
fn opened(asked: &Asked) -> OpenChat {
    let asked = asked.lock().unwrap().clone();
    match asked.last() {
        Some((_, Ask::Open(open))) => (**open).clone(),
        other => panic!("the last line opens: {other:?}"),
    }
}

#[test]
fn a_handoff_from_a_chat_the_app_started_at_the_plane_root_is_stamped_with_the_plane_root() {
    // The defect: the stamp named the ladder's answer — the plane's default — for a chat that is
    // in no workspace. The open still names the workspace the brief goes to.
    let tmp = daily();
    let root = root(&tmp);
    let (socket, _reading, asked) = an_app(&tmp, opens_as_nine);

    let out = charter_with(
        &root,
        Some(&socket),
        &["handoff", "alpha"],
        &[("CHARTER_PLANE_ROOT_SESSION", "1")],
    );

    assert_eq!(text(&out.stderr), "", "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
    let open = opened(&asked);
    assert_eq!(open.workspace, "alpha", "it lands where the brief sends it");
    let read = purlis_core::handoff::stamped(&open.message).expect("stamped");
    assert_eq!(read.chat, ASKING.to_string());
    assert_eq!(read.place(), Some(purlis_core::active::Place::PlaneRoot));
    assert!(
        minute_masked(&open.message).starts_with(&format!(
            "⟨handoff from chat {ASKING} · plane root · <when>⟩\n\n"
        )),
        "{:?}",
        open.message
    );
    // And the todo it leaves in `alpha` says where it came from, the same way.
    let todo = alphas_todos(&root)
        .into_iter()
        .find(|t| t.title == "# Retry the failed webhook deliveries")
        .expect("the todo");
    assert!(
        todo.body
            .contains(&format!("Handed off from chat {ASKING} · plane root.")),
        "{}",
        todo.body
    );
}

#[test]
fn a_handoff_from_a_session_standing_at_the_root_with_no_workspace_is_stamped_the_same() {
    // Nothing chose a workspace for chat 3 in the daily plane, and it stands at the root.
    let tmp = daily();
    let (socket, _reading, asked) = an_app(&tmp, opens_as_nine);

    let out = handoff(&root(&tmp), Some(&socket));

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let open = opened(&asked);
    assert_eq!(
        purlis_core::handoff::stamped(&open.message).and_then(|read| read.place()),
        Some(purlis_core::active::Place::PlaneRoot)
    );
}

#[test]
fn a_handoff_from_a_chat_in_a_workspace_is_stamped_with_that_workspace() {
    let tmp = daily();
    let root = root(&tmp);
    let (socket, _reading, asked) = an_app(&tmp, opens_as_nine);

    let out = charter_with(
        &root,
        Some(&socket),
        &["handoff", "alpha"],
        &[("CHARTER_WORKSPACE", "beta")],
    );

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let open = opened(&asked);
    assert_eq!(
        purlis_core::handoff::stamped(&open.message).and_then(|read| read.place()),
        Some(purlis_core::active::Place::Workspace("beta".to_owned()))
    );
}

#[test]
fn a_report_kept_for_the_plane_root_says_so() {
    // The app keeps a report for the place its closed parent worked in, by `Place::word`.
    let tmp = daily();
    let (socket, _reading, _asked) = an_app(&tmp, |tickets, connection, ask| match ask {
        Ask::Report(back) => {
            match tickets.spend(back.chat, connection, &back.ticket, Instant::now()) {
                Ok(()) => Answer::Reported {
                    to: "steward 1".to_owned(),
                    kept_for: Some("plane root".to_owned()),
                },
                Err(why) => Answer::No { why },
            }
        }
        other => opens_as_nine(tickets, connection, other),
    });

    let out = charter(&root(&tmp), Some(&socket), &["handoff", "report", "Done."]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "charter handoff report: 'steward 1' has closed, so the report is kept for the plane \
         root. The next chat that starts there reads it.\n"
    );
}
