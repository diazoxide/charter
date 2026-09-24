//! Which extensions a project has on, and what it set for them (charter-app#253, ADR 0048).
//!
//! **One function answers it, [`resolve`], and every consumer asks it**: the settings tab, the
//! window's filter on panels, views and themes for the project in front, and the executor's gate
//! before it starts a program for a view opened in that project.
//!
//! # The order, and why approval comes first
//!
//! 1. **This machine's approval** ([`super::Loaded::standing`], ADR 0041). An extension this
//!    machine has not approved contributes nothing, whatever a project says. A project's files
//!    travel — `charter.toml` with every clone — so a project may *want* an extension, and it
//!    may never *approve* one: that is ADR 0041's "an extension does not travel in a plane",
//!    kept. A project that wants one this machine has not approved reads as
//!    [`State::NeedsApproval`] and stays off.
//! 2. **Shared**, `charter.toml`'s `[extensions.<id>]`.
//! 3. **Local**, `charter.local.toml`'s `[extensions.<id>]`, which overrides Shared **key by
//!    key**: `enabled` on its own, and each setting on its own.
//!
//! With neither file naming it, an approved extension is **on** — the machine-wide list keeps
//! working exactly as it did, and a project's choices only ever narrow or name it.
//!
//! # What a file cannot do
//!
//! Neither file can install an extension, approve one, or add a setting it does not declare.
//! A value its declaration would not accept is ignored and said ([`Effective::ignored`]), and
//! the next file down is used — never a guess. A file charter cannot read or parse says
//! nothing, which leaves every extension at the machine's answer rather than turning them all
//! off: the settings tab is where that file's refusal is shown (`crate::settings`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{Setting, SettingValue};
use crate::profiles::{COMMITTED_FILE, LOCAL_FILE};

pub mod theme;

/// The table both files hold choices in.
pub const TABLE: &str = "extensions";

/// Whether the project has it on, inside `[extensions.<id>]`.
pub const ENABLED: &str = "enabled";

/// The table of values for the settings it declares, inside `[extensions.<id>]`.
pub const SETTINGS: &str = "settings";

/// Where an answer came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Neither file said: the machine's answer, or the setting's declared default.
    Default,
    /// `charter.toml`.
    Shared,
    /// `charter.local.toml`.
    Local,
}

impl Source {
    /// The file this source is, or `None` for [`Source::Default`].
    pub fn file(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Shared => Some(COMMITTED_FILE),
            Self::Local => Some(LOCAL_FILE),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Shared => "shared",
            Self::Local => "local",
        }
    }
}

/// What an extension is in this project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Approved here, and the project has it on: it contributes.
    On,
    /// The project turned it off.
    Off,
    /// The project wants it and this machine has not approved it — or it changed since it
    /// was approved. It contributes nothing until it is approved here.
    NeedsApproval,
    /// A project file names it and this machine has not installed it.
    NotInstalled,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::NeedsApproval => "needs-approval",
            Self::NotInstalled => "not-installed",
        }
    }
}

/// What one file says about one extension.
#[derive(Debug, Clone, Default, PartialEq)]
struct Said {
    enabled: Option<bool>,
    settings: BTreeMap<String, toml::Value>,
}

/// What a project's two files say about extensions, by id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Choices {
    shared: BTreeMap<String, Said>,
    local: BTreeMap<String, Said>,
}

impl Choices {
    /// The two files' text, as they would be read: `None` for a file that is not there.
    pub fn from_text(shared: Option<&str>, local: Option<&str>) -> Self {
        Self {
            shared: shared.map(said_in).unwrap_or_default(),
            local: local.map(said_in).unwrap_or_default(),
        }
    }

    /// The plane at `root`'s two files. A file that cannot be read says nothing.
    pub fn read(root: &Path) -> Self {
        let text = |name: &str| std::fs::read_to_string(root.join(name)).ok();
        Self::from_text(text(COMMITTED_FILE).as_deref(), text(LOCAL_FILE).as_deref())
    }

    /// Every id either file names.
    fn named(&self) -> BTreeSet<&str> {
        self.shared
            .keys()
            .chain(self.local.keys())
            .map(String::as_str)
            .collect()
    }
}

