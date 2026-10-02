//! Is the other end of a socket, or a directory or file on disk, this user's alone?
//!
//! Every local trust boundary charter draws between one user and the rest of the machine rests
//! on the checks here, and only here, so they cannot drift apart (FD-6, ADR 0068 §5):
//!
//! - [`admit_peer`]: a connection on a unix socket is read only when its peer runs as the
//!   socket's owner. `charterd.sock` (`charter_session_protocol::local`) and each plane's hook
//!   socket (`charter_core::hookwire`) both call it.
//! - [`private_directory`]: a directory made, or found, `0700` and owned by this user, never a
//!   link, for a socket or for credentials to sit in.
//! - [`read_private_file`]: a file read only when it is a regular file of this user's that
//!   nobody else may read or write, never a link.
//!
//! **An unmapped uid is nobody's.** Inside a user namespace, a uid the namespace does not map
//! reads as the overflow uid, 65534 by default, whoever it really is; `(uid_t)-1` is no uid at
//! all. Two unmapped processes would compare equal without being the same user, so a peer or
//! an owner that is either is refused, never matched ([`Uid::is_unmapped`]).
//!
//! Windows is not ported yet (ADR 0068, *Later decisions*), and nothing here is built there.

#![cfg(unix)]

use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// A user id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uid(u32);

impl Uid {
    /// The overflow uid: what a uid outside a user namespace's map reads as (Linux's default
    /// `/proc/sys/kernel/overflowuid`, and `nobody` on most systems).
    pub const OVERFLOW: Uid = Uid(65534);

    /// This process's effective uid.
    pub fn effective() -> Uid {
        Uid(nix::unistd::geteuid().as_raw())
    }

    /// The uid with this number.
    pub fn from_raw(raw: u32) -> Uid {
        Uid(raw)
    }

    /// The uid's number.
    pub fn as_raw(self) -> u32 {
        self.0
    }

    /// Whether this uid names nobody in particular: the overflow uid, or `(uid_t)-1`.
    pub fn is_unmapped(self) -> bool {
        self == Uid::OVERFLOW || self.0 == u32::MAX
    }
}

impl std::fmt::Display for Uid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The uid of the process at the other end of the unix socket `socket`.
pub fn peer_of(socket: &impl AsFd) -> io::Result<Uid> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let uid = nix::sys::socket::getsockopt(socket, nix::sys::socket::sockopt::PeerCredentials)
        .map(|credentials| credentials.uid());
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let uid = nix::unistd::getpeereid(socket).map(|(uid, _)| uid.as_raw());
    uid.map(Uid).map_err(io::Error::from)
}

/// Why a connection was closed unread.
#[derive(Debug, thiserror::Error)]
pub enum NotThisUser {
    /// The peer runs as another uid.
    #[error("its peer runs as uid {peer}, and this socket serves uid {owner} only")]
    OtherUid { peer: Uid, owner: Uid },
    /// The peer, or this end, is an unmapped uid, which is nobody in particular.
    #[error("uid {uid} is unmapped, so it names nobody in particular")]
    Unmapped { uid: Uid },
    /// The socket would not say who the peer is.
    #[error("its peer could not be identified: {0}")]
    Unidentified(io::Error),
}

/// `Ok` when the peer `identified` as is `owner`, and otherwise why not. A peer that could not
/// be identified is refused, and so is an unmapped uid on either end.
pub fn admit_peer(identified: io::Result<Uid>, owner: Uid) -> Result<(), NotThisUser> {
    let peer = identified.map_err(NotThisUser::Unidentified)?;
    if owner.is_unmapped() {
        return Err(NotThisUser::Unmapped { uid: owner });
    }
    if peer.is_unmapped() {
        return Err(NotThisUser::Unmapped { uid: peer });
    }
    if peer != owner {
        return Err(NotThisUser::OtherUid { peer, owner });
    }
    Ok(())
}

