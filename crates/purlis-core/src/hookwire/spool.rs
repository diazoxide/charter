//! The hook spool (FD-30, ADR 0068 §6): where a hook's line goes when the host does not take it.
//!
//! **One spool per chat**, never one per project: `<n>.jsonl` in the `spool/` directory beside
//! the hook socket ([`dir_for`]), so `.charter/app/spool/` in a project. A hook whose line the
//! host did not take appends it there, with the next number in that chat's sequence and a MAC,
//! and makes it durable before the hook answers its harness ([`append`]). The host drains
//! every spool when it starts ([`drain`]): it checks each line, hands on the ones that check,
//! and says what it found — a line that fails its check is [`Drained::Rejected`], and a number
//! missing from the middle of a sequence is [`Drained::Gap`]. Nothing is dropped silently
//! (O1).
//!
//! **What a line is checked with.** Each line carries a MAC under a key derived from the chat's
//! token ([`SpoolKey`]): HMAC-SHA256 of the token under a fixed label. The host writes each key
//! to `keys.json` as it issues the token ([`remember`]), so a host that started after the one
//! that issued it can still check the line. The key is not the token: holding it does not let
//! anything report on the live channel as the chat. A line is that chat's only if it is in that
//! chat's file and checks under a key the host issued to that chat.
//!
//! **What it holds, stated plainly** (ADR 0068 §6, as amended by V63). A process holding a
//! chat's token can write lines as that chat, as it can on the live channel. It cannot write
//! lines that read as another chat's. A number missing below the highest one in the file is a
//! gap the drain finds; lines removed from the end, or removed and then followed by new ones,
//! are not found. A sandboxed chat is denied reading and writing the whole directory, keys
//! included (ADR 0067 §5's integrity class); the hooks of the harnesses charter sandboxes run
//! outside the sandbox their tools run in, so they still reach it.
//!
//! **Only where that denial reaches** ([`covered`]): a project's `.charter/app/spool/`. Beside
//! a hook socket anywhere else (the fallback for a project whose path is too long for a
//! socket) nothing is spooled and no key is written, and the hook says the line is lost.
//!
//! **Numbers are per key.** A key is one token, and a token is one start of a chat, so its
//! sequence starts at 1 and every one of its lines is drained at the same start of the host:
//! the first one after that start of the chat ended.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{ChatToken, CommitRefused, Line, Report, ToolCall};

/// The directory beside the hook socket that holds every chat's spool.
pub const DIR: &str = "spool";

/// The host's record of the key each token it issued checks under.
pub const KEYS: &str = "keys.json";

/// The label a chat's spool key is derived from its token under.
const LABEL: &[u8] = b"charter hook spool v1";

/// The version of `keys.json` and of a spool line.
const VERSION: u32 = 1;

/// How long [`append`] waits for a chat's spool while another process holds it.
///
/// A hook spools only after the host did not take its line in time, and the harness is waiting
/// for the hook's answer meanwhile: a guard's decided verdict must reach it before the harness's
/// own timeout runs out, or the harness runs the tool. So the wait is bounded, and a line that
/// cannot be spooled in it is lost and said to be lost, never waited on.
const A_LOCK_IS_WAITED_FOR_AT_MOST: Duration = Duration::from_millis(250);

/// How long [`append`] waits for a new spool's directory to be made durable, for the same
/// reason as the lock.
const A_DIRECTORY_SYNC_IS_WAITED_FOR_AT_MOST: Duration = Duration::from_millis(250);

/// How long [`append`] waits for its line to be made durable, for the same reason as the lock.
/// A line written and not yet durable when the wait runs out is still in the file, and reaches
/// the drain unless the machine stops before the disk has it ([`NotYetDurable`]).
const A_LINE_SYNC_IS_WAITED_FOR_AT_MOST: Duration = Duration::from_millis(250);

/// How long the drain at the app's start waits for one chat's spool while another process holds
/// it. Past it the spool is reported `held` and left, with its key, for the next start: one
/// holder never holds up the drain of every other chat, or the app's start.
const THE_DRAIN_WAITS_FOR_A_SPOOL_AT_MOST: Duration = Duration::from_secs(1);

