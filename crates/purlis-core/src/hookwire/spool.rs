//! The hook spool (FD-30, ADR 0068 §6): where a hook's line goes when the host does not take it.
//!
//! **One spool per chat**, never one per project: the folder `<n>/` in the `spool/` directory
//! beside the hook socket ([`dir_for`]), so `.charter/app/spool/<n>/` in a project. A hook whose
//! line the host did not take writes it there as a file of its own, with the next number in
//! that chat's sequence and a MAC, and makes it durable before the hook answers its harness
//! ([`append`]). The host drains every spool when it starts ([`drain`]): it checks each line,
//! hands on the ones that check, and says what it found — a line that fails its check is
//! [`Drained::Rejected`], and a number missing from the middle of a sequence is
//! [`Drained::Gap`]. Nothing is dropped silently (O1).
//!
//! **A file per line, and no lock between hooks** (#983). Hooks of one chat that spool at once
//! (parallel tool calls and sub-agents do) never wait for each other: a hook takes its number
//! by making `<key>.<seq>.part` exclusively, which the file system answers at once, writes and
//! syncs its line there, and links it as `<key>.<seq>.json`. A file with that name is whole and
//! is never written again, so the drain reads and removes exactly the files it listed while
//! hooks go on writing others. Before #983 a chat's spool was one file, `<n>.jsonl`, appended
//! under a lock that each holder kept across its sync, and a hook that waited 250 ms for it
//! lost its line. A file a hook of that build left, or still writes, is drained as it was
//! ([`drain_file`]); nothing writes one now.
//!
//! **What a line is checked with.** Each line carries a MAC under a key derived from the chat's
//! token ([`SpoolKey`]): HMAC-SHA256 of the token under a fixed label. The host writes each key
//! to `keys.json` as it issues the token ([`remember`]), so a host that started after the one
//! that issued it can still check the line. The key is not the token: holding it does not let
//! anything report on the live channel as the chat. A line is that chat's only if it is in that
//! chat's spool and checks under a key the host issued to that chat.
//!
//! **What it holds, stated plainly** (ADR 0068 §6, as amended by V63). A process holding a
//! chat's token can write lines as that chat, as it can on the live channel. It cannot write
//! lines that read as another chat's. A number missing below the highest one a key's lines hold
//! is a gap the drain finds, whether or not a `.part` file still holds it; lines removed from
//! the end, or removed and then followed by new ones, are not found. A sandboxed chat is denied
//! reading and writing the whole directory, keys included (ADR 0067 §5's integrity class).
//! Claude Code runs its hooks outside the sandbox it gives its tools, so they still reach it.
//! opencode's and Codex's hooks run inside purlis's wrap (ADR 0067 as amended by V73c and
//! #1123), so the spool is denied to them too: a line of theirs the host does not take is not
//! kept, and the hook says so.
//!
//! **Only where that denial reaches** ([`covered`]): a project's `.charter/app/spool/`. Beside
//! a hook socket anywhere else (the fallback for a project whose path is too long for a
//! socket) nothing is spooled and no key is written, and the hook says the line is lost.
//!
//! **Numbers are per key.** A key is one token, and a token is one start of a chat, so its
//! sequence starts at 1 and every one of its lines is drained at the same start of the host:
//! the first one after that start of the chat ended.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rustix::fs::{AtFlags, Mode, OFlags};

use super::{ChatToken, CommitRefused, Line, Report, ToolCall};

/// The directory beside the hook socket that holds every chat's spool.
pub const DIR: &str = "spool";

/// The host's record of the key each token it issued checks under.
pub const KEYS: &str = "keys.json";

/// The label a chat's spool key is derived from its token under.
const LABEL: &[u8] = b"charter hook spool v1";

/// The version of `keys.json` and of a spool line.
const VERSION: u32 = 1;

/// How long [`append`] gives a line to be written and made durable.
///
/// A hook spools only after the host did not take its line in time, and the harness is waiting
/// for the hook's answer meanwhile: a guard's decided verdict must reach it before the harness's
/// own timeout runs out, or the harness runs the tool. So the wait is bounded, and a line the
/// disk does not take in it is said to be lost, never waited on. It is the hook's own write that
/// is timed, never another hook's: no hook waits for another (#983).
const A_LINE_IS_SPOOLED_WITHIN: Duration = Duration::from_secs(1);

/// The most names [`append`] lists in a chat's spool to choose the next number. A spool the host
/// has not drained grows by one hook line at a time, so this is a long while without a host; one
/// past it is refused rather than listed for as long as it takes, and the line is said to be
/// lost.
const A_SPOOL_HOLDS_AT_MOST: usize = 16_384;

/// How long after it was last written a `.part` file is a dead hook's, which the drain removes.
/// A hook lives for seconds ([`A_LINE_IS_SPOOLED_WITHIN`], and its harness's own timeout), so
/// nothing still being written is near this old.
const A_PART_IS_A_DEAD_HOOKS_AFTER: Duration = Duration::from_secs(60 * 60);

/// The most of one line's file the drain reads. A hook's line is a few kilobytes; a file past
/// this is not one, and is `unreadable`.
const A_LINE_IS_READ_UP_TO: u64 = 1024 * 1024;

/// The spool directory for the hook socket at `socket`: `spool/` beside it.
pub fn dir_for(socket: &Path) -> PathBuf {
    socket
        .parent()
        .map_or_else(|| PathBuf::from(DIR), |parent| parent.join(DIR))
}

