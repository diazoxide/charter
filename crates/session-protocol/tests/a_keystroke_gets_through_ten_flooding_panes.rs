//! Ten panes flooding output over one link leave a keystroke in an eleventh quick: a
//! keystroke is a control frame up, its echo is the eleventh pane's bytes down, sharing the
//! link with ten panes that write as fast as the client draws them, over a real unix socket.
//! Evidence for #643's third acceptance line (50 ms), printed every run.
//!
//! **The 50 ms budget is not asserted here.** ADR 0086 keeps wall-clock budgets out of plain
//! `cargo test`, where a loaded shared runner would fail them for its own reasons. This
//! keystroke is ADR 0086's L1 row (keystroke to screen while other chats stream: 50 ms,
//! release absolute in `bench.mjs` and CI relative in the `bench` job, both SC-16's), and the
//! flood itself is that record's *Later decisions* flood row, which SC-12's measurement
//! decides. What this test asserts is generous: the 95th percentile inside a second, and a
//! flood that really floods. Measured here: p50 1 to 3 ms, p95 3 to 12 ms.

// Unix sockets and pipes. Windows is not ported yet (ADR 0068, *Later decisions*).
#![cfg(unix)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use charter_session_protocol::auth::{Credentials, Scope};
use charter_session_protocol::link;
use charter_session_protocol::version::{Speaks, Version};
use charter_session_protocol::view::{Attacher, Chunk, Limits, ViewId, Viewer};
use tokio::net::{UnixListener, UnixStream};

/// One start of the host's credentials, which every link in these tests is admitted with.
static HELD: std::sync::LazyLock<Credentials> =
    std::sync::LazyLock::new(|| Credentials::mint().unwrap());

const FLOODING: u32 = 10;
const SAMPLES: usize = 60;
/// Far past ADR 0086's 50 ms, so only something broken reaches it on a loaded runner.
const GENEROUS: Duration = Duration::from_secs(1);

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_keystroke_gets_through_ten_flooding_panes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("charterd.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let stop = Arc::new(AtomicBool::new(false));

    // The host: ten flooding panes, and an eleventh that echoes what is typed.
    let host_stop = Arc::clone(&stop);
    let host = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut link = link::serve(stream, v1(), &HELD).await.unwrap();
        let mut floods = Vec::new();
        for pane in 0..FLOODING {
            let feed = Attacher::new(link.opener(), Limits::default())
                .attach(ViewId(pane), Bytes::new())
                .await
                .unwrap();
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
                    feed.push(line.clone())
                        .expect("a client that draws never falls behind");
                }
            }));
        }
        let echo = Attacher::new(link.opener(), Limits::default())
            .attach(ViewId(FLOODING), Bytes::new())
            .await
            .unwrap();
        while let Some(Ok(key)) = link.control().next().await {
            echo.push(key).unwrap();
        }
        for flood in floods {
            flood.abort();
        }
    });

    // The client: draws every pane as fast as it reads, and types into the eleventh.
    let mut client = link::connect(
        UnixStream::connect(&path).await.unwrap(),
        v1(),
        Scope::LocalUi,
        HELD.of(Scope::LocalUi),
    )
    .await
    .unwrap();
    let drawn = Arc::new(AtomicUsize::new(0));
    let mut echo = None;
    for _ in 0..=FLOODING {
        let mut reader = Viewer::default()
            .accept(client.accept().await.unwrap())
            .await
            .unwrap();
        if reader.view() == ViewId(FLOODING) {
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
        let Some(Ok(Chunk::Live(seen))) = echo.next().await else {
            panic!("no echo")
        };
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
    assert!(
        flood_rate >= 10.0,
        "the panes drew only {flood_rate:.1} MB/s"
    );
    assert!(
        p95 <= GENEROUS,
        "p95 {p95:?} is over {GENEROUS:?} (worst {worst:?})"
    );
}
