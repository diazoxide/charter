//! What the tests share: one start of the host's credentials, and the admission's frames
//! written and read by hand, for the tests that play a client or a host the crate would not.

// Each test file uses some of these.
#![allow(dead_code)]

use charter_session_protocol::auth::Credentials;
use charter_session_protocol::version::MAGIC;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// One start of the host's credentials, which every link in these tests is admitted with.
pub static HELD: std::sync::LazyLock<Credentials> =
    std::sync::LazyLock::new(|| Credentials::mint().unwrap());

/// Writes one message as the handshake frames it: the magic, a big-endian `u16` length, JSON.
pub async fn write_frame(io: &mut (impl AsyncWrite + Unpin), json: &str) {
    io.write_all(MAGIC).await.unwrap();
    io.write_all(&u16::try_from(json.len()).unwrap().to_be_bytes())
        .await
        .unwrap();
    io.write_all(json.as_bytes()).await.unwrap();
    io.flush().await.unwrap();
}

/// Reads one message the handshake framed, as JSON.
pub async fn read_frame(io: &mut (impl AsyncRead + Unpin)) -> serde_json::Value {
    let mut magic = [0u8; 8];
    io.read_exact(&mut magic).await.unwrap();
    assert_eq!(&magic, MAGIC);
    let length = io.read_u16().await.unwrap();
    let mut body = vec![0u8; usize::from(length)];
    io.read_exact(&mut body).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}
