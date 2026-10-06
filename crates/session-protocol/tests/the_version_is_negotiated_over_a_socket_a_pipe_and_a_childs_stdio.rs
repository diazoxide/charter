//! The protocol assumes nothing of its transport but an ordered byte stream (ADR 0068 §4):
//! the same negotiation over a unix socket, a pair of pipes, and a child's stdin and stdout,
//! which is how a connector such as `ssh` reaches a runner (ADR 0078 §2). #643's first
//! acceptance line.
//!
//! Over each of the three, the same outcomes, at the host and at the client:
//!
//! - **a version both speak** is agreed by both ends;
//! - **no shared major** is refused by both ends, each saying what the other speaks, and the
//!   host's refusal ends the stream;
//! - **a stream that is not this protocol** (an ssh banner, a login shell's greeting) is
//!   refused before anything is parsed, and answered with nothing;
//! - **a garbled message** (the magic, then a frame that is not a hello or an answer) is
//!   refused the same way, and so is **one longer than the limit**;
//! - **a peer that goes silent, or stops partway through**, is refused at the handshake's
//!   deadline, on a paused clock.
//!
//! Every refusing end closes its side without waiting for the other to hang up, and every wait
//! here is bounded, so a peer that lingers fails a test instead of hanging it. A hello from a
//! later minor, with fields this version does not know, is still read (the forward half of
//! the promise in `version`'s docs).

// Unix sockets and pipes. Windows is not ported yet (ADR 0068, *Later decisions*).
#![cfg(unix)]

use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use purlis_session_protocol::version::{
    HANDSHAKE_TIMEOUT, MAGIC, Refused, Speaks, Version, answer, offer,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Join, join};
use tokio::net::unix::pipe;
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, ChildStdin, Command};
use tokio::time::{Instant, timeout};

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

/// What an ssh server says first: a stream from something that is not charter.
const NOT_A_HELLO: &[u8] = b"SSH-2.0-OpenSSH_9.9\r\n";

/// How long a refusing end may take to close its side. Generous: it is a hang we look for.
const CLOSES_WITHIN: Duration = Duration::from_secs(5);

/// A message in its frame, as an end of any version writes it: the magic, a big-endian `u16`
/// length, then the body.
fn framed(body: &[u8]) -> Vec<u8> {
    let mut all = MAGIC.to_vec();
    all.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
    all.extend_from_slice(body);
    all
}

/// The magic and a well-framed body that is not JSON: a stream that began as this protocol
/// and was then damaged.
fn garbled() -> Vec<u8> {
    framed(b"\xff\xfe{versions: [1.0]")
}

/// The magic and a length past the limit, for a body that never needs to arrive.
fn too_long() -> Vec<u8> {
    let mut all = MAGIC.to_vec();
    all.extend_from_slice(&u16::MAX.to_be_bytes());
    all
}

