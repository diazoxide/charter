//! The facts file: values an extension leaves for charter to draw as status badges and repo
//! cells, read **without starting its program** (charter-app#340, ADR 0053).
//!
//! # Declared, then filled
//!
//! A manifest that asks for the `badges` capability declares each badge in
//! `contributes.badges` — an id, a label, the surfaces it shows on (`status-bar`, `footer`) and
//! how many seconds a value stays fresh. One that asks for `repo-columns` declares each column in
//! `contributes.repo-columns` — an id, a title and a freshness. All of it is inside the
//! manifest's bytes, so the fingerprint covers it and the approval prompt names each one
//! ([`declares`]).
//!
//! The values are in `<state>/facts.json`, in the extension's declared state directory — the
//! one place it may write without being asked about again:
//!
//! ```json
//! { "badges":       { "<badge>":  { "value": "3", "at": 1790000000 } },
//!   "repo-columns": { "<column>": { "<repo>": { "value": "2", "at": 1790000000 } } } }
//! ```
//!
//! `at` is when the value was true, in Unix seconds. The extension rewrites the file whenever it
//! answers a question.
//!
//! # One reader, and what it refuses
//!
//! [`gather`] is the one reader: the app's status bar, the terminal footer and the repo table
//! all ask it. **It never starts a program** — it reads the record, the manifest, the tree (to
//! re-take the fingerprint) and one bounded file, and nothing else. It fills only what the
//! manifest declared: a field it did not declare contributes nothing and is reported, so a
//! facts file cannot put a badge on the screen the operator was never asked about. An
//! oversized, malformed or unreadable file contributes nothing and says why. An extension that
//! changed since it was approved contributes nothing and says so, and one the project or
//! workspace turned off contributes nothing at all (ADR 0048).
//!
//! **What it costs on the footer's hot path.** A machine with no extension record pays one
//! failed open. An approved extension pays one manifest read, and only one that declares a
//! footer badge reads the project's settings and, if it is on there, pays the tree hash — the same gate the executor takes
//! before it starts anything, because "changed on disk contributes nothing" is only true if the
//! bytes are looked at.
//!
//! **A value is data, drawn as text.** It is bounded, and refused when it holds anything that
//! draws as nothing (`panel::undrawable`), like every other string an extension hands charter.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Utc};

use super::{Capability, Extension, Standing};

/// The facts file's name, inside the extension's state directory.
pub const FILE: &str = "facts.json";

/// The most a facts file may be. It is read on the footer's hot path, every turn.
pub const MOST_BYTES: u64 = 64 << 10;

/// The most characters one value may be. A badge is a count or a word, not a sentence.
pub const MOST_VALUE_CHARS: usize = 40;

/// The most badges, and the most columns, one extension may declare.
const MOST_BADGES: usize = 8;
const MOST_COLUMNS: usize = 4;

/// The most bytes a label or a title may be.
const MOST_LABEL_BYTES: usize = 40;

/// The longest a value may be declared fresh for: a week. A value older than that is not a
/// status, whatever its extension says.
const MOST_FRESH_SECONDS: u64 = 7 * 24 * 3600;

/// The most cells one column may fill. More repos than this in one plane is not a table.
const MOST_CELLS: usize = 256;

/// How far ahead of charter's clock an extension's may be before its `at` is refused.
const MOST_SKEW_SECONDS: i64 = 300;

/// The most notes one extension's facts file may cost a surface.
const MOST_NOTES: usize = 3;

/// How much of a word out of the file a note repeats.
const MOST_WORD_SHOWN: usize = 64;

/// Where a badge may show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Surface {
    /// The app's status line, at the bottom of the window.
    StatusBar,
    /// `charter statusline`'s terminal footer.
    Footer,
}

impl Reading {
    /// The surface a badge has to name to be drawn for this reader.
    fn surface(self) -> Surface {
        match self {
            Self::Footer => Surface::Footer,
            Self::Window => Surface::StatusBar,
        }
    }
}

impl Surface {
    const EVERY: [Self; 2] = [Self::StatusBar, Self::Footer];

