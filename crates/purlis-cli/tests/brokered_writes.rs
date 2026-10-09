//! Brokered writes from inside a chat the app started (ADR 0067 §2, #1333): `purlis persona
//! remember`, `purlis workspace remember`/`note`, `purlis workspace todo` and the MCP server's
//! `persona_remember` hand their write to the app over the chat's hook socket, and print what
//! the app wrote as they print their own writes.
//!
//! The app here is a listener that answers each write with the core's own
//! `purlis_core::brokered::perform`, for chat 3 in `alpha` running as `steward`: what the real
//! app does once it has looked the chat up. That the command did not write it itself is read
//! off the trace, which credits a brokered write to the chat it was made for.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use purlis_core::active::Place;
use purlis_core::brokered::{self, Asker, Write};
use purlis_core::hookwire::{self, Answer, Ask, CHAT_ENV, Listener, SOCKET_ENV, TOKEN_ENV};

/// A project with workspaces `alpha` and `beta` and persona `steward`, and no manifest: none of
/// these commands reads one.
fn a_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    for ws in ["alpha", "beta"] {
        std::fs::create_dir_all(root.join("workspaces").join(ws)).unwrap();
    }
    std::fs::create_dir_all(root.join("personas/steward")).unwrap();
    std::fs::write(
        root.join("personas/steward/persona.md"),
        "---\nname: steward\n---\n# Steward\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("home")).unwrap();
    dir
}

fn root(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("project")
}

/// The app: every write it is asked for is made by the core for chat 3 in `alpha`, as
/// `steward`, unless `answer` overrides it; every ask is handed to the receiver.
struct App {
    _reading: hookwire::Reading,
    token: hookwire::ChatToken,
    socket: PathBuf,
    asked: mpsc::Receiver<Ask>,
}

fn an_app(tmp: &tempfile::TempDir, answer: Option<Answer>) -> App {
    let socket = tmp.path().join("app").join("hooks.sock");
    let (tx, asked) = mpsc::channel();
    let tx = Mutex::new(tx);
    let listener = Listener::bind(tmp.path(), &socket).unwrap_or_else(|e| panic!("a socket: {e}"));
    let token = listener.tokens().issue_to_this_process(3).expect("a token");
    let project = root(tmp);
    let reading = listener.each_answering(
        Box::new(|_| {}),
        Box::new(move |_, ask| {
            tx.lock().unwrap().send(ask.clone()).unwrap();
            if let Some(answer) = &answer {
                return answer.clone();
            }
            let Ask::Write(write) = ask else {
                return Answer::No {
                    why: "not a write".to_owned(),
                };
            };
            let asker = Asker {
                chat: write.chat,
                place: Place::Workspace("alpha".to_owned()),
                persona: Some("steward".to_owned()),
            };
            match brokered::perform(
                &project,
                &asker,
                &write.write,
                chrono::Local::now().naive_local(),
            ) {
                Ok(written) => Answer::Written {
                    to: written.to,
                    path: written.path,
                },
                Err(why) => Answer::No { why },
            }
        }),
    );
    App {
        _reading: reading,
        token,
        socket,
        asked,
    }
}

impl App {
    /// The environment of chat 3, which the app started.
    fn chat_env(&self) -> Vec<(String, String)> {
        vec![
            (SOCKET_ENV.to_owned(), self.socket.display().to_string()),
            (CHAT_ENV.to_owned(), "3".to_owned()),
            (TOKEN_ENV.to_owned(), self.token.expose().to_owned()),
            ("PURLIS_PERSONA".to_owned(), "steward".to_owned()),
        ]
    }

    fn write_asked(&self) -> Write {
        match self.asked.recv_timeout(Duration::from_secs(5)) {
            Ok(Ask::Write(ask)) => {
                assert_eq!(ask.chat, 3);
                ask.write
            }
            other => panic!("the app was not asked for a write: {other:?}"),
        }
    }
}

