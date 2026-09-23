//! What a turn cost, recorded from the one payload that carries it.
//!
//! A port of `charter/statusline.py`'s `_usage_numbers`, `_ctx_percentage`, `record_usage`
//! and `_record_turn`, and of nothing else in that module.
//!
//! # This is the reason `charter statusline` still runs
//!
//! ADR 0019 (`the frame owns the surface`) decided that inside a live frame the status line
//! **prints an empty line**, because the frame's panels already draw everything it would.
//! What the ADR then writes down hardest is the half that looks like dead code:
//!
//! > The suppressed command still reads its payload and still records this turn's token usage
//! > … Claude Code's per-turn JSON is the **only** place those numbers exist: it carries
//! > `context_window.current_usage` to the `statusLine` command and nowhere else, and
//! > `charter/hooks.py` has zero references to any usage field — measured, not assumed.
//!
//! So the tempting cleanup — "this command prints nothing, take it out of
//! `.claude/settings.json`" — does not remove a duplicate. **It deletes the record, silently,
//! and nothing notices until somebody goes looking for a history that stopped being written
//! months earlier.** This module is that record, and
//! `crates/charter-cli/tests/statusline.rs` holds the test that goes red if the drawing path
//! stops writing it.
//!
//! # The file is a session's, and a session id arrives on stdin
//!
//! Python spells the path `SESSIONS_DIR / f"{sid}.usage"` with the id taken straight out of
//! the payload. A payload is whatever is piped in, so an id of `../../x` writes outside the
//! state directory. This port refuses an id that is not one path segment, and
//! [`contain::no_link_on_the_way`] gates the file charter actually opens. See
//! [`self::file_for`] for what that changes and what it deliberately does not.

use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::contain;

/// Where a session's turns are recorded, under the plane's state directory.
/// `charter/config.py`'s `SESSIONS_DIR`.
pub const SESSIONS: &str = ".charter/sessions";

/// How many turns are kept. `charter/statusline.py:_TREND_KEEP`.
///
/// A ring buffer, so a long session's file does not grow without bound and the trend a panel
/// reads is always the recent one.
pub const KEEP: usize = 16;

/// The usage file for `sid`, or `None` when `sid` could not name one file here.
///
/// **A session id is a payload field, and a payload is whatever was piped in.** Python joins
/// it straight onto the sessions directory, so `{"session_id": "../../../x"}` writes a file
/// outside the state directory — digits only, but created and truncated at a path the caller
/// chose. Here the id has to be one segment (`contain::segment_ok`: no separator, no `.`,
/// no `..`, no NUL, not absolute), and every write additionally walks the path for a symlink.
///
/// **This is a deliberate divergence from Python and is declared as one**, rather than
/// reproduced: a write is the one thing a port may not copy bug for bug, and the scenario
/// that compares the two uses an ordinary id, where both write the same file.
///
/// What it does NOT do is validate the id's alphabet. Claude Code's ids are UUIDs, but
/// `charter statusline` is wired for other harnesses too and an id charter refuses is a turn
/// charter does not record — so the rule is the one property the filesystem cares about.
///
/// **`contain::mintable` and not `contain::segment_ok`** (charter-app#96). An id of `nul`
/// makes this path `.charter/sessions/nul.usage`, which on Windows is the null device: the
/// write succeeds, the bytes are gone, and every read answers empty — a turn silently not
/// recorded rather than a refusal. `alpha:evil` is a stream inside `alpha.usage` that no walk
/// over `.charter/` lists.
///
/// This is the one place the rule is asked of a READ as well as of a write, and it is the
/// one place where that costs nothing: `.charter/` is machine-local and never committed, so
/// there is no travelling plane to lock anybody out of — the most that can be lost is the
/// cost trend of a session whose harness named it after a DOS device.
pub fn file_for(plane: &Path, sid: &str) -> Option<PathBuf> {
    contain::mintable(sid)
        .is_ok()
        .then(|| plane.join(SESSIONS).join(format!("{sid}.usage")))
}

/// What one turn's payload says it cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// Claude Code's own session id, which is what the file is keyed by — never the frame's
    /// or the chat's.
    pub session: String,
    pub read: i64,
    pub write: i64,
    /// The share of this turn's input served from cache, as a whole percentage.
    pub hit: i64,
    /// `context_window.used_percentage`, which is the one number nothing can re-derive from
    /// `read` and `write` — and therefore the one a panel could never show if it were not
    /// written down (charter #413).
    pub context: Option<i64>,
}

/// `(session, read, write, hit)` out of a status-line payload, or `None` when this turn
/// carries no live numbers. `statusline._usage_numbers`.
///
/// **`None` and not zeros**: early in a session, and right after `/compact`, the payload has
/// no usage at all, and a zero recorded there would be an invented turn — and a `0/0`
/// divided.
///
/// The one place that knows where those numbers live and what counts as a turn having any, so
/// the drawn number and the recorded number cannot come to disagree about the same turn.
pub fn numbers(payload: &Value) -> Option<Turn> {
    let usage = payload.get("context_window")?.get("current_usage")?;
    let read = whole(usage.get("cache_read_input_tokens"))?;
    let write = whole(usage.get("cache_creation_input_tokens"))?;
    if read == 0 && write == 0 {
        return None;
    }
    Some(Turn {
        session: payload
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        read,
        write,
        hit: half_to_even(100.0 * read as f64 / (read + write) as f64),
        context: context_percentage(payload),
    })
}