    /// The word a manifest writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StatusBar => "status-bar",
            Self::Footer => "footer",
        }
    }

    /// What the approval prompt calls it.
    fn said(self) -> &'static str {
        match self {
            Self::StatusBar => "the status bar",
            Self::Footer => "the terminal footer",
        }
    }
}

/// One badge a manifest declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredBadge {
    pub id: String,
    pub label: String,
    pub surfaces: Vec<Surface>,
    /// How many seconds a value stays fresh. Older is drawn dimmed, with its age.
    pub fresh_seconds: u64,
}

/// One repo column a manifest declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredColumn {
    pub id: String,
    pub title: String,
    pub fresh_seconds: u64,
}

/// The keys a declared badge, and a declared column, may carry.
const BADGE_KEYS: [&str; 4] = ["id", "label", "surfaces", "fresh_seconds"];
const COLUMN_KEYS: [&str; 3] = ["id", "title", "fresh_seconds"];

/// The badges `contributes.badges` declares, or why charter will not read them.
pub(super) fn badges_of(value: &serde_json::Value) -> Result<Vec<DeclaredBadge>, String> {
    let list = entries(value, "badges", MOST_BADGES)?;
    let mut out: Vec<DeclaredBadge> = Vec::with_capacity(list.len());
    for (at, object) in list.iter().enumerate() {
        only(object, &BADGE_KEYS, "a badge", at)?;
        let id = id_of(object, "badge", at, out.iter().map(|it| it.id.as_str()))?;
        let label = words(object, "label", "badge", &id)?;
        let surfaces: Vec<Surface> = match object.get("surfaces").and_then(|it| it.as_array()) {
            Some(named) if !named.is_empty() => {
                let mut surfaces = Vec::new();
                for word in named {
                    let found = Surface::EVERY
                        .into_iter()
                        .find(|it| Some(it.as_str()) == word.as_str())
                        .ok_or_else(|| {
                            format!(
                                "declares the badge {id:?} on a surface charter does not have — \
                                 a badge shows on status-bar, footer or both"
                            )
                        })?;
                    if !surfaces.contains(&found) {
                        surfaces.push(found);
                    }
                }
                surfaces.sort();
                surfaces
            }
            _ => {
                return Err(format!(
                    "declares the badge {id:?} without saying where it shows — 'surfaces' is a \
                     list of status-bar, footer or both"
                ));
            }
        };
        let fresh_seconds = fresh(object, "badge", &id)?;
        out.push(DeclaredBadge {
            id,
            label,
            surfaces,
            fresh_seconds,
        });
    }
    Ok(out)
}

/// The columns `contributes.repo-columns` declares, or why charter will not read them.
pub(super) fn columns_of(value: &serde_json::Value) -> Result<Vec<DeclaredColumn>, String> {
    let list = entries(value, "repo-columns", MOST_COLUMNS)?;
    let mut out: Vec<DeclaredColumn> = Vec::with_capacity(list.len());
    for (at, object) in list.iter().enumerate() {
        only(object, &COLUMN_KEYS, "a repo column", at)?;
        let id = id_of(
            object,
            "repo column",
            at,
            out.iter().map(|it| it.id.as_str()),
        )?;
        let title = words(object, "title", "repo column", &id)?;
        let fresh_seconds = fresh(object, "repo column", &id)?;
        out.push(DeclaredColumn {
            id,
            title,
            fresh_seconds,
        });
    }
    Ok(out)
}

/// A `contributes.<word>` list's objects, bounded.
fn entries<'v>(
    value: &'v serde_json::Value,
    word: &str,
    most: usize,
) -> Result<Vec<&'v serde_json::Map<String, serde_json::Value>>, String> {
    let list = value
        .as_array()
        .ok_or_else(|| format!("has a 'contributes.{word}' that is not a list"))?;
    if list.len() > most {
        return Err(format!(
            "declares {} {word}, and charter draws at most {most} from one extension",
            list.len()
        ));
    }
    list.iter()
        .enumerate()
        .map(|(at, raw)| {
            raw.as_object().ok_or_else(|| {
                format!("declares an entry at {at} of '{word}' that is not an object")
            })
        })
        .collect()
}