/// Whether `dir` is a spool the chat sandbox's integrity denial covers: a project's
/// `.charter/app/spool/` or `.purlis/app/spool/` (ADR 0067 §5, `sandbox::Denied`, which denies
/// both). Nothing is spooled, and no key written, anywhere else (V63).
pub fn covered(dir: &Path) -> bool {
    let mut parts = dir.components().rev().map(|part| part.as_os_str());
    parts.next() == Some(DIR.as_ref())
        && parts.next() == Some("app".as_ref())
        && parts
            .next()
            .is_some_and(|state| crate::names::STATE_DIR.is(state))
}

/// [`covered`], or the refusal a hook reports as the line lost.
fn refused_unless_covered(dir: &Path) -> io::Result<()> {
    if covered(dir) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "{} is not a directory a sandboxed chat is denied, so nothing is spooled there",
            dir.display()
        ),
    ))
}

/// Holds an exclusive lock on the spool directory itself while `keys.json` is read and
/// rewritten, so two tokens issued at once never drop each other's key.
fn keys_locked<T>(dir: &Path, act: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    let held = File::open(dir)?;
    held.lock()?;
    let _held = crate::filelock::Held::locked(held);
    act()
}

/// Chat `chat`'s spool in `dir`: the folder its lines are files of.
fn folder_for(dir: &Path, chat: u32) -> PathBuf {
    dir.join(chat.to_string())
}

/// Chat `chat`'s spool in `dir` as a build before #983 wrote it: one file of lines. Read by the
/// drain, written by nothing.
fn file_for(dir: &Path, chat: u32) -> PathBuf {
    dir.join(format!("{chat}.jsonl"))
}

/// The key a chat's spool lines are checked under, derived from its token.
#[derive(Clone, PartialEq, Eq)]
pub struct SpoolKey([u8; 32]);

impl SpoolKey {
    /// The key `token` writes its spool lines under.
    pub fn of(token: &ChatToken) -> Self {
        Self(mac_over(token.expose().as_bytes(), &[LABEL]))
    }

    /// A short name for the key, which a line carries so the host knows which key to check
    /// it under: the first 16 hex characters of its SHA-256. It says nothing of the key.
    pub fn id(&self) -> String {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(self.0);
        crate::extension::hex(&digest)[..16].to_owned()
    }

    /// The MAC of one line: over the chat, its number, the key's id and the line's own bytes.
    fn sign(&self, chat: u32, seq: u64, id: &str, line: &str) -> String {
        let head = format!("{chat}\n{seq}\n{id}\n");
        crate::extension::hex(&mac_over(&self.0, &[head.as_bytes(), line.as_bytes()]))
    }

    /// Whether `mac` is the MAC of that line, compared in constant time.
    fn checks(&self, chat: u32, seq: u64, id: &str, line: &str, mac: &str) -> bool {
        use subtle::ConstantTimeEq;
        bool::from(
            self.sign(chat, seq, id, line)
                .as_bytes()
                .ct_eq(mac.as_bytes()),
        )
    }
}

impl std::fmt::Debug for SpoolKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SpoolKey(..)")
    }
}

fn mac_over(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    use hmac::{KeyInit, Mac};
    let mut mac =
        hmac::Hmac::<sha2::Sha256>::new_from_slice(key).expect("HMAC takes a key of any length");
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

/// One spooled line, as its file holds it. `line` is the hook's line as JSON text, so the MAC
/// is over the exact bytes the hook wrote.
#[derive(serde::Serialize, serde::Deserialize)]
struct OnDisk {
    v: u32,
    seq: u64,
    key: String,
    line: String,
    mac: String,
}

/// One key in `keys.json`.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Held {
    id: String,
    chat: u32,
    key: String,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Keys {
    v: u32,
    keys: Vec<Held>,
}

/// Makes `dir`, owner-only, where it is not there.
fn private(dir: &Path) -> io::Result<()> {
    crate::secrets::make_private_dir(dir)
}

/// What a file in a chat's spool folder is, by its name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `<key>.<seq>.json`: a whole line.
    Line,
    /// `<key>.<seq>.part`: a number a hook took, and its line while it is written.
    Part,
}

/// A name in a chat's spool folder that is one a hook gives: the key's id, the number, and
/// which of the two files it is.
struct Named<'a> {
    key: &'a str,
    seq: u64,
    kind: Kind,
}

/// `name` read as `<16 hex>.<decimal>.json` or `.part`, and nothing else.
fn named(name: &CStr) -> Option<Named<'_>> {
    let mut parts = name.to_str().ok()?.split('.');
    let (key, seq, kind) = (parts.next()?, parts.next()?, parts.next()?);
    let kind = match (kind, parts.next()) {
        ("json", None) => Kind::Line,
        ("part", None) => Kind::Part,
        _ => return None,
    };
    let reads = key.len() == 16
        && key.bytes().all(|byte| byte.is_ascii_hexdigit())
        && !seq.is_empty()
        && seq.bytes().all(|byte| byte.is_ascii_digit());
    Some(Named {
        key,
        seq: reads.then(|| seq.parse().ok())??,
        kind,
    })
}

/// The name of key `key`'s file for number `seq`.
fn name_of(key: &str, seq: u64, kind: Kind) -> String {
    match kind {
        Kind::Line => format!("{key}.{seq}.json"),
        Kind::Part => format!("{key}.{seq}.part"),
    }
}

/// The refusal for a spool that holds as many names as a hook lists.
fn too_many(lines: usize) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("this chat's spool already holds {lines} lines, as many as a hook lists"),
    )
}

