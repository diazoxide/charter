//! The work link log: `workspaces/<ws>/work/<device>.jsonl` (ADR 0088 §3, V40 b).
//!
//! One append-only JSON Lines file per device per workspace. Every line is one of four closed
//! key sets, and holds tracker keys and chat ULIDs only: never a title, a body, a label, a state,
//! a `ForgeRef`, the local principal or the OS user.
//!
//! | Line | Keys |
//! |---|---|
//! | workspace link | `v`, `ts`, `op: "link"`, `item` |
//! | chat link | `v`, `ts`, `op: "link"`, `item`, `chat` |
//! | unlink | `v`, `ts`, `op: "unlink"`, `item`, and `chat` for a chat's |
//! | alias | `v`, `ts`, `op: "alias"`, `from`, `to`, `cause` |
//!
//! **What a reader folds**: every file in every workspace's `work/`, sorted by `ts`, then by
//! file name, then by line. A chat's link is the last chat link or unlink for that chat across
//! every workspace's log, because a chat that changes workspace keeps its link (V3: zero or
//! one). Every key is read through its aliases before anything is compared.
//!
//! **Tier: Plane when the workspace is LIVE, Clone state when it is LOCAL**, the tier of the
//! todos it sits beside (ADR 0069 row 86). The LIVE block in `.gitignore` and the merge rules in
//! `.gitattributes` are [`crate::wscmd`]'s and [`crate::scaffold`]'s.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use super::TrackerKey;

/// The directory under a workspace holding its work link logs.
pub const DIR_NAME: &str = "work";

/// The only `v` this charter writes or reads.
const VERSION: u64 = 1;

/// How `ts` is spelled: UTC, ISO-8601, whole seconds.
const TS: &str = "%Y-%m-%dT%H:%M:%SZ";

/// `workspaces/<ws>/work`.
pub fn dir_for(root: &Path, ws: &str) -> PathBuf {
    root.join("workspaces").join(ws).join(DIR_NAME)
}

/// Why a key now resolves to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// A todo was promoted to an issue (ADR 0088 §5).
    Promoted,
    /// The item cache found the item under a new locator (FW-7).
    Moved,
    /// `workspace rename` moved a todo's workspace.
    Renamed,
}

impl Cause {
    pub fn word(self) -> &'static str {
        match self {
            Cause::Promoted => "promoted",
            Cause::Moved => "moved",
            Cause::Renamed => "renamed",
        }
    }

    fn parse(word: &str) -> Option<Cause> {
        [Cause::Promoted, Cause::Moved, Cause::Renamed]
            .into_iter()
            .find(|c| c.word() == word)
    }
}

/// What one line says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// The workspace holds `item`, or, with `chat`, that chat works on it.
    Link {
        item: TrackerKey,
        chat: Option<String>,
    },
    /// The link ends.
    Unlink {
        item: TrackerKey,
        chat: Option<String>,
    },
    /// `from` now resolves to `to`. Written only through [`append_alias`], which refuses a cycle.
    Alias {
        from: TrackerKey,
        to: TrackerKey,
        cause: Cause,
    },
}

impl Op {
    /// The workspace holds `item` (FI6 layer 3).
    pub fn link(item: TrackerKey) -> Op {
        Op::Link { item, chat: None }
    }

    /// Chat `chat`, by its ULID, works on `item`.
    pub fn link_chat(item: TrackerKey, chat: &str) -> Op {
        Op::Link {
            item,
            chat: Some(chat.to_string()),
        }
    }

    /// The workspace no longer holds `item`.
    pub fn unlink(item: TrackerKey) -> Op {
        Op::Unlink { item, chat: None }
    }

    /// Chat `chat` no longer works on `item`.
    pub fn unlink_chat(item: TrackerKey, chat: &str) -> Op {
        Op::Unlink {
            item,
            chat: Some(chat.to_string()),
        }
    }

