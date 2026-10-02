//! A peer that opens a stream and then says nothing, or says half a hello, is refused when the
//! handshake's deadline passes, instead of holding the host's task forever (#817 review).

use charter_session_protocol::auth::{self, Credentials, Scope};
use charter_session_protocol::link::{self, LinkError};
use charter_session_protocol::version::{self, HANDSHAKE_TIMEOUT, Refused, Speaks, Version};
use tokio::io::{AsyncWriteExt, duplex};
use tokio::time::Instant;

/// One start of the host's credentials, which every link in these tests is admitted with.
static HELD: std::sync::LazyLock<Credentials> =
    std::sync::LazyLock::new(|| Credentials::mint().unwrap());

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test(start_paused = true)]
async fn a_client_that_never_sends_its_hello_is_refused_at_the_deadline() {
    let (_silent, mut b) = duplex(1024);
    let started = Instant::now();
    let refused = version::answer(&mut b, &v1()).await.unwrap_err();
    assert!(matches!(refused, Refused::TimedOut), "{refused:?}");
    assert_eq!(started.elapsed(), HANDSHAKE_TIMEOUT);
}

#[tokio::test(start_paused = true)]
async fn half_a_hello_is_refused_at_the_deadline() {
    let (mut a, mut b) = duplex(1024);
    a.write_all(version::MAGIC).await.unwrap();
    a.write_all(&[0, 40, b'{']).await.unwrap();
    let refused = version::answer(&mut b, &v1()).await.unwrap_err();
    assert!(matches!(refused, Refused::TimedOut), "{refused:?}");
}

#[tokio::test(start_paused = true)]
async fn a_host_that_never_answers_is_refused_by_the_client_at_the_deadline() {
    let (mut a, _silent) = duplex(1024);
    let refused = version::offer(&mut a, &v1()).await.unwrap_err();
    assert!(matches!(refused, Refused::TimedOut), "{refused:?}");
}

#[tokio::test(start_paused = true)]
async fn a_client_that_negotiates_and_never_opens_the_control_lane_is_refused() {
    let (mut a, b) = duplex(64 * 1024);
    let host = tokio::spawn(link::serve(b, v1(), &HELD));
    // The client negotiates and is admitted by hand, then opens nothing.
    let (_, io) = version::offer(&mut a, &v1()).await.unwrap();
    let _io = auth::present(io, Scope::LocalUi, HELD.of(Scope::LocalUi))
        .await
        .unwrap();
    let started = Instant::now();
    let refused = host.await.unwrap().err().unwrap();
    assert!(matches!(refused, LinkError::TimedOut), "{refused:?}");
    assert_eq!(started.elapsed(), HANDSHAKE_TIMEOUT);
}
