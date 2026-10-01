//! #643's third acceptance line: ten panes flooding output over one link leave the keystroke
//! latency of an eleventh at 50 ms or less. A keystroke is a control frame up; its echo is the
//! eleventh pane's bytes down, sharing the link with ten panes that write as fast as the
//! client draws them. Over a real unix socket.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use charter_session_protocol::link;
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{self, Chunk, Limits};
use tokio::net::{UnixListener, UnixStream};

const FLOODING: u32 = 10;
const SAMPLES: usize = 60;
const BUDGET: Duration = Duration::from_millis(50);

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_keystroke_stays_inside_fifty_milliseconds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("charterd.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let stop = Arc::new(AtomicBool::new(false));

    // The host: ten flooding panes, and an eleventh that echoes what is typed.
    let host_stop = Arc::clone(&stop);
    let host = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut link = link::serve(stream, v1()).await.unwrap();
        let mut floods = Vec::new();
        for pane in 0..FLOODING {
            let feed = view::start(link.open().await.unwrap(), pane, 1, Bytes::new(), Limits::default());
            let stop = Arc::clone(&host_stop);
            floods.push(tokio::spawn(async move {
                let line = Bytes::from(format!("pane {pane}: {}\r\n", "x".repeat(200)).repeat(64));
                while !stop.load(Ordering::Relaxed) {
                    // A terminal's reader blocks on the PTY between reads; this one waits a
                    // millisecond when its queue is full, rather than spinning the runtime.
                    if feed.queued_bytes() > 256 * 1024 {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        continue;
                    }
                    feed.push(line.clone()).expect("a client that draws never falls behind");
                }
            }));
        }
        let echo = view::start(link.open().await.unwrap(), FLOODING, 1, Bytes::new(), Limits::default());
        while let Some(Ok(key)) = link.control().next().await {
            echo.push(key).unwrap();
        }
        for flood in floods {
            flood.abort();
        }
    });

    // The client: draws every pane as fast as it reads, and types into the eleventh.
    let mut client = link::connect(UnixStream::connect(&path).await.unwrap(), v1()).await.unwrap();
    let drawn = Arc::new(AtomicUsize::new(0));
    let mut echo = None;
    for _ in 0..=FLOODING {
        let mut reader = view::accept(client.accept().await.unwrap()).await.unwrap();
        if reader.view() == FLOODING {
            echo = Some(reader);
            continue;
        }
        let drawn = Arc::clone(&drawn);
        tokio::spawn(async move {
            while let Some(Ok(Chunk::Live(bytes) | Chunk::Snapshot(bytes))) = reader.next().await {
                drawn.fetch_add(bytes.len(), Ordering::Relaxed);
                if reader.ack(bytes.len()).await.is_err() {
                    break;
                }
            }
        });
    }
    let mut echo = echo.unwrap();
    // Let the flood reach its steady state before measuring.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let measuring = Instant::now();
    let drawn_before = drawn.load(Ordering::Relaxed);
    let mut took = Vec::with_capacity(SAMPLES);
    for n in 0..SAMPLES {
        let key = Bytes::from(format!("{}", n % 10));
        let typed = Instant::now();
        client.control().send(key.clone()).await.unwrap();
        let Some(Ok(Chunk::Live(seen))) = echo.next().await else { panic!("no echo") };
        took.push(typed.elapsed());
        echo.ack(seen.len()).await.unwrap();
        assert_eq!(seen, key);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let flood_rate = (drawn.load(Ordering::Relaxed) - drawn_before) as f64
        / measuring.elapsed().as_secs_f64()
        / 1e6;
    stop.store(true, Ordering::Relaxed);
    drop(client);
    let _ = host.await;

    took.sort();
    let p95 = took[SAMPLES * 95 / 100];
    let worst = took[SAMPLES - 1];
    eprintln!(
        "keystroke round trip under {FLOODING} panes flooding {flood_rate:.0} MB/s: p50 {:?}, p95 {p95:?}, worst {worst:?}",
        took[SAMPLES / 2]
    );
    // A flood that did not flood would pass anything.
    assert!(flood_rate >= 10.0, "the panes drew only {flood_rate:.1} MB/s");
    // Every sample, not a percentile: the acceptance line is about the keystroke a person
    // types, and the slow one is the one they notice. Without the views' watermark (only
    // Yamux's own windows holding the flood back) the worst sample here was 65 ms.
    assert!(worst <= BUDGET, "the slowest keystroke took {worst:?}, over {BUDGET:?} (p95 {p95:?})");
}