/// `purlis <args>` in `alpha`, with nothing of the caller's environment but what is named.
fn purlis(tmp: &tempfile::TempDir, args: &[&str], env: &[(String, String)]) -> Output {
    let root = root(tmp);
    Command::new(env!("CARGO_BIN_EXE_purlis"))
        .args(args)
        .current_dir(root.join("workspaces/alpha"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("CHARTER_ROOT", &root)
        .env("HOME", tmp.path().join("home"))
        .env("TMPDIR", std::env::temp_dir())
        .env("NO_COLOR", "1")
        .env("CHARTER_NO_BACKGROUND_CHECKS", "1")
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .output()
        .expect("purlis runs")
}

fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// What the app wrote for chat 3: the trace's brokered events.
fn brokered_for_chat_3(tmp: &tempfile::TempDir) -> Vec<String> {
    std::fs::read_to_string(purlis_core::trace::file(&root(tmp), "3"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("\"event\": \"brokered\""))
        .map(str::to_owned)
        .collect()
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name != "MEMORY.md")
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn persona_remember_in_a_chat_is_written_by_the_app_in_the_chat_s_own_persona() {
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let ran = purlis(
        &tmp,
        &["persona", "remember", "Deploys go through the pipeline"],
        &app.chat_env(),
    );

    assert!(ran.status.success(), "{}", said(&ran));
    assert_eq!(
        app.write_asked(),
        Write::PersonaRemember {
            text: "Deploys go through the pipeline".to_owned(),
            title: None,
            shared: false,
        }
    );
    assert!(
        said(&ran).contains(
            "Remembered (persistent) → personas/steward/memory/deploys-go-through-the-pipeline.md"
        ),
        "{}",
        said(&ran)
    );
    assert_eq!(
        files(&root(&tmp).join("personas/steward/memory")),
        ["deploys-go-through-the-pipeline.md"]
    );
    assert_eq!(brokered_for_chat_3(&tmp).len(), 1);
}

#[test]
fn persona_remember_shared_in_a_chat_is_written_by_the_app_in_shared_memory() {
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let ran = purlis(
        &tmp,
        &[
            "persona",
            "remember",
            "--shared",
            "--title",
            "Forge",
            "The forge is self-hosted",
        ],
        &app.chat_env(),
    );

    assert!(ran.status.success(), "{}", said(&ran));
    assert_eq!(
        app.write_asked(),
        Write::PersonaRemember {
            text: "The forge is self-hosted".to_owned(),
            title: Some("Forge".to_owned()),
            shared: true,
        }
    );
    assert!(
        said(&ran).contains("Remembered (shared persistent) → personas/_shared/memory/"),
        "{}",
        said(&ran)
    );
    assert_eq!(files(&root(&tmp).join("personas/_shared/memory")).len(), 1);
    assert_eq!(brokered_for_chat_3(&tmp).len(), 1);
}

#[test]
fn workspace_remember_and_note_in_a_chat_are_written_by_the_app_in_the_chat_s_workspace() {
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let remembered = purlis(
        &tmp,
        &["workspace", "remember", "The build needs two jobs"],
        &app.chat_env(),
    );
    let noted = purlis(&tmp, &["ws", "note", "CI runs on Linux"], &app.chat_env());

    for ran in [&remembered, &noted] {
        assert!(ran.status.success(), "{}", said(ran));
        assert!(
            said(ran).contains("Remembered in 'alpha' → workspaces/alpha/memory/"),
            "{}",
            said(ran)
        );
    }
    assert_eq!(
        app.write_asked(),
        Write::WorkspaceRemember {
            text: "The build needs two jobs".to_owned(),
            title: None,
        }
    );
    assert_eq!(
        app.write_asked(),
        Write::WorkspaceRemember {
            text: "CI runs on Linux".to_owned(),
            title: None,
        }
    );
    assert_eq!(files(&root(&tmp).join("workspaces/alpha/memory")).len(), 2);
    assert_eq!(brokered_for_chat_3(&tmp).len(), 2);
}

#[test]
fn workspace_todo_in_a_chat_is_written_by_the_app_in_the_chat_s_workspace() {
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let ran = purlis(
        &tmp,
        &["ws", "todo", "Port the docs command"],
        &app.chat_env(),
    );

    assert!(ran.status.success(), "{}", said(&ran));
    assert_eq!(
        app.write_asked(),
        Write::Todo {
            text: "Port the docs command".to_owned(),
        }
    );
    assert!(
        said(&ran).contains("Todo recorded in 'alpha' → workspaces/alpha/todos/"),
        "{}",
        said(&ran)
    );
    assert_eq!(files(&root(&tmp).join("workspaces/alpha/todos")).len(), 1);
    assert_eq!(brokered_for_chat_3(&tmp).len(), 1);
}

#[test]
fn a_write_the_app_refuses_is_refused_here_and_never_written_here() {
    let tmp = a_project();
    let app = an_app(
        &tmp,
        Some(Answer::No {
            why: "chat 3 has asked purlis for 60 writes in the last 60 seconds".to_owned(),
        }),
    );

    let ran = purlis(&tmp, &["ws", "todo", "One too many"], &app.chat_env());

    assert_eq!(ran.status.code(), Some(1), "{}", said(&ran));
    assert!(said(&ran).contains("60 writes"), "{}", said(&ran));
    assert!(files(&root(&tmp).join("workspaces/alpha/todos")).is_empty());
}

#[test]
fn an_app_that_does_not_answer_writes_has_them_written_here_as_before() {
    let tmp = a_project();
    let app = an_app(
        &tmp,
        Some(Answer::No {
            why: hookwire::NOTHING_ANSWERS.to_owned(),
        }),
    );

    let ran = purlis(
        &tmp,
        &["workspace", "remember", "Written here"],
        &app.chat_env(),
    );

    assert!(ran.status.success(), "{}", said(&ran));
    assert_eq!(files(&root(&tmp).join("workspaces/alpha/memory")).len(), 1);
    assert!(brokered_for_chat_3(&tmp).is_empty());
}

#[test]
fn a_write_naming_another_workspace_is_not_the_app_s_to_make() {
    // The app writes only the chat's own workspace; `-w` is written here, where the chat's
    // sandbox decides, as it always was.
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let ran = purlis(
        &tmp,
        &["ws", "todo", "-w", "beta", "Over there"],
        &app.chat_env(),
    );

    assert!(ran.status.success(), "{}", said(&ran));
    assert!(app.asked.try_recv().is_err(), "the app was asked");
    assert_eq!(files(&root(&tmp).join("workspaces/beta/todos")).len(), 1);
}

// ---- the MCP server's persona_remember -------------------------------------------------------

/// `purlis mcp` in chat 3, initialized, and `persona_remember` called once with `arguments`.
fn persona_remember_over_mcp(
    tmp: &tempfile::TempDir,
    env: &[(String, String)],
    arguments: serde_json::Value,
) -> serde_json::Value {
    tool_over_mcp(tmp, env, "persona_remember", arguments)
}

/// `purlis mcp` in chat 3, initialized, and `tool` called once with `arguments`.
fn tool_over_mcp(
    tmp: &tempfile::TempDir,
    env: &[(String, String)],
    tool: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let root = root(tmp);
    let mut child = Command::new(env!("CARGO_BIN_EXE_purlis"))
        .arg("mcp")
        .current_dir(root.join("workspaces/alpha"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("CHARTER_ROOT", &root)
        .env("HOME", tmp.path().join("home"))
        .env("TMPDIR", std::env::temp_dir())
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("purlis mcp starts");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut read = || {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<serde_json::Value>(&line).expect("JSON-RPC")
    };
    for message in [
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": { "name": "test-harness", "version": "1" } } }),
        serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": tool, "arguments": arguments } }),
    ] {
        writeln!(stdin, "{message}").unwrap();
    }
    stdin.flush().unwrap();
    let _initialized = read();
    let called = read();
    let _ = child.kill();
    let _ = child.wait();
    called["result"].clone()
}

#[test]
fn the_persona_remember_tool_in_a_chat_is_written_by_the_app() {
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let result = persona_remember_over_mcp(
        &tmp,
        &app.chat_env(),
        serde_json::json!({ "text": "Releases are tagged by hand", "shared": true }),
    );

    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(
        app.write_asked(),
        Write::PersonaRemember {
            text: "Releases are tagged by hand".to_owned(),
            title: None,
            shared: true,
        }
    );
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("Remembered (shared persistent) → personas/_shared/memory/"),
        "{result}"
    );
    assert_eq!(files(&root(&tmp).join("personas/_shared/memory")).len(), 1);
    assert_eq!(brokered_for_chat_3(&tmp).len(), 1);
}

#[test]
fn the_persona_remember_tool_outside_any_chat_the_app_started_writes_the_persona_itself() {
    // No chat number: no app could have written it, so the server does.
    let tmp = a_project();

    let result = persona_remember_over_mcp(
        &tmp,
        &[("PURLIS_PERSONA".to_owned(), "steward".to_owned())],
        serde_json::json!({ "text": "Written by the server" }),
    );

    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(
        files(&root(&tmp).join("personas/steward/memory")),
        ["written-by-the-server.md"]
    );
}

#[test]
fn the_persona_remember_tool_in_a_chat_whose_harness_gave_it_no_connection_writes_nothing() {
    // Codex hands the server the chat's number and never its socket: the server runs outside
    // the sandbox, so it refuses rather than write what no sandbox bounds (#1333).
    let tmp = a_project();

    let result = persona_remember_over_mcp(
        &tmp,
        &[
            ("PURLIS_PERSONA".to_owned(), "steward".to_owned()),
            (
                purlis_core::active::SESSION_ID_ENV.to_owned(),
                "3".to_owned(),
            ),
        ],
        serde_json::json!({ "text": "Never written", "shared": true }),
    );

    assert_eq!(result["isError"], true, "{result}");
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("purlis persona remember"), "{result}");
    assert!(!root(&tmp).join("personas/_shared/memory").exists());
}

#[test]
fn the_persona_remember_tool_whose_app_dropped_the_ask_writes_nothing() {
    let tmp = a_project();
    let app = an_app(
        &tmp,
        Some(Answer::No {
            why: hookwire::NOTHING_ANSWERS.to_owned(),
        }),
    );

    let result = persona_remember_over_mcp(
        &tmp,
        &app.chat_env(),
        serde_json::json!({ "text": "Never written", "shared": true }),
    );

    assert_eq!(result["isError"], true, "{result}");
    assert!(!root(&tmp).join("personas/_shared/memory").exists());
}

#[test]
fn the_persona_remember_tool_whose_app_has_gone_writes_the_persona_itself() {
    // The socket the chat was given is not there any more: there is no app to ask.
    let tmp = a_project();
    let gone = tmp.path().join("app").join("hooks.sock");

    let result = persona_remember_over_mcp(
        &tmp,
        &[
            ("PURLIS_PERSONA".to_owned(), "steward".to_owned()),
            (SOCKET_ENV.to_owned(), gone.display().to_string()),
            (CHAT_ENV.to_owned(), "3".to_owned()),
        ],
        serde_json::json!({ "text": "After the app" }),
    );

    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(
        files(&root(&tmp).join("personas/steward/memory")),
        ["after-the-app.md"]
    );
}

// ---- workspace.md: the vision and the decisions and glossary sections (#1384) --------------

fn alpha_charter(tmp: &tempfile::TempDir) -> String {
    std::fs::read_to_string(root(tmp).join("workspaces/alpha/workspace.md")).unwrap_or_default()
}

#[test]
fn workspace_vision_in_a_chat_is_written_by_the_app_in_the_chat_s_workspace() {
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let ran = purlis(
        &tmp,
        &["ws", "vision", "Ship the docs site"],
        &app.chat_env(),
    );

    assert!(ran.status.success(), "{}", said(&ran));
    assert_eq!(
        app.write_asked(),
        Write::WorkspaceVision {
            text: "Ship the docs site".to_owned(),
        }
    );
    assert_eq!(
        purlis_core::mdsection::section_body(&alpha_charter(&tmp), "Vision"),
        "Ship the docs site"
    );
    assert_eq!(brokered_for_chat_3(&tmp).len(), 1);
}

#[test]
fn the_workspace_section_tool_in_a_chat_is_written_by_the_app() {
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let result = tool_over_mcp(
        &tmp,
        &app.chat_env(),
        "workspace_section",
        serde_json::json!({ "section": "decisions", "text": "Deploys go out on Fridays" }),
    );

    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(
        app.write_asked(),
        Write::WorkspaceSection {
            section: purlis_core::brokered::Section::Decisions,
            text: "Deploys go out on Fridays".to_owned(),
        }
    );
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("Added to ## Context & decisions of 'alpha' → workspaces/alpha/workspace.md"),
        "{result}"
    );
    assert!(
        purlis_core::mdsection::section_body(&alpha_charter(&tmp), "Context & decisions")
            .ends_with("- Deploys go out on Fridays"),
        "{}",
        alpha_charter(&tmp)
    );
    assert_eq!(brokered_for_chat_3(&tmp).len(), 1);
}

#[test]
fn the_workspace_vision_tool_outside_any_chat_the_app_started_writes_it_itself() {
    // No chat number: no app could have written it, so the server does, in the workspace a
    // `purlis` command run here would act on.
    let tmp = a_project();

    let result = tool_over_mcp(
        &tmp,
        &[],
        "workspace_vision",
        serde_json::json!({ "text": "Written by the server" }),
    );

    assert_ne!(result["isError"], true, "{result}");
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("Vision set for 'alpha' → workspaces/alpha/workspace.md"),
        "{result}"
    );
    assert_eq!(
        purlis_core::mdsection::section_body(&alpha_charter(&tmp), "Vision"),
        "Written by the server"
    );
}

#[test]
fn the_workspace_md_tools_in_a_chat_whose_harness_gave_it_no_connection_write_nothing() {
    let tmp = a_project();
    let codex = [(
        purlis_core::active::SESSION_ID_ENV.to_owned(),
        "3".to_owned(),
    )];

    for (tool, arguments) in [
        (
            "workspace_vision",
            serde_json::json!({ "text": "Never written" }),
        ),
        (
            "workspace_section",
            serde_json::json!({ "section": "glossary", "text": "Never written" }),
        ),
    ] {
        let result = tool_over_mcp(&tmp, &codex, tool, arguments);
        assert_eq!(result["isError"], true, "{result}");
    }
    assert!(!root(&tmp).join("workspaces/alpha/workspace.md").exists());
}

// ---- the MCP server's session_record (#1332), under the same rule (#1408) ------------------

const RECORD_BODY: &str = "## Goal\n\ng\n\n## Done\n\nd\n\n## Decisions\n\nx\n\n## Open\n\no\n\n\
                           ## How to resume\n\nr\n";

fn records_in_alpha(tmp: &tempfile::TempDir) -> Vec<String> {
    files(&root(tmp).join("workspaces/alpha/sessions"))
        .into_iter()
        .filter(|name| name != "index.md")
        .collect()
}

fn session_record_over_mcp(tmp: &tempfile::TempDir, env: &[(String, String)]) -> serde_json::Value {
    tool_over_mcp(
        tmp,
        env,
        "session_record",
        serde_json::json!({ "title": "Over MCP", "body": RECORD_BODY }),
    )
}

#[test]
fn the_session_record_tool_outside_any_chat_the_app_started_writes_the_record_itself() {
    let tmp = a_project();

    let result = session_record_over_mcp(&tmp, &[]);

    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(records_in_alpha(&tmp).len(), 1);
}

#[test]
fn the_session_record_tool_in_a_chat_whose_harness_gave_it_no_connection_writes_nothing() {
    let tmp = a_project();

    let result = session_record_over_mcp(
        &tmp,
        &[(
            purlis_core::active::SESSION_ID_ENV.to_owned(),
            "3".to_owned(),
        )],
    );

    assert_eq!(result["isError"], true, "{result}");
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("purlis session record"), "{result}");
    assert!(records_in_alpha(&tmp).is_empty());
}

