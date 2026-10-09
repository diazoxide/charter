//! Is the other end of a socket, or a directory or file on disk, this user's alone?
//!
//! Every local trust boundary charter draws between one user and the rest of the machine rests
//! on the checks here, and only here, so they cannot drift apart (FD-6, ADR 0068 §5):
//!
//! - [`admit_peer`]: a connection on a unix socket is read only when its peer runs as the
//!   socket's owner. `charterd.sock` (`purlis_session_protocol::local`) and each plane's hook
//!   socket (`purlis_core::hookwire`) both call it.
//! - [`private_directory`]: a directory made, or found, `0700` and owned by this user, never a
//!   link, for a socket or for credentials to sit in.
//! - [`read_private_file`]: a file read only when it is a regular file of this user's that
//!   nobody else may read or write, never a link.
//! - [`admit_host`]: the other way round, for a client whose reply carries authority (HP-6's
//!   permission hook): the process listening must be this user's, this process's ancestor,
//!   and hold the socket at the path now.
//! - [`inside_a_chat`]: whether a process runs inside a chat (its program, its session, or
//!   below it), for `charterd`'s refusal of a person's scope to a chat's processes (FD-27).
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

mod ancestry;
pub use ancestry::{
    MOST_GENERATIONS, NotOurHost, Parents, admit_host, holds_a_socket_at, inside_a_chat, judge,
    listening_at, lsof_command, lsof_names, session_of, stat_parent, walk,
};

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

/// This account's home directory, as the user database records it (`getpwuid`), never as an
/// environment says: a process can be started with any `$HOME`, and a caller that must find
/// this user's own files whatever its environment was given asks here. `None` where the
/// database has no entry for this uid, or no home in it.
pub fn account_home() -> Option<std::path::PathBuf> {
    nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .map(|user| user.dir)
        .filter(|dir| dir.is_absolute())
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

/// The process at the other end of the unix socket `socket`: its uid, and its pid as the kernel
/// recorded it for the socket (`SO_PEERCRED` on Linux; `LOCAL_PEERCRED` and `LOCAL_PEERPID` on
/// macOS). For a connection a client made, that is the process that is listening.
pub fn peer_process_of(socket: &impl AsFd) -> io::Result<(Uid, u32)> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let peer = nix::sys::socket::getsockopt(socket, nix::sys::socket::sockopt::PeerCredentials)
        .map(|credentials| (credentials.uid(), credentials.pid()));
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let peer = nix::sys::socket::getsockopt(socket, nix::sys::socket::sockopt::LocalPeerCred)
        .and_then(|credentials| {
            nix::sys::socket::getsockopt(socket, nix::sys::socket::sockopt::LocalPeerPid)
                .map(|pid| (credentials.uid(), pid))
        });
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    )))]
    let peer: nix::Result<(u32, i32)> = Err(nix::errno::Errno::ENOTSUP);
    let (uid, pid) = peer.map_err(io::Error::from)?;
    let pid = u32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| io::Error::other("the socket named no peer process"))?;
    Ok((Uid(uid), pid))
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
    fn this_account_s_home_is_the_user_database_s_and_absolute() {
        let home = account_home().expect("this account has a home");
        assert!(home.is_absolute(), "{}", home.display());
    }

    fn run(command: &mut std::process::Command) -> io::Result<std::process::Output> {
        command.output()
    }

    #[test]
    fn this_process_s_ancestors_start_with_its_parent() {
        let chain = Parents::chain(std::process::id());
        assert_eq!(chain.first(), Some(&std::os::unix::process::parent_id()));
        assert!(!chain.contains(&std::process::id()));
    }

    #[test]
    fn a_socket_this_process_listens_on_is_never_its_own_host() {
        // A hook only connects; the self shortcut a pid could be recycled into is gone.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("s.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).expect("bound");
        let client = std::os::unix::net::UnixStream::connect(&path).expect("connects");
        let (uid, pid) = peer_process_of(&client).expect("identified");
        assert_eq!((uid, pid), (Uid::effective(), std::process::id()));
        assert!(matches!(
            admit_host(&client, &path, run),
            Err(NotOurHost::NotAnAncestor { .. })
        ));
    }

    /// Where [`a_child_that_listens`] binds, when this test binary is run as that child.
    const LISTEN_AT: &str = "CHARTER_SAME_USER_TEST_LISTEN_AT";

    /// Not a test: this binary, run again as a child of the test that wants a listener which
    /// is no ancestor of its own, binds at [`LISTEN_AT`], says so, and waits to be killed.
    #[test]
    #[ignore = "a helper the tests below start as a child"]
    fn a_child_that_listens() {
        let Some(path) = std::env::var_os(LISTEN_AT) else {
            return;
        };
        let _listener = std::os::unix::net::UnixListener::bind(path).expect("bound");
        println!("up");
        std::thread::sleep(std::time::Duration::from_secs(30));
    }

    #[test]
    fn a_socket_a_child_listens_on_is_no_host() {
        // A process started below this one, the shape of anything a chat runs, binds the path.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("s.sock");
        let mut child = std::process::Command::new(std::env::current_exe().expect("this test"))
            .args([
                "--exact",
                "tests::a_child_that_listens",
                "--ignored",
                "--nocapture",
            ])
            .env(LISTEN_AT, &path)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the child starts");
        let mut said = std::io::BufReader::new(child.stdout.take().expect("piped"));
        let mut line = String::new();
        while !line.contains("up") {
            line.clear();
            if std::io::BufRead::read_line(&mut said, &mut line).expect("it speaks") == 0 {
                break;
            }
        }
        let client = std::os::unix::net::UnixStream::connect(&path).expect("connects");

        let refused = admit_host(&client, &path, run);

        let _ = child.kill();
        let _ = child.wait();
        assert!(
            matches!(refused, Err(NotOurHost::NotAnAncestor { .. })),
            "{refused:?}"
        );
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
