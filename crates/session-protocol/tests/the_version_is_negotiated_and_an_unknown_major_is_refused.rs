//! Before anything else crosses a stream, the two ends agree on one version of the protocol,
//! and when they share no major version, both refuse: neither guesses (#643).

use purlis_session_protocol::version::{Refused, Speaks, Version, answer, offer};
use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

fn speaks(versions: &[(u16, u16)]) -> Speaks {
    Speaks::new(
        versions
            .iter()
            .map(|&(major, minor)| Version { major, minor }),
    )
}

async fn negotiate(
    client: Speaks,
    host: Speaks,
) -> (Result<Version, Refused>, Result<Version, Refused>) {
    let (a, b) = duplex(64 * 1024);
    let (client, host) = tokio::join!(offer(a, &client), answer(b, &host));
    (client.map(|(v, _)| v), host.map(|(v, _)| v))
}

#[tokio::test]
async fn both_ends_agree_on_the_highest_major_they_share_and_the_lower_minor_of_it() {
    let (client, host) = negotiate(speaks(&[(2, 3), (1, 9)]), speaks(&[(3, 0), (2, 1)])).await;
    assert_eq!(client.unwrap(), Version { major: 2, minor: 1 });
    assert_eq!(host.unwrap(), Version { major: 2, minor: 1 });
}

#[tokio::test]
async fn a_host_one_major_ahead_still_serves_a_client_of_the_previous_one() {
    // N−1 (ADR 0068 §4): the host speaks its own major and the one before it.
    let (client, host) = negotiate(speaks(&[(1, 4)]), speaks(&[(2, 0), (1, 7)])).await;
    assert_eq!(client.unwrap(), Version { major: 1, minor: 4 });
    assert_eq!(host.unwrap(), Version { major: 1, minor: 4 });
}

#[tokio::test]
async fn no_shared_major_is_refused_by_both_ends_and_says_what_each_speaks() {
    let (client, host) = negotiate(speaks(&[(3, 0)]), speaks(&[(2, 0), (1, 0)])).await;
    match host.unwrap_err() {
        Refused::NoSharedMajor { ours, theirs } => {
            assert_eq!(ours, [2, 1]);
            assert_eq!(theirs, [3]);
        }
        other => panic!("{other:?}"),
    }
    match client.unwrap_err() {
        Refused::ByPeer { speaks } => assert_eq!(speaks, [2, 1]),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_host_that_accepts_a_major_the_client_never_offered_is_refused_by_the_client() {
    // Fail closed on the client too: a host that answers with a version nobody offered is
    // not believed, whatever it says.
    let (mut a, mut b) = duplex(64 * 1024);
    let lying_host = async move {
        let mut hello = [0u8; 512];
        let _ = b.read(&mut hello).await.unwrap();
        let body = br#"{"accept":{"major":9,"minor":0}}"#;
        b.write_all(purlis_session_protocol::version::MAGIC)
            .await
            .unwrap();
        b.write_all(&(body.len() as u16).to_be_bytes())
            .await
            .unwrap();
        b.write_all(body).await.unwrap();
        b
    };
    let ours = speaks(&[(1, 0)]);
    let (client, _b) = tokio::join!(offer(&mut a, &ours), lying_host);
    match client.unwrap_err() {
        Refused::NotOffered { major } => assert_eq!(major, 9),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_stream_that_does_not_open_with_the_magic_is_refused_before_anything_is_parsed() {
    let (mut a, mut b) = duplex(64 * 1024);
    let host = speaks(&[(1, 0)]);
    let (host, _) = tokio::join!(answer(&mut b, &host), async {
        a.write_all(b"SSH-2.0-OpenSSH_9.9\r\n").await.unwrap();
        a.shutdown().await.unwrap();
    });
    assert!(
        matches!(host.unwrap_err(), Refused::NotThisProtocol),
        "an ssh banner is not a hello"
    );
}

#[tokio::test]
async fn a_hello_longer_than_the_limit_is_refused_without_reading_it() {
    let (mut a, mut b) = duplex(64 * 1024);
    let host = speaks(&[(1, 0)]);
    let (host, _) = tokio::join!(answer(&mut b, &host), async {
        a.write_all(purlis_session_protocol::version::MAGIC)
            .await
            .unwrap();
        a.write_all(&u16::MAX.to_be_bytes()).await.unwrap();
    });
    assert!(matches!(host.unwrap_err(), Refused::TooLong { .. }));
}

#[tokio::test]
async fn a_hello_that_is_not_the_expected_shape_is_refused() {
    let (mut a, mut b) = duplex(64 * 1024);
    let host = speaks(&[(1, 0)]);
    let (host, _) = tokio::join!(answer(&mut b, &host), async {
        let body = br#"{"speaks":"everything"}"#;
        a.write_all(purlis_session_protocol::version::MAGIC)
            .await
            .unwrap();
        a.write_all(&(body.len() as u16).to_be_bytes())
            .await
            .unwrap();
        a.write_all(body).await.unwrap();
    });
    assert!(matches!(host.unwrap_err(), Refused::Malformed(_)));
}

#[tokio::test]
async fn bytes_the_host_sent_right_after_its_answer_are_not_lost() {
    // The answer is read through a codec, which reads ahead. What it read past the answer is
    // the multiplexer's first bytes, and the stream handed back starts with them.
    let (a, mut b) = duplex(64 * 1024);
    let host = async move {
        let mut hello = [0u8; 512];
        let _ = b.read(&mut hello).await.unwrap();
        let body = br#"{"accept":{"major":1,"minor":0}}"#;
        let mut all = purlis_session_protocol::version::MAGIC.to_vec();
        all.extend_from_slice(&(body.len() as u16).to_be_bytes());
        all.extend_from_slice(body);
        all.extend_from_slice(b"after the answer");
        b.write_all(&all).await.unwrap();
        b
    };
    let ours = speaks(&[(1, 0)]);
    let (client, _b) = tokio::join!(offer(a, &ours), host);
    let (version, mut rest) = client.unwrap();
    assert_eq!(version, Version { major: 1, minor: 0 });
    let mut next = [0u8; 16];
    rest.read_exact(&mut next).await.unwrap();
    assert_eq!(&next, b"after the answer");
}
