//! The control lane is a priority lane: a frame on it goes out ahead of every terminal byte
//! still waiting to be sent, however many streams are busy (ADR 0068 §4; FD-4, #643).
//!
//! Without it, the multiplexer takes one frame from each busy stream in turn, so a control
//! frame waits behind a frame of every one of them: fifty streams of 16 KiB frames on a
//! 1 MB/s link, a connector's stdio to a remote host, is about 800 ms. With it, the frame waits
//! only for what the link already holds and the one frame being written.
//!
//! The link is the deterministic simulator in virtual time (`common::netsim`), so the bound is
//! asserted: the delay is the model's, not the machine's.

use std::time::Duration;

use bytes::Bytes;
use purlis_session_protocol::auth::Scope;
use purlis_session_protocol::link;
use purlis_session_protocol::version::{Speaks, Version};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Instant, sleep};

mod common;
use common::HELD;
use common::netsim::{self, END_PIPE, Shape};

const STREAMS: usize = 50;
const SAMPLES: usize = 20;
/// The largest frame the multiplexer sends, with its header.
const FRAME: usize = 16 * 1024 + 12;

const SHAPE: Shape = Shape {
    one_way: Duration::from_millis(20),
    bytes_per_second: 1e6,
    // A pipe's worth, as a connector's stdio holds.
    buffer: 64 * 1024,
    loss_one_in: 0,
    packet: 1460,
    retransmit: Duration::from_millis(40),
};

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test(start_paused = true)]
async fn a_control_frame_waits_only_for_what_the_link_already_holds() {
    let (client_end, host_end) = netsim::link(SHAPE, 7);
    let (client, host) = tokio::join!(
        link::connect(client_end, v1(), Scope::LocalUi, HELD.of(Scope::LocalUi)),
        link::serve_any(host_end, v1(), &HELD)
    );
    let (mut client, mut host) = (client.unwrap(), host.unwrap());

    // Fifty streams the host writes to as fast as the multiplexer lets it.
    for n in 0..STREAMS {
        let mut stream = host.open().await.unwrap();
        tokio::spawn(async move {
            let chunk = vec![n as u8; 64 * 1024];
            while stream.write_all(&chunk).await.is_ok() {}
        });
    }
    // The client reads every one of them as fast as it arrives.
    let mut accept = client.acceptor();
    tokio::spawn(async move {
        while let Ok(mut stream) = accept.accept().await {
            tokio::spawn(async move {
                let mut sink = vec![0u8; 64 * 1024];
                while matches!(stream.read(&mut sink).await, Ok(n) if n > 0) {}
            });
        }
    });
    sleep(Duration::from_secs(2)).await;

    let mut took = Vec::with_capacity(SAMPLES);
    for n in 0..SAMPLES {
        let sent = Instant::now();
        host.control()
            .send(Bytes::from(format!("needs-you {n}")))
            .await
            .unwrap();
        let frame = client.control().next().await.unwrap().unwrap();
        assert_eq!(frame, Bytes::from(format!("needs-you {n}")));
        took.push(sent.elapsed());
        sleep(Duration::from_millis(100)).await;
    }
    took.sort();
    let worst = took[SAMPLES - 1];
    // What the link holds, each end's pipe, and two frames: the one being written and the
    // control frame itself, at the link's rate; then the one-way delay; then a millisecond.
    let ahead = SHAPE.buffer + 2 * END_PIPE + 2 * FRAME;
    let bound = Duration::from_secs_f64(ahead as f64 / SHAPE.bytes_per_second)
        + SHAPE.one_way
        + Duration::from_millis(1);
    eprintln!(
        "control frame through {STREAMS} busy streams at 1 MB/s: p50 {:?}, worst {worst:?}, bound {bound:?}",
        took[SAMPLES / 2]
    );
    assert!(
        worst <= bound,
        "a control frame took {worst:?}, over the {bound:?} the link alone accounts for"
    );
}
