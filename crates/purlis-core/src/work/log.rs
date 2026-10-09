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

/// Link chat `chat`, by its ULID, to `item`, in this device's log of the workspace the chat is
/// in (ADR 0088 §3). This is the writer the window's host calls.
///
/// - **A chat at the project root is refused** (ADR 0088 §4, Q5's recommendation): it is in no
///   workspace, and work items are a workspace's Work section (FI5), so there is no log for its
///   link until a ruling gives the project one.
/// - **A chat already linked to `item` by this workspace's log, after both are resolved, writes
///   nothing**, so a window that links twice does not grow a log that is committed when the
///   workspace is LIVE. The same item linked from another workspace is written there, so that
///   workspace's Work list holds it.
/// - **A chat linked to another item gets one link line and no unlink**: a chat's link is its
///   last line (V3: zero or one), so the new line replaces the old link. It is dated after the
///   line it replaces ([`after_its_lines`]).
/// - **A `todo:` key must name a todo** (#918): one the workspace it names holds open, or one
///   a promote aliased to the item it now is. A typo, or a todo since closed, is refused before
///   anything is written, since the Work list could never show the link. The key is never
///   rewritten.
/// - **The fold is read again after the line is written**, and a link it does not show is an
///   error, never a success. A line synced from another device while this one was written, or
///   one already in another log dated later, can end the link: the error says so, and a second
///   try is dated after that line.
pub fn link_chat(
    root: &Path,
    place: Place<'_>,
    device: &str,
    ts: DateTime<Utc>,
    item: TrackerKey,
    chat: &str,
) -> io::Result<()> {
    let ws = in_a_workspace(place, Act::Link)?;
    let folded = fold(root);
    let wanted = folded.resolve(&item);
    names_a_todo(root, &item, &wanted)?;
    if folded
        .chat_link_in(chat)
        .is_some_and(|(written_in, linked)| written_in == ws && linked == wanted)
    {
        return Ok(());
    }
    let ts = after_its_lines(&folded, chat, ts);
    append(root, ws, device, ts, &Op::link_chat(item, chat))?;
    linked_as_asked(&fold(root), chat, &wanted)
}

/// Refused when `wanted`, the item `item` resolves to through its aliases, is a todo key that
/// names no open todo in the workspace it names. Any other tracker's key passes: whether its
/// item exists is the tracker's to say.
///
/// A todo a promote aliased resolves to the issue it became, so it passes however its own file
/// went. A store that cannot be read is that error, never a refusal of the key.
fn names_a_todo(root: &Path, item: &TrackerKey, wanted: &TrackerKey) -> io::Result<()> {
    let Some((ws, stem)) = wanted.todo_parts() else {
        return Ok(());
    };
    let open = match crate::workspaces::Plane::open(root).workspace(ws) {
        Ok(workspace) => workspace.todos()?,
        Err(_) => Vec::new(),
    };
    if open.iter().any(|todo| todo.slug == stem) {
        return Ok(());
    }
    let through = if wanted == item {
        String::new()
    } else {
        format!(", which {item} is an alias of,")
    };
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "{wanted}{through} names no todo: workspace {ws} has no open todo {stem}. Pick the \
             item from the workspace's Work list"
        ),
    ))
}

/// Whether chat `chat` works on `wanted` once its link line is written, or the refusal.
///
/// Every line already in a log for the chat is dated before the new one, so the only line that
/// can undo it is one that arrived while it was written: a sync of another device's log.
fn linked_as_asked(after: &Fold, chat: &str, wanted: &TrackerKey) -> io::Result<()> {
    match after.chat_link(chat) {
        Some(now) if now == *wanted => Ok(()),
        now => Err(io::Error::other(format!(
            "the chat was not linked to {wanted}: a line from another device, synced as this \
             one was written, {}. Try again: a new attempt is dated after it",
            match now {
                Some(other) => format!("links it to {other}"),
                None => "ends the link".to_owned(),
            }
        ))),
    }
}

