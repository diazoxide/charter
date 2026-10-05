//! A workspace's store, held open by descriptor (V74): the one way charter reaches a
//! workspace's todos, memory, session records and changes, for the MCP tools a chat calls
//! ([`crate::chattools`]) and for the operator's own `charter` commands and the app alike
//! ([`crate::memstore`], [`crate::sessionrecord`], [`crate::change::store`]).
//!
//! **Every read and write goes through descriptors, and nothing is looked up by path once the
//! store is open.** The project's root is opened, then `workspaces`, the workspace and the store
//! one component at a time with `O_NOFOLLOW | O_DIRECTORY`, and every file in the store is
//! opened, created, renamed and unlinked relative to that descriptor, with `O_NOFOLLOW`. A chat
//! that swaps its store for a link while a tool or a command runs (a rename-swap loop in its own
//! workspace) either makes the open fail, and the call is refused, or loses the race to a store
//! charter already holds, which stays inside the workspace whatever its name is by then. A
//! check followed by a write by path cannot promise that (measured: 34 of 1,500 writes landed
//! in a persona's memory).
//!
//! The same flock on the store's directory as [`crate::rewrite::Lock`] is taken on the held
//! descriptor ([`Store::lock`]), so a tool and a `charter` command writing the same store take
//! turns.
//!
//! **No link inside either.** Each file is opened with `O_NOFOLLOW` and checked again on its
//! descriptor: a symbolic link, or a file with more than one name (a hard link can name a file
//! outside), is never read or written through. The MCP tools go further and refuse a store
//! holding any such entry, or a tree deeper than charter writes, whole and before anything is
//! read ([`Store::open`]); the operator's commands skip such an entry, as they always skipped
//! one that left the project, so a link a chat planted cannot stop the operator recording a
//! todo.

use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};

use rustix::fs::{AtFlags, FileType, FlockOperation, Mode, OFlags};

use crate::active::Place;

/// How long [`Store::lock`] waits for another holder of a store's lock before it refuses.
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// How deep the store's tree may go before the MCP tools refuse it. charter writes a level or
/// two.
const DEEPEST: usize = 8;

/// The mode a new file is created with, before the umask: what `std::fs::File::create` uses.
fn new_file() -> Mode {
    Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::WGRP | Mode::ROTH | Mode::WOTH
}

/// The mode a new store directory is created with, before the umask.
fn new_dir() -> Mode {
    Mode::RWXU | Mode::RWXG | Mode::RWXO
}

/// Who a sentence about a store is for: the chat whose tool it is, or the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Who {
    /// An MCP tool's answer: `the chat's memory`.
    Chat,
    /// A `charter` command's or the app's: `workspaces/alpha/memory`, never the absolute path.
    Operator,
}

/// What [`Store::hold`] makes when it is not there yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Make {
    /// Nothing: a store, or a workspace, that is not there is `Ok(None)`.
    Nothing,
    /// The store, in a workspace that must already be there.
    Store,
    /// The workspace's directories too, as `create_dir_all` made them for a store by path.
    All,
}

/// One store of a place, held open.
pub(crate) struct Store {
    fd: OwnedFd,
    /// Where the store is below its place, for a sentence: `memory`, `memory/archive`.
    below: String,
    /// The store as the sentence names it: `the chat's memory`, `workspaces/alpha/memory`.
    said: String,
    /// What a link in it reaches outside of, as a sentence names it: `its workspace`, or
    /// `the persona store` for a persona's memory held from the project's root.
    within: String,
}

/// The sentence for a store reached through a link, or holding one at `inside`. Never the
/// absolute path.
fn reaches_outside(said: &str, within: &str, below: &str, inside: Option<&str>) -> String {
    match inside {
        Some(entry) => format!(
            "{said} reaches outside {within} through a link ({below}/{entry}), so nothing was \
             done"
        ),
        None => format!("{said} reaches outside {within} through a link, so nothing was done"),
    }
}

