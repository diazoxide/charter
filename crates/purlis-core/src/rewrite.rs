//! Rewriting a committed plane file whole or not at all, one writer at a time.
//!
//! `charter.toml` and the plane's `.gitignore` are read whole, edited as text and written back
//! whole. A plain `std::fs::write` truncates first and writes second, so a process killed in
//! between, or a full disk, leaves the plane's identity file or the file that keeps `/.charter/`
//! out of git empty or cut short (#357, #358). And every writer rewrites the WHOLE file from
//! what it read, so two of them interleaving does not merge: the later one drops the earlier
//! one's change.
//!
//! This module answers both:
//!
//! - [`replace`] writes the new bytes to a temp file beside the target, flushes them to the
//!   disk and `rename`s the temp over the name. A reader opens the old inode or the new one and
//!   never a half of either; a writer killed before the rename leaves the old file intact.
//! - [`Lock`] serialises the read-modify-write. It is an advisory `flock` on the file's
//!   DIRECTORY, not on the file: the rename replaces the file's inode, so a lock on it would be
//!   a lock on a file that stops being the target the moment the first writer finishes (the
//!   reason `machine.rs` keeps a separate lock file). A directory's inode survives the rename,
//!   and locking it leaves nothing behind in the plane for `git status` to show.
//! - [`update`] is the two together, for the common shape.
//!
//! The lock is best effort, as `machine::Lock` is and for the same reason: a filesystem that
//! cannot `flock` is not a reason to refuse an edit, and what is lost without the lock is
//! exactly what was lost before it existed. It is taken blocking; the kernel releases a
//! `flock` when its descriptor closes, including in a killed process, so a stale lock cannot
//! wedge anything.
//!
//! **Never take a [`Lock`] while holding one on the same directory.** `flock` belongs to the
//! open file description, so a second `Lock::on` in the same process waits for the first.

use std::io;
use std::path::{Path, PathBuf};

/// Held for one read-modify-write of a file in `dir`.
pub struct Lock(Option<std::fs::File>);

impl Lock {
    /// The directory this lock is held on, open: `None` where it could not be taken (see the
    /// module header). A holder that must know the path still names the directory it locked,
    /// which a rename may have moved, compares the two (#1292).
    pub fn held(&self) -> Option<&std::fs::File> {
        self.0.as_ref()
    }

    /// Lock `dir`, blocking until any other holder lets go. Never fails; see the module header.
    pub fn on(dir: &Path) -> Self {
        #[cfg(unix)]
        {
            let Ok(file) =
                crate::contain::nofollow(std::fs::OpenOptions::new().read(true)).open(dir)
            else {
                return Self(None);
            };
            match rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive) {
                Ok(()) => Self(Some(file)),
                Err(_) => Self(None),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            Self(None)
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(file) = self.0.take() {
            #[cfg(unix)]
            let _ = rustix::fs::flock(&file, rustix::fs::FlockOperation::Unlock);
            drop(file);
        }
    }
}

/// Read `path`, hand its text to `change`, and write back what `change` returns — under a
/// [`Lock`] on the file's directory, through [`replace`]. `true` when it wrote.
///
/// `change` gets `None` for a file that is not there, and returns `Ok(None)` to leave the file
/// exactly as it is (nothing is rewritten). A file that is there but cannot be read, or is not
/// UTF-8, is an error and is never overwritten: text charter could not read is not text it
/// may replace.
pub fn update(
    root: &Path,
    path: &Path,
    change: impl FnOnce(Option<&str>) -> io::Result<Option<String>>,
) -> io::Result<bool> {
    let dir = directory_of(path)?;
    let _held = Lock::on(&dir);
    let now = match std::fs::read(path) {
        Ok(bytes) => Some(String::from_utf8(bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not UTF-8 text", path.display()),
            )
        })?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    match change(now.as_deref())? {
        Some(next) => replace(root, path, next.as_bytes(), Mode::Kept).map(|()| true),
        None => Ok(false),
    }
}

/// What the replaced file's permissions are. The one thing that differs between charter's
/// whole-file writers; everything else about [`replace`] is the same for all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A file somebody else may own the mode of — `charter.toml`, `.gitignore`,
    /// `workspace.json`, a generated `settings.json`. An existing file keeps its permissions,
    /// and one somebody made read-only is refused. A new file gets the umask's answer.
    Kept,
    /// [`Mode::Kept`], except that a new file is created 0600 — the Settings tab's
    /// files, which nobody else on the machine has any business reading.
    KeptOrPrivate,
    /// Charter's own state — a consent record, a hook's bookkeeping. 0600 whatever the file
    /// had, and a read-only one is replaced, since charter owns its mode. The chmod is best
    /// effort: a filesystem that cannot hold a mode (exFAT, many network mounts) still gets
    /// the write, as `config.py`'s `_private_fd` does.
    Private,
    /// A secret — a vault file. 0600 before a byte lands, or nothing is written at all: the
    /// error is a [`NotPrivate`] when the filesystem will not hold the mode. (Unix: on
    /// Windows there is no mode to set or read back, and the directory's ACL decides.) A read-only file
    /// is refused, as the in-place write this replaces refused it.
    Secret,
}

