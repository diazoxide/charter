//! **Which theme a project draws** (charter-app#273, ADR 0048) — the sibling of
//! [`super::resolve`], and asked with its answer — **and, in a workspace, which colour**
//! (charter-app#281).
//!
//! A project picks in `[theme] use`, in `charter.toml` (Shared) or `charter.local.toml` (Local),
//! and a workspace in `settings.theme.use` in its `workspace.json`:
//!
//! - `charter-dark` or `charter-light`, charter's own;
//! - `system`, which follows the operating system's light or dark appearance;
//! - `<extension-id>/<theme name>`, a theme an extension contributes.
//!
//! # The order
//!
//! 1. **Local**'s value, when it is one of those shapes; else **the workspace**'s; else
//!    **Shared**'s; else nothing, and the window keeps its own theme — exactly what it drew
//!    before a project could pick. A workspace refines its project for the team, and this
//!    machine's Local file has the last word, as it does for extensions (charter-app#280).
//! 2. **An extension's theme only while that extension is on in this project** — which is
//!    [`super::resolve`]'s answer, so this machine's approval still comes first. A pick whose
//!    extension is off, unapproved, not installed, or does not contribute that theme falls back
//!    to the built-in [`FALLBACK`], and [`Resolved::why`] says why in a sentence the settings tab
//!    draws. A project file can name a theme; it can never bring one.
//!
//! A value that is none of the shapes is ignored with a sentence and the next layer down is used,
//! as a setting's is ([`super::Ignored`]).
//!
//! # A workspace's colour
//!
//! `settings.theme.colour`, in a workspace's `workspace.json` and nowhere else: one of the
//! [`PALETTE`]'s names, or a colour as `#rrggbb` whose hue is taken. It is what tells one
//! workspace from another, so a project's files cannot set one. It picks no theme: the window
//! tints the accent and the tab shades of the theme it draws with that hue (`theme.ts`), and
//! leaves the text and the terminal alone.

use super::{Effective, Ignored, Source, State, WORKSPACE_AT};
use crate::extension::BUILT_IN_THEMES;
use crate::profiles::{COMMITTED_FILE, LOCAL_FILE};

/// The table a project's theme is picked in, in either file — and in a workspace's `settings`.
pub const TABLE: &str = "theme";

/// The key in [`TABLE`] that picks the theme.
pub const USE: &str = "use";

/// The key in a workspace's [`TABLE`] that gives it a colour (charter-app#281).
pub const COLOUR: &str = "colour";

/// The value that follows the operating system's appearance.
pub const SYSTEM: &str = "system";

/// The built-in a pick that cannot be drawn falls back to: the theme the window comes up in.
pub const FALLBACK: &str = "charter-dark";

/// The colours a workspace may name (charter-app#281). Each is a hue the window tints with;
/// `theme.ts` holds the hues, and a test there holds the two lists to one another.
pub const PALETTE: [&str; 8] = [
    "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink",
];

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

/// A workspace's colour (charter-app#281).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Colour {
    /// One of [`PALETTE`].
    Palette(&'static str),
    /// `#rrggbb`, as written: the window takes its hue.
    Custom(String),
}

impl Colour {
    /// `value` as a colour, or `None` when it is neither a [`PALETTE`] name nor `#rrggbb`.
    pub fn parse(value: &str) -> Option<Self> {
        if let Some(name) = PALETTE.iter().find(|name| **name == value) {
            return Some(Self::Palette(name));
        }
        let digits = value.strip_prefix('#')?;
        (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| Self::Custom(value.to_owned()))
    }

    /// The colour as the file holds it.
    pub fn value(&self) -> String {
        match self {
            Self::Palette(name) => (*name).to_owned(),
            Self::Custom(hex) => hex.clone(),
        }
    }
}

/// What a project's two files — and, in a workspace, its `workspace.json` — say about its theme:
/// each layer's `use` as found, and the workspace's `colour`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Said {
    shared: Option<toml::Value>,
    /// `None` outside a workspace, and for a workspace whose settings pick nothing.
    workspace: Option<toml::Value>,
    local: Option<toml::Value>,
    /// The workspace's `settings.theme.colour`, as found.
    colour: Option<toml::Value>,
    /// The workspace these were read in, when they were.
    workspace_name: Option<String>,
}

impl Said {
    /// The two files' text: `None` for a file that is not there.
    pub fn from_text(shared: Option<&str>, local: Option<&str>) -> Self {
        let said = |text: &str| {
            text.parse::<toml::Table>()
                .ok()
                .and_then(|top| said_in(&top, USE))
        };
        Self {
            shared: shared.and_then(said),
            local: local.and_then(said),
            ..Self::default()
        }
    }

