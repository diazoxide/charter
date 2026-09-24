//! **Which theme a project draws** (charter-app#273, ADR 0048) — the sibling of
//! [`super::resolve`], and asked with its answer.
//!
//! A project picks in `[theme] use`, in `charter.toml` (Shared) or `charter.local.toml` (Local):
//!
//! - `charter-dark` or `charter-light`, charter's own;
//! - `system`, which follows the operating system's light or dark appearance;
//! - `<extension-id>/<theme name>`, a theme an extension contributes.
//!
//! # The order
//!
//! 1. **Local**'s value, when it is one of those shapes; else **Shared**'s; else nothing, and the
//!    window keeps its own theme — exactly what it drew before a project could pick.
//! 2. **An extension's theme only while that extension is on in this project** — which is
//!    [`super::resolve`]'s answer, so this machine's approval still comes first. A pick whose
//!    extension is off, unapproved, not installed, or does not contribute that theme falls back
//!    to the built-in [`FALLBACK`], and [`Resolved::why`] says why in a sentence the settings tab
//!    draws. A project file can name a theme; it can never bring one.
//!
//! A value that is none of the shapes is ignored with a sentence and the next file down is used,
//! as a setting's is ([`super::Ignored`]).

use super::{Effective, Ignored, Source, State};
use crate::extension::BUILT_IN_THEMES;
use crate::profiles::{COMMITTED_FILE, LOCAL_FILE};

/// The table a project's theme is picked in, in either file.
pub const TABLE: &str = "theme";

/// The one key in [`TABLE`].
pub const USE: &str = "use";

/// The value that follows the operating system's appearance.
pub const SYSTEM: &str = "system";

/// The built-in a pick that cannot be drawn falls back to: the theme the window comes up in.
pub const FALLBACK: &str = "charter-dark";

/// What a project can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// One of [`BUILT_IN_THEMES`].
    BuiltIn(&'static str),
    /// charter's light theme when the system is light, its dark one otherwise.
    System,
    /// A theme an extension contributes, by the extension's id and the theme's name.
    Extension { id: String, name: String },
}

impl Pick {
    /// `value` as a pick, or `None` when it is not one of the shapes.
    pub fn parse(value: &str) -> Option<Self> {
        if value == SYSTEM {
            return Some(Self::System);
        }
        if let Some(built) = BUILT_IN_THEMES.iter().find(|name| **name == value) {
            return Some(Self::BuiltIn(built));
        }
        let (id, name) = value.split_once('/')?;
        (super::id_ok(id) && !name.trim().is_empty()).then(|| Self::Extension {
            id: id.to_owned(),
            name: name.to_owned(),
        })
    }

    /// The pick as a file holds it.
    pub fn value(&self) -> String {
        match self {
            Self::BuiltIn(name) => (*name).to_owned(),
            Self::System => SYSTEM.to_owned(),
            Self::Extension { id, name } => format!("{id}/{name}"),
        }
    }
}

/// What a project's two files say about its theme: each file's `[theme] use`, as found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Said {
    shared: Option<toml::Value>,
    local: Option<toml::Value>,
}

impl Said {
    /// The two files' text: `None` for a file that is not there.
    pub fn from_text(shared: Option<&str>, local: Option<&str>) -> Self {
        Self {
            shared: shared.and_then(said_in),
            local: local.and_then(said_in),
        }
    }

    /// The plane at `root`'s two files. A file that cannot be read says nothing.
    pub fn read(root: &std::path::Path) -> Self {
        let text = |name: &str| std::fs::read_to_string(root.join(name)).ok();
        Self::from_text(text(COMMITTED_FILE).as_deref(), text(LOCAL_FILE).as_deref())
    }
}

fn said_in(text: &str) -> Option<toml::Value> {
    text.parse::<toml::Table>()
        .ok()?
        .get(TABLE)?
        .as_table()?
        .get(USE)
        .cloned()
}

/// One theme an approved extension contributes, as a survey found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offered {
    pub id: String,
    pub name: String,
}

/// A project's theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// What the files picked, in force: `None` when neither picked anything usable.
    pub picked: Option<Pick>,
    /// Which file [`Self::picked`] came from.
    pub source: Source,
    /// What the window draws for this project: [`Self::picked`], or the built-in [`FALLBACK`]
    /// when that cannot be drawn. `None` leaves the window its own theme.
    pub draws: Option<Pick>,
    /// Why [`Self::draws`] is not [`Self::picked`], when it is not.
    pub why: Option<String>,
    /// Each value a file set that charter did not use, and why.
    pub ignored: Vec<Ignored>,
}