impl Mode {
    /// Whether a file somebody made read-only is refused rather than replaced.
    fn refuses_read_only(self) -> bool {
        self != Mode::Private
    }

    /// The permissions the finished file has, given the ones it has `now`; `None` leaves
    /// them to the umask.
    fn permissions(self, now: Option<std::fs::Permissions>) -> Option<std::fs::Permissions> {
        match (self, now) {
            (Mode::Kept | Mode::KeptOrPrivate, Some(now)) => Some(now),
            (Mode::Kept, None) => None,
            _ => private(),
        }
    }
}

/// Whether `permissions` let nobody but the owner in — a temp that will end so is created
/// that way, and never opened at a looser mode first.
fn owner_only(permissions: &std::fs::Permissions) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.mode() & 0o077 == 0
    }
    #[cfg(not(unix))]
    {
        let _ = permissions;
        false
    }
}

/// The file [`Mode::Secret`] was writing could not be made 0600, so nothing was written.
/// Carried inside the `io::Error` so a caller can word it for its own reader.
#[derive(Debug)]
pub struct NotPrivate {
    /// The permission bits the temp file had after charter asked for 0600.
    pub mode: u32,
}

impl std::fmt::Display for NotPrivate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "it is mode {:03o} and purlis could not make it 0600",
            self.mode
        )
    }
}

impl std::error::Error for NotPrivate {}

/// Every temp file [`replace`] writes starts with this, so one pattern names all of them:
/// `.purlis-generated.<name>.<pid>.<tag>.tmp` ([`crate::names::GENERATED_TEMP_PREFIX`]; an
/// older charter's were `.charter-generated.…`, and are still recognised). It is the prefix
/// the generated layer's temps have, which a guest checkout's exclude block hides
/// (`guest::TEMP_PATTERN`) and its "temps left" check looks for. It is
/// [`crate::layer::MARKER`] and a dot.
pub const TEMP_PREFIX: &str = crate::names::GENERATED_TEMP_PREFIX.write;

