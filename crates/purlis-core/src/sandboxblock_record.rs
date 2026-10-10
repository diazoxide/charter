//! **The network record** (#1662, spec #1661): one record per machine of every **Block** the app
//! heard and every **Allowed host** a person added or removed, kept 30 days in purlis's data home
//! and never in a project, so it is never committed and never sent.
//!
//! It also keeps **every connection** purlis's own proxy carried or refused for a chat (#1664):
//! the host and port, the decision (its `scope`: `open`, `persona`, `you` or `chat` for one let
//! through, `ask` or `refused` for one refused) and how many connections the line stands for, coalesced by the proxy to a line per host and port a
//! minute (`sandbox::egress::Tally`), so a chat cannot turn the record over by connecting.
//!
//! It is the block store of #1338, moved out of the project's state folder and widened: where
//! that kept an operation and a kind for `purlis doctor`'s count, a line here also says which
//! chat it was (its id, its name and its number), the chat's persona, the host and port a
//! refused connection was to, and, for an Allow or its removal, what was allowed, at which
//! scope, who decided and the outcome. Settings' Network page, a chat's Network view and
//! `purlis doctor` read it; nothing decides what a chat may reach from it.
//!
//! # What is never kept
//!
//! A refused path, a command, its arguments or its output. A Block names a host only for a
//! refused connection (the host its Notice offered to allow) or a refused lookup (the host a
//! program said it looked up, never offered), and only what passes the check a grant makes of a
//! host ([`super::named_host`]), so a line a chat wrote cannot put anything else here. What a chat sends is still data: its
//! chat's name is what the person called it, and a host it names may be one it never tried.
//!
//! # Where and how
//!
//! `<data>/network/<project key>.jsonl` (the key is the digest the sandbox's cache homes are
//! named by, `sandbox::Homes::project_key`), one JSON line per event, appended. A write lets
//! go of what is older than [`KEPT_FOR_SECS`] and past [`AT_MOST_KEPT`] by rewriting the file
//! whole, under a lock on its folder. The file is 0600. A line this build cannot read, such as
//! an event word a newer build added, is kept and passed over. A sandboxed chat cannot write it:
//! purlis's data home is outside what a chat may write, but for the caches it is pointed at.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use super::{Block, Kind, Operation};

/// The record's folder under purlis's data home.
pub const DIR: &str = "network";

/// How long a line is kept: 30 days.
pub const KEPT_FOR_SECS: u64 = 30 * 24 * 60 * 60;

/// The window `purlis doctor` counts blocks over: seven days.
pub const COUNTED_FOR_SECS: u64 = 7 * 24 * 60 * 60;

/// The most lines one project's record holds; the oldest go first. Blocks reach it through the
/// app's throttle (`sandboxblock::Throttle`), a handful a minute per chat at most.
pub const AT_MOST_KEPT: usize = 5000;

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    /// A chat's sandbox refused something: a **Block**.
    Block,
    /// purlis held a connection and asked the person (the live ask, #1666).
    Ask,
    /// A person allowed something: an **Allowed host**, a folder, a vault, a persona's hosts.
    Allow,
    /// A person removed what was allowed.
    Remove,
    /// A held connection nobody answered in time (#1666): refused.
    Timeout,
    /// purlis's own proxy carried a chat's connections to a host (#1664).
    Connect,
}

/// How it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Refused,
    Allowed,
    Removed,
    /// Asked, and waiting for an answer.
    Held,
}

/// The chat a line is about, as the app knew it when it wrote the line. All three may be
/// missing: a revoke from Settings comes from no chat.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Chat {
    /// The chat's id, which outlives its runs and a relaunch: what a chat's Network view reads
    /// its own lines by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The name it was shown under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its number in the app that wrote the line, which a relaunch gives again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<u32>,
}

impl Chat {
    fn is_empty(&self) -> bool {
        self.id.is_none() && self.name.is_none() && self.session.is_none()
    }
}

/// One line of the record.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// When, in seconds since 1970.
    pub at: u64,
    pub event: Event,
    /// For a Block: what was refused, as [`super::Block`] keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<Block>,
    /// For an Allow or a removal: the kind of thing, by the word the grant is audited under
    /// (`host`, `write`, `vault`, `persona-hosts`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what: Option<String>,
    /// The host and port (`api.example.com:443`); for an Allow, what it names. For a Block,
    /// only a refused connection's (`connect` + `host`), checked as a grant checks a host
    /// ([`super::named_host`]): the host the Notice offered to allow (#1663).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// For a Block of a refused lookup (`lookup` + `host`): the host the program said it
    /// looked up, checked as a grant checks a host (#1663). **Never one to offer to allow**: a
    /// program's own printed words named it, so it is untrusted, and allowing it would not let
    /// that program through. A reader shows it as text and offers nothing on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub looked_up: Option<String>,
    #[serde(default, skip_serializing_if = "Chat::is_empty")]
    pub chat: Chat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// For an Allow or a removal: `chat`, `you` (this project on this machine) or `project`
    /// (everyone in it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Who decided: `you`, the person at this machine. None for a Block, which nobody decided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub who: Option<String>,
    pub outcome: Outcome,
    /// For a connection line: how many connections it stands for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub times: Option<u64>,
}

