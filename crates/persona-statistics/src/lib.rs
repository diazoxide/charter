//! Persona statistics: what an operator looks at when he asks how the plane's personas are
//! doing, computed from what charter hands a view about personas.
//!
//! **This is a built-in extension, and the executor's first consumer** (ADR 0041 stage 2,
//! charter-app#339). The app ships it inside its bundle. It does not read the plane: it knows
//! what the protocol hands it — one line of JSON on stdin, one line back on stdout — and it
//! links charter's core for one thing, the stats code `charter persona stats` counts with
//! ([`charter_core::personaverbs::stats`]), so that the view and the CLI give the same numbers.
//! Run it in a terminal and paste a request in to see what it answers.
//!
//! # What it shows, and why these three
//!
//! The operator's words were *"some button that will open statistics of personas — with some
//! visualization"*. Statistics nobody acts on are decoration, so each of these answers a
//! question an operator has a next step for:
//!
//! 1. **Memories per persona** (bars). *Which persona carries this plane's knowledge?* It is
//!    the one to route to, and the one whose loss would cost most. The count is on the panel
//!    already; what a chart adds is the *proportion*, which a column of numbers does not show.
//! 2. **Written each week, the last twelve** (columns). *Is this plane still learning?* A plane
//!    whose personas stopped remembering things a month ago has a memory hook that stopped
//!    firing or a team that stopped using it, and both are worth knowing.
//! 3. **Days since each persona last learned something**, quietest first (bars). *Which
//!    persona has gone quiet?* That is the actionable one: a persona nobody delegates to is a
//!    persona to retire or to fix, and a sorted bar is how to see it without reading dates.
//!
//! Rejected, and the reason each time: a pie of the same numbers as (1) (a worse drawing of
//! one fact); memory *size* (bytes are not knowledge, and charter deliberately hands no text);
//! per-kind breakdowns (every persona memory is `persistent`, so it is one bar).
//!
//! With a **focus** — opened from one persona's card — the same view answers about that
//! persona first: a sentence, and its own weekly columns, with the plane-wide bars after it
//! for context.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use charter_core::personaverbs::stats;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde_json::{Value, json};

/// The protocol this program speaks. charter refuses an answer in any other.
pub const PROTOCOL: u64 = 1;

/// The manifest this program is installed with, so `assemble` writes the one that is tested.
pub const MANIFEST: &str = include_str!("../charter-extension.json");

/// How many weeks the weekly columns cover.
pub const WEEKS: i64 = 12;

/// The most bars one chart draws. charter refuses more than 64 points in a chart, so a plane
/// with more personas than this gets its quietest folded into one bar, said as such.
pub const MOST_BARS: usize = 24;

/// The answer to one request line, as the JSON value to print.
///
/// **Never panics on what it is given**: a request it cannot read is an `error` answer, in a
/// sentence, which charter puts in front of the operator in the program's own words.
pub fn answer(request: &Value) -> Value {
    match statistics(request) {
        Ok(blocks) => json!({ "charter": PROTOCOL, "blocks": blocks }),
        Err(why) => json!({ "charter": PROTOCOL, "error": why }),
    }
}

/// One persona, as handed.
struct Persona {
    name: String,
    default: bool,
    /// The date each memory was written, when its stamp had one.
    dated: Vec<NaiveDate>,
    /// Memories whose stamp had no date charter could hand — counted, not placed in time.
    undated: usize,
    refused: Option<String>,
}

impl Persona {
    fn count(&self) -> usize {
        self.dated.len() + self.undated
    }

    fn last(&self) -> Option<NaiveDate> {
        self.dated.iter().max().copied()
    }
}

