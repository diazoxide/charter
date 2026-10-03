//! `subscribe(since)` over the session protocol (ADR 0066): the host pushes every event after
//! the client's cursor, then each new one, and word of any it no longer keeps. And an attach
//! answers with the view it opened beside the control lane (ADR 0068 §4).

use std::sync::{Arc, Mutex};

use bytes::Bytes;
use charter_session_protocol::auth::Scope;
use charter_session_protocol::link;
use charter_session_protocol::link::Acceptor;
use charter_session_protocol::session::{
    self, Answer, Attached, Client, Command, Event, Events, Host, Peer, Pushed, Refusal, Since,
};
use charter_session_protocol::view::{Attacher, Chunk, Feed, Limits, ViewId, Viewer};
use tokio::io::duplex;
use tokio::sync::mpsc;

mod common;
use common::HELD;

fn event(seq: u64, kind: &str) -> Event {
    Event {
        v: 1,
        device_id: "01J9DEVCE00000000000000000".into(),
        seq,
        ulid: format!("01J9EVENT{seq:017}"),
        chat: Some("01J9ZQ3V7K8M2N4P6R8T0V2X4Z".into()),
        run: None,
        parent_run: None,
        kind: kind.into(),
        body: serde_json::json!({}),
    }
}

/// A host whose event log is a channel the test writes, and whose chats each attach to a view
/// drawing `$ `.
#[derive(Default, Clone)]
struct Logged {
    since: Arc<Mutex<Option<Since>>>,
    pushes: Arc<Mutex<Option<mpsc::Sender<Pushed>>>>,
    feeds: Arc<Mutex<Vec<Feed>>>,
}

impl Host for Logged {
    async fn call(&self, command: Command, peer: &Peer) -> Result<Answer, Refusal> {
        match command {
            Command::Attach { chat } if chat == "01J9ZQ3V7K8M2N4P6R8T0V2X4Z" => {
                let feed = Attacher::new(peer.views().clone(), Limits::default())
                    .attach(ViewId(3), Bytes::from_static(b"$ "))
                    .await
                    .map_err(|e| Refusal {
                        code: "failed".into(),
                        why: e.to_string(),
                    })?;
                feed.push(Bytes::from_static(b"ls\r\n")).unwrap();
                self.feeds.lock().unwrap().push(feed);
                Ok(Answer::Attached(Attached {
                    view: 3,
                    cols: 80,
                    rows: 24,
                }))
            }
            _ => Err(Refusal {
                code: session::NO_SUCH_CHAT.into(),
                why: "no such chat".into(),
            }),
        }
    }

    async fn subscribe(
        &self,
        since: Since,
        _peer: &Peer,
    ) -> Result<mpsc::Receiver<Pushed>, Refusal> {
        *self.since.lock().unwrap() = Some(since);
        let (pushes, events) = mpsc::channel(16);
        *self.pushes.lock().unwrap() = Some(pushes);
        Ok(events)
    }
}

/// A host that keeps no events at all.
struct Eventless;

impl Host for Eventless {
    async fn call(&self, _command: Command, _peer: &Peer) -> Result<Answer, Refusal> {
        Ok(Answer::Done)
    }
}

async fn linked<H: Host>(host: H) -> (Client, Acceptor, Events) {
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
    Client::new(client.unwrap())
}

#[tokio::test]
async fn a_subscriber_gets_what_the_host_pushes_after_its_cursor_in_order() {
    let host = Logged::default();
    let (client, _views, mut events) = linked(host.clone()).await;
    let since = Since::from([("01J9DEVCE00000000000000000".to_owned(), 2)]);
    client.subscribe(since.clone()).await.unwrap();
    assert_eq!(host.since.lock().unwrap().clone(), Some(since));

    let pushes = host.pushes.lock().unwrap().clone().unwrap();
    let missed = Pushed::Missed {
        device: "01J9DEVCE00000000000000000".into(),
        after: 2,
        resumes_at: 5,
    };
    pushes.send(missed.clone()).await.unwrap();
    pushes
        .send(Pushed::Event(event(5, "run.started")))
        .await
        .unwrap();
    pushes
        .send(Pushed::Event(event(6, "hook.stop")))
        .await
        .unwrap();

    assert_eq!(events.recv().await.unwrap(), missed);
    assert_eq!(
        events.recv().await.unwrap(),
        Pushed::Event(event(5, "run.started"))
    );
    assert_eq!(
        events.recv().await.unwrap(),
        Pushed::Event(event(6, "hook.stop"))
    );
}

#[tokio::test]
async fn an_unsubscribe_lets_go_of_the_hosts_subscription() {
    let host = Logged::default();
    let (client, _views, _events) = linked(host.clone()).await;
    client.subscribe(Since::new()).await.unwrap();
    let pushes = host.pushes.lock().unwrap().clone().unwrap();
    client.unsubscribe().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), pushes.closed())
        .await
        .expect("the host's end of the subscription is dropped");
}

#[tokio::test]
async fn a_host_that_keeps_no_events_refuses_a_subscription_and_carries_on() {
    let (client, _views, _events) = linked(Eventless).await;
    let refused = client.subscribe(Since::new()).await.unwrap_err();
    assert_eq!(
        refused.refusal().map(|r| r.code.as_str()),
        Some(session::UNKNOWN_COMMAND)
    );
    client.stop("01J9ZQ3V7K8M2N4P6R8T0V2X4Z").await.unwrap();
}

#[tokio::test]
async fn an_attach_answers_with_the_view_it_opened_beside_the_lane() {
    let (client, mut views, _events) = linked(Logged::default()).await;
    let attached = client.attach("01J9ZQ3V7K8M2N4P6R8T0V2X4Z").await.unwrap();
    assert_eq!(
        attached,
        Attached {
            view: 3,
            cols: 80,
            rows: 24
        }
    );
    let mut reader = Viewer::default()
        .accept(views.accept().await.unwrap())
        .await
        .unwrap();
    assert_eq!(reader.view(), ViewId(attached.view));
    let mut drawn = Vec::new();
    while drawn.len() < b"$ ls\r\n".len() {
        let (Chunk::Snapshot(bytes) | Chunk::Live(bytes)) = reader.next().await.unwrap().unwrap();
        reader.ack(bytes.len()).await.unwrap();
        drawn.extend_from_slice(&bytes);
    }
    assert_eq!(drawn, b"$ ls\r\n");
}
