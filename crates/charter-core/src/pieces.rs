//! What a workspace's **pieces** — its linked worktrees — have said about themselves.
//!
//! A port of the read half of `charter/pieces.py`: the append-only event log, the heartbeat
//! store beside it, and the two questions the footer asks of them. Nothing here writes.
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
    out.sort_by(|a, b| ts_key(a).cmp(&ts_key(b)));
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
    let (digits, unit) = age.split_at(age.len().saturating_sub(1));
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
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.into_iter().map(|(_, name)| name).collect()
}

#[cfg(test)]
mod tests;