/// A token count as a whole number. `None` for a value charter will not put in the record.
///
/// Python's `cu.get(k) or 0` takes anything and then multiplies it, so a string count raises
/// a `TypeError` that `record_usage`'s caller swallows — the turn is not recorded, and that
/// is what `None` is here.
///
/// **Two payloads Python does record and this one does not, both declared.** A float count
/// writes `1.0,0,100,` and a `true` writes `True,0,100,`, because the f-string interpolates
/// the value rather than an integer. Every reader of that file then skips or raises on the
/// row (`_pairs` calls `int()` on it), so what Python "records" there is a row nothing can
/// read. A count that is not a whole number is not a token count, and refusing it keeps a
/// line out of the file that no charter — either implementation — can read back.
fn whole(value: Option<&Value>) -> Option<i64> {
    match value {
        None | Some(Value::Null) => Some(0),
        Some(Value::Bool(false)) => Some(0),
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

/// `round()` as Python rounds a float: half to EVEN, not half away from zero.
///
/// `round(0.5)` is `0` and `round(1.5)` is `2` in Python 3, and Rust's `f64::round` answers
/// `1` and `2`. The value here is a percentage of a ratio, so the half case is reachable —
/// `read == write` gives exactly 50.0, and one input token either side of it lands on `.5`
/// often enough that a status line and a panel would disagree about the same turn.
fn half_to_even(value: f64) -> i64 {
    use std::cmp::Ordering;

    let down = value.floor();
    let fraction = value - down;
    let rounded = match fraction.partial_cmp(&0.5) {
        Some(Ordering::Greater) => down + 1.0,
        Some(Ordering::Less) => down,
        // Exactly half — and a comparison rather than `fraction == 0.5`, so a NaN that could
        // only come from a division this function does not do falls here rather than nowhere.
        _ if (down as i64) % 2 == 0 => down,
        _ => down + 1.0,
    };
    rounded as i64
}

/// `context_window.used_percentage` as a whole number, or `None`. `_ctx_percentage`.
///
/// `isinstance(pct, (int, float))` is Python's test, and `isinstance(True, int)` is true — so
/// a boolean answers 1 or 0 rather than nothing. Reproduced, because a payload charter did
/// not write is exactly where the two implementations would otherwise disagree.
fn context_percentage(payload: &Value) -> Option<i64> {
    let pct = payload.get("context_window")?.get("used_percentage")?;
    match pct {
        Value::Bool(yes) => Some(i64::from(*yes)),
        Value::Number(n) => match n.as_i64() {
            Some(whole) => Some(whole),
            // `int()` truncates toward zero.
            None => n
                .as_f64()
                .filter(|f| f.is_finite())
                .map(|f| f.trunc() as i64),
        },
        _ => None,
    }
}

/// Whether a turn was appended, which is what "a new turn happened" means to a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    /// A row was appended and the file rewritten.
    Appended,
    /// The same API response re-rendered: nothing was written.
    SameTurn,
    /// There was nothing to record, or nowhere to record it.
    Nothing,
}

/// Write this turn's numbers into the session's trend. `statusline.record_usage`.
///
/// Reads nothing that is not in `payload`, so a caller that does not intend to draw pays one
/// file read and one file write and nothing else — no git, no forge, no persona scan.
pub fn record(plane: &Path, payload: &Value) -> Recorded {
    match numbers(payload) {
        Some(turn) => record_turn(plane, &turn),
        None => Recorded::Nothing,
    }
}

/// Append one turn to the session's trend. `statusline._record_turn`.
///
/// **The status line can render several times per turn**, so a sample is appended only when
/// the underlying API numbers CHANGE: the payload reflects the most recent API response, so
/// an identical `(read, write)` pair is the same turn re-rendered, not a new one.
///
/// **The de-duplication compares `read`/`write` only.** It used to compare the whole
/// assembled row — equivalent while every other field was derived from those two, and no
/// longer, since the context percentage is not. A row differing only in the percentage is
/// still the same API response re-rendered, and appending it would spend a slot of the ring
/// buffer on a duplicate turn and shift the rebuild history by one.
pub fn record_turn(plane: &Path, turn: &Turn) -> Recorded {
    if turn.session.is_empty() {
        return Recorded::Nothing;
    }
    let Some(path) = file_for(plane, &turn.session) else {
        return Recorded::Nothing;
    };
    let mut rows = rows_at(plane, &path);
    let head = format!("{},{}", turn.read, turn.write);
    if rows
        .last()
        .is_some_and(|last| last.splitn(3, ',').take(2).collect::<Vec<_>>().join(",") == head)
    {
        return Recorded::SameTurn;
    }
    rows.push(format!(
        "{head},{},{}",
        turn.hit,
        // An absent percentage writes an EMPTY field rather than a zero: early in a session,
        // and right after `/compact`, there is no percentage, and `ctx 0%` is a claim rather
        // than a gap.
        turn.context.map(|c| c.to_string()).unwrap_or_default()
    ));
    if rows.len() > KEEP {
        rows.drain(..rows.len() - KEEP);
    }
    let body = format!("{}\n", rows.join("\n"));
    match write_row(plane, &path, body.as_bytes()) {
        Ok(()) => Recorded::Appended,
        // Best-effort, as Python's `except OSError: pass` is: a footer is not worth a crash.
        Err(_) => Recorded::Nothing,
    }
}

/// The session's recorded rows, or none for every way there are none. `_usage_rows`.
///
/// A blank line is dropped, as Python's `if ln.strip()` drops it, so a file that was
/// truncated mid-write does not shift the ring buffer by an empty slot.
///
/// # The read is gated, and it was the last follow in this module that was not
///
/// This was `read_to_string(path)`. [`write_row`] below refuses to write through a link, so a
/// planted `<plane>/.charter/sessions/<sid>.usage -> /elsewhere` produced no write — but the
/// READ had already happened, and `charter statusline` runs on every prompt. Nothing was
/// printed, so nothing leaked; "nothing leaks today" is not the property this repo claims,
/// and the module below it already pays for the two hazards an ungated read carries (ADR
/// 0028, `crate::reopen`, `crate::planegit::push_record`):
///
/// * **a FIFO is not a link**, so a link check waves it through and `read_to_string` on one
///   never returns — on a path a status-line hook opens at every prompt;
/// * **a planted giant** is read whole into memory, and git packs a sparse multi-gigabyte
///   file small.
///
/// So the descriptor is opened `O_NOFOLLOW | O_NONBLOCK` and both questions are asked of
/// THAT, not of the name — an `lstat` and a later open are two different objects with a
/// window between them.
///
/// **The gate is on `path` itself, not on its parent.** `path` is what
/// [`self::file_for`] built and what [`write_row`] opens: the same bytes, both sides. Gating
/// `.charter/sessions/` would leave the `.usage` file — the one component an attacker can
/// actually plant, because it is the one named by a payload field — ungated, which is the
/// mistake this repo has already had six review rounds on.
///
/// Every failure is `Vec::new()`, as Python's `except OSError: pass` is: a refused read means
/// the trend starts empty, the turn is still recorded if the write is allowed, and a footer is
/// never worth taking a session down for.
fn rows_at(plane: &Path, path: &Path) -> Vec<String> {
    let Ok(mut open) = contain::open_no_link(plane, path) else {
        return Vec::new();
    };
    let Ok(found) = open.metadata() else {
        return Vec::new();
    };
    if crate::reopen::refuse_unusable(path, &found).is_err() {
        return Vec::new();
    }
    let mut text = String::new();
    {
        use std::io::Read;

        // Bounded again on the way in: `refuse_unusable` asked how big it was, and a writer
        // that appends between the `fstat` and the read would otherwise still be unbounded.
        if open
            .by_ref()
            .take(crate::reopen::MAX_BYTES)
            .read_to_string(&mut text)
            .is_err()
        {
            return Vec::new();
        }
    }
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// Write the trend file, 0600, with the same ordering `glrefresh` writes the cache with.
fn write_row(plane: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;

    let dir = path.parent().unwrap_or(plane);
    contain::no_link_on_the_way(plane, dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)?;

    contain::no_link_on_the_way(plane, path)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = contain::nofollow(&mut options).open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    // After the mode is settled, never before: otherwise the file is empty and readable by an
    // account the finished file is not.
    file.set_len(0)?;
    file.write_all(bytes)
}

// ---- reading it back ------------------------------------------------------------------------
//
// Everything above WRITES the record. What follows is the first reader outside the writer:
// a port of `charter/statusline.py:recorded_context_gauge` (#413) and the helpers it shares
// with the live gauge — `_last_ctx_of`, `_hits`, `_pairs`, `_rebuilds`, `_fmt_tok` and the
// thresholds in `_ctx_part` / `_cache_part`. Python wrote it for the tmux frame's panel,
// which never sees a payload and so can only draw what was recorded. The app is that reader
// too: a chat tab has no payload either.
//
// **Data, not a drawn line.** Python returns ANSI strings; this returns the numbers and the
// verdict each is drawn in, and the window draws them. The thresholds live HERE for #413's
// own reason: two surfaces drawing one number with two thresholds is a green 60% beside a
// yellow 60%, and nobody can debug that from what is on screen.

/// How a gauge's number reads: `ok`, `warn` or `bad` — `statusline.accent`'s three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Bad,
}

/// A turn whose input was under this share cache-read counts as cold. `_COLD_BELOW`.
pub const COLD_BELOW: i64 = 50;
/// How many cold turns in a row before a cold streak is worth saying. `_COLD_STREAK`.
pub const COLD_STREAK: usize = 3;
/// A cache write this big, with the read collapsed, is a prefix rebuild. `_REBUILD_MIN_WRITE`.
pub const REBUILD_MIN_WRITE: i64 = 15_000;
/// Rebuilds costing this much in total are drawn loud. `_REBUILD_LOUD`.
pub const REBUILD_LOUD: i64 = 200_000;

/// One recorded turn, as far as its row could be read. `None` for a field that is absent or
/// is not a whole number — each reader below skips what it cannot use, as Python's do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub read: Option<i64>,
    pub write: Option<i64>,
    pub hit: Option<i64>,
    pub context: Option<i64>,
    /// How many comma-separated fields the row had. Python's readers branch on it
    /// (`len(p) >= 3`, `len(p) < 4`), so it is kept rather than inferred.
    fields: usize,
}

