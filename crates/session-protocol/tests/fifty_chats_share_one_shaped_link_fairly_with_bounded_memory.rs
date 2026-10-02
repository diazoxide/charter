//! Fifty busy chats share one shaped link fairly, and the memory the views hold is bounded:
//! a needs-you on the control lane is not starved by fifty terminals' output, and the heap
//! stays within what the limits allow. Evidence for #643's second acceptance line, **not** a
//! measurement of it: that line asks for kernel netem, which this is not (see below).
//!
//! **The link is a model, and a simple one.** Each chunk read from the sender is held for half
//! the round trip (150 ms) each way and serialized at 10 MB/s, with at most 2 MiB in flight,
//! after which the sender's writes wait, as on a real socket. Loss is drawn per 1,460-byte
//! packet at 2%; a chunk with a lost packet waits one more round trip (fast retransmit, RFC
//! 5681), and everything behind it waits for it, since the stream is ordered. There is **no
//! congestion control**: no slow start, no window halving after a loss, so the sender is never
//! slowed the way TCP would slow it. What this checks is that the multiplexer shares the link
//! when the terminals' bytes in flight are bounded, not how the protocol fares under real TCP.
//! The real measurement, over TCP on loopback shaped by `tc netem` in CI, is its own ticket.
//!
//! **No timing is asserted tightly here** (ADR 0086: plain `cargo test` holds no wall-clock
//! budget). The numbers are printed as evidence; the asserted bound, ten seconds for every
//! needs-you, is one a loaded runner cannot reach unless something is wrong. The memory bound
//! is the heap of this test process, measured with dhat, both ends of the link included.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use charter_session_protocol::auth::{Credentials, Scope};
use charter_session_protocol::link;
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{Attacher, Chunk, Limits, ViewId, Viewer};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf, duplex};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep, sleep_until};

/// One start of the host's credentials, which every link in these tests is admitted with.
static HELD: std::sync::LazyLock<Credentials> =
    std::sync::LazyLock::new(|| Credentials::mint().unwrap());

const CHATS: u32 = 50;
const ROUND_TRIP: Duration = Duration::from_millis(150);
const RETRANSMIT: Duration = ROUND_TRIP;
const LOSS_IN: u64 = 50; // one packet in fifty, 2%
const PACKET: usize = 1460;
const BYTES_PER_SECOND: f64 = 10e6;
const IN_FLIGHT_CHUNKS: usize = 128; // of up to 16 KiB: 2 MiB

/// A small, seeded generator, so a failing run is the same run again.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// One direction of the shaped link.
async fn shape(mut from: ReadHalf<DuplexStream>, mut to: WriteHalf<DuplexStream>, seed: u64) {
    let (queue, mut due) = mpsc::channel::<(Instant, Bytes)>(IN_FLIGHT_CHUNKS);
    tokio::spawn(async move {
        while let Some((at, bytes)) = due.recv().await {
            sleep_until(at).await;
            if to.write_all(&bytes).await.is_err() {
                return;
            }
        }
    });
    let mut random = Lcg(seed);
    let mut free_at = Instant::now();
    let mut last_at = Instant::now();
    let mut buffer = vec![0u8; 16 * 1024];
    while let Ok(n) = from.read(&mut buffer).await {
        if n == 0 {
            return;
        }
        let now = Instant::now();
        let on_the_wire = Duration::from_secs_f64(n as f64 / BYTES_PER_SECOND);
        free_at = free_at.max(now) + on_the_wire;
        let mut at = free_at + ROUND_TRIP / 2;
        if (0..n.div_ceil(PACKET)).any(|_| random.next().is_multiple_of(LOSS_IN)) {
            at += RETRANSMIT;
        }
        // Ordered: nothing is delivered before a chunk sent ahead of it.
        at = at.max(last_at);
        last_at = at;
        if queue
            .send((at, Bytes::copy_from_slice(&buffer[..n])))
            .await
            .is_err()
        {
            return;
        }
    }
}

/// Two ends joined by the shaped link.
fn shaped_link() -> (DuplexStream, DuplexStream) {
    let (client, client_far) = duplex(64 * 1024);
    let (host, host_far) = duplex(64 * 1024);
    let (client_reads, client_writes) = tokio::io::split(client_far);
    let (host_reads, host_writes) = tokio::io::split(host_far);
    tokio::spawn(shape(client_reads, host_writes, 1));
    tokio::spawn(shape(host_reads, client_writes, 2));
    (client, host)
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_needs_you_gets_through_fifty_busy_terminals_and_the_heap_stays_bounded() {
    let _profiler = dhat::Profiler::builder().testing().build();
    let (client_end, host_end) = shaped_link();
    let (client, host) = tokio::join!(
        link::connect(client_end, v1(), Scope::LocalUi, HELD.of(Scope::LocalUi)),
        link::serve(host_end, v1(), &HELD)
    );
    let (mut client, mut host) = (client.unwrap(), host.unwrap());
    let limits = Limits::default();
    // What the limits allow, both ends of the link in this process: per chat, the host's
    // queue and its watermark's worth in flight, and the client's Yamux receive window; plus
    // the shaped link's 2 MiB each way, and 16 MiB for everything else.
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
    let generous = Duration::from_secs(10);
    assert!(
        worst <= generous,
        "the slowest needs-you took {worst:?}, over {generous:?}"
    );
    assert!(
        peak <= bound,
        "the heap peaked at {peak} bytes, over {bound}"
    );
}