fn statistics(request: &Value) -> Result<Vec<Value>, String> {
    match request.get("charter").and_then(Value::as_u64) {
        Some(PROTOCOL) => {}
        Some(other) => {
            return Err(format!(
                "this program speaks protocol {PROTOCOL} and was asked in protocol {other}"
            ));
        }
        None => return Err("the request does not say which protocol it is in".into()),
    }
    if request.get("about").and_then(Value::as_str) != Some("personas") {
        return Err("this view is about personas, and was asked about something else".into());
    }
    let given = request.get("given").ok_or("the request hands nothing")?;
    let now = given
        .get("now")
        .and_then(Value::as_str)
        .and_then(|now| NaiveDateTime::parse_from_str(now, "%Y-%m-%d %H:%M").ok())
        .ok_or("the request does not say what time it is")?
        .date();
    let personas = read_personas(given)?;
    let focus = request.get("focus").and_then(Value::as_str);

    let mut blocks = Vec::new();
    if personas.is_empty() {
        blocks.push(note(
            "This plane has no personas yet, so there is nothing to count.",
        ));
        return Ok(blocks);
    }
    if given.get("truncated").and_then(Value::as_bool) == Some(true) {
        blocks.push(note(
            "charter stopped handing dates at its limit, so the counts below are a floor.",
        ));
    }
    for persona in personas.iter().filter(|it| it.refused.is_some()) {
        blocks.push(json!({
            "kind": "note",
            "tone": "trouble",
            "text": format!(
                "charter could not read {}'s memories: {}",
                persona.name,
                persona.refused.as_deref().unwrap_or_default()
            ),
        }));
    }

    match focus.and_then(|name| personas.iter().find(|it| it.name == name)) {
        Some(one) => {
            blocks.push(note(&about_one(one, now)));
            blocks.push(weekly(
                &format!("{} — written each week", one.name),
                std::slice::from_ref(one),
                now,
            ));
            blocks.push(per_persona(&personas));
        }
        None => {
            blocks.push(note(&about_all(&personas, now)));
            blocks.push(per_persona(&personas));
            blocks.push(weekly("Written each week", &personas, now));
            blocks.extend(quietest(&personas, now));
        }
    }
    Ok(blocks)
}

fn read_personas(given: &Value) -> Result<Vec<Persona>, String> {
    let listed = given
        .get("personas")
        .and_then(Value::as_array)
        .ok_or("the request hands no list of personas")?;
    let mut personas = Vec::with_capacity(listed.len());
    for row in listed {
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .ok_or("a persona was handed without a name")?
            .to_owned();
        let mut dated = Vec::new();
        let mut undated = 0usize;
        for stamp in row
            .get("written")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            // charter hands a memory's day, `YYYY-MM-DD` (and, before it did, `<date> <time>`):
            // statistics are by the day, in a time zone this program was never told.
            match stamp
                .as_str()
                .and_then(|stamp| stamp.get(..10))
                .and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
            {
                Some(day) => dated.push(day),
                None => undated += 1,
            }
        }
        personas.push(Persona {
            name,
            default: row.get("default").and_then(Value::as_bool) == Some(true),
            dated,
            undated,
            refused: row
                .get("refused")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
    }
    Ok(personas)
}

fn note(text: &str) -> Value {
    json!({ "kind": "note", "text": text })
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The memories `charter persona stats` counts as RECENT, counted by its own code.
fn recent(persona: &Persona, now: NaiveDate) -> usize {
    let days: Vec<Option<NaiveDate>> = persona.dated.iter().copied().map(Some).collect();
    stats::recent(&days, stats::RECENT_DAYS, now)
}

fn about_all(personas: &[Persona], now: NaiveDate) -> String {
    let total: usize = personas.iter().map(Persona::count).sum();
    let lately: usize = personas.iter().map(|it| recent(it, now)).sum();
    let undated: usize = personas.iter().map(|it| it.undated).sum();
    let mut said = format!(
        "{} across {} · {} in the last {} days",
        plural(total, "memory", "memories"),
        plural(personas.len(), "persona", "personas"),
        lately,
        stats::RECENT_DAYS
    );
    if undated > 0 {
        said.push_str(&format!(" · {undated} with no date"));
    }
    said
}

fn about_one(persona: &Persona, now: NaiveDate) -> String {
    let last = persona.last().map_or_else(
        || "nothing dated yet".to_owned(),
        |day| format!("last on {}", day.format("%Y-%m-%d")),
    );
    format!(
        "{} remembers {} · {} in the last {} days · {last}",
        persona.name,
        plural(persona.count(), "thing", "things"),
        recent(persona, now),
        stats::RECENT_DAYS
    )
}

/// Memories per persona, most first, with the default one said as such.
fn per_persona(personas: &[Persona]) -> Value {
    let total: usize = personas.iter().map(Persona::count).sum::<usize>().max(1);
    let mut sorted: Vec<&Persona> = personas.iter().collect();
    sorted.sort_by(|a, b| b.count().cmp(&a.count()).then_with(|| a.name.cmp(&b.name)));
    let mut points: Vec<Value> = sorted
        .iter()
        .take(MOST_BARS)
        .map(|persona| {
            let share = persona.count() * 100 / total;
            let note = if persona.default {
                format!("default · {share}%")
            } else {
                format!("{share}%")
            };
            json!({ "label": persona.name, "value": count(persona.count()), "note": note })
        })
        .collect();
    if sorted.len() > MOST_BARS {
        let rest = &sorted[MOST_BARS..];
        points.push(json!({
            "label": format!("{} others", rest.len()),
            "value": count(rest.iter().map(|it| it.count()).sum()),
        }));
    }
    json!({
        "kind": "chart", "title": "Memories per persona", "shape": "bars",
        "unit": "memories", "points": points,
    })
}

/// Memories written in each of the last [`WEEKS`] weeks, Monday to Sunday, oldest first.
fn weekly(title: &str, personas: &[Persona], now: NaiveDate) -> Value {
    let this_week = now - Duration::days(i64::from(now.weekday().num_days_from_monday()));
    let first = this_week - Duration::weeks(WEEKS - 1);
    let mut counts: BTreeMap<NaiveDate, usize> = (0..WEEKS)
        .map(|n| (first + Duration::weeks(n), 0))
        .collect();
    for day in personas.iter().flat_map(|it| it.dated.iter()) {
        if *day < first || *day > now {
            continue;
        }
        let monday = *day - Duration::days(i64::from(day.weekday().num_days_from_monday()));
        *counts.entry(monday).or_default() += 1;
    }
    let points: Vec<Value> = counts
        .iter()
        .map(|(monday, written)| {
            json!({ "label": monday.format("%b %-d").to_string(), "value": count(*written) })
        })
        .collect();
    json!({
        "kind": "chart", "title": title, "shape": "columns", "unit": "memories",
        "points": points,
    })
}

/// Days since each persona last learned something, quietest first — and, as a sentence, the
/// ones that never have.
fn quietest(personas: &[Persona], now: NaiveDate) -> Vec<Value> {
    let mut dated: Vec<(&Persona, NaiveDate)> = personas
        .iter()
        .filter_map(|it| it.last().map(|last| (it, last)))
        .collect();
    dated.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.name.cmp(&b.0.name)));
    let mut blocks = Vec::new();
    if !dated.is_empty() {
        let points: Vec<Value> = dated
            .iter()
            .take(MOST_BARS)
            .map(|(persona, last)| {
                let days = (now - *last).num_days().max(0);
                json!({
                    "label": persona.name,
                    "value": count(usize::try_from(days).unwrap_or(0)),
                    "note": last.format("%Y-%m-%d").to_string(),
                })
            })
            .collect();
        blocks.push(json!({
            "kind": "chart", "title": "Days since each last learned something",
            "shape": "bars", "unit": "days", "points": points,
        }));
    }
    let never: Vec<&str> = personas
        .iter()
        .filter(|it| it.count() == 0 && it.refused.is_none())
        .map(|it| it.name.as_str())
        .collect();
    if !never.is_empty() {
        blocks.push(note(&format!(
            "Nothing remembered yet: {}.",
            never.join(", ")
        )));
    }
    blocks
}