/// How long issuing a token ([`remember`]) and the drain wait for `keys.json`, under the lock
/// on the spool directory itself, while another process holds it (#1426). Past it, issuing says
/// in the log that the key was not recorded and issues the token all the same, and the drain
/// stops and is run again at the next start: a holder never holds up a chat's start or the
/// app's.
const THE_KEYS_ARE_WAITED_FOR_AT_MOST: Duration = Duration::from_secs(1);

/// How long the drain waits for an emptied spool to be made durable (#1426). Past it the drain
/// goes on: every line in it was recorded before it was emptied, so a spool that comes back
/// whole after a crash is drained again and its lines handed on twice, the side a drain errs
/// on (see [`drain`]).
const THE_DRAINS_SYNC_IS_WAITED_FOR_AT_MOST: Duration = Duration::from_secs(1);

#[cfg(test)]
thread_local! {
    /// How long the drain's sync of an emptied spool sleeps first, on this thread: a disk that
    /// does not answer, as a test makes one.
    static THE_DRAINS_SYNC_STALLS_FOR: std::cell::Cell<Duration> =
        const { std::cell::Cell::new(Duration::ZERO) };
}

/// The most a chat's spool holds, and the most the drain reads of one. A spool the host has not
/// drained grows by one hook line at a time, so this is many thousands of lines. [`append`]
/// refuses a line that would take a spool past it, and the line is said to be lost, so every
/// line it wrote is one the drain reads; a spool past it is reported `too-big` and left.
const A_SPOOL_HOLDS_AT_MOST: u64 = 16 * 1024 * 1024;

/// How much of a spool's end [`append`] reads at a time, looking for the last number.
const A_TAIL_IS_READ_BY: u64 = 64 * 1024;

/// How long [`lock_within`] sleeps between tries.
const A_TRY_EVERY: Duration = Duration::from_millis(5);

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
///
/// **The wait for it is bounded** ([`THE_KEYS_ARE_WAITED_FOR_AT_MOST`], #1426), and the
/// directory is opened without blocking and only as a directory, so nothing planted at its path
/// holds the caller either.
fn keys_locked<T>(dir: &Path, act: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    let held = open_dir(dir)?;
    lock_within(&held, THE_KEYS_ARE_WAITED_FOR_AT_MOST, "the spool's keys")?;
    let _held = crate::filelock::Held::locked(held);
    act()
}

/// The directory `dir`, opened without blocking and only as a directory.
fn open_dir(dir: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let flags = rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NONBLOCK;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags.bits() as i32)
        .open(dir)
}

/// Chat `chat`'s spool in `dir`.
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

/// One line of a spool file. `line` is the hook's line as JSON text, so the MAC is over the
/// exact bytes the hook wrote.
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

/// Opens `path` for reading and appending, made 0600 if it is new, never through a link and
/// never blocking: the open does not wait ([`crate::contain::nofollow`] opens non-blocking),
/// and anything at the path but a plain file is refused as [`io::ErrorKind::InvalidInput`]
/// before it is locked or read. A named pipe planted there would otherwise hold the hook on a
/// read that never ends.
fn open_spool(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).append(true).create(true).mode(0o600);
    let file = crate::contain::nofollow(&mut options)
        .open(path)
        .map_err(crate::rewrite::refused_at(path))?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is not a plain file, so nothing is spooled there",
                path.display()
            ),
        ));
    }
    Ok(file)
}

#[cfg(test)]
thread_local! {
    /// How many directory syncs this thread has started: what a test of when one is made reads.
    static DIRECTORY_SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `fsync`s the directory `dir`, opened without blocking and only as a directory, within `wait`
/// ([`done_within`]).
fn sync_dir_within(dir: &Path, wait: Duration) -> io::Result<()> {
    #[cfg(test)]
    DIRECTORY_SYNCS.with(|n| n.set(n.get() + 1));
    let held = open_dir(dir)?;
    done_within(wait, "the spool's directory", move || {
        rustix::fs::fsync(&held).map_err(io::Error::from)
    })
}

/// `sync`, run on a thread of its own, or [`io::ErrorKind::TimedOut`] once `wait` has passed with
/// it still running. The sync then finishes, or not, on that thread, which the hook's exit ends.
/// `what` is what it makes durable, as the error says it.
fn done_within(
    wait: Duration,
    what: &str,
    sync: impl FnOnce() -> io::Result<()> + Send + 'static,
) -> io::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(sync());
    });
    rx.recv_timeout(wait).unwrap_or_else(|_| {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("{what} was not made durable within {} ms", wait.as_millis()),
        ))
    })
}