    /// The same, in the workspace `name` whose `workspace.json` has this text (`None`: it has
    /// none). Its `settings` go between Shared and Local, as `super::Choices::in_workspace`'s do.
    pub fn in_workspace(self, name: &str, manifest: Option<&str>) -> Self {
        self.with_workspace(
            name,
            manifest.and_then(crate::settings::workspace::table_in),
        )
    }

    fn with_workspace(mut self, name: &str, settings: Option<toml::Table>) -> Self {
        self.workspace = settings.as_ref().and_then(|top| said_in(top, USE));
        self.colour = settings.as_ref().and_then(|top| said_in(top, COLOUR));
        self.workspace_name = Some(name.to_owned());
        self
    }

    /// The plane at `root`'s two files, as [`crate::settings::layer_text`] hands them: a file
    /// that cannot be read says nothing, and neither does a `charter.local.toml` git would carry
    /// (charter-app#308).
    pub fn read(root: &std::path::Path) -> Self {
        use crate::settings::{Which, layer_text};
        Self::from_text(
            layer_text(root, Which::Shared).as_deref(),
            layer_text(root, Which::Local).as_deref(),
        )
    }

    /// [`Self::read`], in `workspace` when there is one — `super::Choices::read_in`'s twin, so
    /// the theme and the extensions it depends on are read in the same place.
    pub fn read_in(root: &std::path::Path, workspace: Option<&str>) -> Self {
        let project = Self::read(root);
        match workspace {
            None => project,
            Some(name) => {
                project.with_workspace(name, crate::settings::workspace::read(root, name))
            }
        }
    }

    /// The workspace's file as a sentence names it, or `None` outside a workspace.
    pub fn workspace_file(&self) -> Option<String> {
        self.workspace_name
            .as_deref()
            .map(crate::settings::workspace::named)
    }
}

/// `[theme] <key>` in `top`: a whole TOML file, or a workspace's `settings` read as one.
fn said_in(top: &toml::Table, key: &str) -> Option<toml::Value> {
    top.get(TABLE)?.as_table()?.get(key).cloned()
}

/// One theme an approved extension contributes, as a survey found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offered {
    pub id: String,
    pub name: String,
}

/// A project's theme, in a workspace when it was asked in one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// What the layers picked, in force: `None` when none picked anything usable.
    pub picked: Option<Pick>,
    /// Which layer [`Self::picked`] came from.
    pub source: Source,
    /// What the window draws for this project: [`Self::picked`], or the built-in [`FALLBACK`]
    /// when that cannot be drawn. `None` leaves the window its own theme.
    pub draws: Option<Pick>,
    /// Why [`Self::draws`] is not [`Self::picked`], when it is not.
    pub why: Option<String>,
    /// The workspace's colour, when it has one charter reads (charter-app#281).
    pub colour: Option<Colour>,
    /// Each value a layer set that charter did not use, and why.
    pub ignored: Vec<Ignored>,
}

/// **The theme, in one place**: Local's pick over the workspace's over Shared's, and an
/// extension's only while `extensions` — [`super::resolve`]'s answer for the same project and
/// workspace — has it on. And the workspace's colour, which no other layer has.
///
/// `offered` is every theme the approved extensions contribute, when the caller surveyed them;
/// with it, a pick an extension does not contribute falls back too. Without it (the window's
/// cheap question, which reads no extension's directory) that is left to the window, which only
/// ever holds what a survey found.
pub fn resolve(extensions: &[Effective], offered: Option<&[Offered]>, said: &Said) -> Resolved {
    let mut ignored = Vec::new();
    let workspace_file = said.workspace_file().unwrap_or_default();
    let colour = colour(said, &workspace_file, &mut ignored);
    let mut candidates = [
        (Source::Local, LOCAL_FILE, "", &said.local),
        (
            Source::Workspace,
            workspace_file.as_str(),
            WORKSPACE_AT,
            &said.workspace,
        ),
        (Source::Shared, COMMITTED_FILE, "", &said.shared),
    ]
    .into_iter()
    .filter_map(|(source, file, at, value)| value.as_ref().map(|value| (source, file, at, value)))
    .peekable();
    let mut picked = None;
    while let Some((source, file, at, value)) = candidates.next() {
        if let Some(pick) = value.as_str().and_then(Pick::parse) {
            picked = Some((pick, source, file));
            break;
        }
        let instead = match candidates.peek() {
            Some((_, next, _, _)) => format!("{next}'s pick is used"),
            None => "the window keeps its own theme".to_owned(),
        };
        ignored.push(Ignored {
            source,
            why: format!("{file} sets {at}{TABLE}.{USE} to {value}, {NOT_A_PICK} — so {instead}"),
        });
    }
    let Some((pick, source, file)) = picked else {
        return Resolved {
            picked: None,
            source: Source::Default,
            draws: None,
            why: None,
            colour,
            ignored,
        };
    };
    let why = unavailable(&pick, file, extensions, offered);
    Resolved {
        draws: Some(if why.is_some() {
            Pick::BuiltIn(FALLBACK)
        } else {
            pick.clone()
        }),
        picked: Some(pick),
        source,
        why,
        colour,
        ignored,
    }
}

