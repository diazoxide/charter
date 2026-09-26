//! What a workspace's **pieces** — its linked worktrees — have said about themselves.
//!
//! A port of `charter/pieces.py`: the append-only event log, the heartbeat store beside it,
//! and the questions the footer and the briefing ask of them. Two writers: [`seen`], the
//! heartbeat the hooks keep, and [`record`], the log itself — `claimed` when `charter
//! worktree add` cuts a piece, and the worker's own `done` or `abandoned` through [`declare`]
//! (charter#368).
//!
//! # The vocabulary is closed, and it has no verdict in it
//!
//! `claimed` is an observation charter makes when it cuts the worktree; `done` and
//! `abandoned` are the worker's own declarations. There is deliberately no `failed`,
//! `blocked` or `timed-out` (ADR 0011): charter can verify none of them, and a state nobody
//! can verify is the marker that lies. A worker that dies declares nothing at all, and that
//! *absence* is what gets reported — as an age, never as a judgement. [`silence`] answers how
//! long a piece has said nothing; whether that is a problem is the reader's call.
//!
//! # Everything here is read best-effort
//!
//! This feeds a footer that repaints on every turn, and an append-only log collects
//! half-written lines from killed processes. A malformed line is skipped, an unreadable file
//! is absent, and a directory charter may not list is empty — because the alternative on this
//! path is no footer at all.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;

/// The directory under a workspace holding its piece records.
pub const DIR_NAME: &str = "pieces";

/// Liveness lives under here, one file per piece, overwritten rather than appended — the log
/// is *what happened*, this is *the latest observation*.
pub const SEEN_DIR: &str = "seen";

/// The whole vocabulary, in `charter/pieces.py`'s order.
pub const EVENTS: [&str; 3] = ["claimed", "done", "abandoned"];

/// The subset a worker declares about itself, as opposed to what charter observed.
pub const DECLARATIONS: [&str; 2] = ["done", "abandoned"];

/// `workspaces/<ws>/pieces`.
pub fn dir_for(plane: &Path, ws: &str) -> PathBuf {
    plane.join("workspaces").join(ws).join(DIR_NAME)
}

/// One of the three things the log can say — the closed vocabulary [`EVENTS`] spells, as a
/// type, so a caller cannot invent a fourth. There is no `failed`: see the module's heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// charter cut the worktree. An observation, not a declaration.
    Claimed,
    /// The worker says the piece is finished.
    Done,
    /// The worker says it gave the piece up. Always carries a reason.
    Abandoned,
}

impl Event {
    /// The word written into the log.
    pub const fn word(self) -> &'static str {
        match self {
            Event::Claimed => "claimed",
            Event::Done => "done",
            Event::Abandoned => "abandoned",
        }
    }
}

/// Who a line in the log is from: the session, the persona and the machine.
///
/// Passed in rather than read here, so this module never reads the process environment and
/// a test names exactly who it is recording as.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Who {
    pub session: Option<String>,
    pub persona: Option<String>,
    /// The filename-safe host ([`crate::dispatch::host`]). The log is one file per machine.
    pub host: String,
}

/// `workspaces/<ws>/pieces/<host>.jsonl` — this machine's log.
pub fn log_path(plane: &Path, ws: &str, host: &str) -> PathBuf {
    dir_for(plane, ws).join(format!("{host}.jsonl"))
}