/// Replace `path` with `bytes`, whole or not at all.
///
/// - The path is gated as itself and every directory above it, up to `root`
///   ([`crate::contain::no_link_on_the_way`]): **a target that is a symlink is refused**, never
///   written through and never replaced. The temp file is created with `O_NOFOLLOW` through
///   the same gate, so neither write can be steered out of `root`. A caller with no tree to
///   gate passes the file's own directory, which gates the file alone.
/// - The permissions are `mode`'s (see [`Mode`]), and they are on the temp file before any
///   content, because a rename carries the source's mode. A temp that will end private is
///   also CREATED 0600, so no descriptor to it is ever opened at a looser mode.
/// - The temp file is flushed to the disk before the rename, and the directory after it, so a
///   power cut cannot leave the new name pointing at an empty inode.
/// - On any failure the temp file is removed and the target is left as it was.
pub fn replace(root: &Path, path: &Path, bytes: &[u8], mode: Mode) -> io::Result<()> {
    let dir = directory_of(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Hidden, and unique per call: the pid separates two processes and the tag two threads.
    let temp = dir.join(format!(
        "{TEMP_PREFIX}{name}.{}.{}.tmp",
        std::process::id(),
        crate::workspaces::scratch_tag()
    ));
    replace_through(root, path, &temp, bytes, mode)
}

/// [`replace`] with the temp file named by the caller — so a test can plant a link, or a
/// killed process's debris, at the path that is actually opened. The name [`replace`] picks
/// carries a pid and a per-call tag, which no test can predict.
fn replace_through(
    root: &Path,
    path: &Path,
    temp: &Path,
    bytes: &[u8],
    mode: Mode,
) -> io::Result<()> {
    use std::io::Write;

    crate::contain::no_link_on_the_way(root, path)?;
    let dir = directory_of(path)?;
    let now = std::fs::symlink_metadata(path)
        .ok()
        .map(|m| m.permissions());
    // A file somebody made read-only is theirs to open up again. A rename needs only the
    // directory's permission, so without this the replace would go straight past what an
    // in-place write used to refuse.
    if mode.refuses_read_only() && now.as_ref().is_some_and(std::fs::Permissions::readonly) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is read-only", path.display()),
        ));
    }
    let wanted = mode.permissions(now);
    let result = (|| {
        let mut out = create_temp(root, temp, wanted.as_ref().is_some_and(owner_only))?;
        #[cfg(test)]
        hook::created(&out);
        // Best effort in every mode, as every chmod in this crate is: a filesystem that
        // cannot hold a mode is not a reason to refuse the write. The one mode for which it
        // is — a secret — reads the answer back below.
        if let Some(permissions) = wanted {
            let _ = out.set_permissions(permissions);
        }
        #[cfg(unix)]
        if mode == Mode::Secret {
            use std::os::unix::fs::PermissionsExt;
            let bits = out.metadata()?.permissions().mode() & 0o7777;
            if bits & 0o077 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    NotPrivate { mode: bits & 0o777 },
                ));
            }
        }
        out.write_all(bytes)?;
        out.sync_all()?;
        drop(out);
        #[cfg(test)]
        hook::before_rename(path, temp)?;
        std::fs::rename(temp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(temp);
        return Err(refused_write(
            e,
            path,
            crate::sandbox::writes_are_sandboxed(),
        ));
    }
    // Best effort: a directory that cannot be opened or flushed does not undo the rename.
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(&dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// `EPERM`, the errno a sandbox answers a write it refuses with. A file's own mode answers
/// `EACCES` instead, so this one is not read as "the file is read-only".
const EPERM: i32 = 1;

/// A write to `path` that failed with `e`, said so it can be acted on (#1345).
///
/// **The one place a refused write gets its words.** Every committed file purlis writes goes
/// through [`replace`], so a write a chat's sandbox refuses lands here, whichever command made
/// it. The OS error alone ("Operation not permitted (os error 1)") named neither the file nor
/// the reason, and an agent reading it could not tell a sandbox from a broken disk.
///
/// Only `EPERM` is reworded, and it always names the file. **The sandbox is blamed only where
/// `sandboxed` says this write was held to one** ([`crate::sandbox::writes_are_sandboxed`]: the
/// app gave the chat a sandbox, and it holds what made the write, which a Claude Code hook it is
/// not, #1421): `EPERM` is also what macOS's privacy controls, its system integrity protection
/// and an immutable flag answer, and none of those is a sandbox's refusal. Every other failure
/// is returned as it was.
///
/// **The errno is kept.** The kind stays [`io::ErrorKind::PermissionDenied`], and the OS error
/// is the source, which [`os_cause`] hands back to a caller that records the errno itself.
pub fn refused_write(e: io::Error, path: &Path, sandboxed: bool) -> io::Error {
    if e.raw_os_error() != Some(EPERM) {
        return e;
    }
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        WriteRefused {
            path: path.to_path_buf(),
            sandboxed,
            source: e,
        },
    )
}

/// [`refused_write`] for a write to `path` by this process, which asks
/// [`crate::sandbox::writes_are_sandboxed`] itself: the `map_err` of every write that does not
/// go through [`replace`] — an append to a log, a directory made ahead of a write (#1359).
pub fn refused_at(path: &Path) -> impl FnOnce(io::Error) -> io::Error + '_ {
    move |e| refused_write(e, path, crate::sandbox::writes_are_sandboxed())
}

