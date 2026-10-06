//! A stub `remote-link` client, in a separate process over its own stdio as a connector gives
//! it, speaks the session protocol to a host (V7, ADR 0068 §4, FD-27): it lists the chats,
//! attaches one and reads its snapshot, and stops it. What its scope is not granted is refused
//! before the host hears of it: answering an ask (V75), and the UI RPC, where a vault's reveal
//! and a settings write are.
//!
//! The host admits the stub as a proved device through the crate's stand-in for ADR 0078 §3's
//! Noise handshake, which RR-13 builds.

use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::future::BoxFuture;
use purlis_session_protocol::link::{self, ProvenDevice};
use purlis_session_protocol::session::{
    self, Answer, Attached, Chat, Command, Host, Peer, Refusal,
};
use purlis_session_protocol::ui::{self, Handler};
use purlis_session_protocol::view::{Attacher, Feed, Limits, ViewId};
use serde_json::Value;
use tokio::io::{AsyncReadExt, join};
use tokio::process::Command as Process;

const CHAT: &str = "01J9ZQ3V7K8M2N4P6R8T0V2X4Z";
const SNAPSHOT: &[u8] = b"\x1b[2J\x1b[Hfix the login$ ";
const BUILD: &str = "0.4.0+9055f7a";

/// A host with one chat, which keeps every command it was sent and every view it opened.
#[derive(Clone, Default)]
struct OneChat {
    heard: Arc<Mutex<Vec<String>>>,
    feeds: Arc<Mutex<Vec<Feed>>>,
}

impl Host for OneChat {
    async fn call(&self, command: Command, peer: &Peer) -> Result<Answer, Refusal> {
        self.heard.lock().unwrap().push(command.word().to_owned());
        match command {
            Command::List => Ok(Answer::Chats(vec![Chat {
                chat: CHAT.into(),
                project: "01J9PRJCT00000000000000000".into(),
                project_path: None,
                title: Some("fix the login".into()),
                state: "needs_you".into(),
            }])),
            Command::Attach { .. } => {
                let feed = Attacher::new(peer.views().clone(), Limits::default())
                    .attach(ViewId(1), Bytes::from_static(SNAPSHOT))
                    .await
                    .map_err(|e| Refusal {
                        code: "view".into(),
                        why: e.to_string(),
                    })?;
                feed.push(Bytes::from_static(b"!")).unwrap();
                self.feeds.lock().unwrap().push(feed);
                Ok(Answer::Attached(Attached {
                    view: 1,
                    cols: 80,
                    rows: 24,
                }))
            }
            _ => Ok(Answer::Done),
        }
    }

    async fn elicits_a_secret(&self, _chat: &str, _ask: &str) -> bool {
        false
    }
}

/// The app's commands, kept as they are called: none should be.
#[derive(Clone, Default)]
struct AppCommands {
    called: Arc<Mutex<Vec<String>>>,
}

impl Handler for AppCommands {
    fn call(&self, method: String, _args: Value) -> BoxFuture<'_, Result<Value, Value>> {
        self.called.lock().unwrap().push(method);
        Box::pin(async { Ok(Value::String("the secret's value".into())) })
    }
}

#[tokio::test]
async fn a_stub_remote_link_lists_snapshots_and_stops_and_is_refused_what_it_is_not_granted() {
    let mut stub = Process::new(env!("CARGO_BIN_EXE_charter-session-peer"))
        .args(["--remote-link", CHAT, BUILD])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let io = join(stub.stdout.take().unwrap(), stub.stdin.take().unwrap());
    let mut said = stub.stderr.take().unwrap();

    let link = link::serve_as_a_device(io, session::speaks(), ProvenDevice::stand_in())
        .await
        .unwrap();
    let host = OneChat::default();
    let app = AppCommands::default();
    let ui = ui::Server::new(
        BUILD,
        ["vault_secret_reveal", "save_project_settings"],
        app.clone(),
    );
    let serving = tokio::spawn(session::serve(link, host.clone(), Some(ui)));

    let waited = tokio::time::timeout(std::time::Duration::from_secs(30), stub.wait()).await;
    if waited.is_err() {
        stub.kill().await.unwrap();
    }
    let mut transcript = String::new();
    said.read_to_string(&mut transcript).await.unwrap();
    let status = waited
        .unwrap_or_else(|_| panic!("the stub did not finish: {transcript}"))
        .unwrap();
    assert!(status.success(), "{transcript}");
    serving.await.unwrap().unwrap();

    let lines: Vec<&str> = transcript.lines().collect();
    assert_eq!(
        lines,
        [
            format!("listed {CHAT} needs_you").as_str(),
            format!("snapshot {}", String::from_utf8_lossy(SNAPSHOT)).as_str(),
            "live !",
            "answer refused not_allowed",
            "vault_secret_reveal refused not_allowed",
            "save_project_settings refused not_allowed",
            "stopped",
        ],
        "{transcript}"
    );
    assert_eq!(
        host.heard.lock().unwrap().clone(),
        ["list", "attach", "stop"],
        "the host never heard the answer"
    );
    assert!(
        app.called.lock().unwrap().is_empty(),
        "no app command was called"
    );
}