/// Append one event — `pieces.record`. `None` when it could not be written.
///
/// The line carries `ts`, `event`, `repo`, `piece`, `session`, `host`, `persona` and, when
/// there is one, `reason` — Python's closed `FIELDS`, and nothing git could answer: no
/// branch, no path, no dirty flag (ADR 0011). Appended `O_APPEND` and `O_NOFOLLOW` through
/// [`crate::dispatch::append`], so parallel workers need no lock and a link planted at the
/// log is refused.
#[allow(clippy::too_many_arguments)]
pub fn record(
    plane: &Path,
    ws: &str,
    event: Event,
    repo: &str,
    piece: &str,
    reason: Option<&str>,
    who: &Who,
    when: DateTime<Utc>,
) -> Option<PathBuf> {
    let text = |v: &Option<String>| v.clone().map_or(Value::Null, Value::String);
    let mut line = serde_json::Map::new();
    line.insert("ts".into(), Value::String(iso_seconds(when)));
    line.insert("event".into(), Value::String(event.word().into()));
    line.insert("repo".into(), Value::String(repo.into()));
    line.insert("piece".into(), Value::String(piece.into()));
    line.insert("session".into(), text(&who.session));
    line.insert("host".into(), Value::String(who.host.clone()));
    line.insert("persona".into(), text(&who.persona));
    if let Some(reason) = reason.filter(|r| !r.is_empty()) {
        line.insert("reason".into(), Value::String(reason.into()));
    }
    crate::dispatch::append(&log_path(plane, ws, &who.host), plane, &Value::Object(line))
}

/// What a worker declares about the piece it stands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declaration<'a> {
    Done,
    /// The reason is required: it is what whoever picks the piece up reads first.
    Abandoned {
        reason: &'a str,
    },
}

/// A declaration, written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub event: Event,
    /// The declaration this one was recorded over, as [`outcome`] reads it. It stays in the
    /// log; only the latest counts.
    pub over: Option<String>,
}

/// Why a declaration was not written.
#[derive(Debug, thiserror::Error)]
pub enum NotDeclared {
    #[error("abandon needs a reason — it is what whoever picks this up reads first.")]
    NoReason,
    #[error(
        "'{piece}' is not a worktree git has for {repo} in workspace '{ws}', so there is \
         nothing to declare. `charter worktree list` shows what exists."
    )]
    NoSuchPiece {
        ws: String,
        repo: String,
        piece: String,
    },
    #[error(transparent)]
    Worktree(#[from] crate::worktree::Refusal),
    #[error("could not write the piece log for workspace '{ws}', so nothing was declared")]
    NotWritten { ws: String },
}

/// Record that the piece `(ws, repo, piece)` is done or abandoned — `commands_worktree._declare`.
///
/// **The piece must be one git has**, with its directory still there: a declaration about a
/// worktree that does not exist would be a line every reader skips, and the worker would
/// believe it had said something. Unlike [`record`] for a claim, a declaration that could not
/// be written is an error, because the declaration IS the command's whole effect.
pub fn declare(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    what: Declaration<'_>,
    who: &Who,
    when: DateTime<Utc>,
) -> Result<Declared, NotDeclared> {
    let (event, reason) = match what {
        Declaration::Done => (Event::Done, None),
        Declaration::Abandoned { reason } => {
            let reason = reason.trim();
            if reason.is_empty() {
                return Err(NotDeclared::NoReason);
            }
            (Event::Abandoned, Some(reason))
        }
    };
    let there = crate::worktree::list(plane, ws, repo)?
        .into_iter()
        .any(|p| p.piece == piece && p.prunable.is_none());
    if !there {
        return Err(NotDeclared::NoSuchPiece {
            ws: ws.to_string(),
            repo: repo.to_string(),
            piece: piece.to_string(),
        });
    }
    let over = declarations(plane, ws)
        .get(&(repo.to_string(), piece.to_string()))
        .map(|e| outcome(Some(e)));
    record(plane, ws, event, repo, piece, reason, who, when)
        .ok_or(NotDeclared::NotWritten { ws: ws.to_string() })?;
    Ok(Declared { event, over })
}

/// How a claim reads in a listing — `pieces.claimant`: the persona, else the session, else the
/// host. `unknown` when nobody claimed it: a worktree made by hand with plain git is
/// first-class and simply has no claim.
pub fn claimant(entry: Option<&Value>) -> String {
    let field = |k: &str| {
        entry
            .and_then(|e| e.get(k))
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
    };
    field("persona")
        .or_else(|| field("session"))
        .or_else(|| field("host"))
        .unwrap_or("unknown")
        .to_string()
}