/// End chat `chat`'s link, in this device's log of the workspace the chat is in now, and answer
/// the item it worked on, or `None` when it had no link and nothing was written.
///
/// The unlink names the item the chat's link resolves to, so it ends that link and only that
/// one (D-0015). It is written where the chat is now, not where its link was written: a chat's
/// link is its last line across every workspace's log (§3), so either ends it once the fold
/// orders it after the link. The fold orders by `(ts, device file, line index, workspace)`, so
/// the unlink is dated after the link line it ends ([`after_its_lines`]), whatever this
/// machine's clock says and wherever each line sits. The fold is read again after the line is
/// written: a line synced from another device in that moment can still link the chat again,
/// and that is an error that says what the chat works on now. A chat at the project root is
/// refused, as [`link_chat`] refuses it.
pub fn unlink_chat(
    root: &Path,
    place: Place<'_>,
    device: &str,
    ts: DateTime<Utc>,
    chat: &str,
) -> io::Result<Option<TrackerKey>> {
    let ws = in_a_workspace(place, Act::Unlink)?;
    let folded = fold(root);
    let Some(item) = folded.chat_link(chat) else {
        return Ok(None);
    };
    let ts = after_its_lines(&folded, chat, ts);
    append(root, ws, device, ts, &Op::unlink_chat(item.clone(), chat))?;
    unlinked_as_asked(&fold(root), chat, &item)?;
    Ok(Some(item))
}

/// Whether chat `chat` has no link once its unlink of `item` is written, or the refusal, which
/// says what the chat works on now. As in [`linked_as_asked`], only a line synced from another
/// device as this one was written can undo it.
fn unlinked_as_asked(after: &Fold, chat: &str, item: &TrackerKey) -> io::Result<()> {
    match after.chat_link(chat) {
        None => Ok(()),
        Some(now) if now == *item => Err(io::Error::other(format!(
            "the chat was not unlinked from {item}: a line from another device, synced as this \
             one was written, links it again. Try again: a new attempt is dated after it"
        ))),
        Some(now) => Err(io::Error::other(format!(
            "the chat was not unlinked from {item}: another device linked it to {now} as this \
             was written, so it works on {now} now"
        ))),
    }
}

/// `ts`, or one second after the last line any log holds for chat `chat`, whichever is later.
///
/// The fold orders by `(ts, device file, line index, workspace)`, so a line in the same second as
/// one in another workspace's log, or a device's other file, can sort before it whatever its
/// place. Dated after every line for the chat, including its current link, a new line is taken
/// after them all, from any workspace and whatever this machine's clock says. Only lines for
/// this chat move it, so a clock that is right writes the time it reads.
fn after_its_lines(folded: &Fold, chat: &str, ts: DateTime<Utc>) -> DateTime<Utc> {
    folded
        .last_line_for(chat)
        .map_or(ts, |at| ts.max(at + chrono::Duration::seconds(1)))
}

/// What a chat's write was, for its refusal.
#[derive(Debug, Clone, Copy)]
enum Act {
    Link,
    Unlink,
}

impl Act {
    /// What the chat could not be: `linked to` or `unlinked from` a work item.
    fn could_not_be(self) -> &'static str {
        match self {
            Act::Link => "linked to",
            Act::Unlink => "unlinked from",
        }
    }
}

/// The workspace a chat at `place` is in, or the refusal for one at the project root.
fn in_a_workspace(place: Place<'_>, act: Act) -> io::Result<&str> {
    match place {
        Place::ProjectRoot => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "a chat at the project root is in no workspace, so it cannot be {} a work \
                 item: open the chat in the workspace the item belongs to",
                act.could_not_be()
            ),
        )),
        Place::Workspace(ws) => Ok(ws),
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
    append_alias_over(&fold(root), root, ws, device, ts, from, to, cause)
}

