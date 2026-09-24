//! `charter persona stats` — roster health mined from committed memory and the dispatch and
//! skill logs. Read-only. A port of `commands_persona.cmd_persona_stats`, `persona.stats`,
//! `dispatch.advice_tally` / `first_advice` / `routed_since_first_advice` / `last_backfill`
//! and `skilluse.drift`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime};

use crate::contain::SHARED_PERSONA;
use crate::repocmd::{Say, Sink};
use crate::tui::{Align, column, pad};

/// Activity profiles for which memory volume is not a usage signal — `_MEMORY_BLIND`.
const MEMORY_BLIND: [&str; 3] = ["orchestrator", "standby", "advisory"];

/// The dispatch log's advice event — `dispatch.ADVICE`.
const ADVICE: &str = "advice";

/// The suffix a backfill writes its rows under — `dispatch.BACKFILL_SUFFIX`.
const BACKFILL_SUFFIX: &str = ".backfill.jsonl";

/// One persona's (or the shared namespace's) row — `persona.stats`'s dict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub persona: String,
    pub count: usize,
    pub recent: usize,
    pub verify_pct: Option<i64>,
    pub dup_pct: Option<i64>,
    pub status: String,
}

/// Python's `round()` on a percentage: half to even.
fn pct(part: usize, total: usize) -> i64 {
    (100.0 * part as f64 / total as f64).round_ties_even() as i64
}

fn verify_re() -> regex::Regex {
    regex::Regex::new(r"(?i)\b(?:confirmed|validated|verified|proven|reproduced)\b")
        .expect("the verification pattern compiles")
}

/// `persona.stats`: one row from `name`'s committed memory.
pub fn row(root: &Path, name: &str, recent_days: i64, today: NaiveDate) -> Row {
    let dir = root.join("personas").join(name).join("memory");
    let (entries, _) = crate::memstore::read_entries(root, &dir);
    let total = entries.len();
    let re = verify_re();
    let mut recent = 0;
    let mut verified = 0;
    for e in &entries {
        if re.is_match(&e.text) {
            verified += 1;
        }
        let file = e.path.file_name().unwrap_or_default().to_string_lossy();
        if let Some(day) = crate::memstore::memory_date(&e.text, &file)
            && (today - day).num_days() <= recent_days
        {
            recent += 1;
        }
    }
    let mut dup: BTreeSet<usize> = BTreeSet::new();
    for (_, a, b) in crate::memstore::duplicates(&entries, 0.5) {
        dup.insert(a);
        dup.insert(b);
    }
    let activity = super::own_meta(root, name)
        .and_then(|m| m.get("activity").cloned())
        .map(|a| crate::memstore::py_strip(&a).to_lowercase())
        .filter(|a| !a.is_empty());
    let status = match activity {
        Some(a) if MEMORY_BLIND.contains(&a.as_str()) => a,
        _ if total == 0 => "dormant".into(),
        _ if recent > 0 => "active".into(),
        _ => "idle".into(),
    };
    Row {
        persona: name.to_string(),
        count: total,
        recent,
        verify_pct: (total > 0).then(|| pct(verified, total)),
        dup_pct: (total > 0).then(|| pct(dup.len(), total)),
        status,
    }
}

/// `dispatch._ts`: a row's `ts` as `datetime.fromisoformat` reads it — `(the instant, in UTC,
/// for ordering; the date as the row wrote it, for printing)`. A stamp with no offset is UTC.
///
/// `fromisoformat` takes more than RFC 3339: a date alone, `T` or a space, an hour with or
/// without minutes and seconds, a fraction, `Z` or an offset, and the basic forms without
/// separators. Each is tried here, because a row whose stamp this rejected was one Python
/// counted, and the advice line then printed an empty date.
fn ts(row: &serde_json::Value) -> Option<(NaiveDateTime, NaiveDate)> {
    let raw = row.get("ts")?.as_str()?.trim();
    let (body, offset) = split_offset(raw);
    let local = naive(body)?;
    let utc = local - chrono::Duration::seconds(offset?);
    Some((utc, local.date()))
}