/// One entry of a held store, as `lstat` sees it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry {
    pub(crate) kind: FileType,
    /// How many names the file has: more than one is read as a link.
    pub(crate) links: u64,
    pub(crate) size: u64,
    /// The permission bits.
    pub(crate) mode: Mode,
}

impl Entry {
    /// A regular file with one name: the only kind of entry charter reads or writes through.
    pub(crate) fn plain(&self) -> bool {
        self.kind == FileType::RegularFile && self.links <= 1
    }

    /// A symbolic link, or a file with another name somewhere: a way out of the store.
    pub(crate) fn link(&self) -> bool {
        self.kind == FileType::Symlink || (self.kind == FileType::RegularFile && self.links > 1)
    }
}

/// The store's lock, let go when it is dropped.
pub(crate) struct Locked<'a>(&'a Store);

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0.fd, FlockOperation::Unlock);
    }
}

/// Why a store could not be held: the sentence, and the filesystem's error number when it was
/// one the store could not be looked at for (a permission), rather than a link on the way.
#[derive(Debug)]
pub(crate) struct Unheld {
    pub(crate) why: String,
    /// `None` for a link on the way, or anything else that is a refusal rather than a look
    /// the filesystem refused.
    pub(crate) errno: Option<i32>,
}

impl From<String> for Unheld {
    fn from(why: String) -> Self {
        Self { why, errno: None }
    }
}

impl From<Unheld> for String {
    fn from(unheld: Unheld) -> Self {
        unheld.why
    }
}

impl std::fmt::Display for Unheld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

impl std::error::Error for Unheld {}

/// A directory on the way that could not be opened: a link (`ELOOP`, or `ENOTDIR` for a link
/// to a file) is a refusal; anything else is a look the filesystem refused, with its number.
fn unopened(e: rustix::io::Errno, link: impl FnOnce() -> String, other: String) -> Unheld {
    if e == rustix::io::Errno::LOOP || e == rustix::io::Errno::NOTDIR {
        Unheld::from(link())
    } else {
        Unheld {
            why: other,
            errno: Some(e.raw_os_error()),
        }
    }
}

impl Store {
    /// The MCP tools' hold: `store` of `place` in the project at `root`, made when it is not
    /// there yet and `make` is true, locked for as long as it is held, and refused whole when
    /// it holds anything but plain files and directories. `Ok(None)` for a store that is not
    /// there and is not to be made. A workspace that is not there is refused, never made.
    pub(crate) fn open(
        root: &std::path::Path,
        place: &Place,
        store: &str,
        make: bool,
    ) -> Result<Option<Self>, String> {
        let wanted = if make { Make::Store } else { Make::Nothing };
        let Some(held) = Self::hold(root, place, store, wanted, Who::Chat)? else {
            if let Place::Workspace(ws) = place
                && !Self::workspace_there(root, ws)?
            {
                return Err(format!("no workspace '{ws}'"));
            }
            return Ok(None);
        };
        // Best effort, as `rewrite::Lock` is: released when the descriptor closes.
        let _ = rustix::fs::flock(&held.fd, FlockOperation::LockExclusive);
        held.refuse_anything_odd()?;
        Ok(Some(held))
    }

