//! **Kill the host mid-turn, and a client that resubscribes from its cursor misses nothing and
//! duplicates nothing** (FD-24's acceptance, ADR 0066 "Every event carries one envelope, and a
//! client subscribes from a cursor").
//!
//! The host is a real process: this test binary, re-run as a child that is the device's one
//! writer and records a turn's events as fast as it can, in segments small enough to be sealed
//! many times over. It is killed with `SIGKILL` three times, once wherever it happens to be, once
//! in the middle of a line it left half-written, and once more at random, and started again after
//! each. Each time it dies the client loses its subscription too, as a client of a dead host
//! does, and subscribes again from the cursor it holds.
//!
//! **And a client that reconnects at every poll misses nothing while segments are sealed**: the
//! second test, over forty fresh logs, because a subscription looking for its segment races the
//! host renaming it.

use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use purlis_core::eventlog::{self, Delivery, Log, Retention, subscribe};

/// Set on the child: the directory of the log it writes.
const WRITER: &str = "FD24_WRITER_DIR";
/// Set on the child: write this many events and stop, or (when absent) write until killed.
const COUNT: &str = "FD24_WRITER_COUNT";
/// Set on the child: after its events, leave half a line and wait to be killed.
const TEAR: &str = "FD24_WRITER_TEAR";

const DEVICE: &str = "01J9ZZDEVICE0000000000000";

/// Small enough to seal a segment every few events.
const SMALL: Retention = Retention {
    segment_bytes: 2048,
    keep: Retention::DEFAULT.keep,
};

/// One turn, as the host records it: a run begins, then tool calls, then the turn stops.
const TURN: [&str; 6] = [
    "run.started",
    "hook.userpromptsubmit",
    "hook.pretooluse",
    "hook.posttooluse",
    "hook.pretooluse",
    "hook.stop",
];

/// The child's body: be the host, and record turns into the log in `dir`.
fn write_as_the_host(dir: &Path) {
    let mut log = Log::open_with(dir, DEVICE, SMALL).expect("the writer opens the log");
    let count = std::env::var(COUNT).ok().map(|n| n.parse::<u64>().unwrap());
    let mut written = 0;
    while count.is_none_or(|count| written < count) {
        let kind = TURN[usize::try_from(written).unwrap() % TURN.len()];
        log.append(
            Some("01J9ZZCHAT00000000000000000"),
            Some("01J9ZZRUN000000000000000000"),
            None,
            kind,
            serde_json::json!({ "n": written }),
        )
        .expect("the event is written");
        written += 1;
    }
    if std::env::var_os(TEAR).is_some() {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join(eventlog::FILE))
            .unwrap();
        std::io::Write::write_all(&mut file, br#"{"v":1,"device_id":"01J9ZZ"#).unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

/// A host this test started, killed when it is dropped, so a failing test leaves no writer
/// running.
struct Host(Child);

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Starts the host as a child of this test binary.
fn host(dir: &Path, count: Option<u64>, tear: bool) -> Host {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "a_client_resubscribing_after_the_host_is_killed_mid_turn_misses_and_duplicates_nothing",
            "--nocapture",
        ])
        .env(WRITER, dir)
        .stdout(std::process::Stdio::null());
    if let Some(count) = count {
        command.env(COUNT, count.to_string());
    }
    if tear {
        command.env(TEAR, "1");
    }
    Host(purlis_core::forklock::spawn(&mut command).expect("the host starts"))
}

