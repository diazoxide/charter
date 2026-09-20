//! The dispatch log: who work was actually routed to. A port of the part of
//! `charter/dispatch.py` the README's persona roster reads — [`tally`] and
//! [`generic_share`].
//!
//! `personas/_dispatch/<month>.<host>.jsonl` is append-only, one JSON object a line, one
//! file per month per machine. It is **committed**, which is what makes the roster block a
//! fact every engineer sees the same way rather than a reading of one laptop.
//!
//! Nothing is written here. The roster reads; `charter persona dispatch` (not in this
//! charter) writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The directory under `personas/` the log lives in — `dispatch.DIR_NAME`.
pub const DIR_NAME: &str = "_dispatch";

/// The agent names that are NOT a persona. `dispatch.GENERIC`, in its order.
///
/// A dispatch to one of these is work a persona might have owned, done without it — which
/// is the ratio the roster's headline is about.
pub const GENERIC: [&str; 4] = ["general-purpose", "Explore", "claude", "Plan"];

pub fn dir(root: &Path) -> PathBuf {
    root.join("personas").join(DIR_NAME)
}

/// Every row of every month file, in filename order — `dispatch._read_all`.
///
/// A file that cannot be read is skipped (Python catches `OSError` per file), and so is a
/// line that is not a JSON object carrying an `agent` or an `event`. Bytes that are not
/// UTF-8 are replaced rather than refused, as Python's `read_text(errors="replace")` does:
/// this log is appended to by a hook, and a half-written line must not cost the whole file.
pub fn rows(root: &Path) -> Vec<serde_json::Value> {
    let d = dir(root);
    let Ok(reader) = std::fs::read_dir(&d) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = reader
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for file in files {
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if !value.is_object() {
                continue;
            }
            if truthy(value.get("agent")) || truthy(value.get("event")) {
                out.push(value);
            }
        }
    }
    out
}

/// Python's truth of a JSON value, for the `o.get("agent") or o.get("event")` guard: a
/// missing key, `null`, `false`, `0` and `""` are all false.
fn truthy(value: Option<&serde_json::Value>) -> bool {
    match value {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        Some(serde_json::Value::Array(a)) => !a.is_empty(),
        Some(serde_json::Value::Object(o)) => !o.is_empty(),
        Some(serde_json::Value::Number(n)) => n.as_f64() != Some(0.0),
    }
}

/// agent → dispatch count — `dispatch.tally()` with no day window.
///
/// **A resume row carries an agent too, and must not land here.** The column is read as
/// "times dispatched as a sub-agent", and it is what personas get retired on — so a row
/// with an `event` is excluded, whatever else it holds.
///
/// The agent is taken as the string it is. A row whose `agent` is a number is truthy to
/// Python and lands in its `Counter` under that value; here it is skipped, because a name
/// that is not a name matches no persona and could only ever be counted against nothing.
pub fn tally(root: &Path) -> BTreeMap<String, u64> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for row in rows(root) {
        if truthy(row.get("event")) {
            continue;
        }
        if let Some(agent) = row.get("agent").and_then(serde_json::Value::as_str)
            && !agent.is_empty()
        {
            *counts.entry(agent.to_string()).or_default() += 1;
        }
    }
    counts
}

/// `(dispatches to generic agents, total dispatches)` — the roster's headline ratio.
pub fn generic_share(counts: &BTreeMap<String, u64>) -> (u64, u64) {
    let generic = counts
        .iter()
        .filter(|(name, _)| GENERIC.contains(&name.as_str()))
        .map(|(_, n)| *n)
        .sum();
    (generic, counts.values().sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(lines: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let d = super::dir(dir.path());
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("2026-03.host.jsonl"), lines.join("\n")).unwrap();
        dir
    }

    #[test]
    fn a_dispatch_is_counted_and_a_resume_is_not() {
        // A resume row carries an agent; counting it would inflate the column personas are
        // retired on, which is the one thing this tally decides.
        let dir = plane(&[
            r#"{"agent": "devops", "ts": "2026-03-02T09:33:00+00:00"}"#,
            r#"{"agent": "devops", "event": "resume", "ts": "2026-03-02T10:00:00+00:00"}"#,
            r#"{"agent": "Explore", "ts": "2026-03-02T11:00:00+00:00"}"#,
        ]);
        let counts = tally(dir.path());

        assert_eq!(counts.get("devops"), Some(&1));
        assert_eq!(counts.get("Explore"), Some(&1));
        assert_eq!(generic_share(&counts), (1, 2));
    }

    #[test]
    fn a_line_that_is_not_a_row_is_skipped_rather_than_ending_the_file() {
        // The log is appended to by a hook: a truncated last line must not lose the rows
        // above it, and a blank line is the same non-row as a broken one.
        let dir = plane(&[
            r#"{"agent": "devops"}"#,
            "",
            "{not json",
            r#"["agent"]"#,
            r#"{"ts": "2026-03-02T09:33:00+00:00"}"#,
            r#"{"agent": "steward"}"#,
        ]);

        assert_eq!(tally(dir.path()).len(), 2);
    }

    #[test]
    fn a_plane_with_no_log_has_no_dispatches_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();

        assert!(tally(dir.path()).is_empty());
        assert_eq!(generic_share(&tally(dir.path())), (0, 0));
    }
}