/// What a piece has said, in one cell: its declaration ([`outcome`]), or `silent <age>` when
/// it declared nothing and charter claimed it, or empty. An age, never a verdict: whether
/// `silent 3d` is a problem is the reader's call.
pub fn said(plane: &Path, ws: &str, repo: &str, piece: &str, now: DateTime<Utc>) -> String {
    let declared = declarations(plane, ws);
    let spoken = outcome(declared.get(&(repo.to_string(), piece.to_string())));
    if !spoken.is_empty() {
        return spoken;
    }
    match silence(plane, ws, repo, piece, now) {
        Ok(Some(age)) => format!("silent {age}"),
        _ => String::new(),
    }
}

/// Where a tree's heartbeat lives.
///
/// The clone's own record is a file *beside* the piece directory, never inside it: piece
/// names come from branch names, which are not charter's to constrain, so a piece named like
/// any sentinel would collide with the clone's record.
pub fn seen_path(plane: &Path, ws: &str, repo: &str, piece: Option<&str>) -> PathBuf {
    let base = dir_for(plane, ws).join(SEEN_DIR);
    match piece {
        None => base.join(format!("{repo}.json")),
        Some(piece) => base.join(repo).join(format!("{piece}.json")),
    }
}

/// Every recorded event, oldest first.
///
/// A malformed line is skipped rather than fatal. Listing an unreadable directory raises on
/// Linux and yields nothing on macOS — the divergence is why the failure is swallowed here
/// rather than left to a caller: a suite green on a developer's Mac went red on CI over
/// exactly that line, and what it produced was the blank footer this module promises never to
/// show.
pub fn events(plane: &Path, ws: &str) -> Vec<Value> {
    let dir = dir_for(plane, ws);
    let Ok(reader) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = reader
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    // `sorted(d.glob("*.jsonl"))` — by path, so one host's log is read whole before the next.
    files.sort();

    let mut out: Vec<Value> = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for raw in text.lines() {
            let Ok(obj) = serde_json::from_str::<Value>(raw) else {
                continue;
            };
            // `isinstance(obj, dict) and obj.get("piece")` — a JSON `null`, `""` or `false`
            // there is falsy to Python, so the line is not about a piece.
            if obj.is_object() && truthy(obj.get("piece")) {
                out.push(obj);
            }
        }
    }
    // `sorted(out, key=lambda e: e.get("ts") or "")` — a STABLE sort by the timestamp string,
    // so lines with no `ts` keep the order they were read in rather than being dropped.
    out.sort_by_key(ts_key);
    out
}

