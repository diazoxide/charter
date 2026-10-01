//! xterm.js's flow control, across the link: the host stops writing a view once it is a high
//! watermark ahead of what the client says it has drawn, and starts again when the client
//! catches up to the low one. A client that draws slowly holds back its own view, and nothing
//! else (#643).

use std::time::Duration;

use bytes::Bytes;
use charter_session_protocol::link::{self, Link};
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{self, Attacher, Chunk, Limits, ViewId, Viewer};
use tokio::io::duplex;
use tokio::time::timeout;

async fn linked() -> (Link, Link) {
    let speaks = Speaks::new([Version { major: 1, minor: 0 }]);
    let (a, b) = duplex(256 * 1024);
    let (client, host) = tokio::join!(link::connect(a, speaks.clone()), link::serve(b, speaks));
    (client.unwrap(), host.unwrap())
}

const HIGH: usize = 64 * 1024;
const LOW: usize = 16 * 1024;
const CHUNK: usize = 4 * 1024;
const TOTAL: usize = 512 * 1024;

fn limits() -> Limits {
    Limits {
        most_queued_bytes: 8 << 20,
        high_watermark: HIGH,
        low_watermark: LOW,
    }
}

/// Read without acknowledging until nothing more comes for a while; return how much came.
async fn read_until_quiet(reader: &mut view::Reader) -> usize {
    let mut got = 0;
    while let Ok(Some(chunk)) = timeout(Duration::from_millis(300), reader.next()).await {
        match chunk.unwrap() {
            Chunk::Live(bytes) | Chunk::Snapshot(bytes) => got += bytes.len(),
        }
    }
    got
}

#[tokio::test]
async fn the_host_stops_at_the_high_watermark_while_nothing_is_acknowledged() {
    let (mut client, host) = linked().await;
    let feed = Attacher::new(host.opener(), limits())
        .attach(ViewId(1), Bytes::new())
        .await
        .unwrap();
    for _ in 0..TOTAL / CHUNK {
        feed.push(Bytes::from(vec![b'x'; CHUNK])).unwrap();
    }
    let mut reader = Viewer::default()
        .accept(client.accept().await.unwrap())
        .await
        .unwrap();
    let got = read_until_quiet(&mut reader).await;
    assert!(
        got >= HIGH - CHUNK,
        "it wrote up to the watermark ({got} bytes)"
    );
    assert!(got <= HIGH + CHUNK, "and stopped there, not at {got} bytes");
}

#[tokio::test]
async fn acknowledging_down_to_the_low_watermark_lets_it_write_again() {
    let (mut client, host) = linked().await;
    let feed = Attacher::new(host.opener(), limits())
        .attach(ViewId(1), Bytes::new())
        .await
        .unwrap();
    for _ in 0..TOTAL / CHUNK {
        feed.push(Bytes::from(vec![b'x'; CHUNK])).unwrap();
    }
    let mut reader = Viewer::default()
        .accept(client.accept().await.unwrap())
        .await
        .unwrap();
    let first = read_until_quiet(&mut reader).await;

    // Drawing less than takes it below the low watermark is not enough.
    reader.ack(first - LOW - 1).await.unwrap();
    assert_eq!(
        read_until_quiet(&mut reader).await,
        0,
        "still above the low watermark"
    );

    // Drawing the rest is.
    reader.ack(LOW + 1).await.unwrap();
    assert!(
        read_until_quiet(&mut reader).await > 0,
        "below the low watermark it writes again"
    );
}

#[tokio::test]
async fn a_client_that_draws_as_it_reads_gets_every_byte_in_order() {
    let (mut client, host) = linked().await;
    let feed = Attacher::new(host.opener(), limits())
        .attach(ViewId(1), Bytes::new())
        .await
        .unwrap();
    let mut sent = Vec::with_capacity(TOTAL);
    for n in 0..TOTAL / CHUNK {
        let chunk = vec![(n % 251) as u8; CHUNK];
        sent.extend_from_slice(&chunk);
        feed.push(Bytes::from(chunk)).unwrap();
    }
    let mut reader = Viewer::default()
        .accept(client.accept().await.unwrap())
        .await
        .unwrap();
    let mut got = Vec::with_capacity(TOTAL);
    while got.len() < TOTAL {
        let chunk = timeout(Duration::from_secs(10), reader.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Chunk::Live(bytes) = chunk else {
            panic!("a snapshot after live bytes")
        };
        reader.ack(bytes.len()).await.unwrap();
        got.extend_from_slice(&bytes);
    }
    assert!(got == sent, "every byte, in order");
}