impl Sample {
    fn of(row: &str) -> Self {
        let p: Vec<&str> = row.split(',').collect();
        let int = |i: usize| {
            p.get(i)
                .and_then(|f| crate::glrefresh::python_int(f))
                .and_then(|n| i64::try_from(n).ok())
        };
        Self {
            read: int(0),
            write: int(1),
            hit: int(2),
            context: int(3),
            fields: p.len(),
        }
    }
}

/// Every turn recorded for Claude Code's session `sid`, oldest first — at most [`KEEP`].
///
/// Empty for every way there is nothing: an id that is not one path segment, no file, a file
/// that is a link or a FIFO or too big ([`rows_at`] refuses each and says nothing). A reader
/// that cannot tell "no turns yet" from "could not read" draws nothing for both, which is the
/// rule the gauge is built on.
pub fn history(plane: &Path, sid: &str) -> Vec<Sample> {
    let Some(path) = file_for(plane, sid) else {
        return Vec::new();
    };
    rows_at(plane, &path)
        .iter()
        .map(|r| Sample::of(r))
        .collect()
}

/// What the gauge draws. Every field is `None` when charter does not know it — never zero.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Gauge {
    /// `ctx NN%`: the most recent recorded `context_window.used_percentage`, and its tone.
    pub context: Option<(i64, Tone)>,
    /// `cache NN%`: the last turn's share of input served from cache, and its tone.
    pub cache: Option<(i64, Tone)>,
    /// `↻N 696k`: how many prefix rebuilds this session has paid for, their total cost in
    /// tokens as `_fmt_tok` spells it, and its tone. `None` when there were none.
    pub rebuilds: Option<(usize, String, Tone)>,
}