/// Python's truthiness for the two shapes that reach these fields: a non-empty string, a
/// non-zero number. Everything else — absent, `null`, `""`, `0`, `false`, `[]`, `{}` — is not.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_none_or(|f| f != 0.0),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// `e.get("ts") or ""` — a non-string `ts` is not a string key in Python either; it would
/// raise on comparison, so charter's own logs only ever carry a string here.
fn ts_key(e: &Value) -> String {
    e.get("ts")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `(repo, piece)` for an event, with a missing repo reading as `""`.
fn key_of(e: &Value) -> Option<(String, String)> {
    let piece = e.get("piece")?.as_str()?.to_string();
    let repo = e
        .get("repo")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some((repo, piece))
}

/// The latest event of each of `kinds`, per piece. Latest wins and the earlier ones stay in
/// the log: a worker that declared `done` and then found it was not done must be able to say
/// so, and the history of it having changed its mind is worth more than a record that
/// pretends the first answer never happened.
fn latest(plane: &Path, ws: &str, kinds: &[&str]) -> BTreeMap<(String, String), Value> {
    let mut out = BTreeMap::new();
    for e in events(plane, ws) {
        let kind = e.get("event").and_then(Value::as_str).unwrap_or_default();
        if !kinds.contains(&kind) {
            continue;
        }
        if let Some(key) = key_of(&e) {
            out.insert(key, e);
        }
    }
    out
}

/// The latest declaration per piece.
pub fn declarations(plane: &Path, ws: &str) -> BTreeMap<(String, String), Value> {
    latest(plane, ws, &DECLARATIONS)
}

/// The latest `claimed` event per piece.
///
/// Note what this is *not*: an answer to which pieces exist. A claim here for a worktree that
/// was since removed is a true statement about the past, so callers start from the filesystem
/// and use this only to put a name on what they found.
pub fn claims(plane: &Path, ws: &str) -> BTreeMap<(String, String), Value> {
    latest(plane, ws, &["claimed"])
}

/// The latest observation of this piece's worker, or `None`.
///
/// A malformed file reads as `None` rather than raising, and so does one with no `ts`.
pub fn last_seen(plane: &Path, ws: &str, repo: &str, piece: &str) -> Option<Value> {
    let text = std::fs::read_to_string(seen_path(plane, ws, repo, Some(piece))).ok()?;
    let obj: Value = serde_json::from_str(&text).ok()?;
    (obj.is_object() && truthy(obj.get("ts"))).then_some(obj)
}

/// What a timestamp field parses to — and whether it can be compared with `now` at all.
///
/// `datetime.fromisoformat` happily returns a **naive** datetime for a stamp with no offset,
/// and subtracting an aware `now` from it raises `TypeError` rather than answering. charter
/// records `isoformat(timespec="seconds")` of an aware UTC instant, so its own stamps always
/// carry one; a hand-edited file may not, and what Python then does is abandon the whole
/// repo's count ([`Summary`] says where). Kept as a third answer rather than folded into
/// `None`, because `None` means "no stamp", which is a state the footer renders.
enum Stamp {
    At(DateTime<Utc>),
    /// Parsed, but with no offset: Python cannot subtract it and neither will charter.
    Naive,
    None,
}

fn parse(ts: Option<&Value>) -> Stamp {
    let Some(text) = ts.and_then(Value::as_str) else {
        return Stamp::None;
    };
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Stamp::At(at.with_timezone(&Utc));
    }
    // `fromisoformat` accepts a naive stamp; charter's own writer never produces one, so this
    // only has to recognise the shape rather than read it. Recognised by hand because the
    // question is "would Python have got a datetime here", not "what time is it".
    if looks_like_a_naive_stamp(text) {
        return Stamp::Naive;
    }
    Stamp::None
}

/// `YYYY-MM-DDTHH:MM:SS…` with nothing after it that could be an offset.
fn looks_like_a_naive_stamp(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() < 19 {
        return false;
    }
    let digit = |i: usize| b[i].is_ascii_digit();
    let shape = (0..4).all(digit)
        && b[4] == b'-'
        && digit(5)
        && digit(6)
        && b[7] == b'-'
        && digit(8)
        && digit(9)
        && (b[10] == b'T' || b[10] == b' ')
        && digit(11)
        && digit(12)
        && b[13] == b':'
        && digit(14)
        && digit(15)
        && b[16] == b':'
        && digit(17)
        && digit(18);
    // An offset or a `Z` would have made `parse_from_rfc3339` answer already; what is left
    // after the seconds is a fraction, which stays naive.
    shape && !text[19..].contains(['+', 'Z', 'z'])
}

/// A coarse age — `3m`, `5h`, `3d`. Coarse on purpose: the number is context for a human
/// decision, not an input to one charter is making.
pub fn since(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - when).num_seconds().max(0);
    if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// How long this piece has said nothing, or `None` if it declared an outcome.