/// A line [`append`] wrote whose `fsync` was still running when the wait ran out: it is in the
/// spool, and reaches the app at its next start unless the machine stops before the disk has it.
/// So it is neither durable, which is what [`append`] answers `Ok` for, nor lost.
#[derive(Debug)]
pub struct NotYetDurable(io::Error);

impl std::fmt::Display for NotYetDurable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the line is in the spool, but {}; it reaches the app at its next start unless the \
             machine stops before the disk has it",
            self.0
        )
    }
}

impl std::error::Error for NotYetDurable {}

/// `why`, a sync given up on, as [`NotYetDurable`].
fn not_yet_durable(why: io::Error) -> io::Error {
    io::Error::new(why.kind(), NotYetDurable(why))
}

/// Whether `why` is a line written and not yet durable ([`NotYetDurable`]) rather than lost.
pub fn is_not_yet_durable(why: &io::Error) -> bool {
    why.get_ref()
        .is_some_and(|inner| inner.is::<NotYetDurable>())
}

/// Refused as [`io::ErrorKind::InvalidData`]: a spool past [`A_SPOOL_HOLDS_AT_MOST`].
fn past_what_a_spool_holds() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "this chat's spool would be past {} MiB, more than the app reads of one",
            A_SPOOL_HOLDS_AT_MOST / 1024 / 1024
        ),
    )
}

/// All of `file`, read from where it stands, refused past [`A_SPOOL_HOLDS_AT_MOST`] bytes
/// ([`past_what_a_spool_holds`]).
fn read_bounded(file: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut text = Vec::new();
    file.take(A_SPOOL_HOLDS_AT_MOST + 1)
        .read_to_end(&mut text)?;
    if text.len() as u64 > A_SPOOL_HOLDS_AT_MOST {
        return Err(past_what_a_spool_holds());
    }
    Ok(text)
}

/// The number of the last line under key `id` in a spool `len` bytes long, and whether the
/// spool ends with a whole line. Read off its end, [`A_TAIL_IS_READ_BY`] at a time, back to the
/// last of that key's lines: a key's numbers grow down the file, since each is chosen under the
/// lock as one past the last, so that line holds the highest. Usually the last line in the file,
/// so a long downtime costs a hook no more than one read. A line that does not read, or is
/// another key's, is read past. 0 for a spool with none of that key's lines.
fn last_number(file: &mut (impl Read + Seek), len: u64, id: &str) -> io::Result<(u64, bool)> {
    let mut ends_whole = true;
    if len > 0 {
        let mut last = [0u8];
        file.seek(SeekFrom::Start(len - 1))?;
        file.read_exact(&mut last)?;
        ends_whole = last[0] == b'\n';
    }
    // The bytes from `end` on that are not yet a whole line: the head of the chunk read last.
    let mut rest: Vec<u8> = Vec::new();
    let mut end = len;
    while end > 0 {
        let start = end.saturating_sub(A_TAIL_IS_READ_BY);
        let mut chunk = vec![0u8; usize::try_from(end - start).map_err(io::Error::other)?];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut chunk)?;
        chunk.extend_from_slice(&rest);
        // Short of the file's start, what comes before the chunk's first newline may be part of
        // a line that starts in the chunk before it.
        let (head, whole) = match chunk.iter().position(|byte| *byte == b'\n') {
            Some(at) if start > 0 => (chunk[..at].to_vec(), &chunk[at + 1..]),
            None if start > 0 => (chunk.clone(), &chunk[chunk.len()..]),
            _ => (Vec::new(), &chunk[..]),
        };
        let found = whole
            .split(|byte| *byte == b'\n')
            .rev()
            .filter_map(|line| serde_json::from_slice::<OnDisk>(line).ok())
            .find(|it| it.key == id);
        if let Some(it) = found {
            return Ok((it.seq, ends_whole));
        }
        rest = head;
        end = start;
    }
    Ok((0, ends_whole))
}

