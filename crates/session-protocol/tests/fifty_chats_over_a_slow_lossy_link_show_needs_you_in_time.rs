//! #643's second acceptance line: fifty chats over one link with 150 ms of round trip and 2%
//! loss show needs-you within a second plus the round trip, with bounded memory.
//!
//! The link is simulated in process, the way netem shapes one: every chunk is held for half
//! the round trip each way; 2% of chunks are "lost", which on an ordered stream is a
//! retransmission: TCP's fast retransmit resends the chunk after about one more round trip
//! (RFC 5681), and every chunk behind it waits for it, since the stream is ordered; and the link carries at most 10 MB/s, a 100 Mbit line, holding at most 2 MiB in
//! flight, about what TCP's window and socket buffers hold at that rate and delay. Past that,
//! the sender's writes wait, as they do on a real socket. The rate is what makes the test
//! bite: bytes the terminals have in flight are bytes a needs-you event waits behind, and only
//! the views' watermark keeps that bounded.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use charter_session_protocol::link;
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{self, Chunk, Limits};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf, duplex};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep, sleep_until};

const CHATS: u32 = 50;
const ROUND_TRIP: Duration = Duration::from_millis(150);
const RETRANSMIT: Duration = ROUND_TRIP;
const LOSS_IN: u64 = 50; // one chunk in fifty, 2%
const BYTES_PER_SECOND: f64 = 10e6;
const IN_FLIGHT_CHUNKS: usize = 128; // of up to 16 KiB: 2 MiB

/// A small, seeded generator, so a failing run is the same run again.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
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
        if random.next() % LOSS_IN == 0 {
            at += RETRANSMIT;
        }
        // Ordered: nothing is delivered before a chunk sent ahead of it.
        at = at.max(last_at);
        last_at = at;
        if queue.send((at, Bytes::copy_from_slice(&buffer[..n]))).await.is_err() {
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

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_needs_you_arrives_inside_a_second_plus_the_round_trip_and_memory_stays_bounded() {
    let (client_end, host_end) = shaped_link();
    let (client, host) = tokio::join!(link::connect(client_end, v1()), link::serve(host_end, v1()));
    let (mut client, mut host) = (client.unwrap(), host.unwrap());
    let limits = Limits::default();
    let stop = Arc::new(AtomicBool::new(false));
    let held = Arc::new(AtomicUsize::new(0));
    let most_held = Arc::new(AtomicUsize::new(0));

    // The host: fifty busy terminals, each re-attached with a fresh snapshot when it falls
    // behind, and a needs-you for each chat in turn on the control lane.
    let opener = host.opener();
    for chat in 0..CHATS {
        let opener = opener.clone();
        let (stop, held, most_held) = (Arc::clone(&stop), Arc::clone(&held), Arc::clone(&most_held));
        tokio::spawn(async move {
            let output = Bytes::from(format!("chat {chat} working… {}\r\n", "▒".repeat(60)).repeat(16));
            let mut epoch = 1;
            while !stop.load(Ordering::Relaxed) {
                let Ok(stream) = opener.open().await else { return };
                let snapshot = Bytes::from(format!("\x1b[2J\x1b[Hchat {chat}, epoch {epoch}"));
                let feed = view::start(stream, chat, epoch, snapshot, limits);
                let mut mine = 0;
                while !stop.load(Ordering::Relaxed) && feed.push(output.clone()).is_ok() {
                    let now = feed.queued_bytes();
                    held.fetch_add(now, Ordering::Relaxed);
                    held.fetch_sub(mine, Ordering::Relaxed);
                    mine = now;
                    most_held.fetch_max(held.load(Ordering::Relaxed), Ordering::Relaxed);
                    sleep(Duration::from_millis(5)).await;
                }
                held.fetch_sub(mine, Ordering::Relaxed);
                epoch += 1;
            }
        });
    }

    // The client: draws every view it is given, as fast as it reads.
    let accepting_stop = Arc::clone(&stop);
    let mut accept = client.acceptor();
    tokio::spawn(async move {
        while !accepting_stop.load(Ordering::Relaxed) {
            let Ok(stream) = accept.accept().await else { return };
            tokio::spawn(async move {
                let Ok(mut reader) = view::accept(stream).await else { return };
                while let Some(Ok(Chunk::Live(bytes) | Chunk::Snapshot(bytes))) = reader.next().await {
                    if reader.ack(bytes.len()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });

    // Let every terminal get going, then ask for the operator once per chat.
    sleep(Duration::from_secs(1)).await;
    let sent: Arc<Mutex<HashMap<u32, Instant>>> = Arc::default();
    let asking = Arc::clone(&sent);
    let asks = tokio::spawn(async move {
        for chat in 0..CHATS {
            asking.lock().unwrap().insert(chat, Instant::now());
            host.control().send(Bytes::from(format!("needs-you {chat}"))).await.unwrap();
            sleep(Duration::from_millis(60)).await;
        }
        host
    });
    let mut took = Vec::new();
    while took.len() < CHATS as usize {
        let frame = client.control().next().await.unwrap().unwrap();
        let text = String::from_utf8(frame.to_vec()).unwrap();
        let chat: u32 = text.strip_prefix("needs-you ").unwrap().parse().unwrap();
        let at = sent.lock().unwrap()[&chat];
        took.push(at.elapsed());
    }
    stop.store(true, Ordering::Relaxed);
    let _host = asks.await.unwrap();

    took.sort();
    let worst = *took.last().unwrap();
    let budget = Duration::from_secs(1) + ROUND_TRIP;
    let bound = CHATS as usize * limits.most_queued_bytes;
    let peak = most_held.load(Ordering::Relaxed);
    eprintln!(
        "needs-you over {CHATS} chats, {ROUND_TRIP:?} round trip, 2% loss, 10 MB/s: p50 {:?}, worst {worst:?}; peak queued {peak} bytes",
        took[took.len() / 2]
    );
    assert!(worst <= budget, "the slowest needs-you took {worst:?}, over {budget:?}");
    assert!(peak <= bound, "the host held {peak} bytes for its views, over {bound}");
}