/// [`append_alias`], checked against a fold the caller already holds, so that one decision reads
/// the log once.
#[allow(clippy::too_many_arguments)]
pub fn append_alias_over(
    folded: &Fold,
    root: &Path,
    ws: &str,
    device: &str,
    ts: DateTime<Utc>,
    from: TrackerKey,
    to: TrackerKey,
    cause: Cause,
) -> io::Result<()> {
    if from == to || folded.resolve(&to) == from || folded.passes(&to, &from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("an alias from {from} to {to} would close a cycle, so it was not written"),
        ));
    }
    write(root, ws, device, ts, &Op::Alias { from, to, cause })
}

/// Everything [`append`] would check before it writes a line to device `device`'s log in
/// workspace `ws`, asked before anything else is done: the device id, the workspace's name and
/// containment of the file. A caller about to do something it cannot undo, such as opening an
/// issue, asks this first, so that a log it could never have written refuses it up front.
pub fn check_writable(root: &Path, ws: &str, device: &str) -> io::Result<PathBuf> {
    let refused = |why: String| io::Error::new(io::ErrorKind::InvalidInput, why);
    if chat_id(device).is_none() {
        return Err(refused(format!(
            "{device:?} is not a device id: the work link log is named by this device's id"
        )));
    }
    if !crate::contain::segment_ok(ws) {
        return Err(refused(format!("{ws:?} is not a workspace name")));
    }
    let path = dir_for(root, ws).join(format!("{device}.jsonl"));
    crate::contain::writable(root, &path)
        .map_err(|why| io::Error::new(io::ErrorKind::PermissionDenied, why.to_string()))?;
    Ok(path)
}

fn write(root: &Path, ws: &str, device: &str, ts: DateTime<Utc>, op: &Op) -> io::Result<()> {
    use std::io::Write;
    if let Some(chat) = op.chat()
        && chat_id(chat).is_none()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{chat:?} is not a chat id: a work link names a chat by its ULID"),
        ));
    }
    let path = check_writable(root, ws, device)?;
    crate::rewrite::create_dir_all(&dir_for(root, ws))?;
    // Never dated before the log's last line, so a clock that stepped back cannot reorder this
    // device's own lines in the fold (which sorts by `ts` first).
    let ts = last_ts(&path).map_or(ts, |last| ts.max(last));
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    let mut file = crate::contain::nofollow(&mut options)
        .open(&path)
        .map_err(crate::rewrite::refused_at(&path))?;
    let line = format!("{}\n", op.to_line(ts));
    // One write of one whole line, so the union merge driver always keeps whole lines.
    file.write_all(line.as_bytes())
        .map_err(crate::rewrite::refused_at(&path))
}

/// The `ts` of the last line in the log at `path`, where it has one that reads.
fn last_ts(path: &Path) -> Option<DateTime<Utc>> {
    let text = std::fs::read_to_string(path).ok()?;
    let line: Value =
        serde_json::from_str(text.lines().rev().find(|l| !l.trim().is_empty())?).ok()?;
    let ts = chrono::NaiveDateTime::parse_from_str(line.get("ts")?.as_str()?, TS).ok()?;
    Some(ts.and_utc())
}

/// Every workspace's log, folded.
#[derive(Debug, Default)]
pub struct Fold {
    /// Each chat's current link: the workspace whose log wrote it, and the item, resolved.
    chats: BTreeMap<String, (String, TrackerKey)>,
    /// Each workspace's own links, resolved.
    held: BTreeMap<String, BTreeSet<TrackerKey>>,
    aliases: BTreeMap<TrackerKey, TrackerKey>,
    /// The `ts` of the last chat link or unlink line naming each chat, whatever it did.
    last_for: BTreeMap<String, DateTime<Utc>>,
    /// Lines skipped, by `<workspace>/<file>`.
    skipped: BTreeMap<String, usize>,
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

    /// Whether some alias leads to `key`. An alias from `key` to anything could then close a
    /// cycle, so a promote asks this before it opens an issue it could not alias.
    pub fn is_reached(&self, key: &TrackerKey) -> bool {
        self.aliases.values().any(|to| to == key)
    }

    /// The item chat `chat` is linked to, through its aliases, or `None`.
    pub fn chat_link(&self, chat: &str) -> Option<TrackerKey> {
        self.chats.get(chat).map(|(_, item)| self.resolve(item))
    }

