//! A chat's store, held open for one tool call (V74).
//!
//! **Every read and write goes through descriptors, and nothing is looked up by path once the
//! store is open.** The project's root is opened, then `workspaces`, the workspace and the store
//! one component at a time with `O_NOFOLLOW | O_DIRECTORY`, and every file in the store is
//! opened, created, renamed and unlinked relative to that descriptor, with `O_NOFOLLOW`. A chat
//! that swaps its store for a link while a tool runs (a rename-swap loop in its own workspace)
//! either makes the open fail, and the call is refused, or loses the race to a store the
//! server already holds, which stays inside the workspace whatever its name is by then. A
//! check followed by a write by path cannot promise that (measured: 34 of 1,500 writes landed
//! in a persona's memory).
//!
//! The same flock on the store's directory as [`crate::rewrite::Lock`] is taken on the held
//! descriptor, so a tool and a `charter` command writing the same store take turns.
//!
//! **No link inside either.** A store holding a symbolic link, a file with more than one name
//! (a hard link can name a file outside), or a tree deeper than charter writes is refused whole
//! before anything is read, and each file opened is checked again on its descriptor.

use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};

use rustix::fs::{AtFlags, FileType, FlockOperation, Mode, OFlags};

use crate::active::Place;

/// How deep the store's tree may go before it is refused. charter writes a level or two.
const DEEPEST: usize = 8;

/// The mode a new file is created with, before the umask: what `std::fs::File::create` uses.
fn new_file() -> Mode {
    Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::WGRP | Mode::ROTH | Mode::WOTH
}

/// The mode a new store directory is created with, before the umask.
fn new_dir() -> Mode {
    Mode::RWXU | Mode::RWXG | Mode::RWXO
}

/// One store of the chat's place, held open and locked.
pub(super) struct Store {
    fd: OwnedFd,
    /// The store's name, for a sentence: `memory`, `todos`, `sessions`, `changes`.
    name: String,
}

/// The sentence for a store that reaches outside its workspace. Never the absolute path.
pub(super) fn reaches_outside(store: &str, inside: Option<&str>) -> String {
    match inside {
        Some(entry) => format!(
            "the chat's {store} reaches outside its workspace through a link ({store}/{entry}), \
             so nothing was done"
        ),
        None => format!(
            "the chat's {store} reaches outside its workspace through a link, so nothing was done"
        ),
    }
}

