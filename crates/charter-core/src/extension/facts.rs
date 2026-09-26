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
pub(super) fn on_here(
    id: &str,
    manifest: &super::Manifest,
    choices: &super::project::Choices,
) -> bool {
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

    // -----------------------------------------------------------------------------------
    // Declaring: what `contributes.badges` and `contributes.repo-columns` may say
    // -----------------------------------------------------------------------------------

    /// A badge that says everything it has to, with `id` and `label` as given.
    fn badge_named(id: &str, label: &str) -> serde_json::Value {
        serde_json::json!({"id": id, "label": label, "surfaces": ["footer"], "fresh_seconds": 60})
    }

    fn one_badge(badge: serde_json::Value) -> Result<Vec<DeclaredBadge>, String> {
        badges_of(&serde_json::json!([badge]))
    }

    #[test]
    fn a_declared_badge_is_read_as_it_was_written() {
        let read = badges_of(&serde_json::json!([{
            "id": "open-prs",
            "label": "  PRs  ",
            "surfaces": ["footer", "status-bar"],
            "fresh_seconds": 3600
        }]))
        .expect("a badge");
        assert_eq!(
            read,
            [DeclaredBadge {
                id: "open-prs".into(),
                label: "PRs".into(),
                surfaces: vec![Surface::StatusBar, Surface::Footer],
                fresh_seconds: 3600,
            }]
        );
    }

    #[test]
    fn a_declared_repo_column_is_read_as_it_was_written() {
        let read = columns_of(&serde_json::json!([
            {"id": "ci", "title": "CI", "fresh_seconds": 90},
            {"id": "cov", "title": "Coverage", "fresh_seconds": 600}
        ]))
        .expect("two columns");
        assert_eq!(
            read,
            [
                DeclaredColumn {
                    id: "ci".into(),
                    title: "CI".into(),
                    fresh_seconds: 90,
                },
                DeclaredColumn {
                    id: "cov".into(),
                    title: "Coverage".into(),
                    fresh_seconds: 600,
                },
            ]
        );
    }

    #[test]
    fn a_badge_named_on_one_surface_twice_shows_there_once() {
        let read = one_badge(serde_json::json!({
            "id": "a", "label": "A", "surfaces": ["footer", "footer"], "fresh_seconds": 60
        }))
        .expect("a badge");
        assert_eq!(read[0].surfaces, [Surface::Footer]);
    }

    #[test]
    fn a_badge_that_shows_nowhere_or_somewhere_charter_has_not_is_refused() {
        for surfaces in [
            serde_json::json!([]),
            serde_json::json!("footer"),
            serde_json::json!(["sidebar"]),
        ] {
            let why = one_badge(serde_json::json!({
                "id": "a", "label": "A", "surfaces": surfaces, "fresh_seconds": 60
            }))
            .expect_err("a badge with nowhere to show");
            assert!(why.contains("\"a\""), "{why}");
        }
        let read = one_badge(serde_json::json!({
            "id": "a", "label": "A", "surfaces": ["status-bar"], "fresh_seconds": 60
        }))
        .expect("a badge");
        assert_eq!(read[0].surfaces, [Surface::StatusBar]);
    }

    #[test]
    fn a_list_may_declare_as_many_as_charter_draws_and_not_one_more() {
        let badges = |count: usize| {
            serde_json::Value::Array(
                (0..count)
                    .map(|n| badge_named(&format!("b{n}"), "B"))
                    .collect(),
            )
        };
        assert_eq!(badges_of(&badges(1)).expect("one").len(), 1);
        assert_eq!(
            badges_of(&badges(MOST_BADGES)).expect("the most").len(),
            MOST_BADGES
        );
        let why = badges_of(&badges(MOST_BADGES + 1)).expect_err("one too many");
        assert!(why.contains("at most 8"), "{why}");

        let columns = |count: usize| {
            serde_json::Value::Array(
                (0..count)
                    .map(|n| serde_json::json!({"id": format!("c{n}"), "title": "C", "fresh_seconds": 60}))
                    .collect(),
            )
        };
        assert_eq!(
            columns_of(&columns(MOST_COLUMNS)).expect("the most").len(),
            MOST_COLUMNS
        );
        assert!(columns_of(&columns(MOST_COLUMNS + 1)).is_err());
    }

    #[test]
    fn an_entry_carrying_a_key_it_may_not_say_is_refused() {
        let mut badge = badge_named("a", "A");
        badge["colour"] = serde_json::json!("red");
        let why = one_badge(badge).expect_err("an unknown key");
        assert!(why.contains("\"colour\""), "{why}");
        assert!(one_badge(badge_named("a", "A")).is_ok());
    }

    #[test]
    fn an_id_is_one_segment_of_letters_digits_dashes_and_underscores() {
        for good in ["a", "9", "open-prs_2", "a_b-c", &"x".repeat(64)] {
            let read = one_badge(badge_named(good, "A"))
                .unwrap_or_else(|why| panic!("{good:?} was refused: {why}"));
            assert_eq!(read[0].id, good);
        }
        for bad in ["", "-a", "_a", "a.b", "a b", "a/b", "é", &"x".repeat(65)] {
            assert!(
                one_badge(badge_named(bad, "A")).is_err(),
                "{bad:?} was accepted"
            );
        }
        let why = one_badge(
            serde_json::json!({"label": "A", "surfaces": ["footer"], "fresh_seconds": 60}),
        )
        .expect_err("no id");
        assert!(why.contains("no id"), "{why}");
    }

    #[test]
    fn two_entries_may_not_share_an_id_and_two_different_ids_are_two_entries() {
        let both = badges_of(&serde_json::json!([
            badge_named("a", "A"),
            badge_named("b", "B")
        ]))
        .expect("two badges");
        assert_eq!(both.len(), 2);
        let why = badges_of(&serde_json::json!([
            badge_named("a", "A"),
            badge_named("a", "B")
        ]))
        .expect_err("a repeated id");
        assert!(why.contains("two of its badges called \"a\""), "{why}");
    }

    #[test]
    fn a_label_is_present_short_and_drawable() {
        let longest = "L".repeat(MOST_LABEL_BYTES);
        let read = one_badge(badge_named("a", &longest)).expect("the longest label");
        assert_eq!(read[0].label, longest);
        for bad in [
            String::new(),
            "   ".to_owned(),
            "L".repeat(MOST_LABEL_BYTES + 1),
            "ok\u{202e}".to_owned(),
        ] {
            assert!(
                one_badge(badge_named("a", &bad)).is_err(),
                "{bad:?} was accepted"
            );
        }
    }

    #[test]
    fn a_freshness_is_from_one_second_to_a_week() {
        let with = |fresh: serde_json::Value| {
            one_badge(serde_json::json!({
                "id": "a", "label": "A", "surfaces": ["footer"], "fresh_seconds": fresh
            }))
        };
        assert_eq!(
            with(serde_json::json!(1)).expect("a second")[0].fresh_seconds,
            1
        );
        assert_eq!(
            with(serde_json::json!(604_800)).expect("a week")[0].fresh_seconds,
            604_800
        );
        for bad in [
            serde_json::json!(0),
            serde_json::json!(604_801),
            serde_json::json!(-1),
            serde_json::json!("60"),
        ] {
            assert!(with(bad.clone()).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn an_age_changes_unit_exactly_at_its_boundaries() {
        assert_eq!(age(89), "89s");
        assert_eq!(age(90), "1m");
        assert_eq!(age(2 * 3600), "2h");
        assert_eq!(age(48 * 3600 - 1), "47h");
        assert_eq!(age(48 * 3600), "2d");
    }

    #[test]
    fn the_prompt_names_each_badge_where_it_shows_and_each_column() {
        let manifest = manifest_of(
            "p",
            &[
                r#"{"id":"a","label":"Open","surfaces":["status-bar","footer"],"fresh_seconds":900}"#,
            ],
            &[r#"{"id":"c","title":"CI","fresh_seconds":90}"#],
        );
        assert_eq!(
            declares(&manifest),
            [
                "a badge, “Open” — shown in the status bar and the terminal footer, read from its \
                 facts file and fresh for 15m; charter never starts its program to draw it",
                "a repo column, “CI” — a column in the repo table, read from its facts file and \
                 fresh for 1m; charter never starts its program to draw it",
            ]
        );
    }

    // -----------------------------------------------------------------------------------
    // Reading: which extension is asked, and what its facts file puts on a surface
    // -----------------------------------------------------------------------------------

    /// The manifest text of an extension with these badges and columns (each a JSON object).
    fn manifest_text(id: &str, badges: &[&str], columns: &[&str]) -> String {
        let mut capabilities = Vec::new();
        let mut contributes = Vec::new();
        if !badges.is_empty() {
            capabilities.push(r#""badges""#);
            contributes.push(format!(r#""badges":[{}]"#, badges.join(",")));
        }
        if !columns.is_empty() {
            capabilities.push(r#""repo-columns""#);
            contributes.push(format!(r#""repo-columns":[{}]"#, columns.join(",")));
        }
        format!(
            r#"{{"version":2,"id":"{id}","name":"Ext {id}","capabilities":[{}],"state":"state",
                "contributes":{{{}}}}}"#,
            capabilities.join(","),
            contributes.join(",")
        )
    }

    fn manifest_of(id: &str, badges: &[&str], columns: &[&str]) -> super::super::Manifest {
        super::super::parse(&manifest_text(id, badges, columns)).expect("a manifest")
    }

    const FOOTER_BADGE: &str =
        r#"{"id":"f","label":"Foot","surfaces":["footer"],"fresh_seconds":60}"#;
    const BAR_BADGE: &str =
        r#"{"id":"s","label":"Bar","surfaces":["status-bar"],"fresh_seconds":60}"#;
    const COLUMN: &str = r#"{"id":"c","title":"CI","fresh_seconds":60}"#;
    const SECOND_COLUMN: &str = r#"{"id":"d","title":"Deploy","fresh_seconds":60}"#;

    /// The clock every reading here is taken at.
    fn noon() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).expect("a time")
    }

    /// `{ "value": value, "at": seconds before noon }`.
    fn said(value: &str, ago: i64) -> serde_json::Value {
        serde_json::json!({"value": value, "at": noon().timestamp() - ago})
    }

    /// Extensions in a temporary directory, installed in a config root beside them.
    struct Place {
        dir: tempfile::TempDir,
    }

    impl Place {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().expect("a directory"),
            }
        }

        fn config(&self) -> std::path::PathBuf {
            self.dir.path().join("config")
        }

        fn ext(&self, id: &str) -> std::path::PathBuf {
            self.dir.path().join(id)
        }

        /// Write the extension `id` and read it back, without installing it.
        fn made(&self, id: &str, badges: &[&str], columns: &[&str]) -> Extension {
            std::fs::create_dir_all(self.ext(id)).expect("its directory");
            std::fs::write(
                self.ext(id).join(super::super::MANIFEST),
                manifest_text(id, badges, columns),
            )
            .expect("its manifest");
            super::super::read_at(&self.ext(id)).expect("an extension")
        }

        /// Write the extension `id`, install it, and approve what is on disk.
        fn approved(&self, id: &str, badges: &[&str], columns: &[&str]) -> Extension {
            let found = self.installed(id, badges, columns);
            super::super::approve(&self.config(), found.id(), &found.path, &found.fingerprint)
                .expect("approved");
            found
        }

        /// Write the extension `id` and install it, without the operator's yes.
        fn installed(&self, id: &str, badges: &[&str], columns: &[&str]) -> Extension {
            self.made(id, badges, columns);
            super::super::install(
                &self.config(),
                &super::super::BuiltIn::none(),
                &self.ext(id),
            )
            .expect("installed")
        }

        /// Its facts file, as the extension would write it.
        fn facts_are(&self, id: &str, doc: &serde_json::Value) {
            self.facts_text(id, &doc.to_string());
        }

        fn facts_text(&self, id: &str, text: &str) {
            let state = self.ext(id).join("state");
            std::fs::create_dir_all(&state).expect("its state directory");
            std::fs::write(state.join(FILE), text).expect("its facts file");
        }

        fn gather(&self, reading: Reading) -> Facts {
            gather(
                &self.config(),
                &super::super::BuiltIn::none(),
                || super::super::project::Choices::from_text(None, None),
                noon(),
                reading,
            )
        }
    }

    /// What `found`'s facts file puts on `reading`'s surface, read alone.
    fn read_alone(found: &Extension, reading: Reading) -> Facts {
        let mut facts = Facts::default();
        read_one(found, noon(), reading, &mut facts);
        facts
    }

    #[test]
    fn a_surface_is_asked_only_of_an_extension_that_declares_something_it_draws() {
        let footer = manifest_of("p", &[FOOTER_BADGE], &[]);
        assert!(wanted(&footer, Reading::Footer));
        assert!(!wanted(&footer, Reading::Window));

        let bar = manifest_of("p", &[BAR_BADGE], &[]);
        assert!(wanted(&bar, Reading::Window));
        assert!(!wanted(&bar, Reading::Footer));

        let columns = manifest_of("p", &[], &[COLUMN]);
        assert!(wanted(&columns, Reading::Window));
        assert!(!wanted(&columns, Reading::Footer));
    }

    #[test]
    fn an_extension_is_on_here_until_the_project_turns_it_off() {
        let manifest = manifest_of("p", &[FOOTER_BADGE], &[]);
        let on = super::super::project::Choices::from_text(None, None);
        assert!(on_here("p", &manifest, &on));
        let off = super::super::project::Choices::from_text(
            Some("[extensions.p]\nenabled = false\n"),
            None,
        );
        assert!(!on_here("p", &manifest, &off));
    }

    #[test]
    fn an_approved_extensions_badges_and_cells_are_gathered_for_the_window() {
        let place = Place::new();
        place.approved("p", &[BAR_BADGE, FOOTER_BADGE], &[COLUMN]);
        place.facts_are(
            "p",
            &serde_json::json!({
                "badges": {"s": said("3", 10), "f": said("4", 10)},
                "repo-columns": {"c": {"svc": said("ok", 120)}}
            }),
        );

        let read = place.gather(Reading::Window);
        assert!(read.notes.is_empty(), "{:#?}", read.notes);
        assert_eq!(
            read.badges,
            [Badge {
                extension: "p".into(),
                name: "Ext p".into(),
                id: "s".into(),
                label: "Bar".into(),
                surfaces: vec![Surface::StatusBar],
                value: "3".into(),
                age_seconds: 10,
                stale: false,
            }]
        );
        assert_eq!(read.columns.len(), 1, "{read:#?}");
        assert_eq!(
            read.columns[0].cells.get("svc"),
            Some(&Cell {
                value: "ok".into(),
                age_seconds: 120,
                stale: true,
            })
        );

        let footer = place.gather(Reading::Footer);
        assert!(footer.notes.is_empty(), "{:#?}", footer.notes);
        assert!(footer.columns.is_empty(), "{footer:#?}");
        assert_eq!(
            footer
                .badges
                .iter()
                .map(|it| it.id.as_str())
                .collect::<Vec<_>>(),
            ["f"]
        );
    }

    #[test]
    fn an_extension_installed_and_never_approved_contributes_nothing_and_says_nothing() {
        let place = Place::new();
        place.installed("p", &[BAR_BADGE], &[COLUMN]);
        place.facts_are("p", &serde_json::json!({"badges": {"s": said("3", 0)}}));
        assert_eq!(place.gather(Reading::Window), Facts::default());
    }

    #[test]
    fn an_extension_the_project_turned_off_contributes_nothing() {
        let place = Place::new();
        place.approved("p", &[BAR_BADGE], &[COLUMN]);
        place.facts_are("p", &serde_json::json!({"badges": {"s": said("3", 0)}}));
        let read = gather(
            &place.config(),
            &super::super::BuiltIn::none(),
            || {
                super::super::project::Choices::from_text(
                    Some("[extensions.p]\nenabled = false\n"),
                    None,
                )
            },
            noon(),
            Reading::Window,
        );
        assert_eq!(read, Facts::default());
    }

    #[test]
    fn an_extension_changed_since_its_approval_says_so_and_shows_nothing() {
        let place = Place::new();
        place.approved("p", &[BAR_BADGE], &[]);
        place.facts_are("p", &serde_json::json!({"badges": {"s": said("3", 0)}}));
        std::fs::write(place.ext("p").join("README"), "added after the yes").expect("written");

        let read = place.gather(Reading::Window);
        assert!(read.badges.is_empty(), "{read:#?}");
        assert_eq!(
            read.notes,
            [
                "Ext p changed since you approved it, so it shows nothing until you approve it \
              again in Extensions"
            ]
        );
    }

    /// A facts file for `id` that fills `count` badges its manifest never declared.
    fn undeclared_badges(place: &Place, id: &str, count: usize) {
        let badges: serde_json::Map<String, serde_json::Value> = (0..count)
            .map(|n| (format!("junk{n}"), said("1", 0)))
            .collect();
        place.facts_are(id, &serde_json::json!({ "badges": badges }));
    }

    #[test]
    fn each_extension_may_say_up_to_three_things_before_its_notes_are_summed_up() {
        let place = Place::new();
        place.approved("aaa", &[BAR_BADGE], &[]);
        place.approved("bbb", &[BAR_BADGE], &[]);
        undeclared_badges(&place, "aaa", 2);
        undeclared_badges(&place, "bbb", 3);

        let read = place.gather(Reading::Window);
        assert_eq!(read.notes.len(), 5, "{:#?}", read.notes);
        assert!(
            read.notes.iter().all(|it| it.contains("does not declare")),
            "{:#?}",
            read.notes
        );
    }

    #[test]
    fn a_fourth_problem_from_one_extension_is_counted_rather_than_said() {
        let place = Place::new();
        place.approved("aaa", &[BAR_BADGE], &[]);
        place.approved("bbb", &[BAR_BADGE], &[]);
        undeclared_badges(&place, "aaa", 2);
        undeclared_badges(&place, "bbb", 6);

        let read = place.gather(Reading::Window);
        assert_eq!(read.notes.len(), 5, "{:#?}", read.notes);
        assert!(
            read.notes[..4]
                .iter()
                .all(|it| it.contains("does not declare"))
        );
        assert!(read.notes[0].starts_with("Ext aaa's"), "{:#?}", read.notes);
        assert!(read.notes[2].starts_with("Ext bbb's"), "{:#?}", read.notes);
        assert_eq!(
            read.notes[4],
            "Ext bbb has 4 more problems with its facts file"
        );
    }

    #[test]
    fn a_facts_file_not_written_yet_is_nothing_to_show_and_nothing_wrong() {
        let place = Place::new();
        let found = place.made("p", &[BAR_BADGE], &[COLUMN]);
        let read = read_alone(&found, Reading::Window);
        assert!(read.notes.is_empty(), "{:#?}", read.notes);
        assert!(read.badges.is_empty());
        // The column is drawn with its heading before anything fills it.
        assert_eq!(read.columns.len(), 1);
        assert_eq!(read.columns[0].title, "CI");
        assert!(read.columns[0].cells.is_empty());
    }

    #[test]
    fn a_facts_file_charter_will_not_read_says_why() {
        let place = Place::new();
        let found = place.made("p", &[BAR_BADGE], &[]);
        place.facts_text(
            "p",
            &" ".repeat(usize::try_from(MOST_BYTES).expect("small") + 1),
        );
        let read = read_alone(&found, Reading::Window);
        assert_eq!(read.notes.len(), 1, "{:#?}", read.notes);
        assert!(
            read.notes[0].starts_with("Ext p's facts file "),
            "{:#?}",
            read.notes
        );

        // At the most it may be, it is read.
        let mut text = serde_json::json!({"badges": {"s": said("3", 0)}}).to_string();
        text.push_str(&" ".repeat(usize::try_from(MOST_BYTES).expect("small") - text.len()));
        place.facts_text("p", &text);
        let read = read_alone(&found, Reading::Window);
        assert!(read.notes.is_empty(), "{:#?}", read.notes);
        assert_eq!(read.badges.len(), 1);
    }

    #[test]
    fn a_key_a_facts_file_does_not_have_is_named_and_the_two_it_has_are_not() {
        let place = Place::new();
        let found = place.made("p", &[BAR_BADGE], &[COLUMN]);
        place.facts_are(
            "p",
            &serde_json::json!({
                "badges": {"s": said("3", 0)},
                "repo-columns": {"c": {"svc": said("ok", 0)}},
                "pad\u{7}": 1
            }),
        );
        let read = read_alone(&found, Reading::Window);
        assert_eq!(
            read.notes,
            [
                "Ext p's facts file holds \"pad\\u0007\", which is not something a facts file says — \
              it holds badges and repo-columns — so that part shows nothing"
            ]
        );
        assert_eq!(read.badges.len(), 1);
        assert_eq!(read.columns[0].cells.len(), 1);
    }

    #[test]
    fn a_field_its_manifest_does_not_declare_is_named_in_the_note() {
        let place = Place::new();
        let found = place.made("p", &[BAR_BADGE], &[COLUMN]);
        place.facts_are(
            "p",
            &serde_json::json!({
                "badges": {"s": said("3", 0), "smuggled": said("9", 0)},
                "repo-columns": {"hidden": {"svc": said("9", 0)}}
            }),
        );
        let read = read_alone(&found, Reading::Window);
        assert_eq!(
            read.notes,
            [
                "Ext p's facts file fills the badge \"smuggled\", which its manifest does not \
                 declare, so it shows nothing",
                "Ext p's facts file fills the repo column \"hidden\", which its manifest does not \
                 declare, so it shows nothing",
            ]
        );
        assert_eq!(read.badges.len(), 1);
    }

    #[test]
    fn a_badge_for_another_surface_is_left_out_without_a_note() {
        let place = Place::new();
        let found = place.made("p", &[BAR_BADGE, FOOTER_BADGE], &[COLUMN]);
        place.facts_are(
            "p",
            &serde_json::json!({
                "badges": {"s": said("3", 0), "f": said("4", 0)},
                "repo-columns": {"c": {"svc": said("ok", 0)}}
            }),
        );
        let footer = read_alone(&found, Reading::Footer);
        assert!(footer.notes.is_empty(), "{:#?}", footer.notes);
        assert!(footer.columns.is_empty());
        assert_eq!(
            footer
                .badges
                .iter()
                .map(|it| it.id.as_str())
                .collect::<Vec<_>>(),
            ["f"]
        );
        let window = read_alone(&found, Reading::Window);
        assert_eq!(
            window
                .badges
                .iter()
                .map(|it| it.id.as_str())
                .collect::<Vec<_>>(),
            ["s"]
        );
    }

    #[test]
    fn a_cell_lands_in_the_column_it_names() {
        let place = Place::new();
        let found = place.made("p", &[], &[COLUMN, SECOND_COLUMN]);
        place.facts_are(
            "p",
            &serde_json::json!({"repo-columns": {"d": {"svc": said("live", 0)}}}),
        );
        // Twice into one `Facts`, as two extensions would be: the second lands after the first.
        let mut read = Facts::default();
        read_one(&found, noon(), Reading::Window, &mut read);
        read_one(&found, noon(), Reading::Window, &mut read);
        assert!(read.notes.is_empty(), "{:#?}", read.notes);
        let filled: Vec<(&str, usize)> = read
            .columns
            .iter()
            .map(|it| (it.id.as_str(), it.cells.len()))
            .collect();
        assert_eq!(filled, [("c", 0), ("d", 1), ("c", 0), ("d", 1)]);
    }

    #[test]
    fn a_column_may_fill_as_many_repos_as_charter_draws_and_not_one_more() {
        let place = Place::new();
        let found = place.made("p", &[], &[COLUMN]);
        let repos = |count: usize| -> serde_json::Map<String, serde_json::Value> {
            (0..count)
                .map(|n| (format!("r{n}"), said("1", 0)))
                .collect()
        };
        place.facts_are(
            "p",
            &serde_json::json!({"repo-columns": {"c": repos(MOST_CELLS)}}),
        );
        let read = read_alone(&found, Reading::Window);
        assert!(read.notes.is_empty(), "{:#?}", read.notes);
        assert_eq!(read.columns[0].cells.len(), MOST_CELLS);

        place.facts_are(
            "p",
            &serde_json::json!({"repo-columns": {"c": repos(MOST_CELLS + 1)}}),
        );
        let read = read_alone(&found, Reading::Window);
        assert!(read.columns[0].cells.is_empty());
        assert_eq!(read.notes.len(), 1, "{:#?}", read.notes);
        assert!(
            read.notes[0].contains("fills 257 repos"),
            "{:#?}",
            read.notes
        );
    }

    #[test]
    fn a_value_may_be_text_or_a_number_up_to_forty_characters() {
        let now = noon();
        let at = now.timestamp();
        let text = cell(&serde_json::json!({"value": " ok ", "at": at}), 10, now).expect("text");
        assert_eq!(text.value, "ok");
        let longest = "é".repeat(MOST_VALUE_CHARS);
        let long = cell(&serde_json::json!({"value": longest, "at": at}), 10, now)
            .expect("the longest value");
        assert_eq!(long.value, longest);
        for bad in [
            serde_json::json!(""),
            serde_json::json!("x".repeat(MOST_VALUE_CHARS + 1)),
            serde_json::json!(true),
        ] {
            assert!(
                cell(&serde_json::json!({"value": bad, "at": at}), 10, now).is_err(),
                "{bad} was drawn"
            );
        }
    }

    #[test]
    fn a_value_is_stale_only_once_it_is_older_than_its_freshness() {
        let now = noon();
        let aged = |ago: i64| {
            cell(
                &serde_json::json!({"value": 1, "at": now.timestamp() - ago}),
                60,
                now,
            )
            .expect("a cell")
        };
        assert!(!aged(60).stale);
        assert!(aged(61).stale);
        assert!(aged(65).stale);
        assert_eq!(aged(65).age_seconds, 65);
    }

    #[test]
    fn a_clock_ahead_by_exactly_the_allowed_skew_is_still_now() {
        let now = noon();
        let ahead = |by: i64| {
            cell(
                &serde_json::json!({"value": 1, "at": now.timestamp() + by}),
                60,
                now,
            )
        };
        assert_eq!(ahead(MOST_SKEW_SECONDS).expect("a cell").age_seconds, 0);
        assert!(ahead(MOST_SKEW_SECONDS + 1).is_err());
    }
}