/// A count as charter draws one: a whole number below 2^32.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Put this program and its manifest in `dir`, as an extension charter can be pointed at.
///
/// **This is how the operator gets it**, and it is the program copying itself rather than a
/// script beside it, so there is no second thing to keep in step:
/// `cargo run --release -p persona-statistics -- assemble ~/charter-extensions/persona-statistics`,
/// then Extensions → install that folder → read what it declares → approve.
pub fn assemble(dir: &Path) -> std::io::Result<PathBuf> {
    let me = std::env::current_exe()?;
    std::fs::create_dir_all(dir.join("bin"))?;
    std::fs::write(dir.join("charter-extension.json"), MANIFEST)?;
    let program = dir.join("bin").join("persona-statistics");
    // A rename over the old copy, never a write through it: an older copy may be running, and
    // on macOS writing a program that has run invalidates its signature (stand-in's notes).
    let beside = dir.join("bin").join(".persona-statistics.new");
    std::fs::copy(&me, &beside)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&beside, &program)?;
    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(personas: Value, focus: Option<&str>) -> Value {
        answer(&json!({
            "charter": 1, "extension": "persona-statistics", "view": "statistics",
            "about": "personas", "focus": focus,
            "given": { "now": "2026-09-23 12:00", "personas": personas, "truncated": false },
        }))
    }

    fn chart<'a>(said: &'a Value, title: &str) -> &'a Value {
        said["blocks"]
            .as_array()
            .expect("blocks")
            .iter()
            .find(|block| block["title"] == title)
            .unwrap_or_else(|| panic!("no chart called {title}: {said}"))
    }

    fn plane() -> Value {
        json!([
            { "name": "steward", "default": true, "refused": null,
              "written": ["2026-09-22 10:00", "2026-09-21 09:00", "2026-08-01 12:00"] },
            { "name": "release", "default": false, "refused": null,
              "written": ["2026-07-01 12:00"] },
            { "name": "forge", "default": false, "refused": null, "written": [] },
        ])
    }

    #[test]
    fn memories_per_persona_are_bars_most_first_with_the_default_said() {
        let said = asked(plane(), None);
        let bars = chart(&said, "Memories per persona");

        assert_eq!(bars["shape"], "bars");
        let points = bars["points"].as_array().expect("points");
        assert_eq!(points[0]["label"], "steward");
        assert_eq!(points[0]["value"], 3);
        assert_eq!(points[0]["note"], "default · 75%");
        assert_eq!(points[1]["label"], "release");
        assert_eq!(points[2]["value"], 0);
    }

    #[test]
    fn the_weekly_columns_are_twelve_weeks_ending_this_one() {
        let said = asked(plane(), None);
        let columns = chart(&said, "Written each week");
        let points = columns["points"].as_array().expect("points");

        assert_eq!(points.len(), 12);
        // 2026-09-23 is a Wednesday, so this week began on Monday the 21st.
        assert_eq!(points[11]["label"], "Sep 21");
        assert_eq!(
            points[11]["value"], 2,
            "the 21st and the 22nd are this week"
        );
        assert_eq!(points[10]["value"], 0);
        assert_eq!(points[10]["label"], "Sep 14");
    }

    #[test]
    fn the_quietest_persona_comes_first_and_one_that_never_learned_is_said() {
        let said = asked(plane(), None);
        let quiet = chart(&said, "Days since each last learned something");
        let points = quiet["points"].as_array().expect("points");

        assert_eq!(points[0]["label"], "release");
        assert_eq!(points[0]["value"], 84);
        assert_eq!(points[1]["label"], "steward");
        let text = said.to_string();
        assert!(text.contains("Nothing remembered yet: forge."), "{text}");
    }

    #[test]
    fn a_focus_answers_about_that_persona_first() {
        let said = asked(plane(), Some("steward"));
        let blocks = said["blocks"].as_array().expect("blocks");

        assert!(
            blocks[0]["text"]
                .as_str()
                .expect("a sentence")
                .starts_with("steward remembers 3 things · 2 in the last 14 days"),
            "{said}"
        );
        chart(&said, "steward — written each week");
    }

    #[test]
    fn an_empty_plane_is_a_sentence_and_not_an_empty_chart() {
        let said = asked(json!([]), None);
        assert_eq!(said["blocks"].as_array().expect("blocks").len(), 1);
        assert!(said.to_string().contains("no personas yet"));
    }

    #[test]
    fn a_request_in_another_protocol_is_answered_with_an_error_not_a_guess() {
        let said = answer(&json!({ "charter": 2, "about": "personas" }));
        assert!(
            said["error"]
                .as_str()
                .expect("an error")
                .contains("protocol 2")
        );
    }

    #[test]
    fn a_persona_charter_could_not_read_is_said_as_trouble() {
        let said = asked(
            json!([{ "name": "x", "default": false, "written": [], "refused": "a link" }]),
            None,
        );
        assert!(
            said.to_string()
                .contains("could not read x's memories: a link")
        );
    }

    #[test]
    fn more_personas_than_a_chart_holds_are_folded_into_one_bar() {
        let many: Vec<Value> = (0..40)
            .map(|n| json!({ "name": format!("p{n:02}"), "written": ["2026-09-01 00:00"] }))
            .collect();
        let said = asked(Value::Array(many), None);
        let points = chart(&said, "Memories per persona")["points"]
            .as_array()
            .expect("points")
            .clone();
        assert_eq!(points.len(), MOST_BARS + 1);
        assert_eq!(points[MOST_BARS]["label"], "16 others");
    }

    #[test]
    fn the_manifest_it_is_installed_with_declares_this_program_and_one_view() {
        let manifest: Value = serde_json::from_str(MANIFEST).expect("JSON");
        assert_eq!(manifest["contributes"]["runs"], "bin/persona-statistics");
        assert_eq!(manifest["contributes"]["views"][0]["about"], "personas");
    }
}
