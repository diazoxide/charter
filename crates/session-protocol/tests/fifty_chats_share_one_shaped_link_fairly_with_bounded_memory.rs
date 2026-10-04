//! Fifty busy chats share one shaped link, and the memory the views hold is bounded: a
//! needs-you on the control lane gets through fifty terminals' output within 1 s plus the round
//! trip, and the heap stays within what the limits allow. #643's second acceptance line, on the
//! deterministic link simulator; the same over TCP shaped by kernel netem is SC-21 (#828).
//!
//! **The link** is `common::netsim` in virtual time: 150 ms round trip, 2% loss per 1,460-byte
//! packet as fast retransmit, 10 MB/s, 2 MiB between the sender's write and the receiver's read,
//! no congestion control. Time moves only when every task waits, so the needs-you's delay is
//! the link's and the protocol's, never the machine's, and the budget is asserted (ADR 0086
//! keeps only wall-clock budgets out of `cargo test`). The ends' own work takes no time here.
//!
//! **The memory bound** is the heap of this test process, measured with dhat, both ends of the
//! link included.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use charter_session_protocol::auth::Scope;
use charter_session_protocol::link;
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{Attacher, Chunk, Limits, ViewId, Viewer};
use tokio::time::{Instant, sleep};

mod common;
use common::HELD;
use common::netsim::{self, Shape};

const CHATS: u32 = 50;
const ROUND_TRIP: Duration = Duration::from_millis(150);
/// #643's budget: a needs-you within 1 s plus the round trip.
const BUDGET: Duration = Duration::from_millis(1000 + 150);

const SHAPE: Shape = Shape {
    one_way: Duration::from_millis(75),
    bytes_per_second: 10e6,
    buffer: 2 << 20,
    loss_one_in: 50, // 2%
    packet: 1460,
    retransmit: ROUND_TRIP,
};

/// A terminal's output is new bytes every time, not one buffer shared by reference, so what
/// the views hold is what the heap holds.
fn fresh(output: &Bytes) -> Bytes {
    Bytes::from(output.to_vec())
}

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

#[tokio::test(start_paused = true)]
async fn every_needs_you_gets_through_fifty_busy_terminals_and_the_heap_stays_bounded() {
    let _profiler = dhat::Profiler::builder().testing().build();
    let (client_end, host_end) = netsim::link(SHAPE, 1);
    let (client, host) = tokio::join!(
        link::connect(client_end, v1(), Scope::LocalUi, HELD.of(Scope::LocalUi)),
        link::serve_any(host_end, v1(), &HELD)
    );
    let (mut client, mut host) = (client.unwrap(), host.unwrap());
    let limits = Limits::default();
    // What the limits allow, both ends of the link in this process: per chat, the host's
    // queue and its watermark's worth in flight, and the client's Yamux receive window; plus
    // the simulated link's 2 MiB each way, and 16 MiB for everything else.
    let per_chat =
        limits.most_queued_bytes + limits.high_watermark + yamux::DEFAULT_CREDIT as usize;
    let bound = CHATS as usize * per_chat + (4 << 20) + (16 << 20);
    // Watch the heap while the terminals run, and stop them the moment it passes the bound,
    // so a view that holds without limit fails here instead of filling the machine.
    let over = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));

    // The host: fifty busy terminals, each re-attached with a fresh snapshot when it falls
    // behind, and a needs-you for each chat in turn on the control lane.
    let attacher = Arc::new(Attacher::new(host.opener(), limits));
    for chat in 0..CHATS {
        let attacher = Arc::clone(&attacher);
        let stop = Arc::clone(&stop);
        tokio::spawn(async move {
            let output =
                Bytes::from(format!("chat {chat} working… {}\r\n", "▒".repeat(60)).repeat(80));
            while !stop.load(Ordering::Relaxed) {
                let snapshot = Bytes::from(format!("\x1b[2J\x1b[Hchat {chat}"));
                let Ok(feed) = attacher.attach(ViewId(chat), snapshot).await else {
                    return;
                };
                while !stop.load(Ordering::Relaxed) && feed.push(fresh(&output)).is_ok() {
                    sleep(Duration::from_millis(5)).await;
                }
            }
        });
    }

    // The client: draws every view it is given, as fast as it reads.
    let accepting_stop = Arc::clone(&stop);
    let mut accept = client.acceptor();
    tokio::spawn(async move {
        while !accepting_stop.load(Ordering::Relaxed) {
            let Ok(stream) = accept.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let Ok(mut reader) = Viewer::default().accept(stream).await else {
                    return;
                };
                while let Some(Ok(Chunk::Live(bytes) | Chunk::Snapshot(bytes))) =
                    reader.next().await
                {
                    if reader.ack(bytes.len()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });

    let (watch_stop, watch_over) = (Arc::clone(&stop), Arc::clone(&over));
    tokio::spawn(async move {
        while !watch_stop.load(Ordering::Relaxed) {
            if dhat::HeapStats::get().curr_bytes > bound {
                watch_over.store(true, Ordering::Relaxed);
                watch_stop.store(true, Ordering::Relaxed);
            }
            sleep(Duration::from_millis(20)).await;
        }
    });

    // Let every terminal get going, then ask for the operator once per chat.
    sleep(Duration::from_secs(1)).await;
    let sent: Arc<Mutex<HashMap<u32, Instant>>> = Arc::default();
    let asking = Arc::clone(&sent);
    let asks = tokio::spawn(async move {
        for chat in 0..CHATS {
            asking.lock().unwrap().insert(chat, Instant::now());
            host.control()
                .send(Bytes::from(format!("needs-you {chat}")))
                .await
                .unwrap();
            sleep(Duration::from_millis(60)).await;
        }
        host
    });
    let mut took = Vec::new();
    while took.len() < CHATS as usize && !over.load(Ordering::Relaxed) {
        let frame = client.control().next().await.unwrap().unwrap();
        let text = String::from_utf8(frame.to_vec()).unwrap();
        let chat: u32 = text.strip_prefix("needs-you ").unwrap().parse().unwrap();
        let at = sent.lock().unwrap()[&chat];
        took.push(at.elapsed());
    }
    stop.store(true, Ordering::Relaxed);
    let _host = asks.await.unwrap();

    took.sort();
    let worst = took.last().copied().unwrap_or_default();
    let peak = dhat::HeapStats::get().max_bytes;
    eprintln!(
        "needs-you through {CHATS} busy chats on the shaped link: p50 {:?}, worst {worst:?}; heap peak {peak} bytes of {bound} allowed",
        took.get(took.len() / 2).copied().unwrap_or_default()
    );
    assert!(
        !over.load(Ordering::Relaxed),
        "the heap passed {bound} bytes while the terminals ran"
    );
    assert_eq!(took.len(), CHATS as usize, "every needs-you arrived");
    assert!(
        worst <= BUDGET,
        "the slowest needs-you took {worst:?}, over {BUDGET:?}"
    );
    assert!(
        peak <= bound,
        "the heap peaked at {peak} bytes, over {bound}"
    );
}
