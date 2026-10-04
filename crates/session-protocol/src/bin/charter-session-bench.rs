//! The session layer's latency, measured over a real unix socket: what `charterd` and the window
//! will speak, with no window in the way (SC-16, ADR 0086's CI relative rows L1 and L3).
//!
//! It runs a host and a client in one process, each on the runtime the crate's tests use, joined
//! by a `charterd.sock` of their own, and prints one JSON array in github-action-benchmark's
//! `customSmallerIsBetter` shape: each row's `value` is the median of the run's samples, in ms.
//!
//! - **keystroke under ten flooding panes**: a keystroke up the control lane and its echo down an
//!   eleventh view, while ten views write as fast as the client draws them (FD-4's third line).
//! - **2 MB burst** and **13 MB burst**: the corpus, ×16 and ×99, pushed into one view in the
//!   pieces a PTY read hands over, from the client asking to the last byte drawn.
//! - **keystroke in a flooding pane** (`--rows own`, not gated): a keystroke echoed into a view
//!   that is itself flooding, so it waits behind what the view holds. It is how the watermarks
//!   and the queue were measured.
//!
//! `tools/bench.mjs --only host` runs it, and in CI runs `main`'s build of it beside the change's,
//! interleaved, and lets `tools/latency-gate.mjs` decide. The other flags are for measuring the
//! view's watermarks ([`Limits`]) on a slower client or a longer link, as SC-16 did:
//!
//! ```text
//! charter-session-bench [--corpus <file>] [--samples <n>] [--rows a,b]
//!                       [--high <bytes>] [--low <bytes>] [--queue <bytes>]
//!                       [--draw-mbps <MB/s>] [--rtt-ms <ms>] [--source-pause <bytes>]
//! ```

// Unix sockets. Windows is not ported yet (ADR 0068, *Later decisions*; M46 Windows, #565).
#[cfg(not(unix))]
fn main() {
    eprintln!("charter-session-bench runs on unix sockets only");
    std::process::exit(2);
}

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    bench::main()
}

