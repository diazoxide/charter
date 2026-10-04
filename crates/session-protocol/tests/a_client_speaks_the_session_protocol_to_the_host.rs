//! The session protocol's commands, from a client to a host over one link (FD-26, ADR 0068 §4):
//! each is answered, in its own words, by the host behind it.

use std::sync::{Arc, Mutex};

use charter_session_protocol::auth::Scope;
use charter_session_protocol::link;
use charter_session_protocol::session::{
    self, Answer, Chat, Client, Command, Host, Peer, Refusal, Start,
};
use tokio::io::duplex;

mod common;
use common::HELD;

/// A host that answers from a fixed board, and keeps every command it was sent.
#[derive(Default, Clone)]
struct Board {
    heard: Arc<Mutex<Vec<Command>>>,
}

impl Host for Board {
    async fn call(&self, command: Command, _peer: &Peer) -> Result<Answer, Refusal> {
        self.heard.lock().unwrap().push(command.clone());
        match command {
            Command::List => Ok(Answer::Chats(vec![Chat {
                chat: "01J9ZQ3V7K8M2N4P6R8T0V2X4Z".into(),
                project: "01J9PRJCT00000000000000000".into(),
                project_path: Some("/home/me/project".into()),
                title: Some("fix the login".into()),
                state: "working".into(),
            }])),
            Command::Start(start) if start.project.is_empty() => Err(Refusal {
                code: session::NO_SUCH_PROJECT.into(),
                why: "no project was named".into(),
            }),
            Command::Start(_) => Ok(Answer::Started {
                chat: "01J9ZQ5000000000000000000A".into(),
            }),
            Command::Stop { chat } if chat == "nobody" => Err(Refusal {
                code: session::NO_SUCH_CHAT.into(),
                why: "no chat nobody".into(),
            }),
            _ => Ok(Answer::Done),
        }
    }
}

async fn linked(host: Board) -> Client {
    let (a, b) = duplex(256 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::Terminal,
            HELD.of(Scope::Terminal)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    tokio::spawn(session::serve(served.unwrap(), host, None));
    let (client, _views, _events) = Client::new(client.unwrap());
    client
}

#[tokio::test]
async fn a_client_lists_the_hosts_chats() {
    let client = linked(Board::default()).await;
    let chats = client.list().await.unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].chat, "01J9ZQ3V7K8M2N4P6R8T0V2X4Z");
    assert_eq!(chats[0].state, "working");
}

#[tokio::test]
async fn every_command_reaches_the_host_as_it_was_sent() {
    let board = Board::default();
    let client = linked(board.clone()).await;
    let chat = "01J9ZQ3V7K8M2N4P6R8T0V2X4Z";
    client.write(chat, b"ls\r\x1b[<0;3;4M\xff").await.unwrap();
    client.resize(chat, 120, 40).await.unwrap();
    client.stop(chat).await.unwrap();
    let started = client
        .start(Start {
            project: "01J9PRJCT00000000000000000".into(),
            workspace: Some("login".into()),
            harness: Some("claude".into()),
        })
        .await
        .unwrap();
    assert_eq!(started, "01J9ZQ5000000000000000000A");

    let heard = board.heard.lock().unwrap().clone();
    assert_eq!(
        heard,
        vec![
            Command::Write {
                chat: chat.into(),
                bytes: b"ls\r\x1b[<0;3;4M\xff".to_vec(),
            },
            Command::Resize {
                chat: chat.into(),
                cols: 120,
                rows: 40,
            },
            Command::Stop { chat: chat.into() },
            Command::Start(Start {
                project: "01J9PRJCT00000000000000000".into(),
                workspace: Some("login".into()),
                harness: Some("claude".into()),
            }),
        ]
    );
}

#[tokio::test]
async fn an_answer_from_terminal_is_refused_and_never_reaches_the_host() {
    // V16, V75: only `local-ui` and `approval` answer an ask; `terminal` is refused before the
    // host hears of it (FD-27's table, `grants`).
    let board = Board::default();
    let client = linked(board.clone()).await;

    let refused = client
        .answer("01J9ZQ3V7K8M2N4P6R8T0V2X4Z", "ask-1", "yes")
        .await
        .unwrap_err();

    assert_eq!(
        refused.refusal().map(|r| r.code.as_str()),
        Some(session::NOT_ALLOWED)
    );
    assert!(board.heard.lock().unwrap().is_empty());
    assert_eq!(client.list().await.unwrap().len(), 1, "the link carries on");
}

#[tokio::test]
async fn a_refusal_comes_back_with_its_code_and_the_link_carries_on() {
    let client = linked(Board::default()).await;
    let refused = client.stop("nobody").await.unwrap_err();
    assert_eq!(
        refused.refusal().map(|r| r.code.as_str()),
        Some(session::NO_SUCH_CHAT)
    );
    assert_eq!(client.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn calls_made_at_once_each_get_their_own_answer() {
    let client = Arc::new(linked(Board::default()).await);
    let mut calls = Vec::new();
    for n in 0..20 {
        let client = Arc::clone(&client);
        calls.push(tokio::spawn(async move {
            if n % 2 == 0 {
                client.list().await.map(|chats| chats.len())
            } else {
                client.stop("nobody").await.map(|()| 99)
            }
        }));
    }
    for (n, call) in calls.into_iter().enumerate() {
        let answered = call.await.unwrap();
        if n % 2 == 0 {
            assert_eq!(answered.unwrap(), 1);
        } else {
            assert!(answered.is_err());
        }
    }
}

#[tokio::test]
async fn a_reply_the_client_cannot_read_fails_its_call_rather_than_hanging() {
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::Terminal,
            HELD.of(Scope::Terminal)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    // A host that answers every call with a reply of a shape this client does not read.
    let mut host = served.unwrap();
    tokio::spawn(async move {
        while let Some(Ok(frame)) = host.control().next().await {
            let call: serde_json::Value = serde_json::from_slice(&frame).unwrap();
            let re = call["call"]["id"].as_u64().unwrap();
            let reply = serde_json::json!({"reply": {"re": re, "ok": {"chats": "not a list"}}});
            host.control()
                .send(bytes::Bytes::from(reply.to_string()))
                .await
                .unwrap();
        }
    });
    let (client, _views, _events) = Client::new(client.unwrap());
    let failed = tokio::time::timeout(std::time::Duration::from_secs(5), client.list())
        .await
        .expect("the call is answered, not left waiting")
        .unwrap_err();
    assert!(
        matches!(failed, session::CallError::Unexpected(_)),
        "{failed}"
    );
}