/// `raw` without its trailing `Z`/`±HH[:MM[:SS]]`, and the offset in seconds — `Some(0)` for
/// none. `None` for an offset that does not read.
fn split_offset(raw: &str) -> (&str, Option<i64>) {
    if let Some(body) = raw.strip_suffix('Z') {
        return (body, Some(0));
    }
    // An offset can only follow the time, which follows the 8th character at the earliest.
    let at = raw
        .char_indices()
        .skip(8)
        .find(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i);
    let Some(at) = at else {
        return (raw, Some(0));
    };
    let (body, off) = raw.split_at(at);
    let sign = if off.starts_with('-') { -1 } else { 1 };
    let digits: String = off[1..].chars().filter(|c| *c != ':').collect();
    let part = |r: std::ops::Range<usize>| digits.get(r).and_then(|d| d.parse::<i64>().ok());
    let seconds = match digits.len() {
        2 => part(0..2).map(|h| h * 3600),
        4 => part(0..2).zip(part(2..4)).map(|(h, m)| h * 3600 + m * 60),
        6 => part(0..2)
            .zip(part(2..4))
            .zip(part(4..6))
            .map(|((h, m), s)| h * 3600 + m * 60 + s),
        _ => None,
    };
    (body, seconds.map(|s| sign * s))
}

fn naive(body: &str) -> Option<NaiveDateTime> {
    const DATES: [&str; 2] = ["%Y-%m-%d", "%Y%m%d"];
    const TIMES: [&str; 7] = [
        "%H:%M:%S%.f",
        "%H:%M:%S",
        "%H:%M",
        "%H%M%S%.f",
        "%H%M%S",
        "%H%M",
        "",
    ];
    for date in DATES {
        for sep in ["T", " "] {
            for time in TIMES {
                let fmt = if time.is_empty() {
                    date.to_string()
                } else {
                    format!("{date}{sep}{time}")
                };
                if time.is_empty() {
                    if let Ok(d) = NaiveDate::parse_from_str(body, &fmt) {
                        return d.and_hms_opt(0, 0, 0);
                    }
                } else if let Ok(at) = NaiveDateTime::parse_from_str(body, &fmt) {
                    return Some(at);
                }
            }
        }
    }
    // An hour alone, which chrono will not make a date-time of without a minute: the date,
    // the separator and exactly two digits. No format above reads such a stamp, so asking
    // after all of them answers what asking in `%H`'s place among them did.
    for date in DATES {
        for sep in ["T", " "] {
            if let Some((d, h)) = body.split_once(sep)
                && h.len() == 2
                && let (Ok(d), Ok(h)) = (NaiveDate::parse_from_str(d, date), h.parse::<u32>())
            {
                return d.and_hms_opt(h, 0, 0);
            }
        }
    }
    None
}

fn is_event(row: &serde_json::Value, event: &str) -> bool {
    row.get("event").and_then(|e| e.as_str()) == Some(event)
}

/// `(fired, first, followed)` — `dispatch.advice_tally`, `first_advice` (its date as the row
/// wrote it) and `routed_since_first_advice`.
fn advice(rows: &[serde_json::Value]) -> (usize, Option<NaiveDate>, usize) {
    let fired = rows.iter().filter(|r| is_event(r, ADVICE)).count();
    let first = rows
        .iter()
        .filter(|r| is_event(r, ADVICE))
        .filter_map(ts)
        .min_by_key(|(utc, _)| *utc);
    let followed = first.map_or(0, |(since, _)| {
        rows.iter()
            .filter(|r| crate::dispatch::truthy(r.get("agent")))
            .filter(|r| ts(r).is_some_and(|(t, _)| t >= since))
            .count()
    });
    (fired, first.map(|(_, day)| day), followed)
}

/// `dispatch.last_backfill`: the newest backfill file's modification time, local.
fn last_backfill(root: &Path) -> Option<NaiveDate> {
    let dir = crate::dispatch::dir(root);
    let newest = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(BACKFILL_SUFFIX))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()?;
    Some(chrono::DateTime::<chrono::Local>::from(newest).date_naive())
}

