//! What each client scope may call, checked once inside `session::serve` before any `Host` is
//! asked (FD-27, ADR 0068 §5 as amended by FD-27). A command the scope's row does not grant is
//! refused `not_allowed` and never reaches the host, and the link carries on.

use std::sync::{Arc, Mutex};

use purlis_session_protocol::auth::Scope;
use purlis_session_protocol::link;
use purlis_session_protocol::session::{
    self, Answer, Chat, Client, Command, Host, Peer, Refusal, Start,
};
use tokio::io::duplex;

mod common;
use common::HELD;

const CHAT: &str = "01J9ZQ3V7K8M2N4P6R8T0V2X4Z";
const ASK: &str = "ask-1";

/// A host that answers every command, keeps each it was sent, and says whether an ask elicits a
/// secret as it is told.
#[derive(Clone)]
struct Board {
    heard: Arc<Mutex<Vec<String>>>,
    secret: bool,
}

impl Board {
    fn new(secret: bool) -> Board {
        Board {
            heard: Arc::default(),
            secret,
        }
    }

    fn heard(&self) -> Vec<String> {
        self.heard.lock().unwrap().clone()
    }
}

impl Host for Board {
    async fn call(&self, command: Command, _peer: &Peer) -> Result<Answer, Refusal> {
        self.heard.lock().unwrap().push(command.word().to_owned());
        Ok(match command {
            Command::List => Answer::Chats(vec![Chat {
                chat: CHAT.into(),
                project: "01J9PRJCT00000000000000000".into(),
                project_path: None,
                title: None,
                state: "needs_you".into(),
            }]),
            Command::Start(_) => Answer::Started { chat: CHAT.into() },
            Command::Attach { .. } => Answer::Attached(session::Attached {
                view: 1,
                cols: 80,
                rows: 24,
            }),
            _ => Answer::Done,
        })
    }

    async fn subscribe(
        &self,
        _since: session::Since,
        _peer: &Peer,
    ) -> Result<tokio::sync::mpsc::Receiver<session::Pushed>, Refusal> {
        self.heard.lock().unwrap().push("subscribe".into());
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(rx)
    }

    async fn elicits_a_secret(&self, _chat: &str, _ask: &str) -> bool {
        self.secret
    }
}

/// A link admitted as `scope`, with `host` behind it.
async fn linked(scope: Scope, host: Board) -> Client {
    let (a, b) = duplex(256 * 1024);
    let (client, served) = if scope == Scope::RemoteLink {
        tokio::join!(
            link::connect_as_a_device(a, session::speaks()),
            link::serve_as_a_device(b, session::speaks(), link::ProvenDevice::stand_in())
        )
    } else {
        tokio::join!(
            link::connect(a, session::speaks(), scope, HELD.of(scope)),
            link::serve_any(b, session::speaks(), &HELD)
        )
    };
    let served = served.unwrap();
    assert_eq!(served.scope(), scope);
    tokio::spawn(session::serve(served, host, None));
    let (client, _views, _events) = Client::new(client.unwrap());
    client
}

/// Every command, once, in the table's order.
fn every_command() -> Vec<Command> {
    vec![
        Command::List,
        Command::Attach { chat: CHAT.into() },
        Command::Detach {
            chat: CHAT.into(),
            view: 1,
        },
        Command::Write {
            chat: CHAT.into(),
            bytes: b"y".to_vec(),
        },
        Command::Resize {
            chat: CHAT.into(),
            cols: 100,
            rows: 30,
        },
        Command::Answer {
            chat: CHAT.into(),
            ask: ASK.into(),
            answer: "yes".into(),
        },
        Command::Stop { chat: CHAT.into() },
        Command::Start(Start {
            project: "01J9PRJCT00000000000000000".into(),
            workspace: None,
            harness: None,
        }),
        Command::Subscribe {
            since: session::Since::new(),
        },
        Command::Unsubscribe,
    ]
}