    /// `store` of `place` in the project at `root`, opened one component at a time without
    /// following a link, and made as `make` says. Not locked ([`Store::lock`]), and what it
    /// holds is checked entry by entry as it is used.
    pub(crate) fn hold(
        root: &std::path::Path,
        place: &Place,
        store: &str,
        make: Make,
        who: Who,
    ) -> Result<Option<Self>, Unheld> {
        let said = match (who, place) {
            (Who::Chat, _) => format!("the chat's {store}"),
            (Who::Operator, Place::Workspace(ws)) => format!("workspaces/{ws}/{store}"),
            (Who::Operator, Place::PlaneRoot) => store.to_owned(),
        };
        let within = match place {
            Place::Workspace(_) => "its workspace",
            Place::PlaneRoot => "the project",
        }
        .to_owned();
        let dir = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
        let step = dir | OFlags::NOFOLLOW;
        // The root is the project, named by the environment or the operator, not by anything a
        // chat can plant: it is opened as it is named.
        let mut fd = rustix::fs::open(root, dir, Mode::empty())
            .map_err(|e| format!("the project cannot be opened: {e}"))?;
        if let Place::Workspace(ws) = place {
            // A tool takes only a name charter would give a workspace. The operator's commands
            // read whatever a chat made under `workspaces/` (`doctor`, #353), but still only one
            // plain name, never a path.
            let one_name = !ws.is_empty() && !ws.contains('/') && ws != "." && ws != "..";
            if !one_name || (who == Who::Chat && !crate::contain::workspace_name_ok(ws)) {
                return Err(format!("'{ws}' is not a workspace's name").into());
            }
            for (component, named) in [("workspaces", "the workspaces"), (ws.as_str(), ws)] {
                let opened = match rustix::fs::openat(&fd, component, step, Mode::empty()) {
                    Err(rustix::io::Errno::NOENT) if make == Make::All => {
                        match rustix::fs::mkdirat(&fd, component, new_dir()) {
                            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                            Err(e) => return Err(format!("{named} cannot be made: {e}").into()),
                        }
                        rustix::fs::openat(&fd, component, step, Mode::empty())
                    }
                    opened => opened,
                };
                fd = match opened {
                    Ok(fd) => fd,
                    Err(rustix::io::Errno::NOENT) if make == Make::Nothing => return Ok(None),
                    Err(rustix::io::Errno::NOENT) => {
                        return Err(format!("no workspace '{ws}'").into());
                    }
                    Err(e) => {
                        let why =
                            format!("{named} cannot be opened without following a link ({e})");
                        return Err(unopened(e, || why.clone(), why.clone()));
                    }
                };
            }
        }
        let below = Self {
            fd,
            below: String::new(),
            said: said.clone(),
            within,
        };
        let Some(mut held) = below.sub(store, make != Make::Nothing)? else {
            return Ok(None);
        };
        held.below = store.to_owned();
        held.said = said;
        Ok(Some(held))
    }

    /// Whether workspace `ws` is there, looked at without following a link.
    fn workspace_there(root: &std::path::Path, ws: &str) -> Result<bool, String> {
        let dir = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
        let fd = rustix::fs::open(root, dir, Mode::empty())
            .map_err(|e| format!("the project cannot be opened: {e}"))?;
        let path = format!("workspaces/{ws}");
        Ok(rustix::fs::statat(&fd, path.as_str(), AtFlags::SYMLINK_NOFOLLOW).is_ok())
    }

