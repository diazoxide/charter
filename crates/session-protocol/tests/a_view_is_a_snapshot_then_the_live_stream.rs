//! A view of a terminal is one stream: a header, the snapshot, then every byte after it, raw
//! (ADR 0068 §4). The client knows where the snapshot ends, so it can reset its terminal for a
//! fresh one and never mistake the two.

use bytes::Bytes;
use charter_session_protocol::auth::Scope;
use charter_session_protocol::link::{self, Link};
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{self, Attacher, Chunk, Epoch, Limits, ViewId, Viewer};
use tokio::io::duplex;

mod common;
use common::HELD;

pub async fn linked() -> (Link, Link) {
    let speaks = Speaks::new([Version { major: 1, minor: 0 }]);
    let (a, b) = duplex(256 * 1024);
    let (client, host) = tokio::join!(
        link::connect(a, speaks.clone(), Scope::LocalUi, HELD.of(Scope::LocalUi)),
        link::serve_any(b, speaks, &HELD)
    );
    (client.unwrap(), host.unwrap())
}

/// Everything the view delivers until `want` live bytes have arrived, acking as it goes.
async fn read_live(reader: &mut view::Reader, want: usize) -> Vec<u8> {
    let mut live = Vec::new();
    while live.len() < want {
        match reader.next().await.unwrap().unwrap() {
            Chunk::Live(bytes) => {
                reader.ack(bytes.len()).await.unwrap();
                live.extend_from_slice(&bytes);
            }
            Chunk::Snapshot(_) => panic!("a snapshot after live bytes"),
        }
    }
    live
}

#[tokio::test]
async fn the_snapshot_comes_first_whole_then_the_live_bytes_in_order() {
    let (mut client, host) = linked().await;
    let snapshot = Bytes::from_static(b"\x1b[2J\x1b[Hprompt$ ");
    let feed = Attacher::new(host.opener(), Limits::default())
        .attach(ViewId(7), snapshot.clone())
        .await
        .unwrap();
    let typed: &[u8] = b"ls\r\n";
    let listed: &[u8] = b"\x1b[1mREADME.md\x1b[0m\r\n";
    feed.push(Bytes::from_static(typed)).unwrap();
    feed.push(Bytes::from_static(listed)).unwrap();

    let mut reader = Viewer::default()
        .accept(client.accept().await.unwrap())
        .await
        .unwrap();
    assert_eq!((reader.view(), reader.epoch()), (ViewId(7), Epoch(1)));
    let mut got = Vec::new();
    while got.len() < snapshot.len() {
        match reader.next().await.unwrap().unwrap() {
            Chunk::Snapshot(bytes) => {
                reader.ack(bytes.len()).await.unwrap();
                got.extend_from_slice(&bytes);
            }
            Chunk::Live(_) => panic!("live bytes before the snapshot was whole"),
        }
    }
    assert_eq!(got, snapshot);
    let live = read_live(&mut reader, typed.len() + listed.len()).await;
    assert_eq!(live, [typed, listed].concat());
}

#[tokio::test]
async fn an_empty_snapshot_is_still_a_snapshot_boundary() {
    let (mut client, host) = linked().await;
    let feed = Attacher::new(host.opener(), Limits::default())
        .attach(ViewId(1), Bytes::new())
        .await
        .unwrap();
    feed.push(Bytes::from_static(b"hi")).unwrap();
    let mut reader = Viewer::default()
        .accept(client.accept().await.unwrap())
        .await
        .unwrap();
    assert_eq!(read_live(&mut reader, 2).await, b"hi");
}

#[tokio::test]
async fn many_views_share_one_link_and_each_keeps_its_own_order() {
    let (mut client, host) = linked().await;
    let mut feeds = Vec::new();
    for v in 0..20u32 {
        let feed = Attacher::new(host.opener(), Limits::default())
            .attach(ViewId(v), Bytes::new())
            .await
            .unwrap();
        for n in 0..50u32 {
            feed.push(Bytes::from(format!("{v}:{n};"))).unwrap();
        }
        feeds.push(feed);
    }
    for _ in 0..20 {
        let mut reader = Viewer::default()
            .accept(client.accept().await.unwrap())
            .await
            .unwrap();
        let v = reader.view().0;
        let want: String = (0..50).map(|n| format!("{v}:{n};")).collect();
        assert_eq!(read_live(&mut reader, want.len()).await, want.as_bytes());
    }
}