/// **The theme, in one place**: Local's pick over Shared's, and an extension's only while
/// `extensions` — [`super::resolve`]'s answer for the same project — has it on.
///
/// `offered` is every theme the approved extensions contribute, when the caller surveyed them;
/// with it, a pick an extension does not contribute falls back too. Without it (the window's
/// cheap question, which reads no extension's directory) that is left to the window, which only
/// ever holds what a survey found.
pub fn resolve(extensions: &[Effective], offered: Option<&[Offered]>, said: &Said) -> Resolved {
    let mut ignored = Vec::new();
    let mut candidates = [
        (Source::Local, LOCAL_FILE, &said.local),
        (Source::Shared, COMMITTED_FILE, &said.shared),
    ]
    .into_iter()
    .filter_map(|(source, file, value)| value.as_ref().map(|value| (source, file, value)))
    .peekable();
    let mut picked = None;
    while let Some((source, file, value)) = candidates.next() {
        if let Some(pick) = value.as_str().and_then(Pick::parse) {
            picked = Some((pick, source));
            break;
        }
        let instead = match candidates.peek() {
            Some((_, next, _)) => format!("{next}'s pick is used"),
            None => "the window keeps its own theme".to_owned(),
        };
        ignored.push(Ignored {
            source,
            why: format!("{file} sets {TABLE}.{USE} to {value}, {NOT_A_PICK} — so {instead}"),
        });
    }
    let Some((pick, source)) = picked else {
        return Resolved {
            picked: None,
            source: Source::Default,
            draws: None,
            why: None,
            ignored,
        };
    };
    let why = unavailable(&pick, source, extensions, offered);
    Resolved {
        draws: Some(if why.is_some() {
            Pick::BuiltIn(FALLBACK)
        } else {
            pick.clone()
        }),
        picked: Some(pick),
        source,
        why,
        ignored,
    }
}

/// What a value that is not a pick is told.
pub const NOT_A_PICK: &str = "which is not charter-dark, charter-light, system or \
                              <extension>/<theme>";

/// Why `pick` cannot be drawn in this project, or `None` when it can.
fn unavailable(
    pick: &Pick,
    source: Source,
    extensions: &[Effective],
    offered: Option<&[Offered]>,
) -> Option<String> {
    let Pick::Extension { id, name } = pick else {
        return None;
    };
    let file = source.file().unwrap_or(COMMITTED_FILE);
    let because = match extensions.iter().find(|it| &it.id == id).map(|it| it.state) {
        Some(State::On) => {
            let contributes =
                offered.is_none_or(|all| all.iter().any(|one| &one.id == id && &one.name == name));
            if contributes {
                return None;
            }
            format!("{id} contributes no theme called “{name}”")
        }
        Some(State::Off) => format!("{id} is off in this project"),
        Some(State::NeedsApproval) => format!("this machine has not approved {id}"),
        Some(State::NotInstalled) | None => format!("{id} is not installed on this machine"),
    };
    Some(format!(
        "{file} picks “{name}” from {id}, but {because} — so the built-in {FALLBACK} is drawn"
    ))
}

/// Everything in `text`'s `[theme]` that charter would not read, as `file` holds it, one sentence
/// each — what the Project settings tab refuses to save. Empty when the text is not TOML at all:
/// that is the file's own reader's refusal.
pub fn refusals(text: &str, file: &str) -> Vec<String> {
    let Ok(top) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(table) = top.get(TABLE) else {
        return Vec::new();
    };
    let Some(table) = table.as_table() else {
        return vec![format!(
            "{TABLE} in {file} is not a table — write [{TABLE}] with {USE} = \"<theme>\""
        )];
    };
    let mut out = Vec::new();
    for (key, value) in table {
        if key != USE {
            out.push(format!(
                "{TABLE}.{} in {file} is not read — [{TABLE}] holds {USE} and nothing else",
                toml_edit::Key::new(key.as_str()).display_repr()
            ));
        } else if value.as_str().and_then(Pick::parse).is_none() {
            out.push(format!("{TABLE}.{USE} in {file} is {value}, {NOT_A_PICK}"));
        }
    }
    out
}

#[cfg(test)]
mod tests;