///
/// Measured from the last observation, falling back to the **claim** when the worker never
/// got as far as a first turn — precisely the case worth seeing, and answering "unknown"
/// there would lose it. `Err(())` is the naive-stamp case [`Stamp`] describes.
#[allow(clippy::result_unit_err)]
pub fn silence(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    now: DateTime<Utc>,
) -> Result<Option<String>, ()> {
    if declarations(plane, ws).contains_key(&(repo.to_string(), piece.to_string())) {
        return Ok(None);
    }
    let Some(claim) = claims(plane, ws)
        .get(&(repo.to_string(), piece.to_string()))
        .cloned()
    else {
        return Ok(None);
    };
    let mark = last_seen(plane, ws, repo, piece);
    // `_parse(mark["ts"]) or _parse(claim["ts"])` — the heartbeat first, the claim behind it.
    let stamp = match parse(mark.as_ref().and_then(|m| m.get("ts"))) {
        Stamp::At(at) => Stamp::At(at),
        Stamp::Naive => Stamp::Naive,
        Stamp::None => parse(claim.get("ts")),
    };
    match stamp {
        Stamp::At(at) => Ok(Some(since(at, now))),
        // Python's `since(None)` is the literal `"?"`, which is an age the footer counts as
        // one more silent piece.
        Stamp::None => Ok(Some("?".to_string())),
        Stamp::Naive => Err(()),
    }
}

/// How a declaration reads in a listing. Empty when there is none — which is *silence*, not a
/// third outcome.
pub fn outcome(entry: Option<&Value>) -> String {
    let Some(entry) = entry else {
        return String::new();
    };
    let event = entry
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match entry.get("reason").and_then(Value::as_str) {
        Some(reason) if !reason.is_empty() => format!("{event}: {reason}"),
        _ => event.to_string(),
    }
}

/// Order silence ages so the OLDEST is the one reported.
///
/// Coarse strings do not sort lexically — `9m` would beat `2d` — and the oldest is the whole
/// point. An age charter did not write (`?`, or a unit nobody knows) ranks zero, as Python's
/// `int(age[:-1])` failing does.
pub fn silence_rank(age: &str) -> i64 {
    // Split at the LAST CHARACTER, not at the last byte: `str::split_at` panics on a byte that
    // is not a boundary, and a panic on the render path is the blank footer this module
    // promises never to show. Every age charter writes is ASCII; this is about the one that
    // charter did not write.
    let Some((at, _)) = age.char_indices().next_back() else {
        return 0;
    };
    let (digits, unit) = age.split_at(at);
    let scale = match unit {
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => return 0,
    };
    digits.parse::<i64>().map(|n| n * scale).unwrap_or(0)
}

/// The workspace line's piece cell, as counts.
///
/// `None` when the workspace has no pieces at all, exactly as the todo count renders nothing
/// at zero: a figure that renders on every session including the ones it means nothing for is
/// how a line stops being read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub total: usize,
    pub done: usize,
    pub gave_up: usize,
    /// One age per piece that has said nothing, in the order they were counted.
    pub quiet: Vec<String>,
}

impl Summary {
    /// The oldest silence, which is the one worth reporting. Python's `max(key=rank)` returns
    /// the FIRST maximal element, so ties keep the counting order.
    pub fn oldest_silence(&self) -> Option<&str> {
        let best = self.quiet.iter().map(|a| silence_rank(a)).max()?;
        self.quiet
            .iter()
            .find(|a| silence_rank(a) == best)
            .map(String::as_str)
    }
}

