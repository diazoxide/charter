//! `feed` against a child that answers without reading its stdin (charter-app CI red on
//! b93e58e and a68cf8e, `the payload is written: Broken pipe`).
//!
//! The race, made deterministic: a payload far past any pipe buffer cannot fit in the pipe,
//! so the write blocks until the child has exited without reading, and then it meets a
//! closed pipe every time rather than only when the child happened to win.
#![cfg(unix)]

use std::process::{Command, Stdio};

/// Four megabytes: past the pipe buffer on every kernel charter runs on (64 KiB on Linux and
/// Darwin), so the write cannot complete before the reader is gone.
const PAST_THE_PIPE_BUFFER: usize = 4 << 20;

fn sh(script: &str) -> std::process::Child {
    Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("/bin/sh runs")
}

#[test]
fn a_child_that_exits_without_reading_leaves_its_answer_to_be_read() {
    let mut child = sh("echo refused >&2; exit 3");

    stand_in::feed(&mut child, &vec![b'x'; PAST_THE_PIPE_BUFFER]);

    let out = child.wait_with_output().expect("the child finishes");
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "refused\n");
}

#[test]
fn a_child_that_reads_gets_every_byte_and_its_end() {
    // `wc -c` only answers once it has read to end of file, so this also pins that `feed`
    // closes the pipe: left open, the child would wait on it for ever.
    let mut child = sh("wc -c");

    stand_in::feed(&mut child, &vec![b'x'; PAST_THE_PIPE_BUFFER]);

    let out = child.wait_with_output().expect("the child finishes");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        PAST_THE_PIPE_BUFFER.to_string()
    );
}