/// [`std::fs::create_dir_all`], with a refusal naming `dir` ([`refused_at`]).
pub fn create_dir_all(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir).map_err(refused_at(dir))
}

/// The OS error under `e`: the one a [`refused_write`] rewording carries, or `e` itself. What a
/// caller reads an errno or the OS's own sentence from.
pub fn os_cause(e: &io::Error) -> &io::Error {
    e.get_ref()
        .and_then(|inner| inner.downcast_ref::<WriteRefused>())
        .map_or(e, |refused| &refused.source)
}

/// The OS's own words for `e`, without Rust's `(os error N)`: for a sentence that names the
/// path itself, which the [`refused_write`] rewording would name a second time.
///
/// **A sandbox's refusal keeps its clause** (#1421): "Operation not permitted: this chat's
/// sandbox refused it", where the rewording blamed the sandbox, or, for an `EPERM` no write
/// site reworded, where this process's writes are held to one
/// ([`crate::sandbox::writes_are_sandboxed`]).
pub fn os_words(e: &io::Error) -> String {
    let sandboxed = match e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<WriteRefused>())
    {
        Some(refused) => refused.sandboxed,
        None => e.raw_os_error() == Some(EPERM) && crate::sandbox::writes_are_sandboxed(),
    };
    words_of(os_cause(e), sandboxed)
}

/// The OS's words for the OS error `e`, with the sandbox's clause on an `EPERM` where
/// `sandboxed`.
fn words_of(e: &io::Error, sandboxed: bool) -> String {
    let text = e.to_string();
    let words = match text.rfind(" (os error ") {
        Some(at) => text[..at].to_owned(),
        None => text,
    };
    if sandboxed && e.raw_os_error() == Some(EPERM) {
        format!("{words}: {SANDBOX_REFUSED_IT}")
    } else {
        words
    }
}

/// The clause [`os_words`] adds to a sandbox's refusal, after "Operation not permitted: ".
/// [`crate::sandboxblock`] reads it as purlis's own refusal.
pub const SANDBOX_REFUSED_IT: &str = "this chat's sandbox refused it";

/// How a sandboxed [`refused_write`] starts, its path straight after, then
/// [`NOT_PERMITTED`]. [`crate::sandboxblock`] reads it as purlis's own refusal.
pub const SANDBOX_REFUSED_WRITING: &str = "this chat's sandbox refused writing ";

/// What follows the path in a sandboxed [`refused_write`].
pub const NOT_PERMITTED: &str = " (Operation not permitted)";

/// A write refused with `EPERM`; see [`refused_write`].
#[derive(Debug)]
pub struct WriteRefused {
    path: PathBuf,
    sandboxed: bool,
    source: io::Error,
}

impl std::fmt::Display for WriteRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = crate::shown::readable(&self.path.display().to_string(), usize::MAX);
        if self.sandboxed {
            // A write purlis itself makes is never granted to a chat: its block is purlis's
            // own, which the notice offers a Report for and no Allow (#1421). So the one way
            // on is the operator's own run.
            write!(
                f,
                "{SANDBOX_REFUSED_WRITING}{path}{NOT_PERMITTED}. purlis does not grant its own \
                 writes to a chat; ask the operator to run this command outside the chat."
            )
        } else {
            write!(f, "could not write {path}: Operation not permitted")
        }
    }
}