impl Entry {
    /// A Block chat `chat` (as persona `persona`) met at `at`, on `target` where the report
    /// named one. Only a refused connection keeps it, and only as the host it reads as.
    pub fn blocked(
        block: &Block,
        target: Option<&str>,
        chat: Chat,
        persona: Option<&str>,
        at: u64,
    ) -> Self {
        Self {
            at,
            event: Event::Block,
            block: Some(*block),
            what: None,
            target: target.and_then(|named| host_of(block, named)),
            looked_up: target.and_then(|named| looked_up_of(block, named)),
            chat,
            persona: persona.map(str::to_owned),
            scope: None,
            who: None,
            outcome: Outcome::Refused,
            times: None,
        }
    }

    /// `times` connections chat `chat` (as persona `persona`) made to `target` through purlis's
    /// own proxy, decided `by` (the layer that let them through, or `ask` or `refused`, which
    /// were refused: each one also raised a Block the first time), told at `at`. A target that is not a host, and
    /// the hosts past what one tally tells apart, keep none.
    pub fn connected(
        target: Option<&str>,
        by: &str,
        times: u64,
        chat: Chat,
        persona: Option<&str>,
        at: u64,
    ) -> Self {
        // The live ask (#1666) tells its ask and its timeout on the same road.
        let (event, outcome) = match by {
            ASKED => (Event::Ask, Outcome::Held),
            TIMED_OUT => (Event::Timeout, Outcome::Refused),
            "ask" | "refused" => (Event::Connect, Outcome::Refused),
            _ => (Event::Connect, Outcome::Allowed),
        };
        Self {
            at,
            event,
            block: None,
            what: None,
            target: target.and_then(super::named_host),
            looked_up: None,
            chat,
            persona: persona.map(str::to_owned),
            scope: matches!(event, Event::Connect).then(|| by.to_owned()),
            who: None,
            outcome,
            times: Some(times),
        }
    }

    /// A person allowed `target` (a `what`) at `scope`, from chat `chat` where it came from one.
    pub fn allowed(
        what: &str,
        target: &str,
        scope: &str,
        chat: Chat,
        persona: Option<&str>,
        at: u64,
    ) -> Self {
        Self {
            at,
            event: Event::Allow,
            block: None,
            what: Some(what.to_owned()),
            target: Some(target.to_owned()),
            looked_up: None,
            chat,
            persona: persona.map(str::to_owned),
            scope: Some(scope.to_owned()),
            who: Some(WHO.to_owned()),
            outcome: Outcome::Allowed,
            times: None,
        }
    }

    /// A person removed what allowed `target` (a `what`) at `scope`.
    pub fn removed(what: &str, target: &str, scope: &str, at: u64) -> Self {
        Self {
            event: Event::Remove,
            outcome: Outcome::Removed,
            ..Self::allowed(what, target, scope, Chat::default(), None, at)
        }
    }

    /// Whether it is a Block of a connection to a host: what Blocked lately lists.
    pub fn is_host_block(&self) -> bool {
        self.event == Event::Block
            && self
                .block
                .is_some_and(|block| matches!(block.kind, Kind::Host | Kind::LocalSocket))
    }
}

/// The word [`Entry::connected`] is told a held connection's ask by (#1666): written as an
/// [`Event::Ask`], waiting on the person.
pub const ASKED: &str = "asked";

/// The word [`Entry::connected`] is told a held connection's timeout by (#1666): written as an
/// [`Event::Timeout`], refused.
pub const TIMED_OUT: &str = "timeout";

/// Who decides every Allow and removal written today: the person at this machine.
pub const WHO: &str = "you";

/// The host `named` is, as a grant would name it ([`super::named_host`]), for a refused
/// connection; nothing for any other block.
fn host_of(block: &Block, named: &str) -> Option<String> {
    ((block.operation, block.kind) == (Operation::Connect, Kind::Host))
        .then(|| super::named_host(named))
        .flatten()
}

/// The host `named` is, checked as a grant checks one, for a refused lookup; nothing for any
/// other block. Shown, never offered (#1663).
fn looked_up_of(block: &Block, named: &str) -> Option<String> {
    ((block.operation, block.kind) == (Operation::Lookup, Kind::Host))
        .then(|| super::named_host(named))
        .flatten()
}

/// One machine's record, under a data home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    dir: PathBuf,
}

impl Record {
    /// The record under the data home `data`.
    pub fn in_data(data: &Path) -> Self {
        Self {
            dir: data.join(DIR),
        }
    }