    fn chat(&self) -> Option<&str> {
        match self {
            Op::Link { chat, .. } | Op::Unlink { chat, .. } => chat.as_deref(),
            Op::Alias { .. } => None,
        }
    }

    /// The line, in its closed key set.
    fn to_line(&self, ts: DateTime<Utc>) -> Value {
        let mut line = Map::new();
        line.insert("v".into(), Value::from(VERSION));
        line.insert("ts".into(), Value::from(ts.format(TS).to_string()));
        match self {
            Op::Link { item, chat } | Op::Unlink { item, chat } => {
                let op = if matches!(self, Op::Link { .. }) {
                    "link"
                } else {
                    "unlink"
                };
                line.insert("op".into(), Value::from(op));
                line.insert("item".into(), Value::from(item.as_str()));
                if let Some(chat) = chat {
                    line.insert("chat".into(), Value::from(chat.as_str()));
                }
            }
            Op::Alias { from, to, cause } => {
                line.insert("op".into(), Value::from("alias"));
                line.insert("from".into(), Value::from(from.as_str()));
                line.insert("to".into(), Value::from(to.as_str()));
                line.insert("cause".into(), Value::from(cause.word()));
            }
        }
        Value::Object(line)
    }

    /// A line read back, or `None` for one that is not exactly one of the four key sets.
    fn of_line(line: &Value) -> Option<(String, Op)> {
        let fields = line.as_object()?;
        if fields.get("v")?.as_u64()? != VERSION {
            return None;
        }
        let ts = fields.get("ts")?.as_str()?;
        chrono::NaiveDateTime::parse_from_str(ts, TS).ok()?;
        let text = |key: &str| fields.get(key).and_then(Value::as_str);
        let key = |name: &str| text(name).and_then(|k| TrackerKey::parse(k).ok());
        let mut names: Vec<&str> = fields.keys().map(String::as_str).collect();
        names.sort_unstable();
        let op = match (text("op")?, names.as_slice()) {
            (word @ ("link" | "unlink"), ["item", "op", "ts", "v"]) => {
                let item = key("item")?;
                match word {
                    "link" => Op::link(item),
                    _ => Op::unlink(item),
                }
            }
            (word @ ("link" | "unlink"), ["chat", "item", "op", "ts", "v"]) => {
                let item = key("item")?;
                let chat = chat_id(text("chat")?)?;
                match word {
                    "link" => Op::link_chat(item, &chat),
                    _ => Op::unlink_chat(item, &chat),
                }
            }
            ("alias", ["cause", "from", "op", "to", "ts", "v"]) => Op::Alias {
                from: key("from")?,
                to: key("to")?,
                cause: Cause::parse(text("cause")?)?,
            },
            _ => return None,
        };
        Some((ts.to_string(), op))
    }
}

/// `word` when it is a ULID in its canonical spelling.
fn chat_id(word: &str) -> Option<String> {
    crate::reopen::a_ulid(word).filter(|id| id == word)
}

/// Append one line to this device's log in workspace `ws`.
///
/// `device` is this device's id (ADR 0066), and a chat is named by its ULID: anything else is
/// refused before a byte is written. An alias goes through [`append_alias`] instead, which
/// refuses one that would close a cycle.
pub fn append(root: &Path, ws: &str, device: &str, ts: DateTime<Utc>, op: &Op) -> io::Result<()> {
    if matches!(op, Op::Alias { .. }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an alias is written through append_alias, which refuses a cycle",
        ));
    }
    write(root, ws, device, ts, op)
}

/// Where a chat is when it is linked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place<'a> {
    /// The project root: in no workspace on purpose (`CONTEXT.md`, **Plane root**).
    ProjectRoot,
    Workspace(&'a str),
}

