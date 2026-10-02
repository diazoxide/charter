//! A view's queue is bounded in bytes. A view that falls behind its bound is dropped, and the
//! host sends it a fresh snapshot instead of holding memory without limit (ADR 0068 §4, which
//! replaces today's unbounded channel).

use std::time::Duration;

use bytes::Bytes;
use charter_session_protocol::auth::Scope;
use charter_session_protocol::link::{self, Link};
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{self, Attacher, Chunk, Epoch, Limits, ViewId, Viewer};
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

const QUEUE: usize = 64 * 1024;
const HIGH: usize = 16 * 1024;
const CHUNK: usize = 4 * 1024;

fn limits() -> Limits {
    Limits {
        most_queued_bytes: QUEUE,
        high_watermark: HIGH,
        low_watermark: 4 * 1024,
    }
}

/// Push until the view falls behind; return how many bytes were accepted.
fn push_until_behind(feed: &view::Feed) -> usize {
    let mut accepted = 0;
    while feed.push(Bytes::from(vec![b'y'; CHUNK])).is_ok() {
        accepted += CHUNK;
        assert!(
            feed.queued_bytes() <= QUEUE,
            "the queue never holds more than its bound"
        );
        assert!(
            accepted <= 4 * QUEUE,
            "a view that never draws must fall behind"
        );
    }
    accepted
}

#[tokio::test]
async fn a_view_that_never_draws_falls_behind_at_its_bound_and_holds_nothing_after() {
    let (mut client, host) = linked().await;
    let attacher = Attacher::new(host.opener(), limits());
    let feed = attacher.attach(ViewId(3), Bytes::new()).await.unwrap();
    let mut reader = Viewer::default()
        .accept(client.accept().await.unwrap())
        .await
        .unwrap();
    // Let the host write what the watermark allows, so the rest has to queue.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let accepted = push_until_behind(&feed);
    assert!(
        accepted <= QUEUE + HIGH + CHUNK,
        "behind after {accepted} bytes, past its bound"
    );
    assert_eq!(
        feed.queued_bytes(),
        0,
        "a dropped view lets go of what it queued"
    );
    assert!(
        feed.push(Bytes::from_static(b"more")).is_err(),
        "and stays dropped"
    );

    // The client's stream ends; it does not wait forever on a view the host gave up.
    let ended = timeout(Duration::from_secs(5), async {
        loop {
            match reader.next().await {
                None => break,
                Some(Ok(_)) => {}
                Some(Err(e)) => panic!("{e}"),
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the dropped view's stream ends");
}

#[tokio::test]
async fn the_view_comes_back_with_a_fresh_snapshot_and_a_higher_epoch() {
    let (mut client, host) = linked().await;
    let attacher = Attacher::new(host.opener(), limits());
    let mut viewer = Viewer::default();
    let feed = attacher.attach(ViewId(3), Bytes::new()).await.unwrap();
    let _stale = viewer.accept(client.accept().await.unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    push_until_behind(&feed);

    // The host attaches the view again, as a new view does: the snapshot first.
    let fresh = Bytes::from_static(b"\x1b[2J\x1b[Hthe screen now");
    let again = attacher.attach(ViewId(3), fresh.clone()).await.unwrap();
    again.push(Bytes::from_static(b"!")).unwrap();
    let mut reader = viewer.accept(client.accept().await.unwrap()).await.unwrap();
    assert_eq!((reader.view(), reader.epoch()), (ViewId(3), Epoch(2)));
    let mut snapshot = Vec::new();
    while snapshot.len() < fresh.len() {
        match reader.next().await.unwrap().unwrap() {
            Chunk::Snapshot(bytes) => snapshot.extend_from_slice(&bytes),
            Chunk::Live(_) => panic!("live before the snapshot"),
        }
    }
    assert_eq!(snapshot, fresh);
    assert_eq!(
        reader.next().await.unwrap().unwrap(),
        Chunk::Live(Bytes::from_static(b"!"))
    );
}