/// The gauge for a recorded history. `recorded_context_gauge`, without the ANSI.
///
/// **An unreadable read/write pair empties the whole gauge**, which is Python's behaviour
/// and is kept: `_pairs` raises on it, and `recorded_context_gauge`'s `except Exception:
/// return []` takes `ctx` and `cache` down with it. A file with a corrupt row in it is a file
/// whose other numbers charter cannot vouch for either, and "draw nothing" is this gauge's
/// answer to "not actually known".
pub fn gauge(history: &[Sample]) -> Gauge {
    let Some(pairs) = pairs(history) else {
        return Gauge::default();
    };
    let (n, cost) = rebuilds(&pairs);
    Gauge {
        context: last_context(history).map(|pct| (pct, context_tone(pct))),
        cache: hits(history).last().map(|&hit| (hit, cache_tone(hit))),
        rebuilds: (n > 0).then(|| {
            (
                n,
                tokens(cost),
                if cost >= REBUILD_LOUD {
                    Tone::Bad
                } else {
                    Tone::Warn
                },
            )
        }),
    }
}

/// `_last_ctx_of`: the LAST row that carries a percentage, not the last row — a turn early in
/// a session has usage and no percentage, and blanking a gauge that was right a moment ago
/// is worse than showing it. Rows with fewer than four fields (pre-#413) are skipped.
fn last_context(history: &[Sample]) -> Option<i64> {
    history
        .iter()
        .rev()
        // No `fields >= 4` filter: a shorter row has no fourth field, so its `context` is
        // already `None` — Python's `len(p) < 4` skip, by construction.
        .find_map(|s| s.context)
}

/// `_hits`: the cache-hit shares, positionally (`rows[2]`, never "the last field", which
/// since #413 is the context percentage). A row too short to hold one is skipped.
pub fn hits(history: &[Sample]) -> Vec<i64> {
    history
        .iter()
        // `len(p) >= 3` needs no filter: a shorter row has no third field to be a hit.
        .filter_map(|s| s.hit)
        .collect()
}

/// `_pairs`: every `(read, write)`, or `None` when a row long enough to carry one does not
/// — Python's `int()` raising.
fn pairs(history: &[Sample]) -> Option<Vec<(i64, i64)>> {
    history
        .iter()
        .filter(|s| s.fields >= 3)
        .map(|s| Some((s.read?, s.write?)))
        .collect()
}

/// `_rebuilds`: `(count, total tokens)` of prefix rebuilds — a big write with the read
/// collapsed to under half the previous turn's, or any big write on the first turn.
fn rebuilds(pairs: &[(i64, i64)]) -> (usize, i64) {
    let mut n = 0;
    let mut cost = 0i64;
    for (i, &(read, write)) in pairs.iter().enumerate() {
        if write < REBUILD_MIN_WRITE {
            continue;
        }
        let prev = if i == 0 { 0 } else { pairs[i - 1].0 };
        // `read < prev * 0.5` in Python's float arithmetic; `2 * read < prev` is the same
        // comparison on integers, with no rounding to disagree about.
        if i == 0 || read.saturating_mul(2) < prev {
            n += 1;
            cost = cost.saturating_add(write);
        }
    }
    (n, cost)
}

/// One turn of the trend, as a reader draws it: its cache-hit share, the context percentage
/// it recorded, and what it wrote to the cache. `None` for a field the row could not give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrendTurn {
    pub hit: Option<i64>,
    pub context: Option<i64>,
    pub written: Option<i64>,
}

/// The session's recorded turns, oldest first — the TREND, where [`gauge`] is the last turn.
///
/// ADR 0038 names it separately from the gauge — *"the trend over a session's turns
/// rather than this turn's percentage"* — and Python never drew it as a picture; it used it
/// for the cold-streak hint and the rebuild count. This is the same rows, handed over whole.
/// A row too short to be a turn (`len(p) < 3`, which every Python reader skips) is not one.
///
/// **Empty wherever [`gauge`] is empty**, for the same reason: an unreadable token count is
/// a file charter cannot vouch for, and a trend drawn from it would be the one picture on
/// screen that contradicts the blank gauge beside it.
pub fn trend(history: &[Sample]) -> Vec<TrendTurn> {
    if pairs(history).is_none() {
        return Vec::new();
    }
    history
        .iter()
        .filter(|s| s.fields >= 3)
        .map(|s| TrendTurn {
            hit: s.hit,
            // A row shorter than four fields has no fourth to read, so this is `None` for it by
            // construction — `_last_ctx_of`'s `len(p) < 4` needs no second spelling here.
            context: s.context,
            written: s.write,
        })
        .collect()
}