/// Refuse a key the shape does not have, for the reason `views_of` does.
fn only(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
    what: &str,
    at: usize,
) -> Result<(), String> {
    match object.keys().find(|key| !keys.contains(&key.as_str())) {
        None => Ok(()),
        Some(key) => Err(format!(
            "declares {what} at {at} carrying {key:?}, which is not part of what it may say — \
             it is {}",
            keys.join(", ")
        )),
    }
}

/// An entry's id: one segment of letters, digits, '-' and '_', unique in its list.
fn id_of<'s>(
    object: &serde_json::Map<String, serde_json::Value>,
    what: &str,
    at: usize,
    mut seen: impl Iterator<Item = &'s str>,
) -> Result<String, String> {
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("declares a {what} at {at} with no id"))?;
    if !id_ok(id) {
        return Err(format!(
            "declares the {what} id {id:?}, and an id is letters, digits, '-' and '_', starting \
             with a letter or a digit"
        ));
    }
    if seen.any(|it| it == id) {
        return Err(format!("declares two of its {what}s called {id:?}"));
    }
    Ok(id.to_owned())
}

fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
}

/// A label or a title: present, short, and drawable.
fn words(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    what: &str,
    id: &str,
) -> Result<String, String> {
    let text = object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("declares the {what} {id:?} with no {key}"))?;
    if text.len() > MOST_LABEL_BYTES || text.contains(crate::panel::undrawable) {
        return Err(format!(
            "declares the {what} {id:?} with a {key} charter will not draw: it is longer than \
             {MOST_LABEL_BYTES} bytes or holds a control or invisible formatting character"
        ));
    }
    Ok(text.to_owned())
}

/// `fresh_seconds`: a whole number of seconds, at least one, at most a week.
fn fresh(
    object: &serde_json::Map<String, serde_json::Value>,
    what: &str,
    id: &str,
) -> Result<u64, String> {
    object
        .get("fresh_seconds")
        .and_then(serde_json::Value::as_u64)
        .filter(|seconds| (1..=MOST_FRESH_SECONDS).contains(seconds))
        .ok_or_else(|| {
            format!(
                "declares the {what} {id:?} without a 'fresh_seconds' charter can use — how many \
                 seconds a value stays fresh, from 1 to {MOST_FRESH_SECONDS}"
            )
        })
}

/// A number of seconds as the prompt and the footer say it: `90s`, `15m`, `3h`, `2d`.
pub fn age(seconds: u64) -> String {
    match seconds {
        s if s < 90 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 48 * 3600 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

/// The approval prompt's line for each badge and each column, in charter's words.
pub(super) fn declares(manifest: &super::Manifest) -> Vec<String> {
    const NEVER_RUNS: &str = "charter never starts its program to draw it";
    let badges = manifest.badges.iter().map(|badge| {
        let surfaces: Vec<&str> = badge.surfaces.iter().map(|it| it.said()).collect();
        format!(
            "a badge, “{}” — shown in {}, read from its facts file and fresh for {}; {NEVER_RUNS}",
            badge.label,
            surfaces.join(" and "),
            age(badge.fresh_seconds)
        )
    });
    let columns = manifest.repo_columns.iter().map(|column| {
        format!(
            "a repo column, “{}” — a column in the repo table, read from its facts file and \
             fresh for {}; {NEVER_RUNS}",
            column.title,
            age(column.fresh_seconds)
        )
    });
    badges.chain(columns).collect()
}

// ---------------------------------------------------------------------------------------
// The reader
// ---------------------------------------------------------------------------------------

/// Who is reading, which decides what is read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// `charter statusline`: badges that show on the footer, and nothing else — so an
    /// extension with no footer badge costs the footer one manifest read and no hash.
    Footer,
    /// The app's window: badges that show on the status bar, and every repo column.
    Window,
}

/// One value, as it is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub value: String,
    /// How long ago the extension said it was true.
    pub age_seconds: u64,
    /// Older than its declared freshness: drawn dimmed, with its age.
    pub stale: bool,
}

/// One badge with its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    /// The extension's id.
    pub extension: String,
    /// What a person calls the extension.
    pub name: String,
    pub id: String,
    pub label: String,
    pub surfaces: Vec<Surface>,
    pub value: String,
    pub age_seconds: u64,
    pub stale: bool,
}