/// Which of [`every_command`] `scope` is granted, by the table in ADR 0068 as amended by FD-27,
/// written out here by hand as the spec, not computed from the code.
fn granted(scope: Scope) -> &'static [&'static str] {
    match scope {
        Scope::LocalUi => &[
            "list",
            "attach",
            "detach",
            "write",
            "resize",
            "answer",
            "stop",
            "start",
            "subscribe",
            "unsubscribe",
        ],
        Scope::Terminal | Scope::RemoteLink => &[
            "list",
            "attach",
            "detach",
            "write",
            "resize",
            "stop",
            "start",
            "subscribe",
            "unsubscribe",
        ],
        Scope::FleetMcp => &["list", "stop", "subscribe", "unsubscribe"],
        Scope::Approval => &["list", "answer", "stop", "subscribe", "unsubscribe"],
        Scope::Editor => &[],
    }
}

async fn check_the_row(scope: Scope) {
    let board = Board::new(false);
    let client = linked(scope, board.clone()).await;
    let mut reached = Vec::new();
    for command in every_command() {
        let word = command.word();
        match client.call(command).await {
            Ok(_) => reached.push(word),
            Err(refused) => assert_eq!(
                refused.refusal().map(|r| r.code.as_str()),
                Some(session::NOT_ALLOWED),
                "{scope} {word}: {refused}"
            ),
        }
    }
    assert_eq!(reached, granted(scope), "{scope}: what it was let call");
    // The refused ones never reached the host; `unsubscribe` is the link's own.
    let to_the_host: Vec<&str> = granted(scope)
        .iter()
        .copied()
        .filter(|word| *word != "unsubscribe")
        .collect();
    assert_eq!(board.heard(), to_the_host, "{scope}: what the host heard");
}

#[tokio::test]
async fn local_ui_may_call_every_command() {
    check_the_row(Scope::LocalUi).await;
}

#[tokio::test]
async fn terminal_may_drive_a_chat_but_never_answer_its_asks() {
    check_the_row(Scope::Terminal).await;
}

#[tokio::test]
async fn fleet_mcp_may_list_watch_and_stop_and_nothing_else() {
    check_the_row(Scope::FleetMcp).await;
}

#[tokio::test]
async fn approval_may_answer_list_watch_and_stop_and_never_drive_a_chat() {
    check_the_row(Scope::Approval).await;
}

#[tokio::test]
async fn editor_may_call_none_of_the_session_commands() {
    check_the_row(Scope::Editor).await;
}

#[tokio::test]
async fn remote_link_may_drive_a_chat_but_never_answer_its_asks() {
    check_the_row(Scope::RemoteLink).await;
}

#[tokio::test]
async fn an_ask_that_elicits_a_secret_is_answered_from_local_ui_only() {
    for (scope, answers) in [(Scope::LocalUi, true), (Scope::Approval, false)] {
        let board = Board::new(true);
        let client = linked(scope, board.clone()).await;
        let answered = client.answer(CHAT, ASK, "yes").await;
        assert_eq!(answered.is_ok(), answers, "{scope}: {answered:?}");
        if !answers {
            assert_eq!(
                answered.unwrap_err().refusal().map(|r| r.code.clone()),
                Some(session::NOT_ALLOWED.to_owned())
            );
            assert!(board.heard().is_empty(), "the host never heard it");
        }
    }
}

/// A host that says nothing about its asks is taken to hold secret ones: `approval` answers
/// only when the host says the ask elicits no secret.
#[derive(Clone, Default)]
struct Silent;

impl Host for Silent {
    async fn call(&self, _command: Command, _peer: &Peer) -> Result<Answer, Refusal> {
        Ok(Answer::Done)
    }
}

#[tokio::test]
async fn a_host_that_says_nothing_of_an_ask_has_it_answered_from_local_ui_only() {
    let (a, b) = duplex(64 * 1024);
    let (client, served) = tokio::join!(
        link::connect(
            a,
            session::speaks(),
            Scope::Approval,
            HELD.of(Scope::Approval)
        ),
        link::serve_any(b, session::speaks(), &HELD)
    );
    tokio::spawn(session::serve(served.unwrap(), Silent, None));
    let (client, _views, _events) = Client::new(client.unwrap());
    let refused = client.answer(CHAT, ASK, "yes").await.unwrap_err();
    assert_eq!(
        refused.refusal().map(|r| r.code.as_str()),
        Some(session::NOT_ALLOWED)
    );
}