/// Makes `dir` this user's alone: created `0700`, or, when it is already there, refused unless
/// it is a directory (never a link to one) that this user owns, and then set to `0700`.
///
/// **The check and the `chmod` are made on one open descriptor**, opened with `O_NOFOLLOW`, so
/// the last component cannot be swapped for a link between them. The components above it are
/// walked by path: whoever can write to `dir`'s parent can replace `dir` itself, so the parent
/// must already be this user's (a runtime directory, the machine store, or a directory the
/// caller has made private). Its parents are created when missing.
pub fn private_directory(dir: &Path) -> io::Result<()> {
    private_directory_of(dir, Uid::effective())
}

fn private_directory_of(dir: &Path, owner: Uid) -> io::Result<()> {
    if let Some(above) = dir.parent() {
        std::fs::create_dir_all(above)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err),
    }
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_DIRECTORY | nix::libc::O_CLOEXEC)
        .open(dir)
        .map_err(|err| refused_open(dir, "a directory", err))?;
    let found = opened.metadata()?;
    if !found.is_dir() {
        return Err(not_private(dir, "is not a directory"));
    }
    owned_by(dir, &found, owner)?;
    opened.set_permissions(std::fs::Permissions::from_mode(0o700))
}

/// The most [`read_private_file`] reads. What it is for is a credential, a few dozen bytes; a
/// file past this is not one, and is refused rather than read into memory.
pub const A_PRIVATE_FILE_IS_AT_MOST: u64 = 64 * 1024;

/// The whole of `path`, read only when it is a regular file, not a link, owned by this user,
/// that nobody else may read or write, and no longer than [`A_PRIVATE_FILE_IS_AT_MOST`].
pub fn read_private_file(path: &Path) -> io::Result<String> {
    read_private_file_of(path, Uid::effective())
}

fn read_private_file_of(path: &Path, owner: Uid) -> io::Result<String> {
    let mut opened = File::options()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC | nix::libc::O_NONBLOCK)
        .open(path)
        .map_err(|err| refused_open(path, "a file", err))?;
    let found = opened.metadata()?;
    if !found.is_file() {
        return Err(not_private(path, "is not a regular file"));
    }
    owned_by(path, &found, owner)?;
    if found.mode() & 0o077 != 0 {
        return Err(not_private(
            path,
            &format!(
                "may be read or written by others (mode {:o})",
                found.mode() & 0o777
            ),
        ));
    }
    let mut text = String::new();
    (&mut opened)
        .take(A_PRIVATE_FILE_IS_AT_MOST + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > A_PRIVATE_FILE_IS_AT_MOST {
        return Err(not_private(
            path,
            &format!("is longer than {A_PRIVATE_FILE_IS_AT_MOST} bytes"),
        ));
    }
    Ok(text)
}

fn owned_by(path: &Path, found: &std::fs::Metadata, owner: Uid) -> io::Result<()> {
    let by = Uid(found.uid());
    if owner.is_unmapped() || by.is_unmapped() {
        return Err(not_private(path, "is owned by an unmapped uid"));
    }
    if by != owner {
        return Err(not_private(
            path,
            &format!("is owned by uid {by}, not by uid {owner}"),
        ));
    }
    Ok(())
}

/// `O_NOFOLLOW` on a link fails with `ELOOP` on Linux and macOS, which reads better as what it
/// means.
fn refused_open(path: &Path, what: &str, err: io::Error) -> io::Error {
    if err.raw_os_error() == Some(nix::libc::ELOOP) {
        not_private(path, &format!("is a link, not {what}"))
    } else {
        err
    }
}

