//! What charter hands an extension's program, per subject, and the one sentence that says so.
//!
//! **One module, so that what leaves charter is one file to audit.** ADR 0041's
//! capability table says of the plane on disk: *"A plugin gets what the protocol hands it.
//! 'Read the plane' is not a capability, it is the absence of one."* So a view does not get a
//! path to the plane, a directory to walk or a handle to read with. It gets a value, built here,
//! out of exactly the facts its [`Subject`] names — and the prompt the operator consents at
//! says those facts in [`what`], from this file, so the sentence and the value cannot drift.
//!
//! **And it is not a boundary**, for the reason [`crate::extension::RUNS_AS_YOU`] is on the
//! screen beside it. The program runs as the operator does and can read every memory itself.
//! What this module bounds is **charter's conduct**: what charter volunteers. That is worth
//! bounding — a program that only ever needs dates is never handed a memory's text, so a
//! program that logs its input, or crashes with it in a report, or is replaced by an update
//! nobody read, has none to leak — and it is worth not over-selling.
//!
//! # Personas: names, the default, and when each memory was written
//!
//! The first subject, and the one the persona statistics view is about. For each persona the
//! plane has, its name, whether the plane defaults to it, and the day each of its memories was
//! written. **Never a title and never a body.** Statistics are counts over time, and a count
//! over time needs a time and nothing else.
//!
//! The read is `charter persona stats`'s ([`crate::personaverbs::stats::written`]), once per
//! persona, when the operator opens a view — never on a workspace focus, never on a timer. It is
//! that one and not the persona card's (charter-app#339): the view and the CLI count the same
//! memories and date them on the same day, so they give the same numbers. A memory's day is
//! what `memstore.memory_date` finds — the stamp line's date, else a `YYYYMMDD-` file name.

use std::path::Path;

use crate::panel::Subject;

/// The most memory stamps charter hands one program. Past it the list stops and says so
/// (`truncated`), rather than the request growing with the plane: a request is written to a
/// program's standard input in one piece, and the executor bounds the answer, so the question
/// has to be bounded too. Twenty thousand is two orders of magnitude past the largest plane this
/// was measured on (the charter plane itself, a few hundred).
pub const MOST_STAMPS: usize = 20_000;

/// What charter hands a view about `about`, as the consent prompt says it.
///
/// It completes the sentence *"when you open it, charter starts this extension's program and
/// hands it …"*, and it is the only description of that value anywhere — a test holds it to
/// the keys [`personas`] actually writes.
pub fn what(about: Subject) -> &'static str {
    match about {
        Subject::Personas => {
            "this plane's persona names, which one is the default, and when each of their \
             memories was written — never what a memory says"
        }
    }
}

/// How `now` is written: a memory stamp's spelling (`memstore.py`:
/// `now.strftime('%Y-%m-%d %H:%M')`).
const STAMP: &str = "%Y-%m-%d %H:%M";

/// How a memory's day is written.
const DAY: &str = "%Y-%m-%d";

/// A memory's day as charter hands it, or `""` for one with none.
///
/// **This is what keeps [`what`]'s "never what a memory says" true.** What leaves here is never a
/// string read out of the file: it is a date *parsed* out of it and written again by charter, so
/// whatever a file holds, a program learns a day from it and nothing else.
fn day(date: Option<chrono::NaiveDate>) -> String {
    date.map(|date| date.format(DAY).to_string())
        .unwrap_or_default()
}

/// What a view about the plane's personas is handed, read now.
///
/// `now` is handed too, as charter's clock read once, so that a program's answer is a function
/// of its request and nothing else. That is what makes a request replayable by hand — paste the
/// line into the program in a terminal and get the same chart — which is ADR 0041's first
/// argument for a subprocess: *"its whole conversation with charter is a log a human can read."*
pub fn personas(root: &Path, now: chrono::NaiveDateTime) -> serde_json::Value {
    personas_within(root, now, MOST_STAMPS)
}