/// The pieces a workspace holds, counted by what they have said.
///
/// Existence comes from the filesystem, which is filesystem-only by design: `git worktree
/// add` and `remove` are what create and delete these directories, so listing them IS reading
/// git's output — and the footer renders on every turn, where a `git worktree list` per clone
/// would be paid over and over.
///
/// **A repo whose walk raises is skipped whole**, which is Python's `except Exception:
/// continue` around the inner loop and is where a naive timestamp lands: the pieces counted
/// before it in that repo stay counted, and the rest of the repo does not. Faithful rather
/// than endorsed — it is recorded here because it is the behaviour a differential compares.
pub fn summary(plane: &Path, ws: &str, now: DateTime<Utc>) -> Option<Summary> {
    let base = crate::worktree::root_of(plane, ws);
    let reader = std::fs::read_dir(&base).ok()?;
    let mut repos: Vec<String> = reader
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    repos.sort();

    let mut out = Summary {
        total: 0,
        done: 0,
        gave_up: 0,
        quiet: Vec::new(),
    };
    let declared = declarations(plane, ws);
    for repo in repos {
        for piece in piece_dirs(&base.join(&repo)) {
            out.total += 1;
            match declared.get(&(repo.clone(), piece.clone())) {
                Some(d) if d.get("event").and_then(Value::as_str) == Some("done") => out.done += 1,
                Some(_) => out.gave_up += 1,
                None => match silence(plane, ws, &repo, &piece, now) {
                    Ok(Some(age)) => out.quiet.push(age),
                    Ok(None) => {}
                    // The repo's walk stops here, as Python's `continue` does.
                    Err(()) => break,
                },
            }
        }
    }
    (out.total > 0).then_some(out)
}