impl std::error::Error for WriteRefused {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// 0600, where there is a mode to set.
fn private() -> Option<std::fs::Permissions> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Some(std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// The temp file, through the gate and `O_NOFOLLOW`. `create`/`truncate` rather than
/// `create_new`: the name is this call's own, so anything already at it is debris from a
/// killed process, and refusing it would refuse every later write. Created 0600 when the
/// finished file will let nobody but its owner in; under the umask otherwise.
fn create_temp(root: &Path, temp: &Path, owner_only: bool) -> io::Result<std::fs::File> {
    crate::contain::no_link_on_the_way(root, temp)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if owner_only {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = owner_only;
    crate::contain::nofollow(&mut options).open(temp)
}

fn directory_of(path: &Path) -> io::Result<PathBuf> {
    match path.parent() {
        Some(p) if p.as_os_str().is_empty() => Ok(PathBuf::from(".")),
        Some(p) => Ok(p.to_path_buf()),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no directory", path.display()),
        )),
    }
}

/// A directory the filesystem refuses every write into with `EPERM`, as a sandbox does: macOS's
/// user immutable flag (`chflags uchg`), lifted again when the guard drops. A mode refuses with
/// `EACCES` instead, and on Linux only root may set the immutable attribute, so the write
/// classes' refusal tests run on macOS (#1359).
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod frozen {
    use std::path::{Path, PathBuf};

    /// `path`, frozen until this drops.
    pub(crate) struct Frozen(PathBuf);

    fn chflags(flag: &str, path: &Path) {
        let ok = crate::forklock::status(
            std::process::Command::new("/usr/bin/chflags")
                .arg(flag)
                .arg(path),
        )
        .is_ok_and(|s| s.success());
        assert!(ok, "chflags {flag} {}", path.display());
    }

    impl Frozen {
        pub(crate) fn at(path: &Path) -> Self {
            chflags("uchg", path);
            Self(path.to_path_buf())
        }
    }

    impl Drop for Frozen {
        fn drop(&mut self) {
            chflags("nouchg", &self.0);
        }
    }

    /// What a refused write must say: the file, and never the bare OS error.
    pub(crate) fn names(e: &std::io::Error, path: &Path) {
        let said = e.to_string();
        assert!(said.contains(&path.display().to_string()), "{said}");
        assert!(!said.contains("os error"), "{said}");
        assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied, "{said}");
        assert_eq!(super::os_cause(e).raw_os_error(), Some(1), "{said}");
    }
}

/// A test's way in between the write and the rename — the instant a crash would land.
///
/// Thread-local, so it fires only in the test that set it: `cargo test` runs tests on a pool.
#[cfg(test)]
pub(crate) mod hook {
    use std::cell::RefCell;
    use std::io;
    use std::path::Path;

    type Hook = Box<dyn FnMut(&Path, &Path) -> io::Result<()>>;

    thread_local! {
        static BEFORE_RENAME: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// Run `f(target, temp)` after the temp file is complete and before it is renamed, for
    /// every [`super::replace`] on this thread until the returned guard drops. An error from
    /// `f` is the write failing at that instant.
    pub(crate) fn set(f: impl FnMut(&Path, &Path) -> io::Result<()> + 'static) -> Unset {
        BEFORE_RENAME.with(|h| *h.borrow_mut() = Some(Box::new(f)));
        Unset
    }

    pub(crate) struct Unset;

    impl Drop for Unset {
        fn drop(&mut self) {
            BEFORE_RENAME.with(|h| *h.borrow_mut() = None);
        }
    }

    pub(super) fn before_rename(target: &Path, temp: &Path) -> io::Result<()> {
        let taken = BEFORE_RENAME.with(|h| h.borrow_mut().take());
        let Some(mut f) = taken else {
            return Ok(());
        };
        let result = f(target, temp);
        BEFORE_RENAME.with(|h| {
            let mut slot = h.borrow_mut();
            if slot.is_none() {
                *slot = Some(f);
            }
        });
        result
    }

    type Watch = Box<dyn FnMut(&std::fs::File)>;

    thread_local! {
        static CREATED: RefCell<Option<Watch>> = const { RefCell::new(None) };
    }

    /// Show `f` the temp file's descriptor the instant it is created: before its mode is
    /// touched and before any content, so its mode here is the one it was created with.
    pub(crate) fn watch_created(f: impl FnMut(&std::fs::File) + 'static) -> Unwatch {
        CREATED.with(|w| *w.borrow_mut() = Some(Box::new(f)));
        Unwatch
    }

    pub(crate) struct Unwatch;

    impl Drop for Unwatch {
        fn drop(&mut self) {
            CREATED.with(|w| *w.borrow_mut() = None);
        }
    }

    pub(super) fn created(file: &std::fs::File) {
        CREATED.with(|w| {
            if let Some(f) = w.borrow_mut().as_mut() {
                f(file);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replace_that_dies_before_its_rename_leaves_the_old_file_whole_and_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("charter.toml");
        std::fs::write(&target, "old\n").unwrap();
        let _hook = hook::set(|target, temp| {
            assert_eq!(std::fs::read_to_string(target).unwrap(), "old\n");
            assert_eq!(std::fs::read_to_string(temp).unwrap(), "new\n");
            Err(io::Error::other("killed"))
        });

        let err = replace(dir.path(), &target, b"new\n", Mode::Kept).unwrap_err();

        assert_eq!(err.to_string(), "killed");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old\n");
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(left.len(), 1, "the temp file was cleaned up");
    }

    #[cfg(unix)]
    #[test]
    fn a_replace_keeps_the_mode_the_file_had_and_gives_a_new_one_the_mode_asked_for() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join("kept");
        std::fs::write(&kept, "x").unwrap();
        std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o640)).unwrap();
        replace(dir.path(), &kept, b"y", Mode::KeptOrPrivate).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&kept), 0o640);