/// Link chat `chat` to `item`, in the log of the workspace it is in.
///
/// **A chat at the project root is refused** (ADR 0088 §4, Q5's recommendation): it is in no
/// workspace, and work items are a workspace's Work section (FI5), so there is no log for its
/// link until a ruling gives the project one.
pub fn link_chat(
    root: &Path,
    place: Place<'_>,
    device: &str,
    ts: DateTime<Utc>,
    item: TrackerKey,
    chat: &str,
) -> io::Result<()> {
    match place {
        Place::ProjectRoot => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a chat at the project root is in no workspace, so it cannot be linked to a work \
             item: open the chat in the workspace the item belongs to",
        )),
        Place::Workspace(ws) => append(root, ws, device, ts, &Op::link_chat(item, chat)),
    }
}

/// Append `from → to` to this device's log in workspace `ws`, refused when `to` already
/// resolves back to `from` (ADR 0088 §2): an alias that would close a cycle is refused when it is
/// written.
pub fn append_alias(
    root: &Path,
    ws: &str,
    device: &str,
    ts: DateTime<Utc>,
    from: TrackerKey,
    to: TrackerKey,
    cause: Cause,
) -> io::Result<()> {
    let folded = fold(root);
    if from == to || folded.resolve(&to) == from || folded.passes(&to, &from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("an alias from {from} to {to} would close a cycle, so it was not written"),
        ));
    }
    write(root, ws, device, ts, &Op::Alias { from, to, cause })
}