#[cfg(unix)]
mod bench {
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use bytes::Bytes;
    use charter_session_protocol::auth::{Credentials, Scope};
    use charter_session_protocol::link::{self, Link};
    use charter_session_protocol::local;
    use charter_session_protocol::version::{Speaks, Version};
    use charter_session_protocol::view::{Attacher, Chunk, Limits, Reader, ViewId, Viewer};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};

    /// The flooding panes beside the one typed into, as FD-4's acceptance line has it.
    const FLOODING: u32 = 10;
    /// What one PTY read hands the host on Linux and macOS: the pieces a burst arrives in.
    const PTY_READ: usize = 4096;
    /// A source past this much queued waits a millisecond, as a terminal's reader blocks on its
    /// PTY, rather than spinning the runtime (the crate's flooding test does the same).
    /// `--source-pause` moves it, to measure a view whose queue is nearly full.
    const SOURCE_PAUSE: usize = 256 * 1024;

    pub struct Options {
        corpus: PathBuf,
        samples: usize,
        rows: Vec<String>,
        limits: Limits,
        /// The client's drawing speed, bytes a second: none is as fast as it reads.
        draw: Option<f64>,
        rtt: Duration,
        source_pause: usize,
    }

    fn options() -> Result<Options, String> {
        let defaults = Limits::default();
        let mut o = Options {
            corpus: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/corpora/claude-code-session.raw"),
            samples: 200,
            rows: vec!["keystroke".into(), "burst2".into(), "burst13".into()],
            limits: defaults,
            draw: None,
            rtt: Duration::ZERO,
            source_pause: SOURCE_PAUSE,
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            let value = args.next().ok_or(format!("{flag} needs a value"))?;
            let number = || {
                value
                    .parse::<f64>()
                    .map_err(|e| format!("{flag} {value}: {e}"))
            };
            match flag.as_str() {
                "--corpus" => o.corpus = value.clone().into(),
                "--samples" => o.samples = number()? as usize,
                "--rows" => o.rows = value.split(',').map(str::to_owned).collect(),
                "--high" => o.limits.high_watermark = number()? as usize,
                "--low" => o.limits.low_watermark = number()? as usize,
                "--queue" => o.limits.most_queued_bytes = number()? as usize,
                "--draw-mbps" => o.draw = Some(number()? * 1e6),
                "--source-pause" => o.source_pause = number()? as usize,
                "--rtt-ms" => o.rtt = Duration::from_secs_f64(number()? / 1000.0),
                _ => return Err(format!("unknown flag {flag}")),
            }
        }
        Ok(o)
    }

    pub fn main() -> ExitCode {
        let o = match options() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("charter-session-bench: {e}");
                return ExitCode::from(2);
            }
        };
        let corpus = match std::fs::read(&o.corpus) {
            Ok(bytes) => Bytes::from(bytes),
            Err(e) => {
                eprintln!("charter-session-bench: {}: {e}", o.corpus.display());
                return ExitCode::from(2);
            }
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("a runtime");
        let mut rows = Vec::new();
        for row in &o.rows {
            let (name, samples) = match row.as_str() {
                "keystroke" => (
                    "keystroke under ten flooding panes",
                    within(&runtime, row, keystroke(&o)),
                ),
                "own" => (
                    "keystroke in a flooding pane",
                    within(&runtime, row, own(&o)),
                ),
                "burst2" => (
                    "2 MB burst, asked to drawn",
                    within(&runtime, row, burst(&o, &corpus, 16, o.samples.div_ceil(5))),
                ),
                "burst13" => (
                    "13 MB burst, asked to drawn",
                    within(
                        &runtime,
                        row,
                        burst(&o, &corpus, 99, o.samples.div_ceil(20)),
                    ),
                ),
                other => {
                    eprintln!("charter-session-bench: unknown row {other}");
                    return ExitCode::from(2);
                }
            };
            let ms: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
            rows.push(serde_json::json!({
                "name": name,
                "unit": "ms",
                "value": median(&ms),
                "extra": format!("median of {} samples; p95 {:.3} ms; worst {:.3} ms",
                    ms.len(), percentile(&ms, 95), percentile(&ms, 100)),
            }));
        }
        println!("{}", serde_json::Value::Array(rows));
        ExitCode::SUCCESS
    }

    /// The most one row may take. A row that hangs (a view that never ends, an echo that never
    /// comes) is a failed run with a reason, not a job that waits out its runner's timeout.
    const ROW_TIMEOUT: Duration = Duration::from_secs(300);

    fn within<T>(
        runtime: &tokio::runtime::Runtime,
        row: &str,
        measuring: impl std::future::Future<Output = T>,
    ) -> T {
        match runtime.block_on(async { tokio::time::timeout(ROW_TIMEOUT, measuring).await }) {
            Ok(samples) => samples,
            Err(_) => {
                eprintln!("charter-session-bench: {row} did not finish within {ROW_TIMEOUT:?}");
                std::process::exit(1);
            }
        }
    }

    fn median(ms: &[f64]) -> f64 {
        let mut sorted = ms.to_vec();
        sorted.sort_by(f64::total_cmp);
        let middle = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            sorted[middle]
        } else {
            (sorted[middle - 1] + sorted[middle]) / 2.0
        }
    }

    fn percentile(ms: &[f64], p: usize) -> f64 {
        let mut sorted = ms.to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted[((sorted.len() - 1) * p) / 100]
    }

    fn v1() -> Speaks {
        Speaks::new([Version { major: 1, minor: 0 }])
    }

    /// A host link and a client link over a fresh `charterd.sock`, through a delaying proxy when
    /// the options ask for a round trip.
    async fn linked(o: &Options) -> (Link, Link) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "charter-session-bench-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory for the socket");
        let path = dir.join("charterd.sock");
        // The host's own path: the uid check, then the admission, as `charterd` serves it.
        let listener =
            local::Listener::new(UnixListener::bind(&path).expect("a socket to listen on"));
        let held = Credentials::mint().expect("the host's credentials");
        let credential = held.of(Scope::LocalUi).clone();
        let host = tokio::spawn(async move {
            let stream = listener
                .accept()
                .await
                .expect("the client")
                .expect("this user");
            link::serve(
                stream,
                v1(),
                &held,
                &std::sync::Arc::new(charter_session_protocol::local::NoChats),
            )
            .await
            .expect("a host link")
        });
        let stream = UnixStream::connect(&path).await.expect("the host's socket");
        let client = if o.rtt.is_zero() {
            link::connect(stream, v1(), Scope::LocalUi, &credential).await
        } else {
            link::connect(
                delayed(stream, o.rtt / 2),
                v1(),
                Scope::LocalUi,
                &credential,
            )
            .await
        }
        .expect("a client link");
        let host = host.await.expect("the host");
        let _ = std::fs::remove_dir_all(&dir);
        (host, client)
    }

    /// `stream`, with every byte each way held for `one_way` before it is written on: a longer
    /// link at a loopback's bandwidth, as `ssh` to a nearby host is.
    fn delayed(stream: UnixStream, one_way: Duration) -> tokio::io::DuplexStream {
        let (near, far) = tokio::io::duplex(1 << 20);
        let (stream_read, stream_write) = stream.into_split();
        let (far_read, far_write) = tokio::io::split(far);
        tokio::spawn(hold(far_read, stream_write, one_way));
        tokio::spawn(hold(stream_read, far_write, one_way));
        near
    }

    async fn hold(
        mut from: impl tokio::io::AsyncRead + Unpin + Send + 'static,
        mut to: impl tokio::io::AsyncWrite + Unpin + Send + 'static,
        one_way: Duration,
    ) {
        let (send, mut held) = tokio::sync::mpsc::unbounded_channel::<(Instant, Vec<u8>)>();
        tokio::spawn(async move {
            while let Some((at, bytes)) = held.recv().await {
                tokio::time::sleep_until((at + one_way).into()).await;
                if to.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        });
        let mut buffer = vec![0u8; 64 * 1024];
        while let Ok(n) = from.read(&mut buffer).await {
            if n == 0 || send.send((Instant::now(), buffer[..n].to_vec())).is_err() {
                break;
            }
        }
    }

    /// Draws as a client at `rate` bytes a second would. It keeps to a clock rather than
    /// sleeping per chunk, so a sleep's overshoot (the timer's millisecond) is made up on the
    /// next chunks instead of adding up. A client idle for longer than [`IDLE`] starts again from
    /// now rather than drawing what came next at no cost.
    const IDLE: Duration = Duration::from_millis(5);

    struct Drawing {
        rate: Option<f64>,
        since: Instant,
        owed: usize,
    }

    impl Drawing {
        fn new(rate: Option<f64>) -> Self {
            Drawing {
                rate,
                since: Instant::now(),
                owed: 0,
            }
        }

        async fn draw(&mut self, bytes: usize) {
            let Some(rate) = self.rate else { return };
            let now = Instant::now();
            if self.since + Duration::from_secs_f64(self.owed as f64 / rate) + IDLE < now {
                self.since = now;
                self.owed = 0;
            }
            self.owed += bytes;
            let done = self.since + Duration::from_secs_f64(self.owed as f64 / rate);
            if done > now + Duration::from_millis(1) {
                tokio::time::sleep_until(done.into()).await;
            }
        }
    }

    /// Reads a view to its end, drawing and acknowledging each chunk; counts what it drew.
    async fn draw_all(mut reader: Reader, rate: Option<f64>, drawn: Arc<AtomicUsize>) {
        let mut drawing = Drawing::new(rate);
        while let Some(Ok(Chunk::Live(bytes) | Chunk::Snapshot(bytes))) = reader.next().await {
            drawing.draw(bytes.len()).await;
            drawn.fetch_add(bytes.len(), Ordering::Relaxed);
            if reader.ack(bytes.len()).await.is_err() {
                break;
            }
        }
    }

    /// FD-4's third acceptance line, as its test runs it, with more samples.
    async fn keystroke(o: &Options) -> Vec<Duration> {
        let (mut host, mut client) = linked(o).await;
        let stop = Arc::new(AtomicBool::new(false));
        let limits = o.limits;
        let pause = o.source_pause;
        let host_stop = Arc::clone(&stop);
        let host = tokio::spawn(async move {
            let mut floods = Vec::new();
            for pane in 0..FLOODING {
                let feed = Attacher::new(host.opener(), limits)
                    .attach(ViewId(pane), Bytes::new())
                    .await
                    .expect("a flooding view");
                let stop = Arc::clone(&host_stop);
                floods.push(tokio::spawn(async move {
                    let line =
                        Bytes::from(format!("pane {pane}: {}\r\n", "x".repeat(200)).repeat(64));
                    while !stop.load(Ordering::Relaxed) {
                        if feed.queued_bytes() > pause {
                            tokio::time::sleep(Duration::from_millis(1)).await;
                            continue;
                        }
                        if feed.push(line.clone()).is_err() {
                            break;
                        }
                    }
                }));
            }
            let echo = Attacher::new(host.opener(), limits)
                .attach(ViewId(FLOODING), Bytes::new())
                .await
                .expect("the typed view");
            while let Some(Ok(key)) = host.control().next().await {
                if echo.push(key).is_err() {
                    break;
                }
            }
            for flood in floods {
                flood.abort();
            }
        });

        let drawn = Arc::new(AtomicUsize::new(0));
        let mut echo = None;
        for _ in 0..=FLOODING {
            let reader = Viewer::default()
                .accept(client.accept().await.expect("a view"))
                .await
                .expect("a view's header");
            if reader.view() == ViewId(FLOODING) {
                echo = Some(reader);
            } else {
                tokio::spawn(draw_all(reader, o.draw, Arc::clone(&drawn)));
            }
        }
        let mut echo = echo.expect("the typed view");
        tokio::time::sleep(Duration::from_millis(300)).await;

        let mut took = Vec::with_capacity(o.samples);
        for n in 0..o.samples {
            let key = Bytes::from(format!("{}", n % 10));
            let typed = Instant::now();
            client
                .control()
                .send(key.clone())
                .await
                .expect("a keystroke");
            let Some(Ok(Chunk::Live(seen))) = echo.next().await else {
                panic!("no echo")
            };
            took.push(typed.elapsed());
            echo.ack(seen.len()).await.expect("an ack");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        stop.store(true, Ordering::Relaxed);
        drop(client);
        let _ = host.await;
        took
    }

    /// A keystroke typed into a pane that is itself flooding, as Ctrl-C into a runaway build is:
    /// its echo is queued behind what the view already holds, so it waits for the client to
    /// draw what is queued and in flight. Not a gated row: it is how the watermarks were
    /// measured (`--rows own`).
    async fn own(o: &Options) -> Vec<Duration> {
        let (mut host, mut client) = linked(o).await;
        let stop = Arc::new(AtomicBool::new(false));
        let limits = o.limits;
        let pause = o.source_pause;
        let host_stop = Arc::clone(&stop);
        let host = tokio::spawn(async move {
            let feed = Arc::new(
                Attacher::new(host.opener(), limits)
                    .attach(ViewId(0), Bytes::new())
                    .await
                    .expect("the flooding view"),
            );
            let flood_feed = Arc::clone(&feed);
            let flood = tokio::spawn(async move {
                let line = Bytes::from(format!("{}\r\n", "x".repeat(200)).repeat(64));
                while !host_stop.load(Ordering::Relaxed) {
                    if flood_feed.queued_bytes() > pause {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        continue;
                    }
                    if flood_feed.push(line.clone()).is_err() {
                        break;
                    }
                }
            });
            while let Some(Ok(key)) = host.control().next().await {
                let mut echo = vec![0x07];
                echo.extend_from_slice(&key);
                echo.push(0x07);
                if feed.push(Bytes::from(echo)).is_err() {
                    break;
                }
            }
            flood.abort();
        });

        let mut reader = Viewer::default()
            .accept(client.accept().await.expect("a view"))
            .await
            .expect("a view's header");
        let mut drawing = Drawing::new(o.draw);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut took = Vec::with_capacity(o.samples);
        let mut carry: Vec<u8> = Vec::new();
        for n in 0..o.samples {
            let key = format!("K{n}");
            let marker = format!("\x07{key}\x07").into_bytes();
            let typed = Instant::now();
            client
                .control()
                .send(Bytes::from(key))
                .await
                .expect("a keystroke");
            loop {
                let Some(Ok(Chunk::Live(bytes))) = reader.next().await else {
                    panic!("the flooding view ended")
                };
                drawing.draw(bytes.len()).await;
                reader.ack(bytes.len()).await.expect("an ack");
                carry.extend_from_slice(&bytes);
                let found = carry.windows(marker.len()).any(|w| w == marker.as_slice());
                let keep = carry.len().saturating_sub(marker.len());
                carry.drain(..keep);
                if found {
                    break;
                }
            }
            took.push(typed.elapsed());
            carry.clear();
        }
        stop.store(true, Ordering::Relaxed);
        drop(client);
        let _ = host.await;
        took
    }

    /// `loops` × the corpus into one view, `samples` times: from the client's asking to its
    /// last byte drawn.
    async fn burst(o: &Options, corpus: &Bytes, loops: usize, samples: usize) -> Vec<Duration> {
        let (mut host, mut client) = linked(o).await;
        let limits = o.limits;
        let pause = o.source_pause;
        let corpus = corpus.clone();
        let total = corpus.len() * loops;
        let host = tokio::spawn(async move {
            let feed = Attacher::new(host.opener(), limits)
                .attach(ViewId(0), Bytes::new())
                .await
                .expect("the bursting view");
            while let Some(Ok(_go)) = host.control().next().await {
                let mut left = total;
                let mut at = 0;
                while left > 0 {
                    if feed.queued_bytes() > pause {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        continue;
                    }
                    let n = PTY_READ.min(left).min(corpus.len() - at);
                    feed.push(corpus.slice(at..at + n))
                        .expect("a client that draws never falls behind");
                    at = (at + n) % corpus.len();
                    left -= n;
                }
            }
        });
        let mut reader = Viewer::default()
            .accept(client.accept().await.expect("a view"))
            .await
            .expect("a view's header");
        let mut drawing = Drawing::new(o.draw);
        let mut took = Vec::with_capacity(samples);
        for _ in 0..samples {
            let asked = Instant::now();
            client
                .control()
                .send(Bytes::from_static(b"go"))
                .await
                .expect("go");
            let mut seen = 0;
            while seen < total {
                let Some(Ok(Chunk::Live(bytes))) = reader.next().await else {
                    panic!("the burst ended early, at {seen} of {total} bytes")
                };
                drawing.draw(bytes.len()).await;
                seen += bytes.len();
                reader.ack(bytes.len()).await.expect("an ack");
            }
            took.push(asked.elapsed());
        }
        drop(client);
        let _ = host.await;
        took
    }
}