        let fresh = dir.path().join("fresh");
        replace(dir.path(), &fresh, b"z", Mode::KeptOrPrivate).unwrap();
        assert_eq!(mode(&fresh), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_replace_refuses_a_target_that_is_a_link_and_leaves_what_it_points_at() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("bashrc");
        std::fs::write(&victim, "mine").unwrap();
        let target = dir.path().join(".gitignore");
        std::os::unix::fs::symlink(&victim, &target).unwrap();

        assert!(replace(dir.path(), &target, b"x", Mode::Kept).is_err());
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "mine");
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_file_is_refused_as_an_in_place_write_would_refuse_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("charter.toml");
        std::fs::write(&target, "old\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o444)).unwrap();

        let err = replace(dir.path(), &target, b"new\n", Mode::Kept).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old\n");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    /// EPERM, which is what a sandbox answers a write it refuses — not EACCES, a file's mode.
    fn eperm() -> io::Error {
        io::Error::from_raw_os_error(1)
    }

    #[test]
    fn a_write_a_sandbox_refuses_names_the_file_it_was_writing() {
        // #1345: `purlis persona remember` from a chat printed only "Operation not permitted
        // (os error 1)", with no path and no next step.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("memory.md");
        let _hook = hook::set(|_, _| Err(eperm()));

        let err = replace(dir.path(), &target, b"new\n", Mode::Kept).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert!(
            err.to_string().contains(&target.display().to_string()),
            "{err}"
        );
    }

    #[test]
    fn only_a_chat_given_a_sandbox_is_told_its_sandbox_refused_the_write() {
        let path = Path::new("/plane/personas/steward/memory/fact.md");
        let said = refused_write(eperm(), path, true).to_string();
        assert!(said.contains("this chat's sandbox refused"), "{said}");
        assert!(
            said.contains("/plane/personas/steward/memory/fact.md"),
            "{said}"
        );
        assert!(said.contains("operator"), "the next step: {said}");

        // EPERM is also privacy controls, system integrity protection and an immutable flag.
        let unsandboxed = refused_write(eperm(), path, false).to_string();
        assert_eq!(
            unsandboxed,
            "could not write /plane/personas/steward/memory/fact.md: Operation not permitted"
        );
    }

    #[test]
    fn a_sandbox_s_refusal_says_what_it_refused_and_the_one_way_on() {
        // #1421: "a sandboxed chat writes only inside its own folder" stopped being true when
        // the cache and host grants came (#1337, #1341); and a write of purlis's own is never
        // granted, so no Allow is promised.
        let path = Path::new("/plane/personas/_dispatch/2026-10.host.jsonl");
        let said = refused_write(eperm(), path, true).to_string();
        assert_eq!(
            said,
            "this chat's sandbox refused writing /plane/personas/_dispatch/2026-10.host.jsonl \
             (Operation not permitted). purlis does not grant its own writes to a chat; ask the \
             operator to run this command outside the chat."
        );
        assert!(!said.contains("only inside its own folder"), "{said}");
    }

    #[test]
    fn the_os_s_words_keep_the_sandbox_s_clause_in_a_sandboxed_chat() {
        // #1421: `init`, `browser install` and the handoff's warning name the path themselves,
        // and said only "(Operation not permitted)" in a sandboxed chat.
        let path = Path::new("/plane/.gitignore");
        let sandboxed = refused_write(eperm(), path, true);
        assert_eq!(
            os_words(&sandboxed),
            "Operation not permitted: this chat's sandbox refused it"
        );
        let not = refused_write(eperm(), path, false);
        assert_eq!(os_words(&not), "Operation not permitted");
        // A refusal no site reworded is judged as this process stands.
        assert_eq!(
            words_of(&eperm(), true),
            "Operation not permitted: this chat's sandbox refused it"
        );
        assert_eq!(words_of(&eperm(), false), "Operation not permitted");
        // Anything else is the OS's words alone, sandboxed or not.
        assert_eq!(
            words_of(&io::Error::from_raw_os_error(28), true),
            "No space left on device"
        );
        assert!(
            !os_words(&sandboxed).contains("/plane"),
            "named once, by the caller"
        );
    }

    #[test]
    fn a_reworded_refusal_still_carries_its_errno() {
        let refused = refused_write(eperm(), Path::new("/plane/x.md"), true);
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(os_cause(&refused).raw_os_error(), Some(1));
        let plain = io::Error::from_raw_os_error(13);
        assert_eq!(os_cause(&plain).raw_os_error(), Some(13));
    }

    #[test]
    fn a_write_site_maps_a_refusal_to_one_naming_its_path() {
        // #1359: what every append and every folder made ahead of a write maps its error
        // through. Run on every platform: the sites' own tests need macOS's immutable flag.
        let path = Path::new("/plane/workspaces/alpha/work/device.jsonl");
        let refused = refused_at(path)(eperm());
        let said = refused.to_string();
        assert!(
            said.contains("/plane/workspaces/alpha/work/device.jsonl"),
            "{said}"
        );
        assert!(!said.contains("os error"), "{said}");
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(os_cause(&refused).raw_os_error(), Some(1));

        let full = refused_at(path)(io::Error::from_raw_os_error(28));
        assert_eq!(
            full.raw_os_error(),
            Some(28),
            "a full disk stays a full disk"
        );
    }

    #[test]
    fn a_folder_made_ahead_of_a_write_is_made_and_a_failure_that_is_not_a_refusal_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a/b/c");
        create_dir_all(&deep).unwrap();
        assert!(deep.is_dir());

        // A file where a folder must go: not EPERM, so the OS's own error comes back.
        let file = dir.path().join("file");
        std::fs::write(&file, "").unwrap();
        let blocked = create_dir_all(&file.join("below")).unwrap_err();
        assert!(blocked.raw_os_error().is_some(), "{blocked}");
    }