fn write(root: &Path, ws: &str, device: &str, ts: DateTime<Utc>, op: &Op) -> io::Result<()> {
    use std::io::Write;
    let refused = |why: String| io::Error::new(io::ErrorKind::InvalidInput, why);
    if chat_id(device).is_none() {
        return Err(refused(format!(
            "{device:?} is not a device id: the work link log is named by this device's id"
        )));
    }
    if let Some(chat) = op.chat()
        && chat_id(chat).is_none()
    {
        return Err(refused(format!(
            "{chat:?} is not a chat id: a work link names a chat by its ULID"
        )));
    }
    if !crate::contain::segment_ok(ws) {
        return Err(refused(format!("{ws:?} is not a workspace name")));
    }
    let dir = dir_for(root, ws);
    let path = dir.join(format!("{device}.jsonl"));
    crate::contain::writable(root, &path)
        .map_err(|why| io::Error::new(io::ErrorKind::PermissionDenied, why.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    let mut file = crate::contain::nofollow(&mut options).open(&path)?;
    let line = format!("{}\n", op.to_line(ts));
    // One write of one whole line, so the union merge driver always keeps whole lines.
    file.write_all(line.as_bytes())
}

/// Every workspace's log, folded.
#[derive(Debug, Default)]
pub struct Fold {
    /// Each chat's last link or unlink: the workspace whose log said it, and the item, or
    /// `None` after an unlink.
    chats: BTreeMap<String, (String, Option<TrackerKey>)>,
    /// Each workspace's own links, as written (unresolved).
    held: BTreeMap<String, BTreeSet<TrackerKey>>,
    aliases: BTreeMap<TrackerKey, TrackerKey>,
    skipped: usize,
}

impl Fold {
    /// `key` read through its aliases to the end. A cycle a merge made stops before the first
    /// key it would repeat, so a reader always lands on a key and never loops.
    pub fn resolve(&self, key: &TrackerKey) -> TrackerKey {
        let mut seen = BTreeSet::from([key.clone()]);
        let mut at = key.clone();
        while let Some(next) = self.aliases.get(&at) {
            if !seen.insert(next.clone()) {
                break;
            }
            at = next.clone();
        }
        at
    }

    /// Whether resolving `start` passes through `key` on the way.
    fn passes(&self, start: &TrackerKey, key: &TrackerKey) -> bool {
        let mut seen = BTreeSet::from([start.clone()]);
        let mut at = start.clone();
        while let Some(next) = self.aliases.get(&at) {
            if next == key {
                return true;
            }
            if !seen.insert(next.clone()) {
                return false;
            }
            at = next.clone();
        }
        false
    }

    /// Whether `key` has an alias: something else now stands for it.
    pub fn is_aliased(&self, key: &TrackerKey) -> bool {
        self.aliases.contains_key(key)
    }

    /// The item chat `chat` is linked to, through its aliases, or `None`.
    pub fn chat_link(&self, chat: &str) -> Option<TrackerKey> {
        let (_, item) = self.chats.get(chat)?;
        item.as_ref().map(|item| self.resolve(item))
    }

    /// Every chat linked to `item`, which is resolved first.
    pub fn chats_on(&self, item: &TrackerKey) -> Vec<String> {
        let item = self.resolve(item);
        self.chats
            .iter()
            .filter(|(chat, _)| self.chat_link(chat).as_ref() == Some(&item))
            .map(|(chat, _)| chat.clone())
            .collect()
    }

    /// Workspace `ws`'s items, each resolved: its own links, and the item each chat whose
    /// current link its log wrote is linked to.
    pub fn items_of(&self, ws: &str) -> Vec<TrackerKey> {
        let mut out: BTreeSet<TrackerKey> = self
            .held
            .get(ws)
            .into_iter()
            .flatten()
            .map(|item| self.resolve(item))
            .collect();
        for (written_in, item) in self.chats.values() {
            if written_in == ws
                && let Some(item) = item
            {
                out.insert(self.resolve(item));
            }
        }
        out.into_iter().collect()
    }

    /// The chats whose current link was written in workspace `ws`'s log.
    pub fn chats_in(&self, ws: &str) -> Vec<String> {
        self.chats
            .iter()
            .filter(|(_, (written_in, item))| written_in == ws && item.is_some())
            .map(|(chat, _)| chat.clone())
            .collect()
    }

    /// Every key an alias cycle holds, which a merge of two devices' logs can make and
    /// `doctor` reports.
    pub fn cycles(&self) -> Vec<TrackerKey> {
        self.aliases
            .keys()
            .filter(|key| self.passes(key, key))
            .cloned()
            .collect()
    }

    /// How many lines were not one of the four key sets, and were skipped.
    pub fn skipped(&self) -> usize {
        self.skipped
    }
}

/// Every workspace's log under `root`, folded. Read best-effort: an unreadable file or
/// directory holds nothing, and a line that is not one of the four key sets is skipped and
/// counted.
pub fn fold(root: &Path) -> Fold {
    let mut lines: Vec<(String, String, String, usize, Op)> = Vec::new();
    let mut out = Fold::default();
    let Ok(workspaces) = std::fs::read_dir(root.join("workspaces")) else {
        return out;
    };
    for ws in workspaces.flatten() {
        let ws_name = ws.file_name().to_string_lossy().into_owned();
        let Ok(files) = std::fs::read_dir(ws.path().join(DIR_NAME)) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".jsonl") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(file.path()) else {
                continue;
            };
            for (n, raw) in text.lines().enumerate() {
                if raw.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(raw)
                    .ok()
                    .as_ref()
                    .and_then(Op::of_line)
                {
                    Some((ts, op)) => lines.push((ts, name.clone(), ws_name.clone(), n, op)),
                    None => out.skipped += 1,
                }
            }
        }
    }
    lines.sort_by(|a, b| (&a.0, &a.1, &a.2, a.3).cmp(&(&b.0, &b.1, &b.2, b.3)));
    for (_, _, ws, _, op) in lines {
        match op {
            Op::Link {
                item,
                chat: Some(chat),
            } => {
                out.chats.insert(chat, (ws, Some(item)));
            }
            Op::Unlink {
                chat: Some(chat), ..
            } => {
                out.chats.insert(chat, (ws, None));
            }
            Op::Link { item, chat: None } => {
                out.held.entry(ws).or_default().insert(item);
            }
            Op::Unlink { item, chat: None } => {
                out.held.entry(ws).or_default().remove(&item);
            }
            Op::Alias { from, to, .. } => {
                out.aliases.insert(from, to);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