/// One chat's spool folder, held open. Every file in it is made, read, linked and removed
/// through this descriptor, so nothing is done through a path swapped after the folder was
/// opened.
struct Folder {
    held: File,
    at: PathBuf,
}

impl Folder {
    /// Opens chat `chat`'s folder in `dir`, never through a link and never blocking. Anything
    /// at the path but a directory is refused as [`io::ErrorKind::InvalidInput`], at once: a
    /// named pipe planted there would otherwise hold the hook on an open that never ends.
    fn open(dir: &Path, chat: u32) -> io::Result<Self> {
        let at = folder_for(dir, chat);
        let flags = OFlags::RDONLY
            | OFlags::DIRECTORY
            | OFlags::NOFOLLOW
            | OFlags::NONBLOCK
            | OFlags::CLOEXEC;
        match rustix::fs::open(&at, flags, Mode::empty()) {
            Ok(held) => Ok(Self {
                held: File::from(held),
                at,
            }),
            Err(rustix::io::Errno::NOTDIR | rustix::io::Errno::LOOP) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} is not a folder, so nothing is spooled there",
                    at.display()
                ),
            )),
            Err(why) => Err(why.into()),
        }
    }

    /// [`Folder::open`] for a hook, which makes the folder, owner-only, where it is not there,
    /// and says whether it did.
    fn of_a_hook(dir: &Path, chat: u32) -> io::Result<(Self, bool)> {
        use std::os::unix::fs::DirBuilderExt;
        let at = folder_for(dir, chat);
        let made = match std::fs::DirBuilder::new().mode(0o700).create(&at) {
            Ok(()) => true,
            Err(why) if why.kind() == io::ErrorKind::AlreadyExists => false,
            Err(why) => return Err(crate::rewrite::refused_at(&at)(why)),
        };
        Ok((Self::open(dir, chat)?, made))
    }

    /// Every name in the folder, each once and in order, refused past `at_most` of them
    /// ([`too_many`]). A name a hook gives or takes away meanwhile may or may not be among
    /// them, which is all a listing of a folder being written promises.
    fn names(&self, at_most: usize) -> io::Result<Vec<CString>> {
        let mut names = Vec::new();
        for entry in rustix::fs::Dir::read_from(&self.held)? {
            let entry = entry?;
            let name = entry.file_name();
            if matches!(name.to_bytes(), b"." | b"..") {
                continue;
            }
            if names.len() >= at_most {
                return Err(too_many(at_most));
            }
            names.push(name.to_owned());
        }
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Makes the file `name`, 0600, only if nothing has that name: the one step two hooks
    /// cannot both take.
    fn make(&self, name: &str) -> io::Result<File> {
        let flags =
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        match rustix::fs::openat(&self.held, name, flags, Mode::from_raw_mode(0o600)) {
            Ok(made) => Ok(File::from(made)),
            Err(rustix::io::Errno::EXIST) => Err(io::ErrorKind::AlreadyExists.into()),
            Err(why) => Err(crate::rewrite::refused_at(&self.at.join(name))(why.into())),
        }
    }

    /// Whether anything in the folder has the name `name`.
    fn has(&self, name: &str) -> io::Result<bool> {
        match rustix::fs::statat(&self.held, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Ok(true),
            Err(rustix::io::Errno::NOENT) => Ok(false),
            Err(why) => Err(why.into()),
        }
    }

    /// Gives the file `from` the name `to` as well, only if nothing has that name.
    fn link(&self, from: &str, to: &str) -> io::Result<()> {
        Ok(rustix::fs::linkat(
            &self.held,
            from,
            &self.held,
            to,
            AtFlags::empty(),
        )?)
    }

    /// Takes the name `name` away. A name that is already gone is as asked.
    fn remove(&self, name: impl rustix::path::Arg) -> io::Result<()> {
        match rustix::fs::unlinkat(&self.held, name, AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => Ok(()),
            Err(why) => Err(why.into()),
        }
    }

    /// Opens what has the name `name` for reading, never through a link and never blocking.
    fn opened(&self, name: impl rustix::path::Arg) -> io::Result<File> {
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        Ok(File::from(rustix::fs::openat(
            &self.held,
            name,
            flags,
            Mode::empty(),
        )?))
    }

    /// The plain file `name`, whole. Anything else at the name, and a file past
    /// [`A_LINE_IS_READ_UP_TO`], is [`io::ErrorKind::InvalidData`].
    fn read(&self, name: &CStr) -> io::Result<Vec<u8>> {
        let file = self.opened(name)?;
        if !file.metadata()?.is_file() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut bytes = Vec::new();
        (&file)
            .take(A_LINE_IS_READ_UP_TO + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > A_LINE_IS_READ_UP_TO {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(bytes)
    }

    /// Whether the name `name` is the file `file`: the same one, not another of that name.
    fn is(&self, name: &str, file: &File) -> io::Result<bool> {
        use std::os::unix::fs::MetadataExt;
        let (named, held) = (self.opened(name)?.metadata()?, file.metadata()?);
        Ok(named.ino() == held.ino() && named.dev() == held.dev())
    }

    /// Removes the name `name` if it is still the file `file`, which a hook made under it: a
    /// name that has become another hook's file is not this one's to remove.
    fn let_go(&self, name: &str, file: &File) {
        if self.is(name, file).unwrap_or(false) {
            let _ = self.remove(name);
        }
    }

    /// Whether the `.part` file `name` is a dead hook's: last written long enough ago
    /// ([`A_PART_IS_A_DEAD_HOOKS_AFTER`]), or at a time that has not come, which no hook
    /// writing now gives it. One that cannot be asked is left.
    fn is_a_dead_hooks(&self, name: &CStr) -> bool {
        let written = self
            .opened(name)
            .and_then(|part| part.metadata()?.modified());
        written.is_ok_and(|written| {
            !written
                .elapsed()
                .is_ok_and(|age| age < A_PART_IS_A_DEAD_HOOKS_AFTER)
        })
    }

    /// Makes the folder's names durable.
    fn sync(&self) -> io::Result<()> {
        Ok(rustix::fs::fsync(&self.held)?)
    }

    /// Takes key `key`'s next number: makes its `.part` file, and answers the number and the
    /// file. The number is the one after the highest the folder's names hold for the key, or
    /// the one after that for each hook that took a number first, so a number is given only
    /// once the one before it is taken. Each number another hook took is a line the spool
    /// holds, so the tries are counted against `lines` across every call for one line
    /// (`tries`), and at `lines` of either the spool is too full ([`too_many`]).
    fn take_a_number(&self, key: &str, lines: usize, tries: &mut usize) -> io::Result<(u64, File)> {
        let names = self.names(lines)?;
        if names.len() >= lines {
            return Err(too_many(lines));
        }
        let mut seq = names
            .iter()
            .filter_map(|name| named(name))
            .filter(|name| name.key == key)
            .map(|name| name.seq)
            .max()
            .unwrap_or(0);
        loop {
            seq = seq.checked_add(1).ok_or_else(|| {
                io::Error::other("this chat's spool has used every number there is")
            })?;
            if *tries >= lines {
                return Err(too_many(lines));
            }
            *tries += 1;
            let part = name_of(key, seq, Kind::Part);
            match self.make(&part) {
                // A hook that listed the folder before this number's line was finished, and
                // made the `.part` after its writer had let it go: the number is taken.
                Ok(_) if self.has(&name_of(key, seq, Kind::Line))? => self.remove(&*part)?,
                Ok(made) => return Ok((seq, made)),
                Err(why) if why.kind() == io::ErrorKind::AlreadyExists => {}
                Err(why) => return Err(why),
            }
        }
    }
}

/// `fsync`s the directory `dir`, opened without blocking and only as a directory.
fn sync_dir(dir: &Path) -> io::Result<()> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NONBLOCK | OFlags::CLOEXEC;
    Ok(rustix::fs::fsync(rustix::fs::open(
        dir,
        flags,
        Mode::empty(),
    )?)?)
}

/// What [`append`] is bound by. A hook's are [`Bounds::A_HOOKS`]; a test narrows them, and
/// stands a slow or a full disk in.
#[derive(Clone, Copy)]
struct Bounds {
    /// The most names listed in the chat's spool ([`A_SPOOL_HOLDS_AT_MOST`]).
    lines: usize,
    /// How long the line is given ([`A_LINE_IS_SPOOLED_WITHIN`]).
    wait: Duration,
    /// Called once per line, after it is written and before it is synced: the disk taking it.
    a_line_reaches_the_disk: fn() -> io::Result<()>,
}

impl Bounds {
    const A_HOOKS: Self = Self {
        lines: A_SPOOL_HOLDS_AT_MOST,
        wait: A_LINE_IS_SPOOLED_WITHIN,
        a_line_reaches_the_disk: || Ok(()),
    };
}

/// Writes `what`, chat `chat`'s line, into its spool in `dir`, under `token`'s key and the next
/// number in that key's sequence, and answers the number.
///
/// **No hook waits for another** (#983). The number is taken by making a file exclusively, and
/// the line is written to a file of its own ([`write_line`]), so however many hooks of one chat
/// spool at once, each is held up by its own write and nothing else. A line is lost, and said
/// to be, only for what its own write met: a disk that refused it or did not take it in time,
/// or a spool already too full.
///
/// **Survives a host crash when it returns** (FD-9's widened acceptance): the line is
/// `fsync`ed, and its folder with it, before this answers. It is the operating system's
/// ordinary `fsync`, which on macOS does not reach through the drive's own cache, so a power
/// loss there can still lose it.
///
/// **Bounded** ([`A_LINE_IS_SPOOLED_WITHIN`]): a hook answers its harness after this returns
/// (ADR 0075 §7), so a disk that does not answer must not hold a decided verdict past the
/// harness's timeout. The write runs on a thread of its own, and once the wait runs out this
/// answers [`io::ErrorKind::TimedOut`] and the hook says on stderr that the line may be lost.
/// The write is then left to finish or not, and the hook's exit ends it, so the drain may
/// still find the line: said to be perhaps lost, and perhaps recorded. The folder is opened
/// without blocking and only as a directory ([`Folder::open`]), and at most
/// [`A_SPOOL_HOLDS_AT_MOST`] names are listed in it.
///
/// Refused where the sandbox's integrity denial does not reach ([`covered`]).
pub fn append(
    dir: &Path,
    chat: u32,
    token: &ChatToken,
    what: &impl serde::Serialize,
) -> io::Result<u64> {
    append_within(dir, chat, token, what, Bounds::A_HOOKS)
}

/// [`append`], within `bounds`.
fn append_within(
    dir: &Path,
    chat: u32,
    token: &ChatToken,
    what: &impl serde::Serialize,
    bounds: Bounds,
) -> io::Result<u64> {
    refused_unless_covered(dir)?;
    private(dir)?;
    let key = SpoolKey::of(token);
    let line = serde_json::to_string(what).map_err(io::Error::other)?;
    let dir = dir.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("purlis-hook-spool".into())
        .spawn(move || {
            let _ = tx.send(write_line(&dir, chat, &key, line, bounds));
        })?;
    use std::sync::mpsc::RecvTimeoutError;
    match rx.recv_timeout(bounds.wait) {
        Ok(answered) => answered,
        Err(RecvTimeoutError::Timeout) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "this chat's spool did not take the line within {} ms",
                bounds.wait.as_millis()
            ),
        )),
        // The thread ended without an answer, which only a panic in the write does.
        Err(RecvTimeoutError::Disconnected) => Err(io::Error::other(
            "the write to this chat's spool stopped before it answered",
        )),
    }
}

