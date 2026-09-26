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
pub struct Lock(#[allow(dead_code)] Option<std::fs::File>);

impl Lock {
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
        Some(next) => replace(root, path, next.as_bytes(), None).map(|()| true),
        None => Ok(false),
    }
}

/// Replace `path` with `bytes`, whole or not at all.
///
/// - The path is gated as itself and every directory above it, up to `root`
///   ([`crate::contain::no_link_on_the_way`]), and the temp file is created with
///   `O_NOFOLLOW` through the same gate, so neither write can be steered out of the plane.
/// - An existing file keeps its permissions: they are put on the temp file BEFORE any content,
///   because a rename carries the source's mode. A new file gets `fresh_mode` when one is
///   given, and the umask's answer otherwise.
/// - The temp file is flushed to the disk before the rename, and the directory after it, so a
///   power cut cannot leave the new name pointing at an empty inode.
/// - On any failure the temp file is removed and the target is left as it was.
pub fn replace(root: &Path, path: &Path, bytes: &[u8], fresh_mode: Option<u32>) -> io::Result<()> {
    use std::io::Write;

    crate::contain::no_link_on_the_way(root, path)?;
    let dir = directory_of(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Hidden, and unique per call: the pid separates two processes and the tag two threads.
    let temp = dir.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        crate::workspaces::scratch_tag()
    ));
    let kept = std::fs::symlink_metadata(path)
        .ok()
        .map(|m| m.permissions());
    // A file somebody made read-only is theirs to open up again. A rename needs only the
    // directory's permission, so without this the replace would go straight past what an
    // in-place write used to refuse.
    if kept.as_ref().is_some_and(std::fs::Permissions::readonly) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is read-only", path.display()),
        ));
    }
    let result = (|| {
        let mut out = crate::contain::create_no_link(root, &temp)?;
        match kept {
            Some(permissions) => out.set_permissions(permissions)?,
            None => {
                #[cfg(unix)]
                if let Some(mode) = fresh_mode {
                    use std::os::unix::fs::PermissionsExt;
                    out.set_permissions(std::fs::Permissions::from_mode(mode))?;
                }
                #[cfg(not(unix))]
                let _ = fresh_mode;
            }
        }
        out.write_all(bytes)?;
        out.sync_all()?;
        drop(out);
        #[cfg(test)]
        hook::before_rename(path, &temp)?;
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        return result;
    }
    // Best effort: a directory that cannot be opened or flushed does not undo the rename.
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(&dir) {
        let _ = d.sync_all();
    }
    Ok(())
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

        let err = replace(dir.path(), &target, b"new\n", None).unwrap_err();

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
        replace(dir.path(), &kept, b"y", Some(0o600)).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&kept), 0o640);

        let fresh = dir.path().join("fresh");
        replace(dir.path(), &fresh, b"z", Some(0o600)).unwrap();
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

        assert!(replace(dir.path(), &target, b"x", None).is_err());
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

        let err = replace(dir.path(), &target, b"new\n", None).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old\n");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
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
}