/// `skilluse.by_persona`: `{leaf skill: count}` for one persona.
fn skills_used(root: &Path, name: &str) -> BTreeSet<String> {
    let dir = root.join("personas").join(crate::skilluse::DIR_NAME);
    let Ok(reader) = std::fs::read_dir(dir) else {
        return BTreeSet::new();
    };
    let mut files: Vec<std::path::PathBuf> = reader
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    let mut out = BTreeSet::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        // `str.splitlines`, whose line breaks include U+2028 and friends: a row holding one
        // raw is two broken rows to Python, and counts for nothing.
        for line in crate::mdsection::split_lines(&text) {
            let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if !row.is_object() || !crate::dispatch::truthy(row.get("skill")) {
                continue;
            }
            let persona = row.get("persona").and_then(|p| p.as_str()).unwrap_or("");
            if persona != name {
                continue;
            }
            let skill = crate::pyrepr::str_json(&row["skill"]);
            let leaf = skill.split_once(':').map_or(skill.as_str(), |(_, l)| l);
            out.insert(leaf.to_string());
        }
    }
    out
}

/// `skilluse.drift`: `(unused, undeclared)` — declared and never invoked, and invoked and
/// never declared.
fn drift(root: &Path, name: &str) -> (Vec<String>, Vec<String>) {
    let declared: BTreeSet<String> = super::declared_skills(root, name)
        .iter()
        .map(|s| s.split_once(':').map_or(s.as_str(), |(_, l)| l).to_string())
        .collect();
    let used = skills_used(root, name);
    (
        declared.difference(&used).cloned().collect(),
        used.difference(&declared).cloned().collect(),
    )
}

/// The measured columns — `_STATS_HEADS`.
const HEADS: [(&str, Align); 6] = [
    ("PERSONA", Align::Left),
    ("MEM", Align::Right),
    ("RECENT", Align::Right),
    ("VERIFY", Align::Right),
    ("DUP", Align::Right),
    ("DISP", Align::Right),
];

/// `_stats_table`: the header row and the body rows, columns measured from the body.
fn table(body: &[[String; 7]]) -> Vec<String> {
    let widths: Vec<usize> = HEADS
        .iter()
        .enumerate()
        .map(|(i, (h, _))| column(h, body.iter().map(|r| r[i].as_str()), 2, None))
        .collect();
    let line = |cells: &[String; 7]| {
        let mut out = String::new();
        for (i, (_, align)) in HEADS.iter().enumerate() {
            out.push_str(&pad(&cells[i], widths[i], *align));
        }
        out.push_str("  ");
        out.push_str(&cells[6]);
        crate::memstore::py_rstrip(&out).to_string()
    };
    let head: [String; 7] = [
        "PERSONA", "MEM", "RECENT", "VERIFY", "DUP", "DISP", "STATUS",
    ]
    .map(String::from);
    std::iter::once(line(&head))
        .chain(body.iter().map(line))
        .collect()
}

fn glyph(status: &str) -> &'static str {
    match status {
        "active" => "●",
        "idle" => "○",
        "dormant" => "✗",
        "draft" => "⚑",
        "orchestrator" => "⬡",
        "standby" | "advisory" => "◇",
        "never dispatched" => "⚑",
        _ => "·",
    }
}