/// The cold streak worth saying — `_cache_hint`'s gate: [`COLD_STREAK`] or more cold turns in a
/// row at the end, else `None`. One cold turn is normal (a model switch, a `/compact`, warming
/// up); a sustained one is the prefix churning, which is the expensive failure.
pub fn cold(history: &[Sample]) -> Option<usize> {
    // A file the gauge will not vouch for says no streak either.
    pairs(history)?;
    let streak = cold_streak(&hits(history));
    (streak >= COLD_STREAK).then_some(streak)
}

/// How many turns in a row, most recent last, were cache-cold. `_cold_streak`.
pub fn cold_streak(hits: &[i64]) -> usize {
    hits.iter()
        .rev()
        .take_while(|&&hit| hit < COLD_BELOW)
        .count()
}

/// `_ctx_part`'s thresholds: under half is fine, under four fifths is a warning.
pub fn context_tone(pct: i64) -> Tone {
    if pct < 50 {
        Tone::Ok
    } else if pct < 80 {
        Tone::Warn
    } else {
        Tone::Bad
    }
}

/// `_cache_part`'s thresholds, the other way round: a high hit share is the healthy one.
pub fn cache_tone(hit: i64) -> Tone {
    if hit >= 80 {
        Tone::Ok
    } else if hit >= 50 {
        Tone::Warn
    } else {
        Tone::Bad
    }
}