/// [`append`]'s write: a number taken, the line written and synced under it, and given its
/// name.
///
/// 1. The number is taken by making `<key>.<seq>.part` ([`Folder::take_a_number`]).
/// 2. The line, with that number and its MAC, is written into that file and `fsync`ed.
/// 3. The file is linked as `<key>.<seq>.json`, which fails rather than replace a line, and the
///    `.part` name is removed.
/// 4. The folder is `fsync`ed, and `dir` too if the folder is new.
///
/// A write the disk refuses leaves nothing: the `.part` file is removed and the number is free
/// again. A hook that dies in the middle leaves the `.part` file, which holds the number and is
/// never read as a line, so the drain finds the gap; the drain removes the file once it is an
/// hour old ([`A_PART_IS_A_DEAD_HOOKS_AFTER`]). A hook that slow would find its file gone, or
/// its name another hook's, and takes another number: the name it linked is checked to be the
/// file it wrote before this answers. A line's name that is already gone when it is checked
/// was read and removed by a drain, so the line landed.
fn write_line(
    dir: &Path,
    chat: u32,
    key: &SpoolKey,
    line: String,
    bounds: Bounds,
) -> io::Result<u64> {
    let (folder, made) = Folder::of_a_hook(dir, chat)?;
    let mut on_disk = OnDisk {
        v: VERSION,
        seq: 0,
        key: key.id(),
        line,
        mac: String::new(),
    };
    let mut tries = 0;
    loop {
        let (seq, mut part) = folder.take_a_number(&on_disk.key, bounds.lines, &mut tries)?;
        on_disk.seq = seq;
        on_disk.mac = key.sign(chat, seq, &on_disk.key, &on_disk.line);
        let mut bytes = serde_json::to_vec(&on_disk).map_err(io::Error::other)?;
        bytes.push(b'\n');
        let name = name_of(&on_disk.key, seq, Kind::Part);
        let written = part
            .write_all(&bytes)
            .and_then(|()| (bounds.a_line_reaches_the_disk)())
            .and_then(|()| Ok(rustix::fs::fsync(&part)?));
        if let Err(why) = written {
            folder.let_go(&name, &part);
            return Err(why);
        }
        let line = name_of(&on_disk.key, seq, Kind::Line);
        match folder.link(&name, &line) {
            Ok(()) => match folder.is(&line, &part) {
                Ok(true) => {}
                // The line's name is gone already: a drain read it and removed it between the
                // link and this look, so it landed.
                Err(why) if why.kind() == io::ErrorKind::NotFound => {}
                // The `.part` name had become another hook's file, and that is what was
                // linked: a drain took this hook's file for a dead hook's. Neither name is
                // this hook's, and its line goes under another number.
                Ok(false) => continue,
                Err(why) => return Err(why),
            },
            // The `.part` file is gone, taken for a dead hook's: another number.
            Err(why) if why.kind() == io::ErrorKind::NotFound => continue,
            // The number already has a line, another hook's: another number.
            Err(why) if why.kind() == io::ErrorKind::AlreadyExists => {
                folder.let_go(&name, &part);
                continue;
            }
            Err(why) => {
                folder.let_go(&name, &part);
                return Err(why);
            }
        }
        folder.let_go(&name, &part);
        folder.sync()?;
        if made {
            sync_dir(dir)?;
        }
        return Ok(seq);
    }
}

