//! After the negotiation, one stream carries many: a control lane the client opens first, and
//! any number of streams either end opens, each with its own flow control (ADR 0068 §4).

use bytes::Bytes;
use charter_session_protocol::auth::{Credentials, Scope};
use charter_session_protocol::link::{self, LinkError};
use charter_session_protocol::version::{Refused, Speaks, Version};
use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

/// One start of the host's credentials, which every link in these tests is admitted with.
static HELD: std::sync::LazyLock<Credentials> =
    std::sync::LazyLock::new(|| Credentials::mint().unwrap());

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test]
async fn control_frames_arrive_whole_and_in_order_both_ways() {
    let (a, b) = duplex(64 * 1024);
    let (client, host) = tokio::join!(
        link::connect(a, v1(), Scope::LocalUi, HELD.of(Scope::LocalUi)),
        link::serve(b, v1(), &HELD)
    );
    let (mut client, mut host) = (client.unwrap(), host.unwrap());
    assert_eq!(client.version(), Version { major: 1, minor: 0 });
    assert_eq!(host.version(), Version { major: 1, minor: 0 });

    // A frame far larger than the transport's own buffer still arrives as one frame.
    let big = Bytes::from(vec![7u8; 200 * 1024]);
    client
        .control()
        .send(Bytes::from_static(b"list"))
        .await
        .unwrap();
    client.control().send(big.clone()).await.unwrap();
    assert_eq!(
        host.control().next().await.unwrap().unwrap(),
        Bytes::from_static(b"list")
    );
    assert_eq!(host.control().next().await.unwrap().unwrap(), big);

    host.control()
        .send(Bytes::from_static(b"chats: 0"))
        .await
        .unwrap();
    assert_eq!(
        client.control().next().await.unwrap().unwrap(),
        Bytes::from_static(b"chats: 0")
    );
}

#[tokio::test]
async fn a_stream_one_end_opens_is_one_the_other_end_accepts() {
    let (a, b) = duplex(64 * 1024);
    let (client, host) = tokio::join!(
        link::connect(a, v1(), Scope::LocalUi, HELD.of(Scope::LocalUi)),
        link::serve(b, v1(), &HELD)
    );
    let (mut client, mut host) = (client.unwrap(), host.unwrap());

    let mut opened = host.open().await.unwrap();
    opened.write_all(b"\x1b[31mred\x1b[0m").await.unwrap();
    opened.flush().await.unwrap();
    let mut accepted = client.accept().await.unwrap();
    let mut read = [0u8; 12];
    accepted.read_exact(&mut read).await.unwrap();
    assert_eq!(
        &read, b"\x1b[31mred\x1b[0m",
        "terminal bytes cross raw, ESC and all"
    );

    // And the other way, a stream the client opens.
    let mut up = client.open().await.unwrap();
    up.write_all(b"pack").await.unwrap();
    up.flush().await.unwrap();
    let mut down = host.accept().await.unwrap();
    let mut read = [0u8; 4];
    down.read_exact(&mut read).await.unwrap();
    assert_eq!(&read, b"pack");
}

#[tokio::test]
async fn a_refused_negotiation_starts_no_link() {
    let (a, b) = duplex(64 * 1024);
    let (client, host) = tokio::join!(
        link::connect(
            a,
            Speaks::new([Version { major: 2, minor: 0 }]),
            Scope::LocalUi,
            HELD.of(Scope::LocalUi)
        ),
        link::serve(b, v1(), &HELD)
    );
    assert!(matches!(
        client.err().unwrap(),
        LinkError::Refused(Refused::ByPeer { .. })
    ));
    assert!(matches!(
        host.err().unwrap(),
        LinkError::Refused(Refused::NoSharedMajor { .. })
    ));
}

#[tokio::test]
async fn a_control_frame_past_the_limit_is_refused_and_the_lane_stays_usable() {
    let (a, b) = duplex(64 * 1024);
    let (client, host) = tokio::join!(
        link::connect(a, v1(), Scope::LocalUi, HELD.of(Scope::LocalUi)),
        link::serve(b, v1(), &HELD)
    );
    let (mut client, mut host) = (client.unwrap(), host.unwrap());
    let too_big = Bytes::from(vec![0u8; link::MOST_CONTROL_FRAME_BYTES + 1]);
    assert!(
        client.control().send(too_big).await.is_err(),
        "the sender refuses it"
    );
    // The host's lane stays usable for frames that fit.
    client
        .control()
        .send(Bytes::from_static(b"ok"))
        .await
        .unwrap();
    assert_eq!(
        host.control().next().await.unwrap().unwrap(),
        Bytes::from_static(b"ok")
    );
}

#[tokio::test]
async fn a_link_the_multiplexer_gave_up_on_says_why() {
    // The host negotiates and admits the client, then sends bytes that are not Yamux. The
    // client's link ends, and what it answers next names the multiplexer's reason rather than
    // only "closed".
    let (a, b) = duplex(64 * 1024);
    let host = tokio::spawn(async move {
        let (_, rest) = charter_session_protocol::version::answer(b, &v1())
            .await
            .unwrap();
        let (_, mut rest) = charter_session_protocol::auth::admit(rest, &HELD)
            .await
            .unwrap();
        // Wait for the client's control lane to arrive, so its link is up first.
        let mut lane = [0u8; 13];
        rest.read_exact(&mut lane).await.unwrap();
        rest.write_all(&[0xff; 64]).await.unwrap();
        rest.flush().await.unwrap();
        rest
    });
    let mut client = link::connect(a, v1(), Scope::LocalUi, HELD.of(Scope::LocalUi))
        .await
        .unwrap();
    let _host = host.await.unwrap();
    match client.accept().await {
        Err(LinkError::Ended(why)) => assert!(why.contains("decode"), "{why}"),
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("a stream from bytes that are not Yamux"),
    }
}
