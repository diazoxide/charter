//! The UI RPC runs on the same link as the session protocol, for the app's window and its own
//! build only (FD-26, ADR 0068 §4 and §5). A window of another build, or a client of another
//! scope, is refused it, and keeps the session protocol.

use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use purlis_session_protocol::auth::Scope;
use purlis_session_protocol::link;
use purlis_session_protocol::session::{self, Answer, Client, Command, Frame, Host, Peer, Refusal};
use purlis_session_protocol::ui::{self, Handler};
use serde_json::{Value, json};
use tokio::io::duplex;

mod common;
use common::HELD;

/// The app's commands, as the host answers them: `rename_chat` succeeds, `opened_chats` fails
/// with the command's own error, and every call is kept.
#[derive(Default, Clone)]
struct Commands {
    asked: Arc<Mutex<Vec<(String, Value)>>>,
}

impl Handler for Commands {
    fn call(&self, method: String, args: Value) -> BoxFuture<'_, Result<Value, Value>> {
        Box::pin(async move {
            self.asked
                .lock()
                .unwrap()
                .push((method.clone(), args.clone()));
            match method.as_str() {
                "rename_chat" => Ok(json!({"renamed": args["name"]})),
                _ => Err(json!("that plane is not open")),
            }
        })
    }
}

struct Sessions;

impl Host for Sessions {
    async fn call(&self, _command: Command, _peer: &Peer) -> Result<Answer, Refusal> {
        Ok(Answer::Chats(Vec::new()))
    }
}

const BUILD: &str = "0.4.0+9055f7a";

async fn linked(scope: Scope, commands: Option<Commands>) -> Client {
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(a, session::speaks(), scope, HELD.of(scope)),
        link::serve_any(b, session::speaks(), &HELD)
    );
    let ui = commands.map(|c| ui::Server::new(BUILD, ["rename_chat", "opened_chats"], c));
    tokio::spawn(session::serve(served.unwrap(), Sessions, ui));
    Client::new(client.unwrap()).0
}

#[tokio::test]
async fn the_window_of_the_same_build_calls_the_apps_commands_on_the_link() {
    let commands = Commands::default();
    let client = linked(Scope::LocalUi, Some(commands.clone())).await;
    let ui = client.ui(BUILD).await.unwrap();

    let renamed = ui
        .call("rename_chat", json!({"chat": 5, "name": "login"}))
        .await
        .unwrap();
    assert_eq!(renamed, Ok(json!({"renamed": "login"})));
    let failed = ui.call("opened_chats", json!({"plane": 2})).await.unwrap();
    assert_eq!(failed, Err(json!("that plane is not open")));
    assert_eq!(
        commands.asked.lock().unwrap().clone(),
        vec![
            (
                "rename_chat".to_owned(),
                json!({"chat": 5, "name": "login"})
            ),
            ("opened_chats".to_owned(), json!({"plane": 2})),
        ]
    );
    // And the session protocol, beside it on the same lane.
    assert_eq!(client.list().await.unwrap(), Vec::new());
}