    /// The directory `name` directly in this store, held the same way: made when it is not
    /// there and `make` is true, `Ok(None)` when it is not there and is not to be.
    pub(crate) fn sub(&self, name: &str, make: bool) -> Result<Option<Self>, Unheld> {
        let step = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
        let below = if self.below.is_empty() {
            name.to_owned()
        } else {
            format!("{}/{name}", self.below)
        };
        let outside = || {
            if self.below.is_empty() {
                reaches_outside(&self.said, &self.within, name, None)
            } else {
                reaches_outside(&self.said, &self.within, &self.below, Some(name))
            }
        };
        let fd = match rustix::fs::openat(&self.fd, name, step, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if make => {
                match rustix::fs::mkdirat(&self.fd, name, new_dir()) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                    Err(e) => {
                        let said = if self.below.is_empty() {
                            self.said.clone()
                        } else {
                            format!("{}/{name}", self.said)
                        };
                        return Err(format!("{said} cannot be made: {e}").into());
                    }
                }
                rustix::fs::openat(&self.fd, name, step, Mode::empty())
                    .map_err(|e| unopened(e, outside, self.failed(e)))?
            }
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(unopened(e, outside, self.failed(e))),
        };
        Ok(Some(Self {
            fd,
            below,
            said: self.said.clone(),
            within: self.within.clone(),
        }))
    }

    /// Take the store's lock, as [`crate::rewrite::Lock::on`] takes it on its directory, until
    /// what this returns is dropped.
    ///
    /// **Waited for a bounded time, never for ever.** The lock is an flock on the store's own
    /// directory, which a chat can take too, and keep: waited for without a bound, that wedged
    /// the operator's command or the window on the chat's store. Asked again every few
    /// milliseconds for [`LOCK_WAIT`], then refused in a sentence. A filesystem that cannot
    /// lock at all is written without one, as `rewrite::Lock` writes.
    pub(crate) fn lock(&self) -> Result<Locked<'_>, String> {
        let gave_up = std::time::Instant::now() + LOCK_WAIT;
        loop {
            match rustix::fs::flock(&self.fd, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(Locked(self)),
                Err(rustix::io::Errno::WOULDBLOCK | rustix::io::Errno::INTR) => {}
                Err(_) => return Ok(Locked(self)),
            }
            if std::time::Instant::now() >= gave_up {
                return Err(format!(
                    "{} is busy: another process has held its lock for over {} seconds, so \
                     nothing was done. Try again once it lets go",
                    self.said,
                    LOCK_WAIT.as_secs()
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// This store with its lock taken ([`Store::lock`]) and kept until it is dropped, when its
    /// descriptor closes.
    pub(crate) fn locked(self) -> Result<Self, String> {
        // The guard would let go at once; the descriptor's close lets go instead.
        std::mem::forget(self.lock()?);
        Ok(self)
    }

    /// Refuse the store whole when it holds a link, a file with more than one name, anything
    /// that is neither a file nor a directory, or a tree deeper than charter writes.
    pub(crate) fn refuse_anything_odd(&self) -> Result<(), String> {
        match odd_inside(self.fd.as_fd(), "", 0) {
            None => Ok(()),
            Some(Odd::Link(entry)) => Err(reaches_outside(
                &self.said,
                &self.within,
                &self.below,
                Some(&entry),
            )),
            Some(Odd::NotAFile(entry)) => Err(format!(
                "{} holds {}/{entry}, which is neither a file nor a directory, so nothing was \
                 done",
                self.said, self.below
            )),
        }
    }

    /// What is at `name` in the store, as `lstat` sees it: `None` when nothing is.
    // `st_nlink` is a u16 on macOS and a u64 on Linux, so the widening is a no-op on one of them.
    #[allow(clippy::useless_conversion)]
    pub(crate) fn entry(&self, name: &str) -> Result<Option<Entry>, String> {
        match rustix::fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => Ok(Some(Entry {
                kind: FileType::from_raw_mode(stat.st_mode),
                links: u64::from(stat.st_nlink),
                size: stat.st_size as u64,
                mode: Mode::from_raw_mode(stat.st_mode as rustix::fs::RawMode) & Mode::all(),
            })),
            Err(rustix::io::Errno::NOENT) => Ok(None),
            Err(e) => Err(self.failed(e)),
        }
    }

    /// The name of every entry directly in the store, whatever it is, sorted.
    pub(crate) fn names(&self) -> Result<Vec<String>, String> {
        let mut names: Vec<String> = entries(self.fd.as_fd())
            .map_err(|e| self.failed(e))?
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        names.sort();
        Ok(names)
    }

    /// The names of the plain files directly in the store (a regular file with one name),
    /// sorted.
    pub(crate) fn files(&self) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        for (name, kind) in entries(self.fd.as_fd()).map_err(|e| self.failed(e))? {
            if kind == FileType::RegularFile {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    /// `name`'s text, read through the store: `None` when it is not there or is not UTF-8, and
    /// every line ending made `\n` ([`crate::memstore::read_text`]). A file that is not a
    /// plain file with one name, or past [`crate::memstore::MAX_BYTES`], is refused.
    pub(crate) fn read(&self, name: &str) -> Result<Option<String>, String> {
        Ok(self.bytes(name)?.and_then(crate::memstore::text_of))
    }

    /// `name`'s text exactly as it is on disk, as [`Store::read`] reads it otherwise; text that
    /// is not UTF-8 is refused.
    pub(crate) fn read_raw(&self, name: &str) -> Result<Option<String>, String> {
        self.bytes(name)?
            .map(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|_| format!("{}/{name} is not UTF-8 text", self.below))
            })
            .transpose()
    }

    fn bytes(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let fd = match rustix::fs::openat(&self.fd, name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(rustix::io::Errno::LOOP) => return Err(self.outside(name)),
            Err(e) => return Err(self.failed(e)),
        };
        self.one_plain_file(&fd, name)?;
        let mut bytes = Vec::new();
        std::fs::File::from(fd)
            .take(crate::memstore::MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| self.failed(e))?;
        if bytes.len() as u64 > crate::memstore::MAX_BYTES {
            return Err(format!("{}/{name} is too big to read", self.below));
        }
        Ok(Some(bytes))
    }

    /// Write `bytes` as `name`, which must not be there yet. `false` when it is.
    pub(crate) fn create(&self, name: &str, bytes: &[u8]) -> Result<bool, String> {
        let flags =
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd = match rustix::fs::openat(&self.fd, name, flags, new_file()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::EXIST) => return Ok(false),
            Err(e) => return Err(self.failed(e)),
        };
        let written = std::fs::File::from(fd).write_all(bytes);
        if let Err(e) = written {
            let _ = rustix::fs::unlinkat(&self.fd, name, AtFlags::empty());
            return Err(self.failed(e));
        }
        Ok(true)
    }

    /// Add `bytes` to the end of `name`, making it with `first` in front when it is not there.
    pub(crate) fn append(&self, name: &str, first: &str, bytes: &[u8]) -> Result<(), String> {
        let mut whole = first.as_bytes().to_vec();
        whole.extend_from_slice(bytes);
        if self.create(name, &whole)? {
            return Ok(());
        }
        if let Some(entry) = self.entry(name)?
            && !entry.plain()
        {
            return Err(self.not_plain(name, entry.kind));
        }
        // Non-blocking: a FIFO planted at the name must never hold the call, and the store's
        // lock with it, waiting for a reader.
        let flags =
            OFlags::WRONLY | OFlags::APPEND | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let fd = rustix::fs::openat(&self.fd, name, flags, Mode::empty()).map_err(|e| {
            if e == rustix::io::Errno::LOOP {
                self.outside(name)
            } else {
                self.failed(e)
            }
        })?;
        self.one_plain_file(&fd, name)?;
        std::fs::File::from(fd)
            .write_all(bytes)
            .map_err(|e| self.failed(e))
    }

    /// Replace `name` whole with `bytes`, or make it: written beside it, flushed, then renamed
    /// over it, both within the held store, as [`crate::rewrite::replace`] writes by path. The
    /// file keeps its mode; a link or a file with another name at `name` is refused, never
    /// replaced, and so is a file somebody made read-only (`rewrite::Mode::Kept`).
    pub(crate) fn replace(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        // The mode the file has, kept; a new file's is the umask's answer, as a by-path
        // `rewrite::replace` leaves it.
        let kept = match self.entry(name)? {
            Some(entry) if !entry.plain() => return Err(self.not_plain(name, entry.kind)),
            Some(entry) if entry.mode & (Mode::WUSR | Mode::WGRP | Mode::WOTH) == Mode::empty() => {
                return Err(format!("{}/{name} is read-only", self.below));
            }
            Some(entry) => Some(entry.mode),
            None => None,
        };
        let mode = kept.unwrap_or_else(new_file);
        let tmp = format!(
            "{}{name}.{}.{}.tmp",
            crate::rewrite::TEMP_PREFIX,
            std::process::id(),
            crate::workspaces::scratch_tag()
        );
        let flags =
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd =
            rustix::fs::openat(&self.fd, tmp.as_str(), flags, mode).map_err(|e| self.failed(e))?;
        // The mode asked for is masked by the umask; a kept mode is the file's own.
        if let Some(mode) = kept {
            let _ = rustix::fs::fchmod(&fd, mode);
        }
        let mut file = std::fs::File::from(fd);
        let written = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        if let Err(e) = written {
            let _ = rustix::fs::unlinkat(&self.fd, tmp.as_str(), AtFlags::empty());
            return Err(self.failed(e));
        }
        rustix::fs::renameat(&self.fd, tmp.as_str(), &self.fd, name).map_err(|e| {
            let _ = rustix::fs::unlinkat(&self.fd, tmp.as_str(), AtFlags::empty());
            self.failed(e)
        })?;
        let _ = rustix::fs::fsync(&self.fd);
        Ok(())
    }

    /// Write `bytes` as `name` whole, made beside it and renamed into place, when nothing is
    /// at `name`. `false`, writing nothing, when something is.
    ///
    /// **Only exclusive under the store's lock** ([`Store::lock`]): it looks, then renames, so
    /// two writers that do not take turns can both find the name free and the second replaces
    /// the first. Every caller holds the lock across the call. Nothing it does follows a link
    /// either way: one planted after the look is refused by [`Store::replace`], or replaced.
    pub(crate) fn create_whole(&self, name: &str, bytes: &[u8]) -> Result<bool, String> {
        if self.entry(name)?.is_some() {
            return Ok(false);
        }
        self.replace(name, bytes)?;
        Ok(true)
    }

    /// Unlink `name` from the store.
    pub(crate) fn remove(&self, name: &str) -> Result<(), String> {
        rustix::fs::unlinkat(&self.fd, name, AtFlags::empty()).map_err(|e| self.failed(e))
    }

    /// This store, named in a sentence as `said`, and a link in it as reaching outside
    /// `within` — for a store held from the project's root that is not the project's own,
    /// such as a persona's memory (KN-3).
    pub(crate) fn named(mut self, said: String, within: String) -> Self {
        self.said = said;
        self.within = within;
        self
    }

    /// Move `name` from this store to `to` in `into`, never over anything: `Ok(false)`, moving
    /// nothing, when something is at `to` already.
    ///
    /// The kernel's no-replace rename where there is one (`renameat2(RENAME_NOREPLACE)` on
    /// Linux, `renameatx_np(RENAME_EXCL)` on macOS), so a file a writer that does not take the
    /// store's lock planted at `to` after the caller looked is refused rather than replaced. A
    /// filesystem that does not have it is renamed onto plainly, after a look (`renameat`).
    pub(crate) fn rename_into_new(
        &self,
        name: &str,
        into: &Store,
        to: &str,
    ) -> Result<bool, String> {
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios"
        ))]
        match rustix::fs::renameat_with(
            &self.fd,
            name,
            &into.fd,
            to,
            rustix::fs::RenameFlags::NOREPLACE,
        ) {
            Ok(()) => return Ok(true),
            Err(rustix::io::Errno::EXIST) => return Ok(false),
            Err(
                rustix::io::Errno::INVAL | rustix::io::Errno::NOSYS | rustix::io::Errno::NOTSUP,
            ) => {}
            Err(e) => return Err(self.failed(e)),
        }
        if into.entry(to)?.is_some() {
            return Ok(false);
        }
        self.rename_into(name, into, to)?;
        Ok(true)
    }

    /// Move `name` from this store to `to` in `into`, another store held the same way.
    pub(crate) fn rename_into(&self, name: &str, into: &Store, to: &str) -> Result<(), String> {
        rustix::fs::renameat(&self.fd, name, &into.fd, to).map_err(|e| self.failed(e))
    }

    /// Whether `fd` is a plain file with one name.
    fn one_plain_file(&self, fd: &OwnedFd, name: &str) -> Result<(), String> {
        let stat = rustix::fs::fstat(fd).map_err(|e| self.failed(e))?;
        let kind = FileType::from_raw_mode(stat.st_mode);
        if kind != FileType::RegularFile || stat.st_nlink > 1 {
            return Err(self.not_plain(name, kind));
        }
        Ok(())
    }

    /// The sentence for `name` in this store being a link.
    pub(crate) fn outside(&self, name: &str) -> String {
        reaches_outside(&self.said, &self.within, &self.below, Some(name))
    }

    /// The sentence for `name`, of `kind`, not being a plain file: a link (or a file with
    /// another name) reaches outside; anything else is said for what it is.
    pub(crate) fn not_plain(&self, name: &str, kind: FileType) -> String {
        match kind {
            FileType::Symlink | FileType::RegularFile => self.outside(name),
            FileType::Directory => format!(
                "{} holds {}/{name}, which is a directory, so nothing was done",
                self.said, self.below
            ),
            _ => format!(
                "{} holds {}/{name}, which is neither a file nor a directory, so nothing was done",
                self.said, self.below
            ),
        }
    }

    fn failed(&self, e: impl std::fmt::Display) -> String {
        format!("{} could not be used: {e}", self.said)
    }
}