/// Every well-formed `[extensions.<id>]` in `text`. What is not well formed is left out here
/// and refused by [`refusals`], in words.
fn said_in(text: &str) -> BTreeMap<String, Said> {
    let Ok(top) = text.parse::<toml::Table>() else {
        return BTreeMap::new();
    };
    let Some(table) = top.get(TABLE).and_then(toml::Value::as_table) else {
        return BTreeMap::new();
    };
    table
        .iter()
        .filter(|(id, _)| id_ok(id))
        .filter_map(|(id, one)| {
            let one = one.as_table()?;
            let said = Said {
                enabled: one.get(ENABLED).and_then(toml::Value::as_bool),
                settings: one
                    .get(SETTINGS)
                    .and_then(toml::Value::as_table)
                    .map(|settings| {
                        settings
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            Some((id.clone(), said))
        })
        .collect()
}

/// One extension as this machine knows it — what [`resolve`] needs and nothing more.
#[derive(Debug, Clone)]
pub struct Installed {
    pub id: String,
    /// What a person calls it.
    pub name: String,
    /// Whether this machine approved it and it is still what was approved. Where only the record
    /// was read, whether the record holds a yes; the executor re-takes the fingerprint anyway.
    pub approved: bool,
    /// What it declares a project may set.
    pub settings: Vec<Setting>,
}

impl Installed {
    /// Every extension a survey found, with its standing as the survey took it.
    pub fn from_survey(seen: &super::Survey) -> Vec<Self> {
        seen.installed
            .iter()
            .map(|row| Self {
                id: row.id.clone(),
                name: row
                    .found
                    .as_ref()
                    .map_or_else(|| row.id.clone(), |found| found.manifest.name.clone()),
                approved: row.standing.may_contribute(),
                settings: row
                    .found
                    .as_ref()
                    .map(|found| found.manifest.settings.clone())
                    .unwrap_or_default(),
            })
            .collect()
    }

    /// Every extension the record names, approved when the record holds a yes — the answer
    /// without reading a single extension's directory. Its settings are not known, so it is for
    /// asking what is on and nothing else.
    pub fn from_record(loaded: &super::Loaded) -> Vec<Self> {
        loaded
            .registry
            .entries
            .iter()
            .map(|(id, entry)| Self {
                id: id.clone(),
                name: id.clone(),
                approved: entry.approved.is_some(),
                settings: Vec::new(),
            })
            .collect()
    }
}

/// One setting's value in this project, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub key: String,
    pub value: SettingValue,
    pub source: Source,
}

/// One extension, in this project.
#[derive(Debug, Clone)]
pub struct Effective {
    pub id: String,
    /// Its name, or its id when this machine has not installed it.
    pub name: String,
    pub state: State,
    /// Which file decided [`Self::state`] — the one that said `enabled`, or neither.
    pub source: Source,
    /// Every setting it declares, resolved.
    pub settings: Vec<Resolved>,
    /// Each value a file set that charter did not use, and why.
    pub ignored: Vec<Ignored>,
}

/// A value a file set that charter did not use: which file, and the sentence that says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    pub source: Source,
    pub why: String,
}

impl Effective {
    /// Whether it contributes in this project.
    pub fn is_on(&self) -> bool {
        self.state == State::On
    }

    /// Its settings as its program is handed them: an object, by key.
    pub fn settings_json(&self) -> serde_json::Value {
        serde_json::Value::Object(
            self.settings
                .iter()
                .map(|it| (it.key.clone(), it.value.to_json()))
                .collect(),
        )
    }
}

/// **The precedence, in one place**: every installed extension, and every one a project file
/// names, with its state in this project, the file that decided it, and its settings.
///
/// In id order.
pub fn resolve(installed: &[Installed], choices: &Choices) -> Vec<Effective> {
    let mut ids: BTreeSet<&str> = choices.named();
    ids.extend(installed.iter().map(|it| it.id.as_str()));
    ids.into_iter()
        .map(|id| one(id, installed.iter().find(|it| it.id == id), choices))
        .collect()
}

fn one(id: &str, here: Option<&Installed>, choices: &Choices) -> Effective {
    let shared = choices.shared.get(id);
    let local = choices.local.get(id);
    let (wanted, source) = match (
        local.and_then(|said| said.enabled),
        shared.and_then(|said| said.enabled),
    ) {
        (Some(on), _) => (on, Source::Local),
        (None, Some(on)) => (on, Source::Shared),
        (None, None) => (true, Source::Default),
    };
    let state = match here {
        _ if !wanted => State::Off,
        None => State::NotInstalled,
        Some(it) if !it.approved => State::NeedsApproval,
        Some(_) => State::On,
    };

    let mut ignored = Vec::new();
    let mut settings = Vec::new();
    if let Some(it) = here {
        for declared in &it.settings {
            settings.push(setting(id, declared, shared, local, &mut ignored));
        }
        // A key a file sets that the extension does not declare is said once per file: it
        // reaches nothing, and a form that silently dropped it would read as a value in force.
        for (source, file, said) in [
            (Source::Local, LOCAL_FILE, local),
            (Source::Shared, COMMITTED_FILE, shared),
        ] {
            for key in said.map(|said| said.settings.keys()).into_iter().flatten() {
                if !it.settings.iter().any(|declared| &declared.key == key) {
                    ignored.push(Ignored {
                        source,
                        why: format!(
                            "{file} sets {TABLE}.{id}.{SETTINGS}.{key}, which {id} does not \
                             declare — charter hands it nothing"
                        ),
                    });
                }
            }
        }
    }

    Effective {
        id: id.to_owned(),
        name: here.map_or_else(|| id.to_owned(), |it| it.name.clone()),
        state,
        source,
        settings,
        ignored,
    }
}

/// One declared setting: Local's value if it is one the setting accepts, else Shared's, else
/// the default — saying each value that was passed over.
fn setting(
    id: &str,
    declared: &Setting,
    shared: Option<&Said>,
    local: Option<&Said>,
    ignored: &mut Vec<Ignored>,
) -> Resolved {
    let key = &declared.key;
    let mut candidates = [
        (Source::Local, LOCAL_FILE, local),
        (Source::Shared, COMMITTED_FILE, shared),
    ]
    .into_iter()
    .filter_map(|(source, file, said)| {
        said.and_then(|said| said.settings.get(key))
            .map(|value| (source, file, value))
    })
    .peekable();
    while let Some((source, file, value)) = candidates.next() {
        match declared.accepts(value) {
            Ok(value) => {
                return Resolved {
                    key: key.clone(),
                    value,
                    source,
                };
            }
            Err(why) => {
                let instead = match candidates.peek() {
                    Some((_, next, _)) => format!("the value from {next}"),
                    None => "its default".to_owned(),
                };
                ignored.push(Ignored {
                    source,
                    why: format!(
                        "{file} sets {TABLE}.{id}.{SETTINGS}.{key} to {value}, {why} — so \
                         {instead} is used"
                    ),
                });
            }
        }
    }
    Resolved {
        key: key.clone(),
        value: declared.default.clone(),
        source: Source::Default,
    }
}

/// Whether `id` could be an extension's id — the rule [`super::parse`] holds a manifest to.
pub fn id_ok(id: &str) -> bool {
    crate::contain::segment_ok(id)
        && id.chars().all(super::ok_in_an_id)
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
}

/// Whether `key` could be a setting's key.
pub fn key_ok(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && key.starts_with(|c: char| c.is_ascii_alphanumeric())
}

/// Everything in `text`'s `[extensions]` that charter would not read, as `file` holds it, one
/// sentence each. Empty when there is nothing to refuse, and when the text is not TOML at all —
/// that is the file's own reader's refusal, not this one's.
pub fn refusals(text: &str, file: &str) -> Vec<String> {
    let Ok(top) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(table) = top.get(TABLE) else {
        return Vec::new();
    };
    let shape = "each extension is [extensions.<id>], holding enabled and \
                 [extensions.<id>.settings]";
    let Some(table) = table.as_table() else {
        return vec![format!("{TABLE} in {file} is not a table — {shape}")];
    };
    let mut out = Vec::new();
    for (id, one) in table {
        if !id_ok(id) {
            out.push(format!(
                "[{TABLE}.{}] in {file} is not an extension id — an id is letters, digits, '-', \
                 '_' and '.', starting with a letter or a digit",
                toml_key(id)
            ));
            continue;
        }
        let Some(one) = one.as_table() else {
            out.push(format!("{TABLE}.{id} in {file} is not a table — {shape}"));
            continue;
        };
        for (key, value) in one {
            match key.as_str() {
                ENABLED if !value.is_bool() => {
                    out.push(format!(
                        "{TABLE}.{id}.{ENABLED} in {file} is not true or false"
                    ));
                }
                ENABLED => {}
                SETTINGS => match value.as_table() {
                    None => out.push(format!(
                        "{TABLE}.{id}.{SETTINGS} in {file} is not a table of settings"
                    )),
                    Some(settings) => {
                        for (name, value) in settings {
                            if !key_ok(name) {
                                out.push(format!(
                                    "{TABLE}.{id}.{SETTINGS}.{} in {file} is not a setting's key \
                                     — a key is letters, digits, '-' and '_'",
                                    toml_key(name)
                                ));
                            } else if !(value.is_bool() || value.is_str()) {
                                out.push(format!(
                                    "{TABLE}.{id}.{SETTINGS}.{name} in {file} is not a value a \
                                     setting can hold — a setting is true, false or text"
                                ));
                            }
                        }
                    }
                },
                other => out.push(format!(
                    "{TABLE}.{id}.{} in {file} is not read — [{TABLE}.<id>] holds {ENABLED} and \
                     {SETTINGS} and nothing else",
                    toml_key(other)
                )),
            }
        }
    }
    out
}

/// A key as TOML would write it: bare when it can be, quoted when it cannot.
fn toml_key(key: &str) -> String {
    toml_edit::Key::new(key).display_repr().into_owned()
}

#[cfg(test)]
mod tests;