/// The piece directories under one repo's worktree root, most recently touched first.
///
/// The ORDER is only ever a tie-break here — every piece is counted and only the oldest
/// silence is reported — but it is Python's: `sorted(key=mtime, reverse=True)`, which is
/// stable, so equal mtimes keep the order the directory listed them in.
fn piece_dirs(base: &Path) -> Vec<String> {
    let Ok(reader) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut entries: Vec<(std::time::SystemTime, String)> = reader
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (mtime, e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    entries.into_iter().map(|(_, name)| name).collect()
}

/// How recently a persona must have been seen at a tree to still count as present there —
/// `pieces.PRESENCE_WINDOW`.
pub const PRESENCE_WINDOW_SECS: i64 = 3600;

/// How many present personas one heartbeat keeps — `pieces.PRESENCE_KEEP`.
pub const PRESENCE_KEEP: usize = 8;

/// The latest observation of a tree — a piece, or with `piece` of `None` the clone itself.
fn seen_record(plane: &Path, ws: &str, repo: &str, piece: Option<&str>) -> Option<Value> {
    let text = std::fs::read_to_string(seen_path(plane, ws, repo, piece)).ok()?;
    let obj: Value = serde_json::from_str(&text).ok()?;
    (obj.is_object() && truthy(obj.get("ts"))).then_some(obj)
}

/// `isoformat(timespec="seconds")` of a UTC instant.
fn iso_seconds(when: DateTime<Utc>) -> String {
    when.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

/// Record that the worker in a tree is alive — `pieces.seen`.
///
/// One small file, overwritten. With a `persona`, the record also carries `by`: every persona
/// seen at this tree within [`PRESENCE_WINDOW_SECS`], newest [`PRESENCE_KEEP`], so a second
/// persona picking up someone else's piece is visible rather than overwriting the first.
///
/// `None` when nothing was written: the path is refused by containment, the write failed, or
/// the previous record holds a stamp with no offset — charter's own subtraction raises on one,
/// and the whole touch is dropped rather than half-done.
pub fn seen(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: Option<&str>,
    session: Option<&str>,
    persona: Option<&str>,
    when: DateTime<Utc>,
) -> Option<PathBuf> {
    let path = seen_path(plane, ws, repo, piece);
    let stamp = iso_seconds(when);
    let mut by = serde_json::Map::new();
    if let Some(persona) = persona.filter(|p| !p.is_empty()) {
        let prev = seen_record(plane, ws, repo, piece);
        if let Some(old) = prev
            .as_ref()
            .and_then(|p| p.get("by"))
            .and_then(Value::as_object)
        {
            for (name, ts) in old {
                match parse(Some(ts)) {
                    Stamp::At(at) if (when - at).num_seconds() <= PRESENCE_WINDOW_SECS => {
                        by.insert(name.clone(), ts.clone());
                    }
                    Stamp::Naive => return None,
                    _ => {}
                }
            }
        }
        by.insert(persona.to_string(), Value::String(stamp.clone()));
        // Python sorts and cuts only `if len(by) > PRESENCE_KEEP`. Here it is unconditional,
        // which is the same answer: at or under the bound the cut drops nothing, and the order
        // the sort leaves is not written — `dumps_sorted` writes `by` in key order either way.
        // A guard that could not change the file was a mutant no test could tell apart (#311).
        let mut keep: Vec<(String, Value)> = by.into_iter().collect();
        // `sorted(key=ts, reverse=True)`: stable, so equal stamps keep their order.
        keep.sort_by(|a, b| {
            b.1.as_str()
                .unwrap_or_default()
                .cmp(a.1.as_str().unwrap_or_default())
        });
        keep.truncate(PRESENCE_KEEP);
        by = keep.into_iter().collect();
    }
    let mut blob = serde_json::Map::new();
    blob.insert("ts".into(), Value::String(stamp));
    blob.insert(
        "session".into(),
        session.map_or(Value::Null, |s| Value::String(s.to_string())),
    );
    if let Some(persona) = persona.filter(|p| !p.is_empty()) {
        blob.insert("persona".into(), Value::String(persona.to_string()));
        blob.insert("by".into(), Value::Object(by));
    }
    crate::contain::writable(plane, &path).ok()?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    let text = format!("{}\n", crate::pyjson::dumps_sorted(&Value::Object(blob)));
    // Replaced whole (#434): a link at the record is refused, and one planted after the gate
    // above answered is replaced, never written through. Gated from the record's own
    // directory: `writable` has already answered for the directories above it, and a link
    // among them that stays inside the plane is followed, as it always was.
    let dir = path.parent()?;
    crate::rewrite::replace(dir, &path, text.as_bytes(), crate::rewrite::Mode::Kept).ok()?;
    Some(path)
}

/// The tree `cwd` stands in: a piece, or a clone (`piece` `None`) — `worktree.locate`, then
/// `workspace.clone_of`. The plane's own root and a workspace's container are neither.
pub fn tree_at(plane: &Path, cwd: &Path) -> Option<(String, String, Option<String>)> {
    if let Some(found) = crate::worktree::locate(plane, cwd) {
        return Some((found.workspace, found.repo, Some(found.piece)));
    }
    let here = std::fs::canonicalize(cwd).ok()?;
    let workspaces = std::fs::canonicalize(plane.join("workspaces")).ok()?;
    let rest = here.strip_prefix(&workspaces).ok()?;
    let mut parts = rest.components().map(|c| c.as_os_str().to_string_lossy());
    let ws = parts.next()?.into_owned();
    let repo = parts.next()?.into_owned();
    (!repo.starts_with('.')).then_some((ws, repo, None))
}

/// `hooks._touch_piece`: mark the worker in `cwd` alive. Silent and best-effort — a turn must
/// never fail over bookkeeping.
pub fn touch(
    plane: &Path,
    cwd: &Path,
    session: Option<&str>,
    persona: Option<&str>,
    now: DateTime<Utc>,
) {
    if let Some((ws, repo, piece)) = tree_at(plane, cwd) {
        let _ = seen(plane, &ws, &repo, piece.as_deref(), session, persona, now);
    }
}

/// How long ago this piece was last seen, falling back to its claim — `pieces.seen_age`.
/// `None` for the stamp with no offset charter cannot subtract.
pub fn seen_age(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    now: DateTime<Utc>,
) -> Option<String> {
    let mark = last_seen(plane, ws, repo, piece);
    let claim = claims(plane, ws)
        .get(&(repo.to_string(), piece.to_string()))
        .cloned();
    let stamp = match parse(mark.as_ref().and_then(|m| m.get("ts"))) {
        Stamp::None => parse(claim.as_ref().and_then(|c| c.get("ts"))),
        other => other,
    };
    match stamp {
        Stamp::At(at) => Some(since(at, now)),
        Stamp::None => Some("?".to_string()),
        Stamp::Naive => None,
    }
}

#[cfg(test)]
mod tests;