/// Appends `what`, chat `chat`'s line, to its spool in `dir`, under `token`'s key and the next
/// number in that key's sequence, and answers the number.
///
/// **Survives a host crash when it returns** (FD-9's widened acceptance): the line is
/// `fsync`ed, and an empty file's directory with it, before this answers. It is the operating
/// system's ordinary `fsync`, which on macOS does not reach through the drive's own cache, so a
/// power loss there can still lose it. The file is locked while the number is chosen and the
/// line written, so two hooks of one chat running at once never take the same number.
///
/// **The wait for that lock is bounded** ([`A_LOCK_IS_WAITED_FOR_AT_MOST`]): a hook answers its
/// harness after this returns (ADR 0075 §7), so a lock nobody lets go of must not hold a decided
/// verdict past the harness's timeout. Once the wait runs out this answers
/// [`io::ErrorKind::TimedOut`], nothing is written and no number is taken, and the hook says on
/// stderr that the line is lost.
///
/// **So is everything else**: the open never blocks and takes only a plain file
/// ([`open_spool`]); the directory sync of a spool with nothing in it yet — a new one, one the
/// drain emptied, or one whose last directory sync was given up on — is waited for at most
/// [`A_DIRECTORY_SYNC_IS_WAITED_FOR_AT_MOST`]; the next number is read off the spool's tail
/// ([`last_number`]); and a line that would take the spool past [`A_SPOOL_HOLDS_AT_MOST`] is
/// refused unread. Each one past its bound is an error the hook reports as the line lost. The
/// line's own `fsync` is waited for at most [`A_LINE_SYNC_IS_WAITED_FOR_AT_MOST`]: past it the
/// line is written and not yet durable, and the error says so ([`NotYetDurable`]).
///
/// Refused where the sandbox's integrity denial does not reach ([`covered`]).
pub fn append(
    dir: &Path,
    chat: u32,
    token: &ChatToken,
    what: &impl serde::Serialize,
) -> io::Result<u64> {
    refused_unless_covered(dir)?;
    private(dir)?;
    let key = SpoolKey::of(token);
    let id = key.id();
    let line = serde_json::to_string(what).map_err(io::Error::other)?;
    let file = open_spool(&file_for(dir, chat))?;
    // A line is written only after this, so a spool with a line in it has had its directory
    // made durable; one with none may not have, whatever made it.
    if file.metadata()?.len() == 0 {
        sync_dir_within(dir, A_DIRECTORY_SYNC_IS_WAITED_FOR_AT_MOST)?;
    }
    lock_within(&file, A_LOCK_IS_WAITED_FOR_AT_MOST, "this chat's spool")?;
    let mut file = crate::filelock::Held::locked(file);
    let len = file.metadata()?.len();
    if len >= A_SPOOL_HOLDS_AT_MOST {
        return Err(past_what_a_spool_holds());
    }
    let (last, ends_whole) = last_number(&mut &*file, len, &id)?;
    let seq = last
        .checked_add(1)
        .ok_or_else(|| io::Error::other("this chat's spool has used every number there is"))?;
    let mac = key.sign(chat, seq, &id, &line);
    let mut bytes = serde_json::to_vec(&OnDisk {
        v: VERSION,
        seq,
        key: id,
        line,
        mac,
    })
    .map_err(io::Error::other)?;
    bytes.push(b'\n');
    // A file a crash left ending in half a line: the new line starts on a line of its own.
    if !ends_whole {
        bytes.insert(0, b'\n');
    }
    if len + bytes.len() as u64 > A_SPOOL_HOLDS_AT_MOST {
        return Err(past_what_a_spool_holds());
    }
    file.write_all(&bytes)?;
    let line = file.try_clone()?;
    done_within(A_LINE_SYNC_IS_WAITED_FOR_AT_MOST, "the line", move || {
        rustix::fs::fsync(&line).map_err(io::Error::from)
    })
    .map_err(not_yet_durable)?;
    Ok(seq)
}