#[test]
fn the_session_record_tool_whose_app_dropped_the_ask_writes_nothing() {
    let tmp = a_project();
    let app = an_app(
        &tmp,
        Some(Answer::No {
            why: hookwire::NOTHING_ANSWERS.to_owned(),
        }),
    );

    let result = session_record_over_mcp(&tmp, &app.chat_env());

    assert_eq!(result["isError"], true, "{result}");
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("purlis session record"), "{result}");
    assert!(records_in_alpha(&tmp).is_empty());
}

#[test]
fn the_session_record_tool_whose_app_has_gone_writes_the_record_itself() {
    let tmp = a_project();
    let gone = tmp.path().join("app").join("hooks.sock");

    let result = session_record_over_mcp(
        &tmp,
        &[
            (SOCKET_ENV.to_owned(), gone.display().to_string()),
            (CHAT_ENV.to_owned(), "3".to_owned()),
        ],
    );

    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(records_in_alpha(&tmp).len(), 1);
}

#[test]
fn the_session_record_tool_refuses_a_title_that_is_not_one_line_before_asking_anyone() {
    let tmp = a_project();
    let app = an_app(&tmp, None);

    let result = tool_over_mcp(
        &tmp,
        &app.chat_env(),
        "session_record",
        serde_json::json!({ "title": "Two\nlines", "body": RECORD_BODY }),
    );

    assert_eq!(result["isError"], true, "{result}");
    assert!(app.asked.try_recv().is_err(), "the app was asked");
    assert!(records_in_alpha(&tmp).is_empty());
}