/// One repo column, with a cell for each repo its facts file filled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub extension: String,
    pub id: String,
    pub title: String,
    /// By repo name. A repo the file does not name has no cell.
    pub cells: BTreeMap<String, Cell>,
}

/// What every extension contributes to one surface, and what contributed nothing and why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    pub badges: Vec<Badge>,
    pub columns: Vec<Column>,
    /// One sentence per thing that contributed nothing and should have: a field the manifest
    /// did not declare, a file charter would not read, an extension that changed.
    pub notes: Vec<String>,
}

/// **The one reader.** What every approved extension that is on in this project (and
/// workspace, when `choices` were read in one) contributes to `reading`'s surface, at `now`.
///
/// Never raises and never starts a program. See the module docstring for what it costs.
///
/// `choices` is asked for only when an approved extension has something for this surface, so a
/// machine with no extension reads no project file for it.
///
/// `built_in` is the running app's built-in extensions (charter-app#339), which are approved
/// through the app at their place in its bundle, and contribute nothing while turned off on this
/// machine. `charter statusline` passes none: the CLI does not know where an app is.
pub fn gather(
    config_root: &Path,
    built_in: &super::BuiltIn,
    choices: impl FnOnce() -> super::project::Choices,
    now: DateTime<Utc>,
    reading: Reading,
) -> Facts {
    let mut choices = Some(choices);
    let mut read_choices: Option<super::project::Choices> = None;
    let mut facts = Facts::default();
    let loaded = super::read(config_root, built_in);
    // An unreadable record approves nothing; the Extensions list is where it is said.
    if loaded.unreadable.is_some() {
        return facts;
    }
    for (id, entry) in &loaded.registry.entries {
        let approved = entry.approved.is_some() || entry.source == super::Source::App;
        if !approved || !entry.on {
            continue;
        }
        // The manifest alone first, so an extension with nothing for this surface costs no
        // hash. One that cannot be read is said on the Extensions list, not here.
        let Ok(declared) = super::manifest_at(&entry.path) else {
            continue;
        };
        if !wanted(&declared, reading) {
            continue;
        }
        let here =
            read_choices.get_or_insert_with(|| choices.take().map(|it| it()).unwrap_or_default());
        if !on_here(id, &declared, here) {
            continue;
        }
        // **The gate the executor takes**, re-taken now: these bytes, at this path.
        let found = match super::read_at(&entry.path) {
            Ok(found) => found,
            Err(why) => {
                facts.notes.push(format!(
                    "{}: charter could not read it, so it shows nothing: {why}",
                    declared.name
                ));
                continue;
            }
        };
        let standing = if found.id() == id {
            loaded.standing(&found)
        } else {
            Standing::Changed
        };
        if standing != Standing::Approved {
            facts.notes.push(format!(
                "{} changed since you approved it, so it shows nothing until you approve it \
                 again in Extensions",
                found.manifest.name
            ));
            continue;
        }
        let before = facts.notes.len();
        read_one(&found, now, reading, &mut facts);
        // **At most a few sentences per extension.** A facts file full of undeclared keys is
        // one broken extension, and the footer draws a line per note on every turn.
        if facts.notes.len() - before > MOST_NOTES {
            let more = facts.notes.len() - before - (MOST_NOTES - 1);
            facts.notes.truncate(before + MOST_NOTES - 1);
            facts.notes.push(format!(
                "{} has {more} more problems with its facts file",
                found.manifest.name
            ));
        }
    }
    facts
}

/// Whether `manifest` declares anything `reading`'s surface draws.
fn wanted(manifest: &super::Manifest, reading: Reading) -> bool {
    (reading == Reading::Window && !manifest.repo_columns.is_empty())
        || manifest
            .badges
            .iter()
            .any(|it| it.surfaces.contains(&reading.surface()))
}

/// Whether the project (and workspace) has it on — `project::resolve`, the one answer.
fn on_here(id: &str, manifest: &super::Manifest, choices: &super::project::Choices) -> bool {
    use super::project::{Installed, resolve};
    let here = Installed {
        id: id.to_owned(),
        name: manifest.name.clone(),
        approved: true,
        settings: manifest.settings.clone(),
    };
    resolve(std::slice::from_ref(&here), choices)
        .iter()
        .any(|it| it.id == id && it.is_on())
}