/// The client: polls until it holds `until` (or a deadline passes), recording each event.
fn follow(
    dir: &Path,
    cursor: u64,
    until: impl Fn(u64) -> bool,
    seen: &mut Vec<eventlog::Event>,
) -> u64 {
    let mut subscription = subscribe(dir, cursor);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !until(subscription.cursor()) {
        assert!(
            Instant::now() < deadline,
            "the client waited too long at {}",
            subscription.cursor()
        );
        for delivery in subscription.poll().unwrap() {
            match delivery {
                Delivery::Event(event) => seen.push(event),
                Delivery::Missed { after, resumes_at } => {
                    panic!("nothing is old enough to be missed: {after}..{resumes_at}")
                }
                Delivery::Ahead {
                    cursor,
                    log_ends_at,
                } => panic!("no line the client held was lost: {cursor} > {log_ends_at}"),
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    subscription.cursor()
}

/// Kills the host with `SIGKILL` and waits until it is gone.
fn kill(host: Host) {
    drop(host);
}

#[test]
fn a_client_resubscribing_after_the_host_is_killed_mid_turn_misses_and_duplicates_nothing() {
    purlis_core::unsteered!();
    if let Some(dir) = std::env::var_os(WRITER) {
        write_as_the_host(Path::new(&dir));
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("events").join(DEVICE);
    let mut seen = Vec::new();

    // Killed wherever it is, mid-turn and likely mid-write.
    let first = host(&dir, None, false);
    let cursor = follow(&dir, 0, |at| at >= 200, &mut seen);
    kill(first);

    // Killed in the middle of a line it left half-written.
    let mut second = host(&dir, Some(150), true);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !std::fs::read(dir.join(eventlog::FILE))
        .unwrap_or_default()
        .ends_with(br#"{"v":1,"device_id":"01J9ZZ"#)
    {
        assert!(
            Instant::now() < deadline,
            "the second host never tore its line"
        );
        assert!(
            second.0.try_wait().unwrap().is_none(),
            "the second host stopped before it tore a line"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let torn_at = eventlog::read(&dir).unwrap().last().unwrap().seq;
    let cursor = follow(&dir, cursor, |at| at >= torn_at, &mut seen);
    kill(second);

    // Killed again at random, then a last host that finishes.
    let third = host(&dir, None, false);
    let cursor = follow(&dir, cursor, |at| at >= cursor + 100, &mut seen);
    kill(third);
    let mut last = host(&dir, Some(50), false);
    assert!(
        last.0.wait().unwrap().success(),
        "the last host writes and exits"
    );
    let written = eventlog::read(&dir).unwrap();
    let end = written.last().unwrap().seq;
    follow(&dir, cursor, |at| at >= end, &mut seen);

    let seqs: Vec<u64> = seen.iter().map(|event| event.seq).collect();
    assert_eq!(
        seqs,
        (1..=end).collect::<Vec<_>>(),
        "every seq once, in order, across three kills"
    );
    assert_eq!(seen, written, "what the client got is what the log holds");
    let sealed = std::fs::read_dir(&dir)
        .unwrap()
        .filter(|entry| {
            let name = entry.as_ref().unwrap().file_name();
            let name = name.to_string_lossy();
            name.starts_with("events.0")
        })
        .count();
    assert!(
        sealed > 10,
        "the run crossed many sealed segments: {sealed}"
    );
}

#[test]
fn a_client_that_subscribes_again_at_every_poll_while_segments_are_sealed_misses_nothing() {
    purlis_core::unsteered!();
    // A client that reconnects all the time: each poll is a new subscription from its cursor,
    // so each one looks for its segment while the host is sealing them. A fresh log each round,
    // because the first segment's seal is the one a subscription from 0 can race.
    for round in 0..40 {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("events").join(DEVICE);
        let count = 60;
        let mut writer = host(&dir, Some(count), false);
        let mut seen = Vec::new();
        let mut cursor = 0;
        let deadline = Instant::now() + Duration::from_secs(60);
        while cursor < count {
            assert!(
                Instant::now() < deadline,
                "round {round}: the client waited too long at {cursor}"
            );
            let mut subscription = subscribe(&dir, cursor);
            for delivery in subscription.poll().unwrap() {
                match delivery {
                    Delivery::Event(event) => seen.push(event.seq),
                    Delivery::Missed { after, resumes_at } => {
                        panic!(
                            "round {round}: nothing is old enough to be missed: {after}..{resumes_at}"
                        )
                    }
                    Delivery::Ahead {
                        cursor,
                        log_ends_at,
                    } => panic!(
                        "round {round}: no line the client held was lost: {cursor} > {log_ends_at}"
                    ),
                }
            }
            cursor = subscription.cursor();
        }
        assert!(writer.0.wait().unwrap().success());
        assert_eq!(seen, (1..=count).collect::<Vec<_>>(), "round {round}");
    }
}