/// Records, in `dir`'s `keys.json`, the key chat `chat`'s spool lines under `token` check with.
///
/// Durable before it returns, and called before the token reaches the chat, so a line the
/// chat spools can always be checked by the next host. Refused where the sandbox's integrity
/// denial does not reach ([`covered`]): the key is a verifier at rest, owner-only, and a
/// sandboxed chat may neither read nor write it (V63).
pub fn remember(dir: &Path, chat: u32, token: &ChatToken) -> io::Result<()> {
    refused_unless_covered(dir)?;
    private(dir)?;
    let key = SpoolKey::of(token);
    let id = key.id();
    keys_locked(dir, || {
        let mut keys = read_keys(dir)?;
        if !keys.keys.iter().any(|held| held.id == id) {
            keys.keys.push(Held {
                id,
                chat,
                key: crate::extension::hex(&key.0),
            });
        }
        write_keys(dir, &keys)
    })
}

/// The keys in `dir`'s `keys.json`. A file that is not there, or that does not read as keys,
/// holds none: a line under a key it lost is `no-key` at the drain, and nothing is stuck on it.
fn read_keys(dir: &Path) -> io::Result<Keys> {
    match crate::contain::read_no_link(dir, &dir.join(KEYS)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_else(|why| {
            tracing::warn!(
                "purlis: {} does not read as spool keys ({why}); every line under them is \
                 rejected as no-key",
                dir.join(KEYS).display()
            );
            Keys::new()
        })),
        Err(why) if why.kind() == io::ErrorKind::NotFound => Ok(Keys::new()),
        Err(why) => Err(why),
    }
}