/// Takes `file`'s exclusive lock, or answers [`io::ErrorKind::TimedOut`] once `wait` has passed
/// with another holder still on it. `what` is what the file is, as the error says it.
fn lock_within(file: &File, wait: Duration, what: &str) -> io::Result<()> {
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::Error(why)) => return Err(why),
            Err(std::fs::TryLockError::WouldBlock) if started.elapsed() >= wait => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("another process held {what} for {} ms", wait.as_millis()),
                ));
            }
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(A_TRY_EVERY),
        }
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
    /// A line in chat `chat`'s spool that did not check, and why. `seq` is the number it
    /// claimed, where it could be read.
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
    /// A number the sequence already had.
    pub const REPEATED: &str = "repeated";
    /// Not a line: another process held the chat's spool past the drain's wait, so it was not
    /// read, and is left, with its key, for the next start.
    pub const HELD: &str = "held";
    /// Not a line: the chat's spool is past what the drain reads of one, so it was not read, and
    /// is left, with its key, for its owner to look at.
    pub const TOO_BIG: &str = "too-big";
}

/// Drains every spool in `dir`: each line checked and handed to `each` in its chat's order,
/// with each gap, each rejected line and each sequence drained.
///
/// **A line leaves its spool only once `each` has taken it.** `each` answers once what it was
/// handed is recorded durably; the first error stops the drain, and the spool it was draining
/// is left as it was, to be drained again at the next start. A line already handed on may
/// then be handed on twice, which is the side a drain errs on.
///
/// **Run before this host issues any token**, as a host does at its start: a line under a key
/// `keys.json` does not hold when the drain begins is rejected (`no-key`).
///
/// **Every wait is bounded**, so the app's start is never held: for `keys.json`
/// ([`THE_KEYS_ARE_WAITED_FOR_AT_MOST`]; before any spool is read, the drain stops with
/// [`io::ErrorKind::TimedOut`] and every line waits for the next start), for each chat's spool
/// ([`THE_DRAIN_WAITS_FOR_A_SPOOL_AT_MOST`], reported `held`), and for each emptied spool's sync
/// ([`THE_DRAINS_SYNC_IS_WAITED_FOR_AT_MOST`]).
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
    let mut chats: Vec<u32> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()?
                    .strip_suffix(".jsonl")?
                    .parse()
                    .ok()
            })
            .collect(),
        Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(why) => return Err(why),
    };
    chats.sort_unstable();
    let mut left = Vec::new();
    for chat in chats {
        if !drain_one(dir, chat, &taken, each)? {
            left.push(chat);
        }
    }
    // Every key taken is drained: forget them, keeping any issued since and those of a spool
    // left for the next start. Keys another process holds past the wait are kept, and
    // forgotten by the next drain: the spools they check are empty, so nothing is lost.
    match keys_locked(dir, || {
        let mut now = read_keys(dir)?;
        now.keys
            .retain(|held| !taken.contains_key(&held.id) || left.contains(&held.chat));
        write_keys(dir, &now)
    }) {
        Err(why) if why.kind() == io::ErrorKind::TimedOut => {
            tracing::warn!(
                "purlis: the hook spool was drained, and its keys were kept ({why}); they are \
                 forgotten at the next start"
            );
            Ok(())
        }
        forgot => forgot,
    }
}

