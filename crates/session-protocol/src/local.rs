//! The local socket's first check: a connection from another uid is refused before it says
//! anything (FD-6, ADR 0068 §5).
//!
//! `charterd.sock` sits in a `0700` directory, which already keeps other users out. This check
//! still holds if that directory is ever wrong: the host reads the peer's uid from the socket
//! itself and closes a connection whose uid is not its own, unread. A peer whose uid cannot be
//! read is refused the same way, and so is an unmapped uid on either end. The checks are
//! `charter_same_user`'s, the same ones each plane's hook socket makes.
//!
//! **The check is a type, not a step to remember.** [`Listener::accept`] hands on a
//! [`SameUser`], which only it can make, and [`crate::link::serve`] takes nothing else. A host
//! cannot serve a connection from `charterd.sock` that skipped the check.

use charter_same_user::Uid;
use tokio::net::{UnixListener, UnixStream};

pub use charter_same_user::NotThisUser;

/// A connection whose peer runs as this host's own uid. Made only by [`Listener::accept`].
#[derive(Debug)]
pub struct SameUser(UnixStream);

impl SameUser {
    /// The connection, for a host that serves it through [`crate::link::serve`].
    pub(crate) fn into_stream(self) -> UnixStream {
        self.0
    }
}

/// How a listener learns who the peer is. A function, so a test can stand in for a socket
/// that will not say, which no real socket can be made to do.
type Identify = fn(&UnixStream) -> std::io::Result<Uid>;

/// A listening socket that hands on only connections from its own uid.
pub struct Listener {
    listener: UnixListener,
    owner: Uid,
    identify: Identify,
}

impl Listener {
    /// `listener`, which hands on connections from this process's effective uid only.
    pub fn new(listener: UnixListener) -> Self {
        Listener {
            listener,
            owner: Uid::effective(),
            identify: charter_same_user::peer_of,
        }
    }

    /// The next connection. One from another uid, from an unmapped uid, or from a peer the
    /// socket cannot identify, is closed before a byte of it is read, and is the `Err` inside,
    /// for the host to record before it accepts the next; the outer `Err` is the listening
    /// socket's own failure.
    pub async fn accept(&self) -> std::io::Result<Result<SameUser, NotThisUser>> {
        let (stream, _) = self.listener.accept().await?;
        Ok(
            charter_same_user::admit_peer((self.identify)(&stream), self.owner)
                .map(|()| SameUser(stream)),
        )
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

        let mut stream = listener.accept().await.unwrap().expect("admitted").0;

        client.write_all(b"x").await.unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), b'x');
    }

    /// The listener's verdict on this test's own connection, with the owner and the way the
    /// peer is identified set by the test.
    async fn verdict(
        owner: Uid,
        identify: Identify,
    ) -> (Result<SameUser, NotThisUser>, UnixStream, tempfile::TempDir) {
        let (dir, path, listener) = listening();
        let listener = Listener {
            listener,
            owner,
            identify,
        };
        let client = UnixStream::connect(&path).await.unwrap();
        (listener.accept().await.unwrap(), client, dir)
    }

    async fn closed_unread(mut client: UnixStream) {
        let mut said = Vec::new();
        client.read_to_end(&mut said).await.unwrap();
        assert!(said.is_empty());
    }

    #[tokio::test]
    async fn a_connection_from_another_uid_is_closed_before_it_says_anything() {
        // No second user to connect as, so the listener is told it serves a uid this test is
        // not: this test's own connection is then the other user's.
        let ours = Uid::effective();
        let theirs = Uid::from_raw(ours.as_raw().wrapping_add(1));
        let (refused, client, _dir) = verdict(theirs, charter_same_user::peer_of).await;

        assert!(
            matches!(refused, Err(NotThisUser::OtherUid { peer, .. }) if peer == ours),
            "{refused:?}"
        );
        closed_unread(client).await;
    }

    #[tokio::test]
    async fn a_peer_the_socket_will_not_identify_is_closed_unread() {
        let (refused, client, _dir) = verdict(Uid::effective(), |_| {
            Err(std::io::Error::other("no peer credentials"))
        })
        .await;

        assert!(
            matches!(refused, Err(NotThisUser::Unidentified(_))),
            "{refused:?}"
        );
        closed_unread(client).await;
    }

    #[tokio::test]
    async fn an_unmapped_peer_is_refused_even_by_a_host_that_is_unmapped_too() {
        let (refused, client, _dir) = verdict(Uid::OVERFLOW, |_| Ok(Uid::OVERFLOW)).await;

        assert!(
            matches!(refused, Err(NotThisUser::Unmapped { .. })),
            "{refused:?}"
        );
        closed_unread(client).await;
    }
}