impl Keys {
    fn new() -> Self {
        Self {
            v: VERSION,
            keys: Vec::new(),
        }
    }
}

/// The lines of `bytes`, each as UTF-8 or the bytes it is: a line a crash tore through a
/// character, or one somebody wrote that is not text, is one line that does not read, never
/// the end of the file.
fn lines_of(bytes: &[u8]) -> impl Iterator<Item = Result<&str, &[u8]>> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .map(|line| std::str::from_utf8(line).map_err(|_| line))
}

fn write_keys(dir: &Path, keys: &Keys) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(keys).map_err(io::Error::other)?;
    bytes.push(b'\n');
    crate::rewrite::replace(dir, &dir.join(KEYS), &bytes, crate::rewrite::Mode::Private)
}

fn key_of(hex: &str) -> Option<SpoolKey> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| {
            hex.get(at..at + 2)
                .and_then(|two| u8::from_str_radix(two, 16).ok())
        })
        .collect::<Option<_>>()?;
    Some(SpoolKey(bytes.try_into().ok()?))
}

/// A line the drain checked and hands on.
#[derive(Debug, Clone, PartialEq)]
pub enum Spooled {
    Report(Report),
    Tool(ToolCall),
    Refused(CommitRefused),
}

/// What a drain found, one item at a time, in each chat's order.
#[derive(Debug, Clone, PartialEq)]
pub enum Drained {
    /// A line that checked: number `seq` in chat `chat`'s spool.
    Line { chat: u32, seq: u64, line: Spooled },
    /// Numbers `from` to `to` of one of chat `chat`'s sequences are not in its spool.
    Gap { chat: u32, from: u64, to: u64 },
    /// A line in chat `chat`'s spool that did not check, or a number a hook took and has no
    /// line under, and why. `seq` is the number it claimed, where it could be read.
    Rejected {
        chat: u32,
        seq: Option<u64>,
        why: &'static str,
    },
    /// One of chat `chat`'s sequences, drained: it ran from `from` to `to`.
    Spool { chat: u32, from: u64, to: u64 },
}

impl Drained {
    /// The number of the chat whose spool it was found in.
    pub fn chat(&self) -> u32 {
        match self {
            Self::Line { chat, .. }
            | Self::Gap { chat, .. }
            | Self::Rejected { chat, .. }
            | Self::Spool { chat, .. } => *chat,
        }
    }
}

/// Why a line was rejected, in the words an event says it in.
pub mod why {
    /// The line is not a spool line.
    pub const UNREADABLE: &str = "unreadable";
    /// No key the host issued has the line's key id.
    pub const NO_KEY: &str = "no-key";
    /// The key was issued to another chat than the one whose spool the line is in.
    pub const ANOTHER_CHATS_KEY: &str = "another-chats-key";
    /// The MAC is not the line's.
    pub const MAC: &str = "mac";
    /// The line checks, and names another chat, or is not a line a spool holds.
    pub const NOT_THIS_CHATS: &str = "not-this-chats";
    /// A number the sequence already had, or a line another of the chat's stores already gave.
    pub const REPEATED: &str = "repeated";
    /// A number a hook took and has no line under: it is still writing it, or died before it
    /// had.
    pub const UNFINISHED: &str = "unfinished";
}

/// Drains every spool in `dir`: each line checked and handed to `each` in its chat's order,
/// with each gap, each rejected line and each sequence drained.
///
/// **A line leaves its spool only once `each` has taken it.** `each` answers once what it was
/// handed is recorded durably; the first error stops the drain, and the spool it was draining
/// is left as it was, to be drained again at the next start. A line already handed on may
/// then be handed on twice, which is the side a drain errs on.
///
/// **Hooks may spool while it runs**, and no line it read is handed on twice: it removes the
/// files it read and the names it could not read, and a hook never writes a file that has a
/// line's name. **A line still being written while it runs is not recorded by it.** Its number
/// is a gap at this drain if a later line was already there, and its `.part` file is said
/// (`unfinished`). The line stays in the spool, and this drain forgets its key as it ends, so
/// the next drain rejects it as `no-key` and its content is never recorded. Only a drain that
/// stopped before its end leaves the key held, and the next one then hands the line on. The
/// same goes for a hook that outlives the drain, as it did when the spool was one file.
///
/// **A spool a build before #983 left is drained too**, first: a chat's `<n>.jsonl`
/// ([`drain_file`]), then its folder ([`drain_folder`]), each a sequence of its own. A line the
/// file gave is not taken from the folder again: the same key, number and MAC there is
/// `repeated`. A hook that changed builds in the middle of a chat has a number 1 in both, with
/// a MAC of its own each, and both are handed on.
///
/// **Two keys of one chat** are handed on in the order `keys.json` holds them, which is the
/// order the host issued them in, for the lines of a folder; for the lines of the old file, in
/// the order they first appear in it.
///
/// **Run before this host issues any token**, as a host does at its start: a line under a key
/// `keys.json` does not hold when the drain begins is rejected (`no-key`).
pub fn drain(dir: &Path, each: &mut dyn FnMut(Drained) -> io::Result<()>) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let keys = keys_locked(dir, || read_keys(dir))?;
    let taken: HashMap<String, Held> = keys
        .keys
        .iter()
        .map(|held| (held.id.clone(), held.clone()))
        .collect();
    // Per chat: whether it has a file of lines, and whether it has a folder of them.
    let mut chats: BTreeMap<u32, (bool, bool)> = BTreeMap::new();
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if let Some(chat) = name.strip_suffix(".jsonl").and_then(|n| n.parse().ok()) {
                    chats.entry(chat).or_default().0 = true;
                } else if let Some(chat) = name
                    .parse::<u32>()
                    .ok()
                    .filter(|chat| chat.to_string() == name)
                {
                    chats.entry(chat).or_default().1 = true;
                }
            }
        }
        Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(why) => return Err(why),
    }
    let issued: Vec<&str> = keys.keys.iter().map(|held| held.id.as_str()).collect();
    for (chat, (file, folder)) in chats {
        let mut given = Given::new();
        if file {
            given = drain_file(dir, chat, &taken, each)?;
        }
        if folder {
            drain_folder(dir, chat, &taken, &issued, given, each)?;
        }
    }
    // Every key taken is drained: forget them, keeping any issued since.
    keys_locked(dir, || {
        let mut now = read_keys(dir)?;
        now.keys.retain(|held| !taken.contains_key(&held.id));
        write_keys(dir, &now)
    })
}

