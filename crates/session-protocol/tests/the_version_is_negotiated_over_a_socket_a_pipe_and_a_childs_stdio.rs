//! The protocol assumes nothing of its transport but an ordered byte stream (ADR 0068 §4):
//! the same negotiation over a unix socket, a pair of pipes, and a child's stdin and stdout,
//! which is how a connector such as `ssh` reaches a runner (ADR 0078 §2). #643's first
//! acceptance line.
//!
//! Over each of the three, the same four outcomes:
//!
//! - **a version both speak** is agreed by both ends;
//! - **no shared major** is refused by both ends, each saying what the other speaks, and the
//!   host's refusal ends the stream;
//! - **a stream that is not this protocol** (an ssh banner, a login shell's greeting) is
//!   refused before anything is parsed, and answered with nothing;
//! - **a garbled hello** (the magic, then a frame that is not a hello) is refused the same way.
//!
//! Every refusing end closes its side without waiting for the other to hang up, and every wait
//! here is bounded, so a peer that lingers fails a test instead of hanging it. A hello from a
//! later minor, with fields this version does not know, is still read (the forward half of
//! the promise in `version`'s docs).

// Unix sockets and pipes. Windows is not ported yet (ADR 0068, *Later decisions*).
#![cfg(unix)]

use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use charter_session_protocol::version::{MAGIC, Refused, Speaks, Version, answer, offer};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Join, join};
use tokio::net::unix::pipe;
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, Command};
use tokio::time::timeout;

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
        answer(&mut stream, &v1()).await.unwrap().0
    });
    let mut stream = UnixStream::connect(&path).await.unwrap();
    assert_eq!(
        offer(&mut stream, &v1()).await.unwrap().0,
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
    assert_eq!(client.unwrap().0, Version { major: 1, minor: 0 });
    assert_eq!(host.unwrap().0, Version { major: 1, minor: 0 });
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
        offer(&mut stdio, &ours).await.unwrap().0,
        Version { major: 1, minor: 3 }
    );
    drop(stdio);
    let out = timeout(CLOSES_WITHIN, child.wait_with_output())
        .await
        .expect("the child exited")
        .unwrap();
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
    let out = timeout(CLOSES_WITHIN, child.wait_with_output())
        .await
        .expect("the child exited")
        .unwrap();
    assert!(
        !out.status.success(),
        "a refused negotiation is a failure on the host too"
    );
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("no major version in common"), "{said}");
}

/// What an ssh server says first: a stream from something that is not charter.
const NOT_A_HELLO: &[u8] = b"SSH-2.0-OpenSSH_9.9\r\n";

/// How long a refusing end may take to close its side. Generous: it is a hang we look for.
const CLOSES_WITHIN: Duration = Duration::from_secs(5);

/// The child's own words, once it has exited.
async fn exited(mut child: Child) -> (ExitStatus, String) {
    let status = timeout(CLOSES_WITHIN, child.wait())
        .await
        .expect("the child exited")
        .unwrap();
    let mut said = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut said)
        .await
        .unwrap();
    (status, said)
}

#[tokio::test]
async fn a_child_refuses_a_stream_that_is_not_this_protocol_and_closes_its_end() {
    let mut child = peer("1.0").spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    stdin.write_all(NOT_A_HELLO).await.unwrap();
    // Our end stays open: the end that refuses must not wait for us to hang up.
    let mut answered = Vec::new();
    timeout(CLOSES_WITHIN, stdout.read_to_end(&mut answered))
        .await
        .expect("the child closed its end of the stream")
        .unwrap();
    assert!(
        answered.is_empty(),
        "a refusal accepts nothing: {answered:?}"
    );
    let (status, said) = exited(child).await;
    assert!(!status.success());
    assert!(
        said.contains("not speaking charter's session protocol"),
        "{said}"
    );
    drop(stdin);
}

/// A hello in its frame, as a client of any version writes it: the magic, a big-endian `u16`
/// length, then the body.
fn framed(body: &[u8]) -> Vec<u8> {
    let mut all = MAGIC.to_vec();
    all.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
    all.extend_from_slice(body);
    all
}