#[tokio::test]
async fn a_method_the_host_does_not_serve_is_refused_before_it_is_asked() {
    let commands = Commands::default();
    let client = linked(Scope::LocalUi, Some(commands.clone())).await;
    let ui = client.ui(BUILD).await.unwrap();
    let refused = ui.call("vault_secret_reveal", json!({})).await.unwrap();
    assert!(refused.is_err());
    assert!(commands.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn answering_an_ask_is_never_served_on_the_link_even_to_the_window() {
    // HP-6: `answer_ask` is the window's over Tauri's IPC alone, even when a host is built with
    // the app's whole command list.
    let commands = Commands::default();
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::LocalUi,
            HELD.of(Scope::LocalUi)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    let ui = ui::Server::new(
        BUILD,
        ["rename_chat", "answer_ask", "answer_task_question"],
        commands.clone(),
    );
    tokio::spawn(session::serve(served.unwrap(), Sessions, Some(ui)));
    let client = Client::new(client.unwrap()).0;
    let ui = client.ui(BUILD).await.unwrap();

    let refused = ui
        .call(
            "answer_ask",
            json!({"plane": 1, "session": 3, "ask": "a", "option": "allow"}),
        )
        .await
        .unwrap();

    assert!(refused.is_err(), "{refused:?}");
    assert!(commands.asked.lock().unwrap().is_empty());
    assert!(ui::WINDOW_ONLY.contains(&"answer_ask"));

    // #1496: the person's answer to a question a task put to its asking chat reaches that
    // task marked as the person's own words, and is held to the same.
    let refused = ui
        .call(
            "answer_task_question",
            json!({"plane": 1, "session": 3, "question": "Which queue?", "text": "The second."}),
        )
        .await
        .unwrap();

    assert!(refused.is_err(), "{refused:?}");
    assert!(commands.asked.lock().unwrap().is_empty());
    assert!(ui::WINDOW_ONLY.contains(&"answer_task_question"));
    // Every command that ends, starts or restarts a chat on the person's word is the window's
    // too (#1488). None of these is served on a link.
    for command in [
        "end_task",
        "task_ending",
        "stop_all_tasks",
        "all_tasks_ending",
        "stop_chat",
        "close_session",
        "close_chat_stopping",
        "end_task_that_did_not_start",
        "smart_close",
        "stop_every_agent",
        "forget_chat_that_did_not_start",
        "close_plane",
        "forget_project",
        "restart_on_the_session_bus",
        "restart_to_update",
        "ask_persona_chat",
        "start_chat",
        "start_chat_here",
        "open_session",
        "open_shell_in_branch",
        "first_task_run",
        "resume_session",
        "retry_chat_that_did_not_start",
        "reopen_finished_task",
        "curate",
        "relaunch",
        "restart_chat_without_sandbox",
        "restart_chat",
        "start_chat_fresh",
        "ask_chat_restart",
    ] {
        assert!(ui::WINDOW_ONLY.contains(&command), "{command}");
    }
    // And the list is exactly that rule's, with Stop all tasks and its question (#1498), the
    // three reads of what chats said or were sent (#1494, #1495, #1496), the person's answer
    // to a task (#1496), the dispatch commands (spec #1483) and the person's "Got it" on the
    // presets Notice (#1385) and the hosts Notice (#1550): nothing else is kept from a link by it.
    assert_eq!(ui::STANDING_DISPATCH.len(), 26);
    assert!(ui::WINDOW_ONLY.contains(&"acknowledge_project_presets"));
    // And the person's acts named beside them: a task's merge and discard (#1511), the answer
    // to a chat's sandbox block and to several tasks' (#1538, #1508), and the Allow of a
    // persona's hosts on this machine (#1362), the delete of a task's merged branch whose
    // folder is gone (#1472), and Allow on Settings' Blocked lately (#1662).
    for command in [
        "task_branch_merge",
        "dispatch_worktree_discard",
        "task_branch_delete",
        "allow_sandbox_block",
        "allow_sandbox_block_for_tasks",
        "keep_sandbox_block_for_tasks",
        // One chat's Keep blocked, which refuses a connection its proxy holds (#1666).
        "keep_sandbox_block",
        "allow_persona_hosts",
        "allow_blocked_host",
    ] {
        assert!(ui::WINDOW_ONLY.contains(&command), "{command}");
    }
    assert!(ui::WINDOW_ONLY.contains(&"acknowledge_project_hosts"));
    // And the start of the person's editor on this machine's extension record (#1296).
    assert!(ui::WINDOW_ONLY.contains(&"open_extension_record"));
    assert_eq!(
        ui::WINDOW_ONLY.len(),
        35 + 26 + 3 + 2 + 3 + 1 + 2 + 1 + 1 + 1
    );
}

#[tokio::test]
async fn no_standing_dispatch_grant_is_made_changed_or_taken_back_on_any_link() {
    // Spec #1483, train 64: every command that makes, widens, accepts, declines or takes back
    // a standing dispatch grant, every answer to a dispatch's question (Keep blocked too: it
    // is the person's answer), the reads of what stands and what waits, and the lists a
    // person answers from, are the window's over
    // Tauri's IPC alone, even when a host is built with every command. Named here one by one,
    // so taking one off the list fails this test and not only a generated file's diff.
    const STANDING: [&str; 26] = [
        "dispatch_grants_needed",
        "dispatch_standing",
        "dispatch_grants",
        "allow_dispatch",
        "allow_dispatch_anywhere",
        "keep_dispatch_blocked",
        "never_dispatch",
        "lift_dispatch_never",
        "allow_dispatch_to_any",
        "add_dispatch_grant",
        "revoke_dispatch_to_any",
        "revoke_dispatch_grant",
        "accept_project_dispatch",
        "decline_project_dispatch",
        "accept_project_dispatch_in",
        "decline_project_dispatch_in",
        "set_dispatch_workspace",
        "give_back_dispatch",
        "remove_dormant_dispatch",
        "dispatch_arrival",
        "answer_dispatch_arrival",
        "dispatch_gone_told",
        "dispatch_away",
        "allow_dispatch_away",
        "dismiss_dispatch_away",
        "never_dispatch_away",
    ];
    assert_eq!(ui::STANDING_DISPATCH, STANDING);
    let commands = Commands::default();
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::LocalUi,
            HELD.of(Scope::LocalUi)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    let ui = ui::Server::new(
        BUILD,
        STANDING.into_iter().chain(["rename_chat"]),
        commands.clone(),
    );
    tokio::spawn(session::serve(served.unwrap(), Sessions, Some(ui)));
    let client = Client::new(client.unwrap()).0;
    let ui = client.ui(BUILD).await.unwrap();

    for command in STANDING {
        let refused = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            ui.call(
                command,
                json!({"plane": 1, "id": 1, "level": "project", "asking": "steward", "target": "devops"}),
            ),
        )
        .await
        .unwrap_or_else(|_| panic!("{command} was answered"))
        .unwrap();
        assert!(refused.is_err(), "{command}: {refused:?}");
    }
    assert!(commands.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_refusal_kept_while_nobody_was_there_is_read_and_answered_on_no_link() {
    // #1507: the list and its three answers (one a standing grant in one press) are the
    // window's over Tauri's IPC alone, even when a host is built with every command.
    const AWAY: [&str; 4] = [
        "dispatch_away",
        "allow_dispatch_away",
        "dismiss_dispatch_away",
        "never_dispatch_away",
    ];
    let commands = Commands::default();
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::LocalUi,
            HELD.of(Scope::LocalUi)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    let ui = ui::Server::new(
        BUILD,
        AWAY.into_iter().chain(["rename_chat"]),
        commands.clone(),
    );
    tokio::spawn(session::serve(served.unwrap(), Sessions, Some(ui)));
    let client = Client::new(client.unwrap()).0;
    let ui = client.ui(BUILD).await.unwrap();

    for command in AWAY {
        let refused = ui
            .call(
                command,
                json!({"plane": 1, "asking": "steward", "target": "devops", "workspace": null}),
            )
            .await
            .unwrap();
        assert!(refused.is_err(), "{command}: {refused:?}");
        assert!(ui::WINDOW_ONLY.contains(&command), "{command}");
    }
    assert!(commands.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_task_s_brief_is_never_served_on_the_link_even_to_the_window() {
    // #1494: the brief a task was sent is read over Tauri's IPC alone, even when a host is
    // built with the app's whole command list.
    let commands = Commands::default();
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::LocalUi,
            HELD.of(Scope::LocalUi)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    let ui = ui::Server::new(BUILD, ["rename_chat", "task_brief"], commands.clone());
    tokio::spawn(session::serve(served.unwrap(), Sessions, Some(ui)));
    let client = Client::new(client.unwrap()).0;
    let ui = client.ui(BUILD).await.unwrap();

    for of in [
        json!({"chat": 3}),
        json!({"dispatch": "01K6DISPATCH00000000000000"}),
    ] {
        let refused = ui
            .call("task_brief", json!({"plane": 1, "of": of}))
            .await
            .unwrap();
        assert!(refused.is_err(), "{refused:?}");
    }

    assert!(commands.asked.lock().unwrap().is_empty());
    assert!(ui::WINDOW_ONLY.contains(&"task_brief"));
}

#[tokio::test]
async fn what_a_session_and_its_tasks_said_is_never_served_on_the_link_even_to_the_window() {
    // A session's Activity (#1495) and the question a task is paused on (#1496) are words
    // chats wrote, read by a session the caller names: Tauri's IPC alone, as a brief is.
    let commands = Commands::default();
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::LocalUi,
            HELD.of(Scope::LocalUi)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    let ui = ui::Server::new(
        BUILD,
        ["rename_chat", "activity", "task_question"],
        commands.clone(),
    );
    tokio::spawn(session::serve(served.unwrap(), Sessions, Some(ui)));
    let client = Client::new(client.unwrap()).0;
    let ui = client.ui(BUILD).await.unwrap();

    for method in ["activity", "task_question"] {
        let refused = ui
            .call(method, json!({"plane": 1, "session": 3}))
            .await
            .unwrap();
        assert!(refused.is_err(), "{method}: {refused:?}");
        assert!(ui::WINDOW_ONLY.contains(&method), "{method}");
    }
    assert!(commands.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_window_of_another_build_is_refused_the_ui_rpc_and_keeps_the_session_protocol() {
    let commands = Commands::default();
    let client = linked(Scope::LocalUi, Some(commands.clone())).await;
    let refused = client.ui("0.3.9+1111111").await.err().unwrap();
    assert!(
        refused.to_string().contains("private to one build"),
        "{refused}"
    );
    assert_eq!(client.list().await.unwrap(), Vec::new());
}

#[tokio::test]
async fn a_client_of_another_scope_is_refused_the_ui_rpc() {
    for scope in [
        Scope::Terminal,
        Scope::FleetMcp,
        Scope::Approval,
        Scope::Editor,
    ] {
        let commands = Commands::default();
        let client = linked(scope, Some(commands.clone())).await;
        let refused = client.ui(BUILD).await.err().unwrap();
        assert!(refused.to_string().contains("local-ui"), "{refused}");
        assert!(commands.asked.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_host_that_serves_no_ui_rpc_refuses_it() {
    let client = linked(Scope::LocalUi, None).await;
    assert!(client.ui(BUILD).await.is_err());
    assert_eq!(client.list().await.unwrap(), Vec::new());
}

/// A UI RPC call sent raw, as `scope`, with no hello first: what the host answers, and the
/// commands it may have asked.
async fn called_without_a_hello(scope: Scope) -> (Value, Commands) {
    let commands = Commands::default();
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(a, session::speaks(), scope, HELD.of(scope)),
        link::serve_any(b, session::speaks(), &HELD)
    );
    let ui = ui::Server::new(BUILD, ["rename_chat"], commands.clone());
    tokio::spawn(session::serve(served.unwrap(), Sessions, Some(ui)));
    let mut link = client.unwrap();
    link.control()
        .send(bytes::Bytes::from(
            json!({"ui": {"call": {"id": 1, "method": "rename_chat", "args": {}}}}).to_string(),
        ))
        .await
        .unwrap();
    let answer: Value =
        serde_json::from_slice(&link.control().next().await.unwrap().unwrap()).unwrap();
    (answer, commands)
}

#[tokio::test]
async fn a_call_before_the_hello_is_refused() {
    let (answer, commands) = called_without_a_hello(Scope::LocalUi).await;
    assert_eq!(answer["ui"]["reply"]["re"], 1);
    assert!(answer["ui"]["reply"]["err"].is_string());
    assert!(commands.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_client_of_another_scope_that_skips_the_hello_is_refused_too() {
    for scope in [
        Scope::Terminal,
        Scope::FleetMcp,
        Scope::Approval,
        Scope::Editor,
    ] {
        let (answer, commands) = called_without_a_hello(scope).await;
        assert!(answer["ui"]["reply"]["err"].is_string(), "{scope:?}");
        assert!(answer["ui"]["reply"].get("ok").is_none(), "{scope:?}");
        assert!(commands.asked.lock().unwrap().is_empty(), "{scope:?}");
    }
}

#[tokio::test]
async fn a_refused_hello_closes_what_an_earlier_one_opened() {
    let commands = Commands::default();
    let client = linked(Scope::LocalUi, Some(commands.clone())).await;
    let ui = client.ui(BUILD).await.unwrap();
    assert!(client.ui("0.3.9+1111111").await.is_err());
    let refused = ui.call("rename_chat", json!({})).await.unwrap();
    assert!(refused.is_err());
    assert!(commands.asked.lock().unwrap().is_empty());
}

/// The frames the app's TypeScript client writes and reads (`app/src/uiRpcLink.test.ts` holds
/// the same lines), so the two ends agree on the wire.
#[test]
fn the_typescript_clients_frames_are_the_ones_the_host_reads() {
    let hello: Frame =
        serde_json::from_str(r#"{"ui":{"hello":{"build":"0.4.0+9055f7a"}}}"#).unwrap();
    assert_eq!(
        hello,
        Frame::Ui(ui::Frame::Hello {
            build: BUILD.into()
        })
    );
    let call: Frame = serde_json::from_str(
        r#"{"ui":{"call":{"id":1,"method":"rename_chat","args":{"plane":"p1","session":5,"label":"login"}}}}"#,
    )
    .unwrap();
    assert_eq!(
        call,
        Frame::Ui(ui::Frame::Call(ui::Call {
            id: 1,
            method: "rename_chat".into(),
            args: json!({"plane": "p1", "session": 5, "label": "login"}),
        }))
    );
    let welcome = Frame::Ui(ui::Frame::Welcome {
        build: BUILD.into(),
    });
    assert_eq!(
        serde_json::to_string(&welcome).unwrap(),
        r#"{"ui":{"welcome":{"build":"0.4.0+9055f7a"}}}"#
    );
    let ok = Frame::Ui(ui::Frame::Reply(ui::Reply {
        re: 1,
        outcome: ui::Outcome::Ok(Value::Null),
    }));
    assert_eq!(
        serde_json::to_string(&ok).unwrap(),
        r#"{"ui":{"reply":{"re":1,"ok":null}}}"#
    );
    let err = Frame::Ui(ui::Frame::Reply(ui::Reply {
        re: 1,
        outcome: ui::Outcome::Err(json!("that plane is not open")),
    }));
    assert_eq!(
        serde_json::to_string(&err).unwrap(),
        r#"{"ui":{"reply":{"re":1,"err":"that plane is not open"}}}"#
    );
}
