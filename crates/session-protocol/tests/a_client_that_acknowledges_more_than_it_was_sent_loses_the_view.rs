//! A client acknowledges only what it has drawn, so never more than it was sent. One that
//! acknowledges more would lift the watermark for bytes not yet written, and is refused: the
//! host closes that view (#817 review).

use std::time::Duration;

use bytes::Bytes;
use purlis_session_protocol::auth::Scope;
use purlis_session_protocol::link::{self, Link};
use purlis_session_protocol::version::{Speaks, Version};
use purlis_session_protocol::view::{Attacher, Closed, Limits, ViewId, Viewer};
use tokio::io::duplex;
use tokio::time::timeout;

mod common;
use common::HELD;

async fn linked() -> (Link, Link) {
    let speaks = Speaks::new([Version { major: 1, minor: 0 }]);
    let (a, b) = duplex(256 * 1024);
    let (client, host) = tokio::join!(
        link::connect(a, speaks.clone(), Scope::LocalUi, HELD.of(Scope::LocalUi)),
        link::serve_any(b, speaks, &HELD)
    );
    (client.unwrap(), host.unwrap())
}

#[tokio::test]
async fn acknowledging_bytes_never_sent_closes_the_view() {
    let (mut client, host) = linked().await;
    let feed = Attacher::new(host.opener(), Limits::default())
        .attach(ViewId(1), Bytes::from_static(b"snap"))
        .await
        .unwrap();
    let mut reader = Viewer::default()
        .accept(client.accept().await.unwrap())
        .await
        .unwrap();
    let _ = reader.next().await.unwrap().unwrap();
    reader.ack(1 << 30).await.unwrap();

    let ended = timeout(Duration::from_secs(5), async {
        while let Some(Ok(_)) = reader.next().await {}
    })
    .await;
    assert!(ended.is_ok(), "the host ends the view");
    // The host sees why.
    let pushed = timeout(Duration::from_secs(5), async {
        loop {
            match feed.push(Bytes::from_static(b"more")) {
                Err(closed) => break closed,
                Ok(()) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(pushed, Closed::OverAcknowledged);
}

#[tokio::test]
async fn acknowledging_exactly_what_was_drawn_is_fine() {
    let (mut client, host) = linked().await;
    let feed = Attacher::new(host.opener(), Limits::default())
        .attach(ViewId(1), Bytes::from_static(b"snap"))
        .await
        .unwrap();
    let mut reader = Viewer::default()
        .accept(client.accept().await.unwrap())
        .await
        .unwrap();
    let _ = reader.next().await.unwrap().unwrap();
    reader.ack(4).await.unwrap();
    feed.push(Bytes::from_static(b"live")).unwrap();
    let next = timeout(Duration::from_secs(5), reader.next())
        .await
        .unwrap();
    assert!(matches!(next, Some(Ok(_))), "{next:?}");
    assert_eq!(feed.push(Bytes::from_static(b"!")), Ok(()));
}