fn not_private(path: &Path, why: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{} {why}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ours() -> Uid {
        Uid::effective()
    }

    fn other() -> Uid {
        Uid(ours().0.wrapping_add(1))
    }

    #[test]
    fn a_peer_of_the_owners_uid_is_admitted() {
        assert!(admit_peer(Ok(ours()), ours()).is_ok());
    }

    #[test]
    fn a_peer_of_another_uid_is_refused() {
        assert!(matches!(
            admit_peer(Ok(other()), ours()),
            Err(NotThisUser::OtherUid { .. })
        ));
    }

    #[test]
    fn a_peer_the_socket_cannot_identify_is_refused() {
        assert!(matches!(
            admit_peer(Err(io::Error::other("no")), ours()),
            Err(NotThisUser::Unidentified(_))
        ));
    }

    #[test]
    fn an_unmapped_uid_is_refused_even_when_both_ends_are_it() {
        for unmapped in [Uid::OVERFLOW, Uid(u32::MAX)] {
            assert!(matches!(
                admit_peer(Ok(unmapped), unmapped),
                Err(NotThisUser::Unmapped { .. })
            ));
            assert!(matches!(
                admit_peer(Ok(unmapped), ours()),
                Err(NotThisUser::Unmapped { .. })
            ));
        }
    }

    #[test]
    fn the_peer_of_a_real_socket_is_this_process() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        assert_eq!(peer_of(&a).unwrap(), ours());
    }

    fn mode(path: &Path) -> u32 {
        std::fs::symlink_metadata(path).unwrap().mode() & 0o777
    }

    #[test]
    fn a_private_directory_is_made_0700_and_one_left_open_is_closed_again() {
        let home = tempfile::tempdir().unwrap();
        let made = home.path().join("deep").join("made");
        private_directory(&made).unwrap();
        assert_eq!(mode(&made), 0o700);

        let open = home.path().join("open");
        std::fs::DirBuilder::new()
            .mode(0o755)
            .create(&open)
            .unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        private_directory(&open).unwrap();
        assert_eq!(mode(&open), 0o700);
    }

    #[test]
    fn a_link_standing_in_for_a_private_directory_is_refused_and_its_target_untouched() {
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join("target");
        std::fs::DirBuilder::new()
            .mode(0o755)
            .create(&target)
            .unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = home.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(private_directory(&link).is_err());
        assert_eq!(mode(&target), 0o755);
    }

    #[test]
    fn a_directory_another_uid_owns_is_refused_explicitly_before_any_chmod() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("theirs");
        std::fs::DirBuilder::new().mode(0o755).create(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        let refused = private_directory_of(&dir, other()).unwrap_err();

        assert!(refused.to_string().contains("owned by uid"), "{refused}");
        assert_eq!(mode(&dir), 0o755, "it was changed before it was refused");
    }

    #[test]
    fn a_file_in_its_place_is_refused_as_a_private_directory() {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("file");
        std::fs::write(&file, "").unwrap();
        assert!(private_directory(&file).is_err());
    }

    fn private_file(dir: &Path, text: &str) -> std::path::PathBuf {
        let path = dir.join("secret");
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path
    }

    #[test]
    fn a_private_file_of_this_user_is_read() {
        let home = tempfile::tempdir().unwrap();
        let path = private_file(home.path(), "abc");
        assert_eq!(read_private_file(&path).unwrap(), "abc");
    }

    #[test]
    fn a_private_file_longer_than_the_cap_is_refused_and_one_at_the_cap_is_read() {
        let home = tempfile::tempdir().unwrap();
        let cap = usize::try_from(A_PRIVATE_FILE_IS_AT_MOST).unwrap();
        let path = private_file(home.path(), &"a".repeat(cap));
        assert_eq!(read_private_file(&path).unwrap().len(), cap);

        let path = private_file(home.path(), &"a".repeat(cap + 1));
        let refused = read_private_file(&path).unwrap_err();
        assert!(refused.to_string().contains("longer than"), "{refused}");
    }

    #[test]
    fn a_file_others_may_read_or_write_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let path = private_file(home.path(), "abc");
        for open in [0o640, 0o604, 0o620, 0o602] {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(open)).unwrap();
            assert!(read_private_file(&path).is_err(), "{open:o} was read");
        }
    }

    #[test]
    fn a_link_to_a_private_file_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let path = private_file(home.path(), "abc");
        let link = home.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_private_file(&link).is_err());
    }

    #[test]
    fn a_file_another_uid_owns_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let path = private_file(home.path(), "abc");
        assert!(read_private_file_of(&path, other()).is_err());
        assert!(read_private_file_of(&path, Uid::OVERFLOW).is_err());
    }

    #[test]
    fn a_directory_or_a_missing_path_is_not_a_private_file() {
        let home = tempfile::tempdir().unwrap();
        assert!(read_private_file(home.path()).is_err());
        assert!(read_private_file(&home.path().join("missing")).is_err());
    }
}