/// [`personas`], handing at most `most` stamps. The bound is a parameter so that a test can
/// reach it with a plane of three memories rather than twenty thousand files.
fn personas_within(root: &Path, now: chrono::NaiveDateTime, most: usize) -> serde_json::Value {
    let plane = crate::workspaces::Plane::open(root);
    let default = plane.default_persona();
    let mut handed = 0usize;
    let mut truncated = false;
    let mut listed = Vec::new();
    // A plane charter cannot list personas on is a plane with none, said as such: the view is
    // told nothing, which draws as its own empty state, and the card that opened it already
    // says why the panel is empty.
    for name in plane.personas().unwrap_or_default() {
        let mut row = serde_json::Map::new();
        row.insert("name".into(), serde_json::Value::String(name.clone()));
        row.insert(
            "default".into(),
            serde_json::Value::Bool(default.as_deref() == Some(name.as_str())),
        );
        match refused(root, &name) {
            None => {
                let mut written = Vec::new();
                for date in crate::personaverbs::stats::written(root, &name) {
                    if handed == most {
                        truncated = true;
                        break;
                    }
                    // A memory with no date is still a memory: it is counted, and it has no
                    // date. An empty string says that without inventing one.
                    written.push(serde_json::Value::String(day(date)));
                    handed += 1;
                }
                row.insert("written".into(), serde_json::Value::Array(written));
                row.insert("refused".into(), serde_json::Value::Null);
            }
            // charter's own sentence about why this persona's store could not be read, which
            // is a fact about the plane and not about any memory in it.
            Some(why) => {
                row.insert("written".into(), serde_json::Value::Array(Vec::new()));
                row.insert("refused".into(), serde_json::Value::String(why));
            }
        }
        listed.push(serde_json::Value::Object(row));
    }
    let mut doc = serde_json::Map::new();
    doc.insert(
        "now".into(),
        serde_json::Value::String(now.format(STAMP).to_string()),
    );
    doc.insert("personas".into(), serde_json::Value::Array(listed));
    doc.insert("truncated".into(), serde_json::Value::Bool(truncated));
    serde_json::Value::Object(doc)
}