/// The entries of the directory `fd` holds, `.` and `..` left out, each with its own type as
/// `lstat` gives it, and a regular file with more than one name read as a link.
fn entries(fd: rustix::fd::BorrowedFd<'_>) -> rustix::io::Result<Vec<(String, FileType)>> {
    let mut out = Vec::new();
    let mut dir = rustix::fs::Dir::read_from(fd)?;
    while let Some(entry) = dir.read() {
        let entry = entry?;
        let Ok(name) = entry.file_name().to_str() else {
            // A name charter could not have written, and cannot say: refused as unreadable.
            return Err(rustix::io::Errno::ILSEQ);
        };
        if name == "." || name == ".." {
            continue;
        }
        let stat = rustix::fs::statat(fd, name, AtFlags::SYMLINK_NOFOLLOW)?;
        let kind = FileType::from_raw_mode(stat.st_mode);
        let kind = if kind == FileType::RegularFile && stat.st_nlink > 1 {
            FileType::Symlink
        } else {
            kind
        };
        out.push((name.to_owned(), kind));
    }
    Ok(out)
}

/// What a store may not hold, for the MCP tools.
enum Odd {
    /// A symbolic link, a file with more than one name, an entry that cannot be looked at, or
    /// a tree deeper than [`DEEPEST`]: each a way out of the workspace.
    Link(String),
    /// A FIFO, a socket or a device, which charter never writes and a read or an append could
    /// wait on for ever.
    NotAFile(String),
}

/// The first entry under the directory `fd` holds that the store may not hold, as a path
/// below the store.
fn odd_inside(fd: rustix::fd::BorrowedFd<'_>, below: &str, depth: usize) -> Option<Odd> {
    let here = |name: &str| {
        if below.is_empty() {
            name.to_owned()
        } else {
            format!("{below}/{name}")
        }
    };
    let Ok(found) = entries(fd) else {
        return Some(Odd::Link(if below.is_empty() {
            ".".to_owned()
        } else {
            below.to_owned()
        }));
    };
    for (name, kind) in found {
        match kind {
            FileType::RegularFile => {}
            FileType::Symlink => return Some(Odd::Link(here(&name))),
            FileType::Directory => {
                if depth >= DEEPEST {
                    return Some(Odd::Link(here(&name)));
                }
                let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
                let Ok(child) = rustix::fs::openat(fd, name.as_str(), flags, Mode::empty()) else {
                    return Some(Odd::Link(here(&name)));
                };
                if let Some(odd) = odd_inside(child.as_fd(), &here(&name), depth + 1) {
                    return Some(odd);
                }
            }
            _ => return Some(Odd::NotAFile(here(&name))),
        }
    }
    None
}