/// The workspace's colour, saying why a value that is not one was passed over.
fn colour(said: &Said, file: &str, ignored: &mut Vec<Ignored>) -> Option<Colour> {
    let value = said.colour.as_ref()?;
    let colour = value.as_str().and_then(Colour::parse);
    if colour.is_none() {
        ignored.push(Ignored {
            source: Source::Workspace,
            why: format!(
                "{file} sets {WORKSPACE_AT}{TABLE}.{COLOUR} to {value}, {} — so this workspace \
                 has no colour",
                not_a_colour()
            ),
        });
    }
    colour
}

/// The colour of the workspace `ws`, as [`resolve`] reads it — for the window's workspace tabs,
/// which each show their own. `None` for a workspace with no colour, or one charter does not
/// read. Handed the workspace the caller already opened, so a listing of every workspace reads
/// each manifest once.
pub fn colour_of(ws: &crate::workspaces::Workspace) -> Option<Colour> {
    let settings = ws
        .manifest()
        .0
        .as_ref()
        .and_then(crate::settings::workspace::table_of);
    let said = Said::default().with_workspace(ws.name(), settings);
    colour(&said, "", &mut Vec::new())
}

/// What a value that is not a pick is told.
pub const NOT_A_PICK: &str = "which is not charter-dark, charter-light, system or \
                              <extension>/<theme>";

/// What a value that is not a colour is told.
fn not_a_colour() -> String {
    format!("which is not {} or #rrggbb", PALETTE.join(", "))
}

/// Why `pick`, which `file` made, cannot be drawn in this project, or `None` when it can.
fn unavailable(
    pick: &Pick,
    file: &str,
    extensions: &[Effective],
    offered: Option<&[Offered]>,
) -> Option<String> {
    let Pick::Extension { id, name } = pick else {
        return None;
    };
    let it = extensions.iter().find(|it| &it.id == id);
    let because = match it.map(|it| (it.state, it.source)) {
        Some((State::On, _)) => {
            let contributes =
                offered.is_none_or(|all| all.iter().any(|one| &one.id == id && &one.name == name));
            if contributes {
                return None;
            }
            format!("{id} contributes no theme called “{name}”")
        }
        // Off in the workspace's layer is off there, not in the project (charter-app#281).
        Some((State::Off, Source::Workspace)) => format!("{id} is off in this workspace"),
        Some((State::Off, _)) => format!("{id} is off in this project"),
        Some((State::NeedsApproval, _)) => format!("this machine has not approved {id}"),
        Some((State::NotInstalled, _)) | None => {
            format!("{id} is not installed on this machine")
        }
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
            let colour = if key == COLOUR {
                "; a colour is a workspace's"
            } else {
                ""
            };
            out.push(format!(
                "{TABLE}.{} in {file} is not read — [{TABLE}] holds {USE} and nothing else{colour}",
                toml_edit::Key::new(key.as_str()).display_repr()
            ));
        } else if value.as_str().and_then(Pick::parse).is_none() {
            out.push(format!("{TABLE}.{USE} in {file} is {value}, {NOT_A_PICK}"));
        }
    }
    out
}

/// [`refusals`], for a workspace's `settings` read as the table they mirror
/// (`crate::settings::workspace`), where the theme may also hold a [`COLOUR`]. `file` is the
/// manifest as a sentence names it; each key is named by its path in the JSON.
pub fn refusals_in_workspace(top: &toml::Table, file: &str) -> Vec<String> {
    let Some(table) = top.get(TABLE) else {
        return Vec::new();
    };
    let Some(table) = table.as_table() else {
        return vec![format!(
            "{WORKSPACE_AT}{TABLE} in {file} is not an object — write \
             {{\"{USE}\": \"<theme>\", \"{COLOUR}\": \"<colour>\"}}"
        )];
    };
    let mut out = Vec::new();
    for (key, value) in table {
        let at = format!("{WORKSPACE_AT}{TABLE}.{key} in {file}");
        match key.as_str() {
            USE if value.as_str().and_then(Pick::parse).is_none() => {
                out.push(format!("{at} is {value}, {NOT_A_PICK}"));
            }
            COLOUR if value.as_str().and_then(Colour::parse).is_none() => {
                out.push(format!("{at} is {value}, {}", not_a_colour()));
            }
            USE | COLOUR => {}
            _ => out.push(format!(
                "{at} is not read — a workspace's {TABLE} holds {USE} and {COLOUR} and nothing \
                 else"
            )),
        }
    }
    out
}

#[cfg(test)]
mod tests;
