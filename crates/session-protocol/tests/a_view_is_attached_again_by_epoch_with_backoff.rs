//! The crate numbers a view's attachments itself: each re-attach is the next epoch, and a
//! client refuses one that is not newer than the last it saw. A client that never draws is
//! re-attached after a growing pause, not in a loop (#817 review).

use std::time::Duration;

use bytes::Bytes;
use charter_session_protocol::auth::Scope;
use charter_session_protocol::link::{self, Link};
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{self, Attacher, Epoch, Limits, ViewId, Viewer};
use tokio::io::{AsyncWriteExt, duplex};
use tokio::time::Instant;

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

fn small() -> Limits {
    Limits {
        most_queued_bytes: 32 * 1024,
        high_watermark: 8 * 1024,
        low_watermark: 2 * 1024,
    }
}

/// Push until the view falls behind.
async fn overrun(feed: &view::Feed) {
    while feed.push(Bytes::from(vec![b'q'; 4096])).is_ok() {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn each_attachment_of_a_view_is_the_next_epoch() {
    let (mut client, host) = linked().await;
    let attacher = Attacher::new(host.opener(), Limits::default());
    let mut viewer = Viewer::default();
    for want in 1..=3 {
        let _feed = attacher
            .attach(ViewId(5), Bytes::from_static(b"s"))
            .await
            .unwrap();
        let reader = viewer.accept(client.accept().await.unwrap()).await.unwrap();
        assert_eq!((reader.view(), reader.epoch()), (ViewId(5), Epoch(want)));
    }
    // Another view counts on its own.
    let _other = attacher.attach(ViewId(6), Bytes::new()).await.unwrap();
    let reader = viewer.accept(client.accept().await.unwrap()).await.unwrap();
    assert_eq!((reader.view(), reader.epoch()), (ViewId(6), Epoch(1)));
}

#[tokio::test]
async fn a_client_refuses_an_epoch_that_is_not_newer_than_the_last() {
    let (mut client, mut host) = linked().await;
    let mut viewer = Viewer::default();
    // A host that is wrong about epochs: two attachments of view 9, both epoch 4.
    for _ in 0..2 {
        let mut raw = host.open().await.unwrap();
        let mut header = vec![view::VIEW];
        header.extend_from_slice(&9u32.to_be_bytes());
        header.extend_from_slice(&4u32.to_be_bytes());
        header.extend_from_slice(&0u32.to_be_bytes());
        raw.write_all(&header).await.unwrap();
        raw.flush().await.unwrap();
        std::mem::forget(raw);
    }
    assert!(viewer.accept(client.accept().await.unwrap()).await.is_ok());
    let stale = viewer.accept(client.accept().await.unwrap()).await;
    assert!(stale.is_err(), "the second epoch 4 of view 9 is refused");
}

#[tokio::test(start_paused = true)]
async fn a_client_that_never_draws_is_attached_again_after_a_growing_pause() {
    let (mut client, host) = linked().await;
    let attacher = Attacher::new(host.opener(), small());
    let mut viewer = Viewer::default();
    let mut waited = Vec::new();
    let mut feed = attacher.attach(ViewId(1), Bytes::new()).await.unwrap();
    let mut _held = vec![viewer.accept(client.accept().await.unwrap()).await.unwrap()];
    for _ in 0..3 {
        overrun(&feed).await;
        let asked = Instant::now();
        feed = attacher.attach(ViewId(1), Bytes::new()).await.unwrap();
        waited.push(asked.elapsed());
        _held.push(viewer.accept(client.accept().await.unwrap()).await.unwrap());
    }
    assert!(waited[0] >= view::FIRST_BACKOFF, "{waited:?}");
    assert!(
        waited[1] >= waited[0] * 2 && waited[2] >= waited[1] * 2,
        "it grows: {waited:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_client_that_drew_something_is_attached_again_at_once() {
    let (mut client, host) = linked().await;
    let attacher = Attacher::new(host.opener(), small());
    let mut viewer = Viewer::default();
    let feed = attacher
        .attach(ViewId(1), Bytes::from_static(b"snapshot"))
        .await
        .unwrap();
    let mut reader = viewer.accept(client.accept().await.unwrap()).await.unwrap();
    let first = reader.next().await.unwrap().unwrap();
    let drawn = match first {
        view::Chunk::Snapshot(b) | view::Chunk::Live(b) => b.len(),
    };
    reader.ack(drawn).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    overrun(&feed).await;
    let asked = Instant::now();
    let _again = attacher.attach(ViewId(1), Bytes::new()).await.unwrap();
    assert!(
        asked.elapsed() < view::FIRST_BACKOFF,
        "{:?}",
        asked.elapsed()
    );
}
