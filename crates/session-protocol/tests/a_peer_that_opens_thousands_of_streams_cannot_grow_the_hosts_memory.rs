//! A peer can open streams faster than anyone accepts them, and write into each. The host
//! holds a bounded number: Yamux's own limit on streams, and a bounded queue of streams nobody
//! has accepted yet, past which a new one is refused. Measured on the host's heap, with the
//! hostile peer in a process of its own (#817 review).

// The peer is reached over a child's stdio.
#![cfg(unix)]

use std::process::Stdio;
use std::time::Duration;

use charter_session_protocol::link;
use charter_session_protocol::version::{Speaks, Version};
use tokio::process::Command;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

const STREAMS: usize = 2_000;
const BYTES_EACH: usize = 128 * 1024;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_host_holds_a_bounded_number_of_streams_and_bytes_however_many_are_opened() {
    let _profiler = dhat::Profiler::builder().testing().build();
    let mut child = Command::new(env!("CARGO_BIN_EXE_charter-session-peer"))
        .args(["--flood", &STREAMS.to_string(), &BYTES_EACH.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let stdio = tokio::io::join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let speaks = Speaks::new([Version { major: 1, minor: 0 }]);
    let mut host = link::serve(stdio, speaks).await.unwrap();

    // Nobody accepts while the flood arrives. Sample what the host holds as it does.
    let mut most_unaccepted = 0;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        most_unaccepted = most_unaccepted.max(host.unaccepted());
    }
    let heap = dhat::HeapStats::get();
    let all_of_it = STREAMS * BYTES_EACH;
    eprintln!(
        "{STREAMS} streams of {BYTES_EACH} bytes opened ({all_of_it} bytes); the host held at most {most_unaccepted} unaccepted; heap peak {} bytes",
        heap.max_bytes
    );
    assert!(
        most_unaccepted <= link::MOST_UNACCEPTED_STREAMS,
        "held {most_unaccepted} unaccepted streams"
    );
    // Measured: 2.2 MB with the bounds, 33.7 MB before them (2,000 streams kept, which only
    // the pipe's speed held to that).
    let bound = 16 << 20;
    assert!(
        heap.max_bytes <= bound,
        "the host's heap peaked at {} bytes, over {bound}",
        heap.max_bytes
    );
}
