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
pub fn file_for(plane: &Path, sid: &str) -> Option<PathBuf> {
    contain::segment_ok(sid).then(|| plane.join(SESSIONS).join(format!("{sid}.usage")))
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
    /// The write side has refused a linked record since M1, so `record` answered `Nothing`
    /// either way and the escape was invisible from there. `rows_at` is where it was:
    /// `read_to_string` on a name follows the link, and a status-line hook runs it at every
    /// prompt. The link is planted at the EXACT path `file_for` returns — one level up would
    /// be `.charter/sessions/` itself, which the old code also refused, so a test that
    /// planted it there would have passed against the unfixed read.
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
        let made = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
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
}