/// Why charter will not read `name`'s memories, when it will not: a name it refuses, or a store
/// that leads out of the plane. The persona card's two refusals, in its words.
fn refused(root: &Path, name: &str) -> Option<String> {
    crate::personas::name_refusal(root, name).or_else(|| {
        let store = root.join("personas").join(name).join("memory");
        crate::contain::readable(root, &store)
            .err()
            .map(|why| why.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a directory");
        std::fs::write(
            dir.path().join("charter.toml"),
            "[persona]\ndefault = \"steward\"\n",
        )
        .expect("a manifest");
        for (name, memories) in [("steward", 2), ("release", 0)] {
            let at = dir.path().join("personas").join(name);
            std::fs::create_dir_all(at.join("memory")).expect("a persona");
            std::fs::write(at.join("persona.md"), "---\nrole: x\n---\n").expect("a definition");
            for n in 0..memories {
                std::fs::write(
                    at.join("memory").join(format!("fact-{n}.md")),
                    format!(
                        "# a fact {n}\n\n_2026-09-2{n} 10:00 · persistent_\n\nThe SECRET body {n}.\n"
                    ),
                )
                .expect("a memory");
            }
        }
        dir
    }

    fn at_noon() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 9, 23)
            .and_then(|day| day.and_hms_opt(12, 0, 0))
            .expect("a time")
    }

    #[test]
    fn a_view_about_personas_is_handed_names_the_default_and_dates() {
        let dir = plane();
        let handed = personas(dir.path(), at_noon());

        assert_eq!(handed["now"], "2026-09-23 12:00");
        let listed = handed["personas"].as_array().expect("a list");
        let steward = listed
            .iter()
            .find(|row| row["name"] == "steward")
            .expect("steward");
        assert_eq!(steward["default"], true);
        let mut written: Vec<&str> = steward["written"]
            .as_array()
            .expect("stamps")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        written.sort_unstable();
        assert_eq!(written, ["2026-09-20", "2026-09-21"]);
        let release = listed
            .iter()
            .find(|row| row["name"] == "release")
            .expect("release");
        assert_eq!(release["default"], false);
        assert_eq!(release["written"], serde_json::json!([]));
    }

    #[test]
    fn past_the_bound_the_stamps_stop_and_say_so() {
        let dir = plane();
        let stamps = |handed: &serde_json::Value| -> usize {
            handed["personas"]
                .as_array()
                .expect("a list")
                .iter()
                .map(|row| row["written"].as_array().expect("stamps").len())
                .sum()
        };

        // Exactly as many as the plane holds: every one, and nothing cut.
        let whole = personas_within(dir.path(), at_noon(), 2);
        assert_eq!(stamps(&whole), 2);
        assert_eq!(whole["truncated"], false);

        // One fewer: the list stops at the bound and says it stopped.
        let cut = personas_within(dir.path(), at_noon(), 1);
        assert_eq!(stamps(&cut), 1);
        assert_eq!(cut["truncated"], true);

        assert_eq!(personas(dir.path(), at_noon()), whole);
    }

    #[test]
    fn a_memory_s_words_are_never_handed() {
        // `what` promises the operator "never what a memory says". This is that promise, held
        // against the value rather than against the sentence: neither a title nor a body may
        // appear anywhere in what a program is given.
        let dir = plane();
        let text = personas(dir.path(), at_noon()).to_string();

        assert!(
            !text.contains("SECRET"),
            "a memory's body was handed: {text}"
        );
        assert!(
            !text.contains("a fact"),
            "a memory's title was handed: {text}"
        );
    }

    #[test]
    fn the_prompt_s_sentence_names_every_key_that_is_handed() {
        // The prompt and the value are two things that must agree, so the agreement is tested
        // rather than trusted: a key added to `personas` that the sentence does not account for
        // is a fact handed to a stranger that the operator was not told about.
        let dir = plane();
        let handed = personas(dir.path(), at_noon());
        let top: Vec<&str> = handed
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(top, ["now", "personas", "truncated"]);
        let row: Vec<&str> = handed["personas"][0]
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(row, ["name", "default", "written", "refused"]);

        let said = what(Subject::Personas);
        for fact in [
            "persona names",
            "default",
            "when each",
            "never what a memory says",
        ] {
            assert!(
                said.contains(fact),
                "the prompt no longer says {fact:?}: {said}"
            );
        }
    }

    #[test]
    fn a_plane_with_no_personas_hands_an_empty_list_rather_than_failing() {
        let dir = tempfile::tempdir().expect("a directory");
        let handed = personas(dir.path(), at_noon());
        assert_eq!(handed["personas"], serde_json::json!([]));
        assert_eq!(handed["truncated"], false);
    }

    #[test]
    fn a_memory_with_no_stamp_line_hands_no_line_of_its_body() {
        // The reader takes the first `_` line anywhere as a memory's stamp. In a file charter
        // did not write there may be none, and then a body line that happens to start with an
        // underscore — markdown emphasis, a python dunder — is what it takes. `what` promises
        // "never what a memory says"; this is that promise held against those two files.
        let dir = tempfile::tempdir().expect("a directory");
        let at = dir.path().join("personas").join("steward");
        std::fs::create_dir_all(at.join("memory")).expect("a persona");
        std::fs::write(at.join("persona.md"), "---\nrole: x\n---\n").expect("a definition");
        std::fs::write(
            at.join("memory").join("db.md"),
            "---\nname: db\n---\n# prod db\n\n_the prod password is hunter2_\n",
        )
        .expect("a memory");
        std::fs::write(
            at.join("memory").join("py.md"),
            "# layout\n\n__init__.py re-exports the SECRET_TOKEN loader\n",
        )
        .expect("a memory");

        let handed = personas(dir.path(), at_noon());

        let text = handed.to_string();
        assert!(!text.contains("hunter2"), "a body line was handed: {text}");
        assert!(
            !text.contains("SECRET_TOKEN"),
            "a body line was handed: {text}"
        );
        // Both are still counted, with no date.
        assert_eq!(
            handed["personas"][0]["written"],
            serde_json::json!(["", ""])
        );
    }

    #[test]
    fn a_memory_is_dated_as_charter_persona_stats_dates_it() {
        // No stamp line, and a `YYYYMMDD-` file name: the CLI's RECENT column counts it on that
        // day, so the view is handed that day (charter-app#339).
        let dir = tempfile::tempdir().expect("a directory");
        let at = dir.path().join("personas").join("steward");
        std::fs::create_dir_all(at.join("memory")).expect("a persona");
        std::fs::write(at.join("persona.md"), "---\nrole: x\n---\n").expect("a definition");
        std::fs::write(
            at.join("memory").join("20260919-note.md"),
            "# a note\n\nwritten by hand\n",
        )
        .expect("a memory");
        std::fs::write(
            at.join("memory").join("iso.md"),
            "# iso\n\n_2026-09-18T08:00 · persistent_\n\nbody\n",
        )
        .expect("a memory");

        let handed = personas(dir.path(), at_noon());

        let mut written: Vec<&str> = handed["personas"][0]["written"]
            .as_array()
            .expect("dates")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        written.sort_unstable();
        assert_eq!(written, ["2026-09-18", "2026-09-19"]);
    }
}