/// The lines a drain has handed on from one of a chat's stores, each by its key's id, its number
/// and its MAC: what the chat's other store must not give again.
type Given = HashSet<(String, u64, String)>;

/// What a drain has read of one chat's spool: its lines that check, by key and number, and the
/// ones it rejects.
struct Checked<'a> {
    chat: u32,
    taken: &'a HashMap<String, Held>,
    /// What the chat's other store, drained before this one, already gave.
    already: Given,
    /// What this one gives.
    given: Given,
    /// The keys, in the order their lines were first read.
    order: Vec<String>,
    sequences: HashMap<String, BTreeMap<u64, Spooled>>,
    rejected: Vec<Drained>,
}

impl<'a> Checked<'a> {
    fn of(chat: u32, taken: &'a HashMap<String, Held>, already: Given) -> Self {
        Self {
            chat,
            taken,
            already,
            given: Given::new(),
            order: Vec::new(),
            sequences: HashMap::new(),
            rejected: Vec::new(),
        }
    }

    fn reject(&mut self, seq: Option<u64>, why: &'static str) {
        self.rejected.push(Drained::Rejected {
            chat: self.chat,
            seq,
            why,
        });
    }

    /// Checks one line as it was read: kept under its key and number, or rejected with why.
    /// `named` is the key and number its file's name gives, for a line that is a file of its
    /// own: a line that is not the one its name says is not read (`unreadable`), so the names
    /// hooks number by are the lines' own.
    fn line(&mut self, raw: Result<&str, &[u8]>, named: Option<(&str, u64)>) {
        let chat = self.chat;
        let Ok(disk) = raw
            .map_err(drop)
            .and_then(|raw| serde_json::from_str::<OnDisk>(raw).map_err(drop))
        else {
            return self.reject(None, why::UNREADABLE);
        };
        if named.is_some_and(|(key, seq)| key != disk.key || seq != disk.seq) {
            return self.reject(None, why::UNREADABLE);
        }
        let Some(held) = self.taken.get(&disk.key) else {
            return self.reject(Some(disk.seq), why::NO_KEY);
        };
        if held.chat != chat {
            return self.reject(Some(disk.seq), why::ANOTHER_CHATS_KEY);
        }
        let Some(key) = key_of(&held.key) else {
            return self.reject(Some(disk.seq), why::NO_KEY);
        };
        if !key.checks(chat, disk.seq, &disk.key, &disk.line, &disk.mac) {
            return self.reject(Some(disk.seq), why::MAC);
        }
        let this = (disk.key.clone(), disk.seq, disk.mac.clone());
        if self.already.contains(&this) {
            return self.reject(Some(disk.seq), why::REPEATED);
        }
        let line = match super::read_line(&disk.line) {
            Some((Line::Report(report), _)) if report.chat == chat => Spooled::Report(report),
            Some((Line::Tool(call), _)) if call.chat == chat => Spooled::Tool(call),
            Some((Line::Refused(refused), _)) if refused.chat == chat => Spooled::Refused(refused),
            _ => return self.reject(Some(disk.seq), why::NOT_THIS_CHATS),
        };
        let sequence = self.sequences.entry(disk.key.clone()).or_default();
        if sequence.contains_key(&disk.seq) || disk.seq == 0 {
            return self.reject(Some(disk.seq), why::REPEATED);
        }
        if !self.order.contains(&disk.key) {
            self.order.push(disk.key.clone());
        }
        sequence.insert(disk.seq, line);
        self.given.insert(this);
    }

    /// Checks the bytes of the file named for key `key`'s number `seq`: one line, that one, or
    /// it does not read. A file with no line, or with a second, is not one a hook wrote.
    fn file(&mut self, bytes: &[u8], key: &str, seq: u64) {
        let mut lines = lines_of(bytes);
        match (lines.next(), lines.next()) {
            (Some(raw), None) => self.line(raw, Some((key, seq))),
            _ => self.reject(None, why::UNREADABLE),
        }
    }

    /// Puts the keys in the order the host issued them (`issued`, the ids as `keys.json` holds
    /// them), where they were in the order their lines were read.
    fn in_the_order_of(&mut self, issued: &[&str]) {
        self.order
            .sort_by_key(|id| issued.iter().position(|held| *held == id.as_str()));
    }