    /// The record under this process's data home ([`crate::datahome::root`]), when there is
    /// one and it is not inside a project or a git work tree (ADR 0075 §6).
    pub fn here() -> Option<Self> {
        let data = crate::datahome::root()?;
        crate::datahome::refusal(&data)
            .is_none()
            .then(|| Self::in_data(&data))
    }

    /// The record under the data home this process's environment names, for a reader:
    /// [`Self::here`] without holding a fenced build to it, since nothing is written. A data
    /// home inside a project or a git work tree is refused here too.
    pub fn to_read() -> Option<Self> {
        let data = crate::datahome::root_in(&crate::envvar::var)?;
        crate::datahome::refusal(&data)
            .is_none()
            .then(|| Self::in_data(&data))
    }

    /// The file the project at `root` is recorded in.
    pub fn file(&self, root: &Path) -> PathBuf {
        self.dir.join(format!(
            "{}.jsonl",
            crate::sandbox::Homes::project_key(root)
        ))
    }

    /// Appends `entry` to the project at `root`'s record, letting go of what is older than
    /// [`KEPT_FOR_SECS`] before `entry.at` and past [`AT_MOST_KEPT`].
    pub fn write(&self, root: &Path, entry: &Entry) -> io::Result<()> {
        let line = serde_json::to_string(entry).map_err(io::Error::other)?;
        let file = self.file(root);
        std::fs::create_dir_all(&self.dir)?;
        let _held = crate::rewrite::Lock::on(&self.dir);
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let mut kept: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|line| heard_at(line).is_none_or(|then| within(then, entry.at)))
            .collect();
        let over = (kept.len() + 1).saturating_sub(AT_MOST_KEPT);
        kept.drain(..over);
        if kept.len() == lines.len() && (text.is_empty() || text.ends_with('\n')) {
            let mut out = private_append(&file)?;
            return out.write_all(format!("{line}\n").as_bytes());
        }
        let mut whole: String = kept.iter().map(|one| format!("{one}\n")).collect();
        whole.push_str(&line);
        whole.push('\n');
        crate::rewrite::replace(
            &self.dir,
            &file,
            whole.as_bytes(),
            crate::rewrite::Mode::Private,
        )
    }

    /// Every line of the project at `root`'s record from the [`KEPT_FOR_SECS`] before `now`,
    /// oldest first, that this build reads.
    pub fn read(&self, root: &Path, now: u64) -> Vec<Entry> {
        let Ok(text) = std::fs::read_to_string(self.file(root)) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
            .filter(|entry| entry.at <= now && within(entry.at, now))
            .collect()
    }
}

/// Whether something heard at `then` is still kept at `now`.
fn within(then: u64, now: u64) -> bool {
    then.saturating_add(KEPT_FOR_SECS) > now
}

/// When a line was heard, if it says.
fn heard_at(line: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("at")?
        .as_u64()
}

/// `file`, opened to append, made 0600 if it is new.
fn private_append(file: &Path) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    crate::contain::nofollow(&mut options).open(file)
}

/// The blocks of one operation over the last seven days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    pub operation: Operation,
    pub blocks: u64,
    /// Of them, purlis's own operations: purlis bugs.
    pub ours: u64,
}

/// The Blocks among `entries` in the [`COUNTED_FOR_SECS`] before `now`, per operation, in
/// [`Operation::ALL`]'s order, and only the operations that had any.
pub fn counts(entries: &[Entry], now: u64) -> Vec<Count> {
    let recent: Vec<Block> = recent_blocks(entries, now)
        .map(|(block, _)| block)
        .collect();
    Operation::ALL
        .into_iter()
        .filter_map(|operation| {
            let mine: Vec<&Block> = recent
                .iter()
                .filter(|block| block.operation == operation)
                .collect();
            (!mine.is_empty()).then(|| Count {
                operation,
                blocks: mine.len() as u64,
                ours: mine.iter().filter(|block| block.ours).count() as u64,
            })
        })
        .collect()
}

/// The hosts refused in the [`COUNTED_FOR_SECS`] before `now`, each with how many times, the
/// most refused first (then by name).
pub fn hosts_refused(entries: &[Entry], now: u64) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = Vec::new();
    for host in recent_blocks(entries, now).filter_map(|(_, entry)| entry.target.clone()) {
        match out.iter_mut().find(|(seen, _)| *seen == host) {
            Some((_, times)) => *times += 1,
            None => out.push((host, 1)),
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Each Block of `entries` in the [`COUNTED_FOR_SECS`] before `now`.
fn recent_blocks(entries: &[Entry], now: u64) -> impl Iterator<Item = (Block, &Entry)> {
    entries.iter().filter_map(move |entry| {
        let block = entry.block.filter(|_| entry.event == Event::Block)?;
        (entry.at <= now && entry.at.saturating_add(COUNTED_FOR_SECS) > now)
            .then_some((block, entry))
    })
}

#[cfg(test)]
#[path = "sandboxblock_record_tests.rs"]
mod tests;
