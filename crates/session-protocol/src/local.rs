//! The local socket's first check: a connection from another uid is refused before it says
//! anything (FD-6, ADR 0068 §5).
//!
//! `charterd.sock` sits in a `0700` directory, which already keeps other users out. This check
//! still holds if that directory is ever wrong: the host reads the peer's uid from the socket
//! itself (`SO_PEERCRED` on Linux, `getpeereid` on macOS, through tokio's
//! [`UnixStream::peer_cred`]) and closes a connection whose uid is not its own, unread. A peer
//! whose uid cannot be read is refused the same way. The scope's credential ([`crate::auth`])
//! is the second check, on what is left.
//!
//! The hook channel makes the same check on its own socket (`hookwire::Listener`).

use tokio::net::{UnixListener, UnixStream};

/// Why a connection was closed unread.
#[derive(Debug, thiserror::Error)]
pub enum NotThisUser {
    /// The peer runs as another uid.
    #[error("a connection from uid {peer} was refused: this host serves uid {owner} only")]
    OtherUid { peer: u32, owner: u32 },
    /// The socket would not say who the peer is.
    #[error("a connection whose peer could not be identified was refused: {0}")]
    Unidentified(std::io::Error),
}

/// A listening socket that hands on only connections from its own uid.
pub struct Listener {
    listener: UnixListener,
    owner: u32,
}

impl Listener {
    /// `listener`, which hands on connections from this process's effective uid only.
    pub fn new(listener: UnixListener) -> Self {
        Listener {
            listener,
            owner: rustix::process::geteuid().as_raw(),
        }
    }

    /// The next connection. One from another uid, or from a peer the socket cannot identify,
    /// is closed before a byte of it is read, and is the `Err` inside, for the host to record
    /// before it accepts the next; the outer `Err` is the listening socket's own failure.
    pub async fn accept(&self) -> std::io::Result<Result<UnixStream, NotThisUser>> {
        let (stream, _) = self.listener.accept().await?;
        Ok(match stream.peer_cred() {
            Ok(peer) if peer.uid() == self.owner => Ok(stream),
            Ok(peer) => Err(NotThisUser::OtherUid {
                peer: peer.uid(),
                owner: self.owner,
            }),
            Err(err) => Err(NotThisUser::Unidentified(err)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn listening() -> (tempfile::TempDir, std::path::PathBuf, UnixListener) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("charterd.sock");
        let listener = UnixListener::bind(&path).unwrap();
        (dir, path, listener)
    }

    #[tokio::test]
    async fn a_connection_from_this_uid_is_handed_on() {
        let (_dir, path, listener) = listening();
        let listener = Listener::new(listener);
        let mut client = UnixStream::connect(&path).await.unwrap();

        let mut stream = listener.accept().await.unwrap().expect("admitted");

        client.write_all(b"x").await.unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), b'x');
    }

    #[tokio::test]
    async fn a_connection_from_another_uid_is_closed_before_it_says_anything() {
        // No second user to connect as, so the listener is told it serves a uid this test is
        // not: this test's own connection is then the other user's.
        let (_dir, path, listener) = listening();
        let ours = rustix::process::geteuid().as_raw();
        let listener = Listener {
            listener,
            owner: ours.wrapping_add(1),
        };
        let mut client = UnixStream::connect(&path).await.unwrap();

        let refused = listener.accept().await.unwrap();

        assert!(
            matches!(refused, Err(NotThisUser::OtherUid { peer, .. }) if peer == ours),
            "{refused:?}"
        );
        // Closed unread: the client finds the end of the stream at once.
        let mut said = Vec::new();
        client.read_to_end(&mut said).await.unwrap();
        assert!(said.is_empty());
    }
}