/// The magic and a well-framed body that is not JSON: a stream that began as this protocol
/// and was then damaged.
fn garbled_hello() -> Vec<u8> {
    framed(b"\xff\xfe{versions: [1.0]")
}

/// Everything the other end writes until it closes. A unix socket closed with bytes unread in
/// it is reset on some kernels rather than ended, and a reset is a close too.
async fn until_closed<R: tokio::io::AsyncRead + Unpin>(mut from: R) -> Vec<u8> {
    let mut all = Vec::new();
    match timeout(CLOSES_WITHIN, from.read_to_end(&mut all))
        .await
        .expect("the other end closed the stream")
    {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
        Err(e) => panic!("{e}"),
    }
    all
}

/// Both ends of a unix socket, as the app and `charterd.sock` hold them.
async fn socket_pair() -> (UnixStream, UnixStream, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("charterd.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (client, accepted) = tokio::join!(UnixStream::connect(&path), listener.accept());
    (client.unwrap(), accepted.unwrap().0, dir)
}

/// Both ends of a pair of pipes, one each way.
fn pipes() -> (
    Join<pipe::Receiver, pipe::Sender>,
    Join<pipe::Receiver, pipe::Sender>,
) {
    let (host_reads, client_writes) = pipe_pair();
    let (client_reads, host_writes) = pipe_pair();
    (
        join(client_reads, client_writes),
        join(host_reads, host_writes),
    )
}

/// A host of major 2 and a client of major 1: both refuse, and the host's refusal ends the
/// stream rather than leaving it open.
async fn no_shared_major_is_refused_by_both_and_the_stream_ends<C, H>(mut client: C, host: H)
where
    C: AsyncRead + AsyncWrite + Unpin,
    H: AsyncRead + AsyncWrite + Unpin,
{
    let theirs = Speaks::new([Version { major: 2, minor: 0 }]);
    let host = async move { answer(host, &theirs).await.map(|(v, _)| v) };
    let client_side = async { offer(&mut client, &v1()).await.map(|(v, _)| v) };
    let (client_side, host_side) = tokio::join!(client_side, host);
    match client_side.unwrap_err() {
        Refused::ByPeer { speaks } => assert_eq!(speaks, [2]),
        other => panic!("{other:?}"),
    }
    match host_side.unwrap_err() {
        Refused::NoSharedMajor { ours, theirs } => {
            assert_eq!(ours, [2]);
            assert_eq!(theirs, [1]);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        until_closed(client).await.is_empty(),
        "nothing after a refusal"
    );
}

/// A client that writes `first` instead of a hello: the host refuses with `expected`, writes
/// nothing back, and ends the stream.
async fn a_bad_hello_is_refused_and_answered_with_nothing<C, H>(
    mut client: C,
    host: H,
    first: &[u8],
    expected: fn(&Refused) -> bool,
) where
    C: AsyncRead + AsyncWrite + Unpin,
    H: AsyncRead + AsyncWrite + Unpin,
{
    let host = async move { answer(host, &v1()).await.map(|(v, _)| v) };
    let client_side = async {
        client.write_all(first).await.unwrap();
        // Our end stays open: the end that refuses must not wait for us to hang up.
        until_closed(&mut client).await
    };
    let (answered, host_side) = tokio::join!(client_side, host);
    let refused = host_side.unwrap_err();
    assert!(expected(&refused), "{refused:?}");
    assert!(
        answered.is_empty(),
        "a refusal accepts nothing: {answered:?}"
    );
}

fn not_this_protocol(refused: &Refused) -> bool {
    matches!(refused, Refused::NotThisProtocol)
}

fn malformed(refused: &Refused) -> bool {
    matches!(refused, Refused::Malformed(_))
}

#[tokio::test]
async fn over_a_unix_socket_no_shared_major_is_refused() {
    let (client, host, _dir) = socket_pair().await;
    no_shared_major_is_refused_by_both_and_the_stream_ends(client, host).await;
}

#[tokio::test]
async fn over_a_unix_socket_a_stream_that_is_not_this_protocol_is_refused() {
    let (client, host, _dir) = socket_pair().await;
    a_bad_hello_is_refused_and_answered_with_nothing(client, host, NOT_A_HELLO, not_this_protocol)
        .await;
}

#[tokio::test]
async fn over_a_unix_socket_a_garbled_hello_is_refused() {
    let (client, host, _dir) = socket_pair().await;
    a_bad_hello_is_refused_and_answered_with_nothing(client, host, &garbled_hello(), malformed)
        .await;
}

#[tokio::test]
async fn over_a_pair_of_pipes_no_shared_major_is_refused() {
    let (client, host) = pipes();
    no_shared_major_is_refused_by_both_and_the_stream_ends(client, host).await;
}

#[tokio::test]
async fn over_a_pair_of_pipes_a_stream_that_is_not_this_protocol_is_refused() {
    let (client, host) = pipes();
    a_bad_hello_is_refused_and_answered_with_nothing(client, host, NOT_A_HELLO, not_this_protocol)
        .await;
}

#[tokio::test]
async fn over_a_pair_of_pipes_a_garbled_hello_is_refused() {
    let (client, host) = pipes();
    a_bad_hello_is_refused_and_answered_with_nothing(client, host, &garbled_hello(), malformed)
        .await;
}

#[tokio::test]
async fn a_child_refuses_a_garbled_hello_and_closes_its_end() {
    let mut child = peer("1.0").spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&garbled_hello()).await.unwrap();
    let answered = until_closed(child.stdout.take().unwrap()).await;
    assert!(
        answered.is_empty(),
        "a refusal accepts nothing: {answered:?}"
    );
    let (status, said) = exited(child).await;
    assert!(!status.success());
    assert!(said.contains("cannot be read"), "{said}");
    drop(stdin);
}

#[tokio::test]
async fn a_child_reads_a_hello_from_a_later_minor_and_ignores_what_it_does_not_know() {
    // The module's promise: fields a version does not know are ignored, so a later minor may
    // add some. A client of 1.9 with a field 1.0 never heard of still gets 1.0.
    let mut child = peer("1.0").spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(&framed(
            br#"{"versions":[{"major":1,"minor":9,"since":"1.9"}],"wants":["resume"]}"#,
        ))
        .await
        .unwrap();
    let answered = until_closed(child.stdout.take().unwrap()).await;
    assert_eq!(answered, framed(br#"{"accept":{"major":1,"minor":0}}"#));
    let (status, said) = exited(child).await;
    assert!(status.success(), "{said}");
    drop(stdin);
}

/// A child that is not a charter peer, held open after it speaks so that the client refuses on
/// what it said and not on the stream ending: a login shell's banner where `charter bridge`
/// should be, or something that only looks like a host.
fn a_child_that_says(what: &str) -> Child {
    Command::new("sh")
        .arg("-c")
        .arg(format!("printf '{what}'; exec cat >/dev/null"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[tokio::test]
async fn a_client_refuses_a_child_that_is_not_a_charter_peer() {
    let mut child = a_child_that_says("Last login: Thu Oct  1 09:00:00 2026\\r\\n");
    let mut stdio = join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let refused = timeout(CLOSES_WITHIN, offer(&mut stdio, &v1()))
        .await
        .expect("the client decided")
        .unwrap_err();
    assert!(not_this_protocol(&refused), "{refused:?}");
}

#[tokio::test]
async fn a_client_refuses_a_child_whose_answer_is_garbled() {
    // The magic, a frame of three bytes, and the three bytes are not an answer.
    let mut child = a_child_that_says("\\211CSP\\r\\n\\032\\n\\000\\003abc");
    let mut stdio = join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let refused = timeout(CLOSES_WITHIN, offer(&mut stdio, &v1()))
        .await
        .expect("the client decided")
        .unwrap_err();
    assert!(malformed(&refused), "{refused:?}");
}