/// One approved, unchanged extension's facts file, into `facts`.
fn read_one(found: &Extension, now: DateTime<Utc>, reading: Reading, facts: &mut Facts) {
    let manifest = &found.manifest;
    let name = &manifest.name;
    let badges: Vec<&DeclaredBadge> = manifest
        .badges
        .iter()
        .filter(|badge| badge.surfaces.contains(&reading.surface()))
        .collect();
    let columns: &[DeclaredColumn] = match reading {
        Reading::Footer => &[],
        Reading::Window => &manifest.repo_columns,
    };
    // A declared column is drawn with its heading even before the file fills it: a column that
    // appears only once a value does would move the table under the operator.
    let at = facts.columns.len();
    facts.columns.extend(columns.iter().map(|column| Column {
        extension: found.id().to_owned(),
        id: column.id.clone(),
        title: column.title.clone(),
        cells: BTreeMap::new(),
    }));

    // `parse` refuses badges or columns without a state directory, so this is always there.
    let Some(state) = &manifest.state else { return };
    let path = found.path.join(state).join(FILE);
    let text = match super::slurp(&found.path, &path, MOST_BYTES) {
        Ok(text) => text,
        // Not written yet: the extension has not been asked anything. Nothing to show and
        // nothing wrong.
        Err(why) if why == super::NOT_THERE => return,
        Err(why) => {
            facts
                .notes
                .push(format!("{name}'s facts file {why}, so it shows nothing"));
            return;
        }
    };
    let doc: serde_json::Value = match serde_json::from_str(&text) {
        Ok(doc) => doc,
        Err(why) => {
            facts.notes.push(format!(
                "{name}'s facts file is not JSON ({why}), so it shows nothing"
            ));
            return;
        }
    };
    let Some(doc) = doc.as_object() else {
        facts.notes.push(format!(
            "{name}'s facts file is not a JSON object, so it shows nothing"
        ));
        return;
    };

    let badge_word = Capability::Badges.as_str();
    let column_word = Capability::RepoColumns.as_str();
    for key in doc.keys() {
        if key != badge_word && key != column_word {
            facts.notes.push(format!(
                "{name}'s facts file holds \"{}\", which is not something a facts file says — \
                 it holds {badge_word} and {column_word} — so that part shows nothing",
                shown(key)
            ));
        }
    }

    let said_badges = doc.get(badge_word).map(serde_json::Value::as_object);
    if let Some(None) = said_badges {
        facts.notes.push(format!(
            "{name}'s facts file has '{badge_word}' that is not an object, so no badge of its \
             shows"
        ));
    }
    if let Some(Some(said)) = said_badges {
        for (id, field) in said {
            let declared = manifest.badges.iter().find(|it| &it.id == id);
            let Some(declared) = declared else {
                facts.notes.push(undeclared(name, "badge", id));
                continue;
            };
            if !badges.iter().any(|it| it.id == declared.id) {
                continue;
            }
            match cell(field, declared.fresh_seconds, now) {
                Ok(cell) => facts.badges.push(Badge {
                    extension: found.id().to_owned(),
                    name: name.clone(),
                    id: declared.id.clone(),
                    label: declared.label.clone(),
                    surfaces: declared.surfaces.clone(),
                    value: cell.value,
                    age_seconds: cell.age_seconds,
                    stale: cell.stale,
                }),
                Err(why) => facts.notes.push(format!(
                    "{name}'s badge \"{}\" {why}, so it shows nothing",
                    declared.label
                )),
            }
        }
    }

    if reading == Reading::Footer {
        return;
    }
    let Some(said) = doc.get(column_word) else {
        return;
    };
    let Some(said) = said.as_object() else {
        facts.notes.push(format!(
            "{name}'s facts file has '{column_word}' that is not an object, so no column of its \
             is filled"
        ));
        return;
    };
    for (id, repos) in said {
        let Some(offset) = columns.iter().position(|it| &it.id == id) else {
            facts.notes.push(undeclared(name, "repo column", id));
            continue;
        };
        let declared = &columns[offset];
        let Some(repos) = repos.as_object() else {
            facts.notes.push(format!(
                "{name}'s column \"{}\" is not an object of repos, so it is not filled",
                declared.title
            ));
            continue;
        };
        if repos.len() > MOST_CELLS {
            facts.notes.push(format!(
                "{name}'s column \"{}\" fills {} repos, and charter draws at most {MOST_CELLS}, \
                 so it is not filled",
                declared.title,
                repos.len()
            ));
            continue;
        }
        let column = &mut facts.columns[at + offset];
        for (repo, field) in repos {
            match cell(field, declared.fresh_seconds, now) {
                Ok(cell) => {
                    column.cells.insert(repo.clone(), cell);
                }
                Err(why) => facts.notes.push(format!(
                    "{name}'s column \"{}\" for the repo \"{}\" {why}, so that cell is empty",
                    declared.title,
                    shown(repo)
                )),
            }
        }
    }
}

