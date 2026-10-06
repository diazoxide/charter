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
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::{ChatToken, CommitRefused, Line, Report, ToolCall};

/// The directory beside the hook socket that holds every chat's spool.
pub const DIR: &str = "spool";

/// The host's record of the key each token it issued checks under.
pub const KEYS: &str = "keys.json";

/// The label a chat's spool key is derived from its token under.
const LABEL: &[u8] = b"charter hook spool v1";

/// The version of `keys.json` and of a spool line.
const VERSION: u32 = 1;

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

/// Opens `path` for reading and appending, made 0600 if it is new, and never through a link.
fn open_spool(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).append(true).create(true).mode(0o600);
    crate::contain::nofollow(&mut options).open(path)
}

/// Appends `what`, chat `chat`'s line, to its spool in `dir`, under `token`'s key and the next
/// number in that key's sequence, and answers the number.
///
/// **Survives a host crash when it returns** (FD-9's widened acceptance): the line is
/// `fsync`ed, and a new file's directory with it, before this answers. It is the operating
/// system's ordinary `fsync`, which on macOS does not reach through the drive's own cache, so a
/// power loss there can still lose it. The file is locked while the number is chosen and the
/// line written, so two hooks of one chat running at once never take the same number.
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
    let path = file_for(dir, chat);
    let new = !path.exists();
    let file = open_spool(&path)?;
    if new {
        rustix::fs::fsync(File::open(dir)?)?;
    }
    file.lock()?;
    let mut file = crate::filelock::Held::locked(file);
    let mut text = Vec::new();
    file.read_to_end(&mut text)?;
    let last = lines_of(&text)
        .filter_map(Result::ok)
        .filter_map(|it| serde_json::from_str::<OnDisk>(it).ok())
        .filter(|it| it.key == id)
        .map(|it| it.seq)
        .max()
        .unwrap_or(0);
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
    if !text.is_empty() && !text.ends_with(b"\n") {
        bytes.insert(0, b'\n');
    }
    file.write_all(&bytes)?;
    rustix::fs::fsync(&file)?;
    Ok(seq)
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
    for chat in chats {
        drain_one(dir, chat, &taken, each)?;
    }
    // Every key taken is drained: forget them, keeping any issued since.
    keys_locked(dir, || {
        let mut now = read_keys(dir)?;
        now.keys.retain(|held| !taken.contains_key(&held.id));
        write_keys(dir, &now)
    })
}

fn drain_one(
    dir: &Path,
    chat: u32,
    taken: &HashMap<String, Held>,
    each: &mut dyn FnMut(Drained) -> io::Result<()>,
) -> io::Result<()> {
    let file = open_spool(&file_for(dir, chat))?;
    file.lock()?;
    let mut file = crate::filelock::Held::locked(file);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
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
    rustix::fs::fsync(&file)?;
    Ok(())
}

#[cfg(test)]
mod tests;