/// The first half of a real hello: the peer stops partway through it.
fn half_a_hello() -> Vec<u8> {
    let hello = framed(br#"{"versions":[{"major":1,"minor":0}]}"#);
    hello[..hello.len() / 2].to_vec()
}

fn not_this_protocol(refused: &Refused) -> bool {
    matches!(refused, Refused::NotThisProtocol)
}

fn malformed(refused: &Refused) -> bool {
    matches!(refused, Refused::Malformed(_))
}

fn longer_than_the_limit(refused: &Refused) -> bool {
    matches!(refused, Refused::TooLong { .. })
}

// ---- The transports ----

/// Both ends of a unix socket, as the app and `charterd.sock` hold them.
async fn socket_pair() -> (UnixStream, UnixStream, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("charterd.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (client, accepted) = tokio::join!(UnixStream::connect(&path), listener.accept());
    (client.unwrap(), accepted.unwrap().0, dir)
}

/// One direction of a pipe, as tokio reads or writes it.
fn pipe_pair() -> (pipe::Receiver, pipe::Sender) {
    let (reader, writer) = std::io::pipe().unwrap();
    (
        pipe::Receiver::from_owned_fd(reader.into()).unwrap(),
        pipe::Sender::from_owned_fd(writer.into()).unwrap(),
    )
}

/// Both ends of a pair of pipes, one each way: the client's, then the host's.
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

/// The peer program, a host on its own stdin and stdout, as `charter bridge` will be at the far
/// end of a connector.
fn peer(speaks: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_charter-session-peer"))
        .arg("--speaks")
        .arg(speaks)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

/// A child that is not a charter peer: it writes `what` to its stdout, then keeps both stdin
/// and stdout open, so that a client refuses on what it said and not on the stream ending. A
/// login shell's banner where `charter bridge` should be, or something that only looks like a
/// host. The bytes go through a file, so nothing in them is ever read by the shell.
fn a_child_that_says(what: &[u8]) -> (Child, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let said = dir.path().join("said");
    std::fs::write(&said, what).unwrap();
    // No `exec` before the second `cat`: the shell stays, and holds its stdout open.
    let child = Command::new("sh")
        .arg("-c")
        .arg(r#"cat -- "$1"; cat >/dev/null"#)
        .arg("sh")
        .arg(&said)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    (child, dir)
}

/// The child's stdio as one stream, its stdout read once it has something to read. Waiting
/// here on a paused clock arms no timer, so the clock cannot run past the child's first bytes.
async fn stdio_once_it_speaks(child: &mut Child) -> Join<pipe::Receiver, ChildStdin> {
    let stdout = child.stdout.take().unwrap().into_owned_fd().unwrap();
    let stdout = pipe::Receiver::from_owned_fd(stdout).unwrap();
    stdout.readable().await.unwrap();
    join(stdout, child.stdin.take().unwrap())
}

/// The child's exit status and its stderr, once it has exited, within [`CLOSES_WITHIN`].
async fn exited(mut child: Child) -> (ExitStatus, String) {
    let status = timeout(CLOSES_WITHIN, child.wait())
        .await
        .expect("the child exited")
        .unwrap();
    let mut said = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        stderr.read_to_string(&mut said).await.unwrap();
    }
    (status, said)
}

/// Everything the other end writes until it closes. A unix socket closed with bytes unread in
/// it is reset on some kernels rather than ended, and a reset is a close too.
async fn until_closed<R: AsyncRead + Unpin>(mut from: R) -> Vec<u8> {
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

/// Whether the other end is still open: it does not end the stream within a short wait, once
/// whatever it already sent is drained.
async fn still_open<R: AsyncRead + Unpin>(mut from: R) -> bool {
    let mut drain = [0u8; 256];
    loop {
        match timeout(Duration::from_millis(200), from.read(&mut drain)).await {
            Err(_) => return true,
            Ok(Ok(0)) | Ok(Err(_)) => return false,
            Ok(Ok(_)) => {}
        }
    }
}

// ---- The bodies each transport runs ----

/// Both ends speak 1.0, and both agree on it.
async fn both_agree_on_the_version_they_share<C, H>(mut client: C, mut host: H)
where
    C: AsyncRead + AsyncWrite + Unpin,
    H: AsyncRead + AsyncWrite + Unpin,
{
    let client_side = async { offer(&mut client, &v1()).await.map(|(v, _)| v) };
    let host_side = async { answer(&mut host, &v1()).await.map(|(v, _)| v) };
    let (client_side, host_side) = tokio::join!(client_side, host_side);
    assert_eq!(client_side.unwrap(), Version { major: 1, minor: 0 });
    assert_eq!(host_side.unwrap(), Version { major: 1, minor: 0 });
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

/// A host that writes `said` instead of an answer, and keeps its end open: the client refuses
/// it with `expected`.
async fn a_bad_answer_is_refused_by_the_client<C, H>(
    mut client: C,
    mut host: H,
    said: &[u8],
    expected: fn(&Refused) -> bool,
) where
    C: AsyncRead + AsyncWrite + Unpin,
    H: AsyncRead + AsyncWrite + Unpin,
{
    host.write_all(said).await.unwrap();
    let refused = timeout(CLOSES_WITHIN, offer(&mut client, &v1()))
        .await
        .expect("the client decided")
        .err()
        .expect("the client refused");
    assert!(expected(&refused), "{refused:?}");
    drop(host);
}

/// A client that writes `first` and then nothing more, on a paused clock: the host refuses it
/// at the handshake's deadline, not before and not never.
async fn a_client_that_stalls_is_refused_at_the_deadline<C, H>(
    mut client: C,
    mut host: H,
    first: &[u8],
) where
    C: AsyncRead + AsyncWrite + Unpin,
    H: AsyncRead + AsyncWrite + Unpin,
{
    client.write_all(first).await.unwrap();
    let started = Instant::now();
    let refused = answer(&mut host, &v1())
        .await
        .err()
        .expect("the host refused");
    assert!(matches!(refused, Refused::TimedOut), "{refused:?}");
    assert_eq!(started.elapsed(), HANDSHAKE_TIMEOUT);
    drop(client);
}

// ---- Over a unix socket ----

#[tokio::test]
async fn over_a_unix_socket() {
    let (client, host, _dir) = socket_pair().await;
    both_agree_on_the_version_they_share(client, host).await;
}

#[tokio::test]
async fn over_a_unix_socket_no_shared_major_is_refused() {
    let (client, host, _dir) = socket_pair().await;
    no_shared_major_is_refused_by_both_and_the_stream_ends(client, host).await;
}

#[tokio::test]
async fn over_a_unix_socket_a_bad_hello_is_refused_by_the_host() {
    for (first, expected) in [
        (
            NOT_A_HELLO.to_vec(),
            not_this_protocol as fn(&Refused) -> bool,
        ),
        (garbled(), malformed),
        (too_long(), longer_than_the_limit),
    ] {
        let (client, host, _dir) = socket_pair().await;
        a_bad_hello_is_refused_and_answered_with_nothing(client, host, &first, expected).await;
    }
}

#[tokio::test]
async fn over_a_unix_socket_a_bad_answer_is_refused_by_the_client() {
    for (said, expected) in [
        (
            NOT_A_HELLO.to_vec(),
            not_this_protocol as fn(&Refused) -> bool,
        ),
        (garbled(), malformed),
        (too_long(), longer_than_the_limit),
    ] {
        let (client, host, _dir) = socket_pair().await;
        a_bad_answer_is_refused_by_the_client(client, host, &said, expected).await;
    }
}

#[tokio::test(start_paused = true)]
async fn over_a_unix_socket_a_client_that_goes_silent_or_stops_partway_is_refused_in_time() {
    for first in [Vec::new(), half_a_hello()] {
        let (client, host, _dir) = socket_pair().await;
        a_client_that_stalls_is_refused_at_the_deadline(client, host, &first).await;
    }
}

// ---- Over a pair of pipes ----

#[tokio::test]
async fn over_a_pair_of_pipes() {
    let (client, host) = pipes();
    both_agree_on_the_version_they_share(client, host).await;
}

#[tokio::test]
async fn over_a_pair_of_pipes_no_shared_major_is_refused() {
    let (client, host) = pipes();
    no_shared_major_is_refused_by_both_and_the_stream_ends(client, host).await;
}

#[tokio::test]
async fn over_a_pair_of_pipes_a_bad_hello_is_refused_by_the_host() {
    for (first, expected) in [
        (
            NOT_A_HELLO.to_vec(),
            not_this_protocol as fn(&Refused) -> bool,
        ),
        (garbled(), malformed),
        (too_long(), longer_than_the_limit),
    ] {
        let (client, host) = pipes();
        a_bad_hello_is_refused_and_answered_with_nothing(client, host, &first, expected).await;
    }
}

#[tokio::test]
async fn over_a_pair_of_pipes_a_bad_answer_is_refused_by_the_client() {
    for (said, expected) in [
        (
            NOT_A_HELLO.to_vec(),
            not_this_protocol as fn(&Refused) -> bool,
        ),
        (garbled(), malformed),
        (too_long(), longer_than_the_limit),
    ] {
        let (client, host) = pipes();
        a_bad_answer_is_refused_by_the_client(client, host, &said, expected).await;
    }
}

#[tokio::test(start_paused = true)]
async fn over_a_pair_of_pipes_a_client_that_goes_silent_or_stops_partway_is_refused_in_time() {
    for first in [Vec::new(), half_a_hello()] {
        let (client, host) = pipes();
        a_client_that_stalls_is_refused_at_the_deadline(client, host, &first).await;
    }
}

// ---- Over a child's stdio ----

#[tokio::test]
async fn over_a_childs_stdio() {
    let mut child = peer("2.1,1.3");
    let mut stdio = join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let ours = Speaks::new([Version { major: 1, minor: 5 }]);
    assert_eq!(
        offer(&mut stdio, &ours).await.unwrap().0,
        Version { major: 1, minor: 3 }
    );
    drop(stdio);
    let (status, said) = exited(child).await;
    assert!(status.success(), "{said}");
}

#[tokio::test]
async fn a_child_that_shares_no_major_refuses_and_says_so() {
    let mut child = peer("3.0");
    let mut stdio = join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
    match offer(&mut stdio, &v1()).await.unwrap_err() {
        Refused::ByPeer { speaks } => assert_eq!(speaks, [3]),
        other => panic!("{other:?}"),
    }
    drop(stdio);
    let (status, said) = exited(child).await;
    assert!(
        !status.success(),
        "a refused negotiation is a failure on the host too"
    );
    assert!(said.contains("no major version in common"), "{said}");
}

#[tokio::test]
async fn a_child_refuses_a_bad_hello_and_closes_its_end() {
    for (first, says) in [
        (
            NOT_A_HELLO.to_vec(),
            "not speaking purlis's session protocol",
        ),
        (garbled(), "cannot be read"),
        (too_long(), "longer than"),
    ] {
        let mut child = peer("1.0");
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&first).await.unwrap();
        // Our end stays open: the end that refuses must not wait for us to hang up.
        let answered = until_closed(child.stdout.take().unwrap()).await;
        assert!(
            answered.is_empty(),
            "a refusal accepts nothing: {answered:?}"
        );
        let (status, said) = exited(child).await;
        assert!(!status.success());
        assert!(said.contains(says), "{said}");
        drop(stdin);
    }
}

#[tokio::test]
async fn a_child_reads_a_hello_from_a_later_minor_and_ignores_what_it_does_not_know() {
    // The module's promise: fields a version does not know are ignored, so a later minor may
    // add some. A client of 1.9 with a field 1.0 never heard of still gets 1.0.
    let mut child = peer("1.0");
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

#[tokio::test]
async fn a_client_refuses_a_child_whose_answer_is_bad() {
    for (said, expected) in [
        // A login shell's greeting where `charter bridge` should be.
        (
            b"Last login: Thu Oct  1 09:00:00 2026\r\n".to_vec(),
            not_this_protocol as fn(&Refused) -> bool,
        ),
        (garbled(), malformed),
        (too_long(), longer_than_the_limit),
    ] {
        let (mut child, _dir) = a_child_that_says(&said);
        let mut stdio = stdio_once_it_speaks(&mut child).await;
        let refused = timeout(CLOSES_WITHIN, offer(&mut stdio, &v1()))
            .await
            .expect("the client decided")
            .unwrap_err();
        assert!(expected(&refused), "{refused:?}");
        assert!(
            still_open(&mut stdio).await,
            "the child still holds its end, so the refusal was on what it said"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_client_refuses_a_child_that_goes_silent_or_stops_partway_in_time() {
    let answer = framed(br#"{"accept":{"major":1,"minor":0}}"#);
    for said in [Vec::new(), answer[..answer.len() / 2].to_vec()] {
        let (mut child, _dir) = a_child_that_says(&said);
        let mut stdio = if said.is_empty() {
            join(
                pipe::Receiver::from_owned_fd(
                    child.stdout.take().unwrap().into_owned_fd().unwrap(),
                )
                .unwrap(),
                child.stdin.take().unwrap(),
            )
        } else {
            stdio_once_it_speaks(&mut child).await
        };
        let started = Instant::now();
        let refused = offer(&mut stdio, &v1()).await.unwrap_err();
        assert!(matches!(refused, Refused::TimedOut), "{refused:?}");
        assert_eq!(started.elapsed(), HANDSHAKE_TIMEOUT);
    }
}