/// `charter persona stats [NAME] [--recent-days N]`, and its exit code.
pub fn stats(root: &Path, name: Option<&str>, recent_days: i64, today: NaiveDate, say: Sink) -> u8 {
    let name = name.filter(|n| !n.is_empty());
    if let Some(n) = name
        && n != SHARED_PERSONA
        && let Some(refused) = crate::personas::name_refusal(root, n)
    {
        say(Say::Fail(refused));
        return 1;
    }
    let mut names: Vec<String> = match name {
        Some(n) => vec![n.to_string()],
        None => super::names(root),
    };
    if names.is_empty() {
        say(Say::Info("No personas yet.".into()));
        return 0;
    }
    if name.is_none() {
        names.push(SHARED_PERSONA.to_string());
    }
    let mut rows: Vec<Row> = names
        .iter()
        .map(|n| row(root, n, recent_days, today))
        .collect();
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.persona.cmp(&b.persona))
    });
    let disp: BTreeMap<String, u64> = crate::dispatch::tally(root);
    let (mut dormant, mut idle, mut unused, mut drafts) = (0, 0, 0, 0);
    let mut body: Vec<[String; 7]> = Vec::new();
    for r in &rows {
        let shared = r.persona == SHARED_PERSONA;
        let v = r
            .verify_pct
            .map_or_else(|| "—".to_string(), |p| format!("{p}%"));
        let d = r
            .dup_pct
            .map_or_else(|| "—".to_string(), |p| format!("{p}%"));
        let rec = if r.count > 0 {
            r.recent.to_string()
        } else {
            "—".to_string()
        };
        let n_disp = disp.get(&r.persona).copied().unwrap_or(0);
        let mut status = r.status.clone();
        if !shared && super::is_draft(root, &r.persona) {
            status = "draft".into();
            drafts += 1;
        } else if !shared && n_disp == 0 && !disp.is_empty() {
            status = "never dispatched".into();
            unused += 1;
        }
        body.push([
            crate::personas::one_line(&r.persona),
            r.count.to_string(),
            rec,
            v,
            d,
            if shared {
                "—".to_string()
            } else {
                n_disp.to_string()
            },
            format!("{} {status}", glyph(&status)),
        ]);
        dormant += usize::from(r.status == "dormant");
        idle += usize::from(r.status == "idle");
    }
    for line in table(&body) {
        say(Say::Out(line));
    }
    say(Say::Out(String::new()));

    let drifted: Vec<(String, Vec<String>, Vec<String>)> = rows
        .iter()
        .filter(|r| r.persona != SHARED_PERSONA)
        .map(|r| {
            let (unused, undeclared) = drift(root, &r.persona);
            (r.persona.clone(), unused, undeclared)
        })
        .filter(|(_, u, d)| !u.is_empty() || !d.is_empty())
        .collect();
    if !drifted.is_empty() {
        say(Say::Out("SKILLS — declared vs actually invoked".into()));
        let shown: Vec<String> = drifted
            .iter()
            .map(|(n, _, _)| crate::personas::one_line(n))
            .collect();
        let nw = column("", shown.iter().map(String::as_str), 2, None);
        for (name, (_, unused, undeclared)) in shown.iter().zip(&drifted) {
            let padded = pad(name, nw, Align::Left);
            let joined = |items: &[String]| {
                items
                    .iter()
                    .map(|s| crate::personas::one_line(s))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            if !unused.is_empty() {
                say(Say::Out(format!(
                    "  {padded} unused: {}   (preloaded every dispatch)",
                    joined(unused)
                )));
            }
            if !undeclared.is_empty() {
                say(Say::Out(format!(
                    "  {padded} used but not declared: {}",
                    joined(undeclared)
                )));
            }
        }
        say(Say::Out(String::new()));
    }
    say(Say::Info(format!(
        "RECENT = memories in the last {recent_days} days · VERIFY = share carrying a \
         verification marker (quality proxy) · DUP = share in a near-dup pair (noise) · DISP = \
         times DISPATCHED as a sub-agent (committed tally) · ⬡/◇ = memory-blind role \
         (activity: profile), not judged by volume."
    )));
    let (generic, total) = crate::dispatch::generic_share(&disp);
    if total > 0 {
        say(Say::Info(format!(
            "Routing: {}/{total} dispatches went to a persona · {generic} to a generic agent \
             ({}%). A high generic share means the work a persona owns is being done without \
             it.",
            total - generic,
            100 * generic / total
        )));
    }
    let log = crate::dispatch::rows(root);
    let (fired, first, followed) = advice(&log);
    if fired > 0 {
        let since = first.map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string());
        say(Say::Info(format!(
            "Routing advice: fired {fired} time(s) · work handed to a persona {followed} \
             time(s) since the first one ({since}). Advice that fires and is never followed is \
             the block failing, not the roster — read it that way before adding more personas."
        )));
    }
    if !drifted.is_empty() {
        say(Say::Info(
            "SKILLS drift is named, not resolved: an unused declaration may be dead weight or a \
             skill whose moment has not come, and an undeclared one may be a charter out of \
             date or a persona reaching past its remit. Which it is depends on intent charter \
             cannot read."
                .into(),
        ));
    }
    if total == 0 {
        say(Say::Info(
            "No dispatches recorded yet — the tally starts filling as sub-agents are dispatched; \
             seeding it from past sessions is not in this version yet."
                .into(),
        ));
    }
    let when = last_backfill(root).map_or_else(
        || "never reconciled".to_string(),
        |d| format!("last reconciled {}", d.format("%Y-%m-%d")),
    );
    say(Say::Info(format!(
        "Tallied live from a PostToolUse hook, which can miss background dispatches — treat \
         DISP and ⚑ as a FLOOR ({when}). Reconciling it against this project's transcripts \
         is not in this version yet."
    )));
    if unused > 0 {
        say(Say::Warn(format!(
            "{unused} persona(s) NEVER dispatched — they exist, lint green, and are unused. \
             Check whether their work is routing to a generic agent instead."
        )));
    }
    if drafts > 0 {
        say(Say::Info(format!(
            "{drafts} draft persona(s) — charter generates no sub-agent while `draft: true` is \
             set, so they are undispatchable BY DESIGN and are not counted above. Finish the \
             charter, drop the line, then sync-agents."
        )));
    }
    if dormant > 0 {
        say(Say::Warn(format!(
            "{dormant} dormant persona(s) — a REAL prune signal (old, zero memory, no declared \
             activity: profile). The steward can quiz-propose removal (cite this)."
        )));
    }
    if idle > 0 {
        say(Say::Info(format!(
            "{idle} idle persona(s) — have memory but none recent; watch, don't prune yet."
        )));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percentage_rounds_half_to_even_as_python_does() {
        assert_eq!(pct(1, 8), 12);
        assert_eq!(pct(3, 8), 38);
        assert_eq!(pct(1, 3), 33);
        assert_eq!(pct(2, 3), 67);
    }

    #[test]
    fn advice_is_followed_only_by_what_came_at_or_after_it() {
        let rows: Vec<serde_json::Value> = [
            r#"{"agent": "a", "ts": "2026-03-02T09:00:00"}"#,
            r#"{"event": "advice", "ts": "2026-03-02T09:30:00"}"#,
            r#"{"agent": "b", "ts": "2026-03-02T09:30:00"}"#,
            r#"{"agent": "c", "event": "resume", "ts": "2026-03-02T10:00:00+00:00"}"#,
        ]
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
        let (fired, first, followed) = advice(&rows);
        assert_eq!(fired, 1);
        assert_eq!(first.unwrap().to_string(), "2026-03-02");
        assert_eq!(followed, 2);
    }

    #[test]
    fn a_stamp_is_read_as_fromisoformat_reads_it_and_dated_in_its_own_offset() {
        let at = |raw: &str| ts(&serde_json::json!({ "ts": raw }));
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(at("2026-03-02T09:33").unwrap().1, d("2026-03-02"));
        assert_eq!(
            at("2026-03-04T10").unwrap().0.to_string(),
            "2026-03-04 10:00:00"
        );
        assert_eq!(
            at("20260303T101010").unwrap().0.to_string(),
            "2026-03-03 10:10:10"
        );
        assert_eq!(
            at("2026-03-02").unwrap().0.to_string(),
            "2026-03-02 00:00:00"
        );
        let late = at("2026-03-01T23:30:00-05:00").unwrap();
        assert_eq!(late.0.to_string(), "2026-03-02 04:30:00");
        assert_eq!(late.1, d("2026-03-01"));
        assert_eq!(
            at("2026-03-02T09:00:00Z").unwrap().0.to_string(),
            "2026-03-02 09:00:00"
        );
        assert!(at("yesterday").is_none());
    }

    #[test]
    fn a_memory_blind_role_is_never_dormant() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("personas/router");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("persona.md"), "---\nactivity: Orchestrator\n---\n").unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        assert_eq!(row(dir.path(), "router", 14, today).status, "orchestrator");
    }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod recorded;