    #[test]
    fn a_failure_that_is_not_a_refusal_is_passed_on_as_it_was() {
        let path = Path::new("/plane/x.md");
        let full = refused_write(io::Error::from_raw_os_error(28), path, true);
        assert_eq!(
            full.raw_os_error(),
            Some(28),
            "a full disk stays a full disk"
        );
    }

    #[test]
    fn a_file_charter_cannot_read_as_text_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".gitignore");
        std::fs::write(&target, b"\xff\xfe").unwrap();
        let err = update(dir.path(), &target, |_| Ok(Some("new".into()))).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(std::fs::read(&target).unwrap(), b"\xff\xfe");
    }

    #[cfg(unix)]
    fn mode(p: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    fn chmod(p: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_kept_file_that_is_new_gets_the_umask_s_mode_and_not_a_private_one() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain");
        std::fs::write(&plain, "x").unwrap();
        let fresh = dir.path().join("workspace.json");
        replace(dir.path(), &fresh, b"{}", Mode::Kept).unwrap();
        assert_eq!(mode(&fresh), mode(&plain));
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_ends_0600_over_a_loose_or_read_only_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("launched.json");
        std::fs::write(&state, "old").unwrap();
        chmod(&state, 0o644);
        replace(dir.path(), &state, b"new", Mode::Private).unwrap();
        assert_eq!(mode(&state), 0o600);

        // Charter's own state: its mode is charter's, so read-only is not a refusal here.
        chmod(&state, 0o400);
        replace(dir.path(), &state, b"newer", Mode::Private).unwrap();
        assert_eq!(std::fs::read_to_string(&state).unwrap(), "newer");
        assert_eq!(mode(&state), 0o600);

        let fresh = dir.path().join("fresh");
        replace(dir.path(), &fresh, b"z", Mode::Private).unwrap();
        assert_eq!(mode(&fresh), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_secret_ends_0600_and_a_read_only_one_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("app.json");
        std::fs::write(&vault, "{}").unwrap();
        chmod(&vault, 0o644);
        replace(dir.path(), &vault, b"{\"K\": 1}", Mode::Secret).unwrap();
        assert_eq!(mode(&vault), 0o600);

        chmod(&vault, 0o400);
        let err = replace(dir.path(), &vault, b"{}", Mode::Secret).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(&vault).unwrap(), "{\"K\": 1}");
    }

    /// A temp that will end private is CREATED 0600: a descriptor opened on it in the instant
    /// before a chmod would keep reading whatever is written after.
    #[cfg(unix)]
    #[test]
    fn a_private_temp_is_0600_from_the_instant_it_exists() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let loose = dir.path().join("loose");
        std::fs::write(&loose, "old").unwrap();
        chmod(&loose, 0o644);
        let seen: std::rc::Rc<std::cell::RefCell<Vec<(u32, u64)>>> = Default::default();
        let log = seen.clone();
        let _watch = hook::watch_created(move |file| {
            let meta = file.metadata().unwrap();
            log.borrow_mut()
                .push((meta.permissions().mode() & 0o777, meta.len()));
        });

        replace(dir.path(), &dir.path().join("new"), b"s", Mode::Secret).unwrap();
        replace(dir.path(), &loose, b"s", Mode::Private).unwrap();
        replace(
            dir.path(),
            &dir.path().join("tab"),
            b"s",
            Mode::KeptOrPrivate,
        )
        .unwrap();
        let kept = dir.path().join("kept");
        std::fs::write(&kept, "old").unwrap();
        chmod(&kept, 0o600);
        replace(dir.path(), &kept, b"s", Mode::Kept).unwrap();

        assert_eq!(*seen.borrow(), [(0o600, 0); 4]);
    }

    #[test]
    fn the_temp_prefix_is_the_layer_s_marker_and_a_dot() {
        assert_eq!(TEMP_PREFIX, format!("{}.", crate::layer::MARKER));
    }

    #[test]
    fn every_temp_carries_the_one_prefix_charter_s_temps_are_known_by() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        let _hook = hook::set(|_, temp| {
            let name = temp.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                name.starts_with(".purlis-generated.settings.json."),
                "{name}"
            );
            assert!(name.ends_with(".tmp"), "{name}");
            Ok(())
        });
        replace(dir.path(), &target, b"{}", Mode::Kept).unwrap();
    }

    /// The link is planted at the TEMP path, which is the path the write actually opens.
    /// Planting it at the target proves nothing about this: the target's gate answers first.
    #[cfg(unix)]
    #[test]
    fn a_link_at_the_temp_path_is_refused_and_what_it_points_at_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let captured = dir.path().join("captured.json");
        let target = dir.path().join("launched.json");
        let temp = dir.path().join(".launched.json.writing");
        std::os::unix::fs::symlink(&captured, &temp).unwrap();

        let refused = replace_through(dir.path(), &target, &temp, b"{}", Mode::Private);

        assert!(refused.is_err(), "the temp path was written through");
        assert!(!captured.exists(), "the bytes went through a link");
        assert!(!target.exists());
    }

    /// The walk, not `O_NOFOLLOW`, answers about a directory ABOVE the file: the flag speaks
    /// only for the last component.
    #[cfg(unix)]
    #[test]
    fn a_directory_swapped_for_a_link_is_refused_at_the_moment_of_the_write() {
        let plane = tempfile::tempdir().unwrap();
        let elsewhere = plane.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, plane.path().join(".charter")).unwrap();

        let target = plane.path().join(".charter/launched.json");
        assert!(replace(plane.path(), &target, b"{}", Mode::Private).is_err());
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
    }

    /// Debris from a killed process at the temp's name is written over, not a refusal of
    /// every later write.
    #[test]
    fn a_temp_left_by_a_killed_process_does_not_stop_the_next_write() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("launched.json");
        let temp = dir.path().join(".launched.json.writing");
        std::fs::write(&temp, "half a record from a process that died").unwrap();

        replace_through(dir.path(), &target, &temp, b"{}", Mode::Private).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"{}");
        assert!(!temp.exists());
    }
}