    /// The workspace whose log wrote chat `chat`'s current link, and the item, resolved.
    pub fn chat_link_in(&self, chat: &str) -> Option<(&str, TrackerKey)> {
        self.chats
            .get(chat)
            .map(|(ws, item)| (ws.as_str(), self.resolve(item)))
    }

    /// When the last line naming chat `chat`, a link or an unlink, was dated.
    fn last_line_for(&self, chat: &str) -> Option<DateTime<Utc>> {
        self.last_for.get(chat).copied()
    }

    /// Every chat linked to `item`, which is resolved first.
    pub fn chats_on(&self, item: &TrackerKey) -> Vec<String> {
        let item = self.resolve(item);
        self.chats
            .keys()
            .filter(|chat| self.chat_link(chat).as_ref() == Some(&item))
            .cloned()
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
            if written_in == ws {
                out.insert(self.resolve(item));
            }
        }
        out.into_iter().collect()
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
        self.skipped.values().sum()
    }

    /// The skipped lines, by the file they are in (`<workspace>/<file>`).
    pub fn skipped_in(&self) -> &BTreeMap<String, usize> {
        &self.skipped
    }
}

/// Every workspace's log under `root`, folded. Read best-effort: a directory or file containment
/// refuses, or one that cannot be read, holds nothing, and a line that is not one of the four key
/// sets is skipped and counted.
///
/// Lines are taken in ADR 0088 §3's order, `ts`, then file name, then line; a tie that order
/// leaves, the same file name and line in two workspaces, goes to the workspace's name. Aliases
/// are read first, so an unlink matches the item it names after both are resolved.
pub fn fold(root: &Path) -> Fold {
    let mut lines: Vec<(String, String, usize, String, Op)> = Vec::new();
    let mut out = Fold::default();
    let workspaces_dir = root.join("workspaces");
    if crate::contain::readable(root, &workspaces_dir).is_err() {
        return out;
    }
    let Ok(workspaces) = std::fs::read_dir(&workspaces_dir) else {
        return out;
    };
    for ws in workspaces.flatten() {
        let ws_name = ws.file_name().to_string_lossy().into_owned();
        let dir = ws.path().join(DIR_NAME);
        if crate::contain::readable(root, &dir).is_err() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".jsonl") || crate::contain::readable(root, &file.path()).is_err() {
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
                    Some((ts, op)) => lines.push((ts, name.clone(), n, ws_name.clone(), op)),
                    None => *out.skipped.entry(format!("{ws_name}/{name}")).or_default() += 1,
                }
            }
        }
    }
    lines.sort_by(|a, b| (&a.0, &a.1, a.2, &a.3).cmp(&(&b.0, &b.1, b.2, &b.3)));
    for (_, _, _, _, op) in &lines {
        if let Op::Alias { from, to, .. } = op {
            out.aliases.insert(from.clone(), to.clone());
        }
    }
    for (ts, _, _, ws, op) in lines {
        if let Some(chat) = op.chat()
            && let Ok(at) = chrono::NaiveDateTime::parse_from_str(&ts, TS)
        {
            out.last_for.insert(chat.to_string(), at.and_utc());
        }
        match op {
            Op::Link {
                item,
                chat: Some(chat),
            } => {
                let item = out.resolve(&item);
                out.chats.insert(chat, (ws, item));
            }
            Op::Unlink {
                item,
                chat: Some(chat),
            } => {
                let item = out.resolve(&item);
                if out
                    .chats
                    .get(&chat)
                    .is_some_and(|(_, linked)| *linked == item)
                {
                    out.chats.remove(&chat);
                }
            }
            Op::Link { item, chat: None } => {
                let item = out.resolve(&item);
                out.held.entry(ws).or_default().insert(item);
            }
            Op::Unlink { item, chat: None } => {
                let item = out.resolve(&item);
                out.held.entry(ws).or_default().remove(&item);
            }
            Op::Alias { .. } => {}
        }
    }
    out
}

#[cfg(test)]
mod tests;