impl Store {
    /// `store` of `place` in the project at `root`, opened without following a link, made when
    /// it is not there yet and `make` is true. `Ok(None)` for a store that is not there and is
    /// not to be made.
    pub(super) fn open(
        root: &std::path::Path,
        place: &Place,
        store: &str,
        make: bool,
    ) -> Result<Option<Self>, String> {
        let dir = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
        let step = dir | OFlags::NOFOLLOW;
        // The root is the project, named by the chat's environment, not by anything a chat can
        // plant: it is opened as it is named.
        let mut fd = rustix::fs::open(root, dir, Mode::empty())
            .map_err(|e| format!("the project cannot be opened: {e}"))?;
        if let Place::Workspace(ws) = place {
            if !crate::contain::workspace_name_ok(ws) {
                return Err(format!("'{ws}' is not a workspace's name"));
            }
            for (component, said) in [("workspaces", "the workspaces"), (ws.as_str(), ws)] {
                fd = rustix::fs::openat(&fd, component, step, Mode::empty()).map_err(|e| {
                    if e == rustix::io::Errno::NOENT {
                        format!("no workspace '{ws}'")
                    } else {
                        format!("{said} cannot be opened without following a link ({e})")
                    }
                })?;
            }
        }
        let held = match rustix::fs::openat(&fd, store, step, Mode::empty()) {
            Ok(held) => held,
            Err(rustix::io::Errno::NOENT) if make => {
                match rustix::fs::mkdirat(&fd, store, new_dir()) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                    Err(e) => return Err(format!("the chat's {store} cannot be made: {e}")),
                }
                rustix::fs::openat(&fd, store, step, Mode::empty())
                    .map_err(|_| reaches_outside(store, None))?
            }
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(_) => return Err(reaches_outside(store, None)),
        };
        let held = Self {
            fd: held,
            name: store.to_owned(),
        };
        // Best effort, as `rewrite::Lock` is: released when the descriptor closes.
        let _ = rustix::fs::flock(&held.fd, FlockOperation::LockExclusive);
        match odd_inside(held.fd.as_fd(), "", 0) {
            None => {}
            Some(Odd::Link(entry)) => return Err(reaches_outside(store, Some(&entry))),
            Some(Odd::NotAFile(entry)) => {
                return Err(format!(
                    "the chat's {store} holds {store}/{entry}, which is neither a file nor a \
                     directory, so nothing was done"
                ));
            }
        }
        Ok(Some(held))
    }

    /// The names of the regular files directly in the store, sorted.
    pub(super) fn files(&self) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        for (name, kind) in entries(self.fd.as_fd()).map_err(|e| self.failed(e))? {
            if kind == FileType::RegularFile {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    /// `name`'s text, read through the store: `None` when it is not there. A file that is not
    /// a plain file with one name, or past [`crate::memstore::MAX_BYTES`], is refused.
    pub(super) fn read(&self, name: &str) -> Result<Option<String>, String> {
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let fd = match rustix::fs::openat(&self.fd, name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(rustix::io::Errno::LOOP) => return Err(reaches_outside(&self.name, Some(name))),
            Err(e) => return Err(self.failed(e)),
        };
        self.one_plain_file(&fd, name)?;
        let mut bytes = Vec::new();
        std::fs::File::from(fd)
            .take(crate::memstore::MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| self.failed(e))?;
        if bytes.len() as u64 > crate::memstore::MAX_BYTES {
            return Err(format!("{}/{name} is too big to read", self.name));
        }
        Ok(crate::memstore::text_of(bytes))
    }

    /// Write `bytes` as `name`, which must not be there yet. `false` when it is.
    pub(super) fn create(&self, name: &str, bytes: &[u8]) -> Result<bool, String> {
        let flags =
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd = match rustix::fs::openat(&self.fd, name, flags, new_file()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::EXIST) => return Ok(false),
            Err(e) => return Err(self.failed(e)),
        };
        std::fs::File::from(fd)
            .write_all(bytes)
            .map_err(|e| self.failed(e))?;
        Ok(true)
    }

    /// Add `bytes` to the end of `name`, making it with `first` in front when it is not there.
    pub(super) fn append(&self, name: &str, first: &str, bytes: &[u8]) -> Result<(), String> {
        let mut whole = first.as_bytes().to_vec();
        whole.extend_from_slice(bytes);
        if self.create(name, &whole)? {
            return Ok(());
        }
        // Non-blocking: a FIFO planted at the name must never hold the call, and the store's
        // lock with it, waiting for a reader.
        let flags =
            OFlags::WRONLY | OFlags::APPEND | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let fd = rustix::fs::openat(&self.fd, name, flags, Mode::empty()).map_err(|e| {
            if e == rustix::io::Errno::LOOP {
                reaches_outside(&self.name, Some(name))
            } else {
                self.failed(e)
            }
        })?;
        self.one_plain_file(&fd, name)?;
        std::fs::File::from(fd)
            .write_all(bytes)
            .map_err(|e| self.failed(e))
    }

    /// Replace `name` whole with `bytes`: written beside it, then renamed over it, both within
    /// the held store. The file keeps its mode.
    pub(super) fn replace(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let mode = match rustix::fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => Mode::from_raw_mode(stat.st_mode as rustix::fs::RawMode) & Mode::all(),
            Err(_) => new_file(),
        };
        let tmp = format!(".{name}.charter-{}", std::process::id());
        let _ = rustix::fs::unlinkat(&self.fd, tmp.as_str(), AtFlags::empty());
        let flags =
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd =
            rustix::fs::openat(&self.fd, tmp.as_str(), flags, mode).map_err(|e| self.failed(e))?;
        let written = std::fs::File::from(fd).write_all(bytes);
        if let Err(e) = written {
            let _ = rustix::fs::unlinkat(&self.fd, tmp.as_str(), AtFlags::empty());
            return Err(self.failed(e));
        }
        rustix::fs::renameat(&self.fd, tmp.as_str(), &self.fd, name).map_err(|e| {
            let _ = rustix::fs::unlinkat(&self.fd, tmp.as_str(), AtFlags::empty());
            self.failed(e)
        })
    }

    /// Unlink `name` from the store.
    pub(super) fn remove(&self, name: &str) -> Result<(), String> {
        rustix::fs::unlinkat(&self.fd, name, AtFlags::empty()).map_err(|e| self.failed(e))
    }

    /// Whether `fd` is a plain file with one name.
    fn one_plain_file(&self, fd: &OwnedFd, name: &str) -> Result<(), String> {
        let stat = rustix::fs::fstat(fd).map_err(|e| self.failed(e))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile || stat.st_nlink > 1 {
            return Err(reaches_outside(&self.name, Some(name)));
        }
        Ok(())
    }

    fn failed(&self, e: impl std::fmt::Display) -> String {
        format!("the chat's {} could not be used: {e}", self.name)
    }
}

/// The entries of the directory `fd` holds, `.` and `..` left out, each with its own type as
/// `lstat` gives it.
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

/// What a store may not hold.
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