/// `_fmt_tok`: `1.2M`, `696k`, `999`.
pub fn tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload(read: i64, write: i64) -> Value {
        json!({
            "session_id": "s1",
            "context_window": {
                "current_usage": {
                    "cache_read_input_tokens": read,
                    "cache_creation_input_tokens": write,
                },
                "used_percentage": 42,
            }
        })
    }

    #[test]
    fn a_turn_with_no_usage_at_all_is_not_a_turn() {
        assert_eq!(numbers(&json!({})), None);
        assert_eq!(numbers(&payload(0, 0)), None, "a zero turn is not invented");
        assert_eq!(
            numbers(&json!({"session_id": "s1", "context_window": {"current_usage": {}}})),
            None
        );
    }

    #[test]
    fn a_fresh_session_records_nothing_rather_than_a_turn_of_nulls() {
        // **Measured on Claude Code 2.1.280**: the first render of a session hands the
        // `statusLine` command a payload whose `current_usage`, `used_percentage` and
        // `remaining_percentage` are all JSON `null` — not absent. That is every chat's normal
        // first render, so it must record nothing: a row of zeros there would be an invented
        // turn, and the gauge would read `cache 0%` on a session that has not spent anything.
        //
        // An explicit `null` and an absent key are different values, and only the absent one
        // was covered before this. `whole()` answers `Some(0)` for both, and the
        // `read == 0 && write == 0` guard is what turns them into "no turn".
        let fresh = json!({
            "session_id": "s1",
            "context_window": {
                "current_usage": {
                    "cache_read_input_tokens": null,
                    "cache_creation_input_tokens": null,
                },
                "used_percentage": null,
                "remaining_percentage": null,
            }
        });

        assert_eq!(numbers(&fresh), None);

        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(record(&plane, &fresh), Recorded::Nothing);
        assert!(
            !file_for(&plane, "s1").unwrap().exists(),
            "a fresh session wrote a record"
        );
        // And a turn that HAS spent something still records, percentage or no percentage.
        assert_eq!(record(&plane, &payload(90, 10)), Recorded::Appended);
    }

    #[test]
    fn the_hit_share_rounds_half_to_even_as_python_does() {
        // Python: round(0.5) == 0, round(1.5) == 2, round(2.5) == 2.
        assert_eq!(half_to_even(0.5), 0);
        assert_eq!(half_to_even(1.5), 2);
        assert_eq!(half_to_even(2.5), 2);
        assert_eq!(half_to_even(49.5), 50);
        assert_eq!(half_to_even(50.5), 50, "f64::round would answer 51");
        assert_eq!(half_to_even(100.0), 100);
    }

    #[test]
    fn a_row_carries_the_four_fields_in_pythons_order() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();

        assert_eq!(record(&plane, &payload(90, 10)), Recorded::Appended);

        let file = file_for(&plane, "s1").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "90,10,90,42\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the record is {mode:o}");
        }
    }

    #[test]
    fn a_missing_context_percentage_writes_an_empty_field_and_never_a_zero() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        let mut doc = payload(90, 10);
        doc["context_window"]
            .as_object_mut()
            .unwrap()
            .remove("used_percentage");

        record(&plane, &doc);

        // `ctx 0%` is a claim; an empty field is a gap, which is what this is.
        assert_eq!(
            std::fs::read_to_string(file_for(&plane, "s1").unwrap()).unwrap(),
            "90,10,90,\n"
        );
    }

    #[test]
    fn the_same_api_response_rendered_again_is_not_a_second_turn() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();

        record(&plane, &payload(90, 10));
        // Claude Code re-renders the footer several times per turn, and only the percentage
        // moves. That is the same turn.
        let mut again = payload(90, 10);
        again["context_window"]["used_percentage"] = json!(43);
        assert_eq!(record(&plane, &again), Recorded::SameTurn);
        record(&plane, &payload(95, 10));

        assert_eq!(
            std::fs::read_to_string(file_for(&plane, "s1").unwrap()).unwrap(),
            "90,10,90,42\n95,10,90,42\n"
        );
    }

    #[test]
    fn the_trend_keeps_the_last_sixteen_turns_and_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();

        for turn in 1..=20 {
            record(&plane, &payload(turn * 10, 10));
        }

        let rows: Vec<String> = std::fs::read_to_string(file_for(&plane, "s1").unwrap())
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(rows.len(), KEEP);
        assert_eq!(rows[0], "50,10,83,42");
        assert_eq!(rows[KEEP - 1], "200,10,95,42");
    }

    #[test]
    fn a_session_id_that_is_not_one_name_records_nothing_anywhere() {
        let dir = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(dir.path()).unwrap();
        let plane = here.join("plane");
        std::fs::create_dir_all(&plane).unwrap();

        for bad in ["../../escape", "a/b", "", ".", "..", "x\0y"] {
            assert_eq!(file_for(&plane, bad), None, "{bad:?}");
            let mut doc = payload(90, 10);
            doc["session_id"] = json!(bad);
            assert_eq!(record(&plane, &doc), Recorded::Nothing, "{bad:?}");
        }
        // Python joins the id straight on, so this is the file it would have written.
        assert!(!here.join("escape.usage").exists());
    }

    #[test]
    fn a_session_id_the_next_machine_reads_as_a_device_records_nothing_anywhere() {
        // charter-app#96, measured on macOS: `contain::segment_ok` said `true` to every one
        // of these, and `segment_ok` is the whole gate this path used to have. `nul` makes
        // the file `.charter/sessions/nul.usage`, which on Windows is the null device — the
        // write succeeds, the bytes are gone, and the trend a panel reads is empty for ever.
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();

        for bad in [
            "nul",
            "NUL",
            "con",
            "aux",
            "lpt9",
            "com1.txt",
            "alpha.",
            "alpha ",
            "alpha:evil",
            "PROGRA~1",
        ] {
            assert_eq!(file_for(&plane, bad), None, "{bad:?}");
            let mut doc = payload(90, 10);
            doc["session_id"] = json!(bad);
            assert_eq!(record(&plane, &doc), Recorded::Nothing, "{bad:?}");
        }
        // The ids a harness actually mints are untouched: a UUID, and Claude Code's own.
        assert!(file_for(&plane, "8f14e45f-ceea-467a-9d3f-1b2c3d4e5f60").is_some());
        assert!(file_for(&plane, "fixture-session-1").is_some());
    }

    #[test]
    fn the_record_is_not_written_through_a_link_out_of_the_plane() {
        let dir = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(dir.path()).unwrap();
        let plane = here.join("plane");
        let outside = here.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(plane.join(".charter")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, plane.join(".charter/sessions")).unwrap();

        assert_eq!(record(&plane, &payload(90, 10)), Recorded::Nothing);
        assert!(!outside.join("s1.usage").exists());
    }

    #[test]
    fn a_record_that_is_itself_a_link_is_not_written_through() {
        // The half a check on the parent cannot see: `.charter/sessions/` is an ordinary
        // directory and only the file is a link. Held by the walk and by the `O_NOFOLLOW` on
        // the open together — `contain::open_no_link`'s own pairing — so this pins the
        // property rather than either guard alone.
        let dir = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(dir.path()).unwrap();
        let plane = here.join("plane");
        let outside = here.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("theirs"), "NOT CHARTER'S\n").unwrap();
        std::fs::create_dir_all(plane.join(SESSIONS)).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            outside.join("theirs"),
            file_for(&plane, "s1").expect("an ordinary id"),
        )
        .unwrap();

        assert_eq!(record(&plane, &payload(90, 10)), Recorded::Nothing);
        assert_eq!(
            std::fs::read_to_string(outside.join("theirs")).unwrap(),
            "NOT CHARTER'S\n",
            "the record was written through the link"
        );
    }

    /// A plane with an ordinary `.charter/sessions/`, and somewhere outside it to point at.
    fn a_plane_and_an_outside() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let held = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(held.path()).unwrap();
        let plane = here.join("plane");
        let outside = here.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(plane.join(SESSIONS)).unwrap();
        (held, plane, outside)
    }

    /// The READ, not the write — the follow this module did not gate (M2.20).
    ///
    /// **Two things make this test bite, and the module already had a test that had neither.**
    /// `the_record_is_not_written_through_a_link_out_of_the_plane` plants its link one level
    /// up, at `.charter/sessions/`, and asserts `record(…) == Nothing`. Both halves of that
    /// pass against the UNFIXED read: `read_to_string` followed a directory link just as
    /// happily as a file one, and `record` answered `Nothing` either way because the WRITE
    /// refused. The escape was invisible from there.
    ///
    /// So this one plants the link at the exact path [`self::file_for`] returns — the leaf,
    /// which is the component named by a payload field and therefore the one an attacker
    /// picks — and asserts on what `rows_at` RETURNED, which is the value that had the outside
    /// file in it.
    #[test]
    #[cfg(unix)]
    fn the_trend_is_not_read_through_a_link_at_the_records_own_name() {
        let (_held, plane, outside) = a_plane_and_an_outside();
        let theirs = outside.join("theirs");
        std::fs::write(&theirs, "1,2,3,4\n5,6,7,8\n").unwrap();
        let path = file_for(&plane, "s1").expect("an ordinary id");
        std::os::unix::fs::symlink(&theirs, &path).unwrap();

        assert_eq!(
            rows_at(&plane, &path),
            Vec::<String>::new(),
            "the rows outside the plane were read through the link"
        );
    }

    /// A link that lands back INSIDE the plane is refused too, and that is deliberate.
    ///
    /// `contain::readable` follows a link that stays inside, because a plane that links a
    /// persona directory depends on it. This path is not that: `.charter/sessions/` is
    /// charter's own, created by charter, and nothing legitimate makes any part of it a link
    /// — which is the predicate `open_no_link` already declares for every other record in the
    /// state directory.
    #[test]
    #[cfg(unix)]
    fn a_link_that_stays_inside_the_plane_is_refused_here_as_well() {
        let (_held, plane, _outside) = a_plane_and_an_outside();
        let elsewhere = plane.join("elsewhere.usage");
        std::fs::write(&elsewhere, "1,2,3,4\n").unwrap();
        let path = file_for(&plane, "s1").expect("an ordinary id");
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();

        assert_eq!(rows_at(&plane, &path), Vec::<String>::new());
    }

    /// A FIFO is not a link, so the walk waves it through — and reading one never returns.
    #[test]
    #[cfg(unix)]
    fn a_trend_that_is_not_a_plain_file_is_refused_instead_of_read_for_ever() {
        let (_held, plane, _outside) = a_plane_and_an_outside();
        let path = file_for(&plane, "s1").expect("an ordinary id");
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&path))
            .expect("mkfifo runs");
        assert!(made.success(), "the test needs a fifo to plant");

        // In a thread, because the whole point is that the unguarded version never returns.
        let (say, heard) = std::sync::mpsc::channel();
        let asked = plane.clone();
        let at = path.clone();
        std::thread::spawn(move || say.send(rows_at(&asked, &at)));
        let rows = heard
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("reading a fifo trend must not block");

        assert_eq!(rows, Vec::<String>::new());
    }

    /// A sparse giant packs small in git and arrives full size, and this file is read whole.
    #[test]
    fn a_trend_too_large_to_be_one_is_refused_rather_than_read_whole() {
        let (_held, plane, _outside) = a_plane_and_an_outside();
        let path = file_for(&plane, "s1").expect("an ordinary id");
        let planted = std::fs::File::create(&path).unwrap();
        planted.set_len(crate::reopen::MAX_BYTES + 1).unwrap();
        drop(planted);

        assert_eq!(rows_at(&plane, &path), Vec::<String>::new());
    }

    /// And the gate changes nothing for the file charter itself wrote, which is the half a
    /// containment test that only plants attacks never proves.
    #[test]
    fn an_ordinary_trend_is_still_read_row_for_row() {
        let (_held, plane, _outside) = a_plane_and_an_outside();
        let path = file_for(&plane, "s1").expect("an ordinary id");
        std::fs::write(&path, "1,2,3,4\n\n5,6,7,8\n").unwrap();

        assert_eq!(
            rows_at(&plane, &path),
            vec!["1,2,3,4".to_string(), "5,6,7,8".to_string()],
            "a blank line is dropped, as Python's `if ln.strip()` drops it"
        );
        // A trend that is not there yet is no rows, not a failure.
        assert_eq!(
            rows_at(&plane, &file_for(&plane, "s2").unwrap()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_context_percentage_is_read_the_way_python_reads_it() {
        let with = |value: Value| {
            let mut doc = payload(1, 1);
            doc["context_window"]["used_percentage"] = value;
            numbers(&doc).unwrap().context
        };
        assert_eq!(with(json!(42)), Some(42));
        assert_eq!(with(json!(42.9)), Some(42), "int() truncates");
        // `isinstance(True, int)` is true in Python, so a bool is a number here.
        assert_eq!(with(json!(true)), Some(1));
        assert_eq!(with(json!("42")), None);
        assert_eq!(with(json!(null)), None);
    }

    // ---- reading it back ----------------------------------------------------------------
    //
    // Each expected value below is what the Python charter's own helpers answer for the same
    // rows (`_last_ctx_of`, `_hits`, `_pairs`, `_rebuilds`, `_fmt_tok`, `_cold_streak`),
    // run against the frozen oracle on 2026-09-22. `recorded_context_gauge` is internal to
    // the frame's panel and had no CLI surface for the differential to drive, so
    // the oracle was asked directly and its answers are pinned here.

    fn rows(lines: &[&str]) -> Vec<Sample> {
        lines.iter().map(|l| Sample::of(l)).collect()
    }

    /// `(ctx, cache, rebuild count, rebuild cost as drawn, cold streak)`, as the oracle
    /// printed them.
    fn drawn(lines: &[&str]) -> (Option<i64>, Option<i64>, usize, Option<String>, usize) {
        let h = rows(lines);
        let g = gauge(&h);
        (
            g.context.map(|(n, _)| n),
            g.cache.map(|(n, _)| n),
            g.rebuilds.as_ref().map_or(0, |r| r.0),
            g.rebuilds.map(|r| r.1),
            cold_streak(&hits(&h)),
        )
    }

    #[test]
    fn the_gauge_answers_what_the_python_charter_answers_for_the_same_rows() {
        let none = None::<String>;
        assert_eq!(
            drawn(&["900,100,90,12", "950,20,98,14", "990,5,99,"]),
            (Some(14), Some(99), 0, none.clone(), 0),
            "the last row with a percentage, not the last row"
        );
        assert_eq!(
            drawn(&["900,100,90", "950,20,98"]),
            (None, Some(98), 0, none.clone(), 0),
            "pre-#413 rows carry no percentage"
        );
        assert_eq!(
            drawn(&[
                "800000,2000,100,40",
                "10000,696088,1,41",
                "700000,3000,100,42"
            ]),
            (Some(42), Some(100), 1, Some("696k".into()), 0)
        );
        assert_eq!(
            drawn(&["0,20000,0,3"]),
            (Some(3), Some(0), 1, Some("20k".into()), 1),
            "a big write on the first turn is a rebuild"
        );
        assert_eq!(
            drawn(&["900,100,90,12", "950,20,98,abc"]),
            (Some(12), Some(98), 0, none.clone(), 0),
            "a corrupt percentage is skipped past"
        );
        assert_eq!(
            drawn(&["900,100", "950,20,98,7"]),
            (Some(7), Some(98), 0, none.clone(), 0),
            "a row too short to carry a hit is skipped"
        );
        assert_eq!(
            drawn(&[" 900 , 100 ,90, 55 "]),
            (Some(55), Some(90), 0, none.clone(), 0),
            "int() takes surrounding whitespace"
        );
        assert_eq!(
            drawn(&[
                "10,90,10,1",
                "900,100,90,2",
                "10,90,10,3",
                "10,90,10,4",
                "20,80,20,5"
            ]),
            (Some(5), Some(20), 0, none, 3)
        );
        assert_eq!(
            drawn(&["0,1250000,0,9"]),
            (Some(9), Some(0), 1, Some("1.2M".into()), 1)
        );
        assert_eq!(
            drawn(&["0,150000,0,1", "100000,1000,99,2", "1000,60000,2,3"]),
            (Some(3), Some(2), 2, Some("210k".into()), 1)
        );
    }

    #[test]
    fn the_trend_is_every_readable_turn_and_the_cold_streak_is_said_only_from_three() {
        let h = rows(&[
            "900,100",
            "10,90,10,1",
            "900,100,90",
            "10,90,10,3",
            "10,90,10,4",
            "20,80,20,",
        ]);

        let t = trend(&h);

        // The two-field row is not a turn; a three-field row has no percentage.
        assert_eq!(t.len(), 5);
        assert_eq!(
            t[1],
            TrendTurn {
                hit: Some(90),
                context: None,
                written: Some(100)
            }
        );
        assert_eq!(
            t[4],
            TrendTurn {
                hit: Some(20),
                context: None,
                written: Some(80)
            }
        );
        // The oracle's `_cold_streak` for these hits is 3, which is `_cache_hint`'s threshold.
        assert_eq!(cold(&h), Some(3));
        assert_eq!(
            cold(&rows(&["10,90,10,1", "10,90,10,2"])),
            None,
            "two is not a streak"
        );
    }

    #[test]
    fn a_file_the_gauge_will_not_vouch_for_has_no_trend_either() {
        let h = rows(&["900,100,90,12", "x,20,98,14"]);

        assert!(trend(&h).is_empty());
        assert_eq!(
            cold(&rows(&[
                "10,90,10,1",
                "10,90,10,2",
                "10,90,10,3",
                "y,1,1,1"
            ])),
            None
        );
    }

    #[test]
    fn an_unreadable_token_count_empties_the_whole_gauge_as_python_does() {
        // `_pairs` raises, and `recorded_context_gauge` answers `[]` for everything —
        // the percentage and the cache share included.
        assert_eq!(
            gauge(&rows(&["900,100,90,12", "x,20,98,14"])),
            Gauge::default()
        );
    }

    #[test]
    fn each_number_is_drawn_in_the_tone_its_threshold_gives_it() {
        assert_eq!(context_tone(49), Tone::Ok);
        assert_eq!(context_tone(50), Tone::Warn);
        assert_eq!(context_tone(79), Tone::Warn);
        assert_eq!(context_tone(80), Tone::Bad);
        assert_eq!(cache_tone(80), Tone::Ok);
        assert_eq!(cache_tone(79), Tone::Warn);
        assert_eq!(cache_tone(50), Tone::Warn);
        assert_eq!(cache_tone(49), Tone::Bad);
        // Loud at the Python charter's 200k, and not a token before.
        let quiet = gauge(&rows(&["0,199999,0,1"]));
        assert_eq!(quiet.rebuilds.unwrap().2, Tone::Warn);
        let loud = gauge(&rows(&["0,200000,0,1"]));
        assert_eq!(loud.rebuilds.unwrap().2, Tone::Bad);
    }

    #[test]
    fn the_history_is_read_back_from_the_file_the_statusline_writes() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        record(&plane, &payload(90, 10));
        record(&plane, &payload(95, 5));

        let h = history(&plane, "s1");

        assert_eq!(h.len(), 2);
        assert_eq!(gauge(&h).context, Some((42, Tone::Ok)));
        assert_eq!(gauge(&h).cache, Some((95, Tone::Ok)));
        // An id that is not one path segment names no file at all.
        assert!(history(&plane, "../s1").is_empty());
        assert!(history(&plane, "nobody").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_history_file_that_is_a_link_is_not_followed() {
        // The reader goes through the same gated open the writer's read does: a planted
        // `<sid>.usage -> elsewhere` reads as no history rather than as someone else's file.
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        let elsewhere = plane.join("elsewhere");
        std::fs::write(&elsewhere, "900,100,90,77\n").unwrap();
        std::fs::create_dir_all(plane.join(SESSIONS)).unwrap();
        std::os::unix::fs::symlink(&elsewhere, file_for(&plane, "s1").unwrap()).unwrap();

        assert!(history(&plane, "s1").is_empty());
    }
}
