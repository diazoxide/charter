//! The protocol assumes nothing of its transport but an ordered byte stream (ADR 0068 §4):
//! the same negotiation over a unix socket, a pair of pipes, and a child's stdin and stdout,
//! which is how a connector such as `ssh` reaches a runner (ADR 0078 §2). #643's first
//! acceptance line.

// Unix sockets and pipes. Windows is not ported yet (ADR 0068, *Later decisions*).
#![cfg(unix)]

use std::process::Stdio;

use charter_session_protocol::version::{Refused, Speaks, Version, answer, offer};
use tokio::io::join;
use tokio::net::unix::pipe;
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command;

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test]
async fn over_a_unix_socket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("charterd.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let host = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        answer(&mut stream, &v1()).await.unwrap()
    });
    let mut stream = UnixStream::connect(&path).await.unwrap();
    assert_eq!(
        offer(&mut stream, &v1()).await.unwrap(),
        Version { major: 1, minor: 0 }
    );
    assert_eq!(host.await.unwrap(), Version { major: 1, minor: 0 });
}

/// One direction of a pipe, as tokio reads or writes it.
fn pipe_pair() -> (pipe::Receiver, pipe::Sender) {
    let (reader, writer) = std::io::pipe().unwrap();
    (
        pipe::Receiver::from_owned_fd(reader.into()).unwrap(),
        pipe::Sender::from_owned_fd(writer.into()).unwrap(),
    )
}

#[tokio::test]
async fn over_a_pair_of_pipes() {
    let (host_reads, client_writes) = pipe_pair();
    let (client_reads, host_writes) = pipe_pair();
    let mut host_end = join(host_reads, host_writes);
    let mut client_end = join(client_reads, client_writes);
    let speaks = v1();
    let (client, host) = tokio::join!(
        offer(&mut client_end, &speaks),
        answer(&mut host_end, &speaks)
    );
    assert_eq!(client.unwrap(), Version { major: 1, minor: 0 });
    assert_eq!(host.unwrap(), Version { major: 1, minor: 0 });
}

/// The peer program, a host on its own stdin and stdout, as `charter bridge` will be at the far
/// end of a connector.
fn peer(speaks: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter-session-peer"));
    command
        .arg("--speaks")
        .arg(speaks)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

#[tokio::test]
async fn over_a_childs_stdio() {
    let mut child = peer("2.1,1.3").spawn().unwrap();
    let mut stdio = join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let ours = Speaks::new([Version { major: 1, minor: 5 }]);
    assert_eq!(
        offer(&mut stdio, &ours).await.unwrap(),
        Version { major: 1, minor: 3 }
    );
    drop(stdio);
    let out = child.wait_with_output().await.unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn a_child_that_shares_no_major_refuses_and_says_so() {
    let mut child = peer("3.0").spawn().unwrap();
    let mut stdio = join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
    match offer(&mut stdio, &v1()).await.unwrap_err() {
        Refused::ByPeer { speaks } => assert_eq!(speaks, [3]),
        other => panic!("{other:?}"),
    }
    drop(stdio);
    let out = child.wait_with_output().await.unwrap();
    assert!(
        !out.status.success(),
        "a refused negotiation is a failure on the host too"
    );
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("no major version in common"), "{said}");
}