/// The sentence for a field the manifest did not declare.
fn undeclared(name: &str, what: &str, id: &str) -> String {
    format!(
        "{name}'s facts file fills the {what} \"{}\", which its manifest does not declare, so it \
         shows nothing",
        shown(id)
    )
}

/// A word out of the file, as a note may repeat it.
fn shown(word: &str) -> String {
    crate::shown::readable(word, MOST_WORD_SHOWN)
}

/// One `{ "value": …, "at": … }`, as drawn at `now` — or the end of a sentence saying why not.
fn cell(field: &serde_json::Value, fresh_seconds: u64, now: DateTime<Utc>) -> Result<Cell, String> {
    let field = field
        .as_object()
        .ok_or("is not an object holding a value and when it was true")?;
    let value = match field.get("value") {
        Some(serde_json::Value::String(text)) => text.trim().to_owned(),
        Some(serde_json::Value::Number(number)) => number.to_string(),
        _ => return Err("holds no value that is text or a number".into()),
    };
    if value.is_empty()
        || value.chars().count() > MOST_VALUE_CHARS
        || value.contains(crate::panel::undrawable)
    {
        return Err(format!(
            "holds a value charter will not draw: it is empty, longer than {MOST_VALUE_CHARS} \
             characters, or holds a control or invisible formatting character"
        ));
    }
    let at = field
        .get("at")
        .and_then(serde_json::Value::as_i64)
        .ok_or("does not say when it was true ('at', in Unix seconds)")?;
    // A clock a little ahead of charter's is an age of nothing, not a negative one. Further
    // ahead than that is a value that would never go stale, which is not a status.
    if at > now.timestamp().saturating_add(MOST_SKEW_SECONDS) {
        return Err("says it was true in the future".into());
    }
    let age_seconds = u64::try_from(now.timestamp().saturating_sub(at)).unwrap_or(0);
    Ok(Cell {
        value,
        age_seconds,
        stale: age_seconds > fresh_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_age_is_said_in_the_largest_unit_that_reads() {
        assert_eq!(age(45), "45s");
        assert_eq!(age(3600), "1h");
        assert_eq!(age(15 * 60), "15m");
        assert_eq!(age(3 * 86_400), "3d");
        // Down, never up: just under an hour is not an hour.
        assert_eq!(age(3599), "59m");
    }

    #[test]
    fn a_value_from_a_clock_ahead_of_charters_is_fresh_and_no_age() {
        let now = DateTime::from_timestamp(1_000, 0).expect("a time");
        let field = serde_json::json!({"value": 3, "at": 1_060});
        let cell = cell(&field, 10, now).expect("a cell");
        assert_eq!(
            (cell.value.as_str(), cell.age_seconds, cell.stale),
            ("3", 0, false)
        );
    }

    #[test]
    fn a_value_from_far_in_the_future_is_refused_rather_than_fresh_forever() {
        let now = DateTime::from_timestamp(1_000, 0).expect("a time");
        let field = serde_json::json!({"value": 3, "at": 1_000 + 86_400});
        assert!(cell(&field, 10, now).is_err());
    }

    #[test]
    fn a_value_that_draws_as_nothing_is_refused() {
        let now = DateTime::from_timestamp(1_000, 0).expect("a time");
        let field = serde_json::json!({"value": "ok\u{202e}", "at": 1_000});
        assert!(cell(&field, 10, now).is_err());
    }
}
