//! The local socket's first check: a connection from another uid is refused before it says
//! anything (FD-6, ADR 0068 §5).
//!
//! `charterd.sock` sits in a `0700` directory, which already keeps other users out. This check
//! still holds if that directory is ever wrong: the host reads the peer's uid from the socket
//! itself and closes a connection whose uid is not its own, unread. A peer whose uid cannot be
//! read is refused the same way, and so is an unmapped uid on either end. The checks are
//! `purlis_same_user`'s, the same ones each plane's hook socket makes.
//!
//! **The check is a type, not a step to remember.** [`Listener::accept`] hands on a
//! [`SameUser`], which only it can make, and [`crate::link::serve`] takes nothing else. A host
//! cannot serve a connection from `charterd.sock` that skipped the check.
//!
//! **The second check: is the peer inside a chat?** (FD-27, V16a, ADR 0068 §5.) A [`SameUser`]
//! also carries the peer's pid, read from the socket as it was accepted, and
//! [`crate::link::serve`] asks the host's [`Chats`] which chats it holds. A peer that is a chat's
//! program, is in a chat's session, or has a chat's program among its ancestors is refused
//! every scope on `charterd.sock`, all of them a person's, even when it proves a credential.
//! So is a peer whose ancestry or session cannot be read. This is the second layer; the first is
//! that a chat's sandbox denies it the credentials (ADR 0067 §5, class 3).

use purlis_same_user::Uid;
use tokio::net::{UnixListener, UnixStream};

pub use purlis_same_user::NotThisUser;

/// A connection whose peer runs as this host's own uid. Made only by [`Listener::accept`].
#[derive(Debug)]
pub struct SameUser {
    stream: UnixStream,
    pid: u32,
}

impl SameUser {
    /// The connection, for a host that serves it through [`crate::link::serve`].
    pub(crate) fn into_stream(self) -> UnixStream {
        self.stream
    }

    /// The peer's pid, as the socket recorded it when the peer connected.
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

/// The chats a host holds, for the check that no process inside one is admitted as a person's
/// scope.
pub trait Chats: Send + Sync + 'static {
    /// Every chat's program, by pid: each leads its own session (`portable-pty` starts it with
    /// `setsid`), so a process it started is in its session or below it.
    ///
    /// **Two promises a host keeps, or the check passes what it should not:**
    /// - a chat whose program has exited stays in the set while anything in its session lives,
    ///   since a survivor in that session is still inside the chat;
    /// - a program's pid is in the set from the moment the program can run, before its first
    ///   instruction, never only once the start has returned.
    fn programs(&self) -> Vec<u32>;
}

/// A host that holds no chats: no process is inside one.
pub struct NoChats;

impl Chats for NoChats {
    fn programs(&self) -> Vec<u32> {
        Vec::new()
    }
}

/// Which of `chats`' programs process `pid` runs inside, if any, read from the kernel off the
/// async runtime (`purlis_same_user::inside_a_chat`). An error is a doubt, and the caller
/// refuses.
pub(crate) async fn inside_a_chat<C: Chats>(
    pid: u32,
    chats: std::sync::Arc<C>,
) -> std::io::Result<Option<u32>> {
    let programs = chats.programs();
    if programs.is_empty() {
        return Ok(None);
    }
    tokio::task::spawn_blocking(move || {
        purlis_same_user::inside_a_chat(
            pid,
            &programs,
            purlis_same_user::Parents::of,
            purlis_same_user::session_of,
        )
    })
    .await
    .map_err(std::io::Error::other)?
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
            identify: purlis_same_user::peer_of,
        }
    }

    /// The next connection. One from another uid, from an unmapped uid, or from a peer the
    /// socket cannot identify (its uid or its pid), is closed before a byte of it is read, and
    /// is the `Err` inside, for the host to record before it accepts the next; the outer `Err`
    /// is the listening socket's own failure.
    pub async fn accept(&self) -> std::io::Result<Result<SameUser, NotThisUser>> {
        let (stream, _) = self.listener.accept().await?;
        Ok(
            purlis_same_user::admit_peer((self.identify)(&stream), self.owner).and_then(|()| {
                let (_, pid) = purlis_same_user::peer_process_of(&stream)
                    .map_err(NotThisUser::Unidentified)?;
                Ok(SameUser { stream, pid })
            }),
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

        let accepted = listener.accept().await.unwrap().expect("admitted");
        assert_eq!(
            accepted.pid(),
            std::process::id(),
            "the peer's pid is this test's"
        );
        let mut stream = accepted.stream;

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
        let (refused, client, _dir) = verdict(theirs, purlis_same_user::peer_of).await;

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