/// Drains chat `chat`'s spool, and answers whether it did: `false` for one left, unread, for the
/// next start — held by another process past [`THE_DRAIN_WAITS_FOR_A_SPOOL_AT_MOST`], or past
/// [`A_SPOOL_HOLDS_AT_MOST`] — each reported as a line rejected `held` or `too-big`.
fn drain_one(
    dir: &Path,
    chat: u32,
    taken: &HashMap<String, Held>,
    each: &mut dyn FnMut(Drained) -> io::Result<()>,
) -> io::Result<bool> {
    let left = |why| Drained::Rejected {
        chat,
        seq: None,
        why,
    };
    // A spool path that does not open as a plain file (a directory, a link, live or dangling,
    // a pipe) holds no line to read, and must not stop the drain of every chat after it: it is
    // reported as one unreadable line, and left for its owner to look at.
    let file = match open_spool(&file_for(dir, chat)) {
        Err(why) if why.kind() != io::ErrorKind::NotFound => {
            return each(left(why::UNREADABLE)).map(|()| true);
        }
        opened => opened?,
    };
    match lock_within(
        &file,
        THE_DRAIN_WAITS_FOR_A_SPOOL_AT_MOST,
        "this chat's spool",
    ) {
        Err(why) if why.kind() == io::ErrorKind::TimedOut => {
            return each(left(why::HELD)).map(|()| false);
        }
        locked => locked?,
    }
    let file = crate::filelock::Held::locked(file);
    let bytes = match read_bounded(&mut &*file) {
        Err(why) if why.kind() == io::ErrorKind::InvalidData => {
            return each(left(why::TOO_BIG)).map(|()| false);
        }
        read => read?,
    };
    // Per key, in the order keys first appear: its lines by number.
    let mut order: Vec<String> = Vec::new();
    let mut sequences: HashMap<String, BTreeMap<u64, Spooled>> = HashMap::new();
    let mut rejected = Vec::new();
    for raw in lines_of(&bytes) {
        let reject = |seq, why| Drained::Rejected { chat, seq, why };
        let Ok(disk) = raw
            .map_err(drop)
            .and_then(|raw| serde_json::from_str::<OnDisk>(raw).map_err(drop))
        else {
            rejected.push(reject(None, why::UNREADABLE));
            continue;
        };
        let Some(held) = taken.get(&disk.key) else {
            rejected.push(reject(Some(disk.seq), why::NO_KEY));
            continue;
        };
        if held.chat != chat {
            rejected.push(reject(Some(disk.seq), why::ANOTHER_CHATS_KEY));
            continue;
        }
        let Some(key) = key_of(&held.key) else {
            rejected.push(reject(Some(disk.seq), why::NO_KEY));
            continue;
        };
        if !key.checks(chat, disk.seq, &disk.key, &disk.line, &disk.mac) {
            rejected.push(reject(Some(disk.seq), why::MAC));
            continue;
        }
        let line = match super::read_line(&disk.line) {
            Some((Line::Report(report), _)) if report.chat == chat => Spooled::Report(report),
            Some((Line::Tool(call), _)) if call.chat == chat => Spooled::Tool(call),
            Some((Line::Refused(refused), _)) if refused.chat == chat => Spooled::Refused(refused),
            _ => {
                rejected.push(reject(Some(disk.seq), why::NOT_THIS_CHATS));
                continue;
            }
        };
        let sequence = sequences.entry(disk.key.clone()).or_default();
        if sequence.contains_key(&disk.seq) || disk.seq == 0 {
            rejected.push(reject(Some(disk.seq), why::REPEATED));
            continue;
        }
        if !order.contains(&disk.key) {
            order.push(disk.key.clone());
        }
        sequence.insert(disk.seq, line);
    }
    for item in rejected {
        each(item)?;
    }
    for id in order {
        let Some(sequence) = sequences.remove(&id) else {
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
    // Everything handed on is recorded, and every line was this drain's: emptied, not removed,
    // so a hook holding it open appends to the file the next drain reads.
    file.set_len(0)?;
    let emptied = file.try_clone()?;
    #[cfg(test)]
    let stall = THE_DRAINS_SYNC_STALLS_FOR.with(std::cell::Cell::get);
    match done_within(
        THE_DRAINS_SYNC_IS_WAITED_FOR_AT_MOST,
        "the emptied spool",
        move || {
            #[cfg(test)]
            std::thread::sleep(stall);
            rustix::fs::fsync(&emptied).map_err(io::Error::from)
        },
    ) {
        Err(why) if why.kind() == io::ErrorKind::TimedOut => tracing::warn!(
            "purlis: chat {chat}'s spool was drained, and {why}; if the machine stops before \
             the disk has it, its lines are handed on again at the next start"
        ),
        synced => synced?,
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