    /// Hands everything on: the rejected lines, then each key's lines by number with its gaps,
    /// and the sequence drained. Answers what it gave.
    fn hand_on(mut self, each: &mut dyn FnMut(Drained) -> io::Result<()>) -> io::Result<Given> {
        let chat = self.chat;
        for item in std::mem::take(&mut self.rejected) {
            each(item)?;
        }
        for id in std::mem::take(&mut self.order) {
            let Some(sequence) = self.sequences.remove(&id) else {
                continue;
            };
            let mut expected = 1;
            let mut last = 0;
            for (seq, line) in sequence {
                if seq > expected {
                    each(Drained::Gap {
                        chat,
                        from: expected,
                        to: seq - 1,
                    })?;
                }
                each(Drained::Line { chat, seq, line })?;
                expected = seq + 1;
                last = seq;
            }
            each(Drained::Spool {
                chat,
                from: 1,
                to: last,
            })?;
        }
        Ok(self.given)
    }
}

/// Drains chat `chat`'s folder of lines, after its file of them gave `already`.
///
/// It holds the folder's lock, which only another drain waits for: no hook takes it. It reads
/// each file that has a line's name and checks it as it reads, hands them on, and then removes
/// those files and every name it could not read, and no other. A `.part` file is a number a
/// hook holds and is not read. One with no line of its name yet is said (`unfinished`), left
/// or not; one an hour old, or dated in a time to come, is a dead hook's and is removed
/// ([`A_PART_IS_A_DEAD_HOOKS_AFTER`]). The folder itself is never removed.
fn drain_folder(
    dir: &Path,
    chat: u32,
    taken: &HashMap<String, Held>,
    issued: &[&str],
    already: Given,
    each: &mut dyn FnMut(Drained) -> io::Result<()>,
) -> io::Result<()> {
    // A spool path that does not open as a folder (a file, a link, live or dangling, a pipe)
    // holds no line to read, and must not stop the drain of every chat after it: it is
    // reported as one unreadable line, and left for its owner to look at.
    let folder = match Folder::open(dir, chat) {
        Ok(folder) => folder,
        Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {
            return each(Drained::Rejected {
                chat,
                seq: None,
                why: why::UNREADABLE,
            });
        }
    };
    let held = folder.held.try_clone()?;
    held.lock()?;
    let _one_drain_at_a_time = crate::filelock::Held::locked(held);

    let names = folder.names(usize::MAX)?;
    let mut checked = Checked::of(chat, taken, already);
    // The names this drain takes away once everything is handed on: the lines it read, which
    // must go, and what it could not read or a dead hook left, which goes if it will.
    let (mut read, mut unread) = (Vec::new(), Vec::new());
    for name in &names {
        match named(name) {
            Some(Named {
                kind: Kind::Part,
                key,
                seq,
            }) => {
                let line = name_of(key, seq, Kind::Line);
                let landed = names
                    .binary_search_by(|other| other.to_bytes().cmp(line.as_bytes()))
                    .is_ok();
                if !landed {
                    checked.reject(Some(seq), why::UNFINISHED);
                }
                if folder.is_a_dead_hooks(name) {
                    unread.push(name);
                }
            }
            Some(Named {
                kind: Kind::Line,
                key,
                seq,
            }) => match folder.read(name) {
                Ok(bytes) => {
                    checked.file(&bytes, key, seq);
                    read.push(name);
                }
                Err(why) if why.kind() == io::ErrorKind::NotFound => {}
                Err(_) => {
                    checked.reject(None, why::UNREADABLE);
                    unread.push(name);
                }
            },
            // Not a name a hook gives: nothing of it is read.
            None => {
                checked.reject(None, why::UNREADABLE);
                unread.push(name);
            }
        }
    }
    checked.in_the_order_of(issued);
    checked.hand_on(each)?;
    // Everything handed on is recorded.
    for name in read {
        folder.remove(&**name)?;
    }
    // A directory, or anything else that does not go, is reported again at the next drain.
    for name in unread {
        let _ = folder.remove(&**name);
    }
    folder.sync()
}

/// Drains chat `chat`'s file of lines, the spool of a build before #983: read under the file's
/// lock, as that build's hooks append under it, handed on, and emptied. Answers what it gave.
fn drain_file(
    dir: &Path,
    chat: u32,
    taken: &HashMap<String, Held>,
    each: &mut dyn FnMut(Drained) -> io::Result<()>,
) -> io::Result<Given> {
    // A spool path that does not open as a plain file (a directory, a link, live or dangling,
    // a pipe) holds no line to read, and must not stop the drain of every chat after it: it is
    // reported as one unreadable line, and left for its owner to look at.
    let path = file_for(dir, chat);
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true);
    let file = match crate::contain::nofollow(&mut options).open(&path) {
        Ok(file) if file.metadata()?.is_file() => file,
        Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(Given::new()),
        _ => {
            each(Drained::Rejected {
                chat,
                seq: None,
                why: why::UNREADABLE,
            })?;
            return Ok(Given::new());
        }
    };
    file.lock()?;
    let mut file = crate::filelock::Held::locked(file);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let mut checked = Checked::of(chat, taken, Given::new());
    for raw in lines_of(&bytes) {
        checked.line(raw, None);
    }
    let given = checked.hand_on(each)?;
    // Everything handed on is recorded, and every line was this drain's: emptied, not removed,
    // so a hook of that build holding it open appends to the file the next drain reads.
    file.set_len(0)?;
    rustix::fs::fsync(&file)?;
    Ok(given)
}

#[cfg(test)]
mod tests;
