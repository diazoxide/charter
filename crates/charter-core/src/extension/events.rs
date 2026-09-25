//! Events: an extension hears what happened, once it has (charter-app#343, ADR 0053).
//!
//! # Declared
//!
//! A manifest that asks for the `events` capability says which events it hears in
//! `contributes.events`, and may name one folder it keeps inside each workspace:
//!
//! ```json
//! "events": { "hears": ["workspace-created", "plane-saved"], "workspace_folder": "todos-ext" }
//! ```
//!
//! The words are [`Kind::EVERY`]. All of it is inside the manifest's bytes, so the fingerprint
//! covers it and the approval prompt names each event ([`declares`]).
//!
//! # Delivered after, and never in the way
//!
//! [`deliver`] is called by the surface that did the action — the app's command, or the
//! `charter` binary — **after the action has completed**, and never by the action itself. It
//! asks each approved extension that hears the event and is on in the project one question
//! ([`crate::executor::Executor::tell`]), all of them at once, each with the normal deadline. It
//! answers with notes: one sentence, naming the extension, for each one that could not be told.
//! Nothing it returns is a result the action could have had, so there is nothing for it to
//! change — the action's own answer was already decided, and the caller shows these beside it.
//!
//! **Never in the action's way.** The app runs it on a thread it starts once a command has its
//! answer, off the command's path. The `charter` binary runs it after it has printed and
//! flushed its own answer, before the process ends — so a caller waiting for the process waits
//! at most one deadline, and only for a slow extension that hears it (`crates/charter-cli`,
//! ADR 0041's amendment of 2026-09-25). A machine with no extension hearing the event pays one
//! read of the record and one manifest read per approved extension.
//!
//! # A fork carries an extension's folder whether or not it is on
//!
//! `workspace_folder` is data the extension keeps in the plane, beside the workspace's memory
//! and todos, and **a fork copies it** ([`carried`], `wscmd::fork`) — whether or not the
//! extension is on in the project, because a project that turned an extension off for now has
//! not decided its data should be left behind by the next fork. It is copied only for an
//! extension this machine approved, as it is on disk now: the name comes from a manifest, and a
//! manifest nobody approved names nothing charter acts on.

use std::path::Path;

use super::project::Choices;
use crate::executor::Executor;

/// One kind of event, as a manifest names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    WorkspaceFocused,
    WorkspaceCreated,
    WorkspaceForked,
    WorkspaceRemoved,
    HandoffCreated,
    SessionStarted,
    PlaneSaved,
}

impl Kind {
    /// Every event there is, in the order a prompt lists them.
    pub const EVERY: [Self; 7] = [
        Self::WorkspaceFocused,
        Self::WorkspaceCreated,
        Self::WorkspaceForked,
        Self::WorkspaceRemoved,
        Self::HandoffCreated,
        Self::SessionStarted,
        Self::PlaneSaved,
    ];

    /// The word a manifest writes, and the word a request carries as `event`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceFocused => "workspace-focused",
            Self::WorkspaceCreated => "workspace-created",
            Self::WorkspaceForked => "workspace-forked",
            Self::WorkspaceRemoved => "workspace-removed",
            Self::HandoffCreated => "handoff-created",
            Self::SessionStarted => "session-started",
            Self::PlaneSaved => "plane-saved",
        }
    }

    /// What the prompt and a note call it.
    pub fn said(self) -> &'static str {
        match self {
            Self::WorkspaceFocused => "a workspace being focused",
            Self::WorkspaceCreated => "a workspace being created",
            Self::WorkspaceForked => "a workspace being forked",
            Self::WorkspaceRemoved => "a workspace being removed",
            Self::HandoffCreated => "a handoff being created",
            Self::SessionStarted => "a chat starting",
            Self::PlaneSaved => "the plane being saved",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        Self::EVERY.into_iter().find(|it| it.as_str() == word)
    }
}

/// One thing that happened, with what an extension is told about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The operator brought a workspace to the front in the window.
    WorkspaceFocused {
        workspace: String,
    },
    WorkspaceCreated {
        workspace: String,
    },
    /// `workspace` is the fork, `from` the workspace it was forked from.
    WorkspaceForked {
        workspace: String,
        from: String,
    },
    WorkspaceRemoved {
        workspace: String,
    },
    /// A handoff was written for a new chat in `workspace`.
    HandoffCreated {
        workspace: String,
    },
    /// A chat started in `workspace` (`charter hook sessionstart`).
    SessionStarted {
        workspace: String,
    },
    PlaneSaved,
}

impl Event {
    pub fn kind(&self) -> Kind {
        match self {
            Self::WorkspaceFocused { .. } => Kind::WorkspaceFocused,
            Self::WorkspaceCreated { .. } => Kind::WorkspaceCreated,
            Self::WorkspaceForked { .. } => Kind::WorkspaceForked,
            Self::WorkspaceRemoved { .. } => Kind::WorkspaceRemoved,
            Self::HandoffCreated { .. } => Kind::HandoffCreated,
            Self::SessionStarted { .. } => Kind::SessionStarted,
            Self::PlaneSaved => Kind::PlaneSaved,
        }
    }

    /// The workspace it happened in, when it happened in one.
    pub fn workspace(&self) -> Option<&str> {
        match self {
            Self::WorkspaceFocused { workspace }
            | Self::WorkspaceCreated { workspace }
            | Self::WorkspaceForked { workspace, .. }
            | Self::WorkspaceRemoved { workspace }
            | Self::HandoffCreated { workspace }
            | Self::SessionStarted { workspace } => Some(workspace),
            Self::PlaneSaved => None,
        }
    }

    /// The workspace a fork was made from.
    pub fn from(&self) -> Option<&str> {
        match self {
            Self::WorkspaceForked { from, .. } => Some(from),
            _ => None,
        }
    }
}

/// What `contributes.events` declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// The events it hears, in the order [`Kind::EVERY`] has them.
    pub hears: Vec<Kind>,
    /// The one folder it keeps directly inside each workspace, which a fork copies.
    pub workspace_folder: Option<String>,
}

/// The keys `contributes.events` may carry.
const KEYS: [&str; 2] = ["hears", "workspace_folder"];

/// The names a workspace's own files and folders have, which no extension may claim: a fork
/// already carries or deliberately leaves each of them, and an extension naming one would be
/// asking charter to copy what the core decided about.
const CORE_NAMES: [&str; 11] = [
    "workspace.md",
    "workspace.json",
    "memory",
    "todos",
    "refs",
    "pieces",
    "worktrees",
    "README.md",
    "CLAUDE.md",
    "AGENTS.md",
    "manifest.json",
];

/// The events `contributes.events` declares, or why charter will not read them.
pub(super) fn declared_of(value: &serde_json::Value) -> Result<Declared, String> {
    let object = value
        .as_object()
        .ok_or("has a 'contributes.events' that is not an object")?;
    if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!(
            "declares 'contributes.events' carrying {key:?}, which is not part of what it may \
             say — it is {}",
            KEYS.join(", ")
        ));
    }
    let words = object
        .get("hears")
        .and_then(serde_json::Value::as_array)
        .filter(|list| !list.is_empty())
        .ok_or("declares 'contributes.events' without a list of the events it hears ('hears')")?;
    let mut hears: Vec<Kind> = Vec::with_capacity(words.len());
    for word in words {
        let word = word
            .as_str()
            .ok_or("declares an event in 'contributes.events.hears' that is not a word")?;
        let Some(kind) = Kind::parse(word) else {
            let every: Vec<&str> = Kind::EVERY.iter().map(|it| it.as_str()).collect();
            return Err(format!(
                "hears the event \"{}\", which charter does not have — it has {}",
                crate::shown::readable(word, 64),
                every.join(", ")
            ));
        };
        if hears.contains(&kind) {
            return Err(format!("hears the event \"{word}\" twice"));
        }
        hears.push(kind);
    }
    hears.sort();
    let workspace_folder = match object.get("workspace_folder") {
        None => None,
        Some(value) => {
            let name = value
                .as_str()
                .ok_or("names a 'workspace_folder' that is not a folder name")?;
            let plain = crate::contain::segment_ok(name)
                && !name.starts_with('.')
                && name.len() <= 64
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !plain {
                return Err(format!(
                    "names the workspace folder {:?}, and a workspace folder is one plain name — \
                     letters, digits, '-' and '_'",
                    crate::shown::readable(name, 64)
                ));
            }
            if CORE_NAMES.contains(&name) {
                return Err(format!(
                    "names the workspace folder {name:?}, which is one of charter's own in a \
                     workspace"
                ));
            }
            Some(name.to_owned())
        }
    };
    Ok(Declared {
        hears,
        workspace_folder,
    })
}

/// The approval prompt's lines for what `contributes.events` declares, in charter's words.
pub(super) fn declares(manifest: &super::Manifest) -> Vec<String> {
    let Some(declared) = &manifest.events else {
        return Vec::new();
    };
    let heard: Vec<&str> = declared.hears.iter().map(|it| it.said()).collect();
    let mut lines = vec![format!(
        "events it hears: {} — charter starts its program once for each, after it has \
         happened; what it answers never changes what happened",
        heard.join(", ")
    )];
    if let Some(folder) = &declared.workspace_folder {
        lines.push(format!(
            "a folder in each workspace, “{folder}/” — a fork copies it into the new \
             workspace, whether or not this extension is on there"
        ));
    }
    lines
}

/// Tell every approved extension that hears `event` and is on in `choices`' project about it,
/// all at once, and answer one note for each that could not be told. See the module docstring.
///
/// **Called after the action `event` reports has completed**, by the surface that did it.
pub fn deliver(
    executor: &Executor,
    config_root: &Path,
    choices: &Choices,
    event: &Event,
) -> Vec<String> {
    let kind = event.kind();
    let loaded = super::read(config_root);
    // An unreadable record approves nothing; the Extensions list is where it is said.
    if loaded.unreadable.is_some() {
        return Vec::new();
    }
    // Only the manifest, to learn who hears it — the fingerprint is re-taken by `tell`, as it
    // is before any program starts.
    let hearing: Vec<(String, String)> = loaded
        .registry
        .entries
        .iter()
        .filter(|(_, entry)| entry.approved.is_some())
        .filter_map(|(id, entry)| {
            let declared = super::manifest_at(&entry.path).ok()?;
            (declared.hears(kind) && super::facts::on_here(id, &declared, choices))
                .then(|| (id.clone(), declared.name))
        })
        .collect();
    if hearing.is_empty() {
        return Vec::new();
    }
    std::thread::scope(|scope| {
        let asked: Vec<_> = hearing
            .iter()
            .map(|(id, name)| {
                let told = scope.spawn(move || executor.tell(config_root, choices, id, event));
                (name, told)
            })
            .collect();
        asked
            .into_iter()
            .filter_map(|(name, told)| {
                let why = match told.join() {
                    Ok(Ok(())) => return None,
                    Ok(Err(why)) => why,
                    Err(_) => "charter's own thread asking it stopped".to_owned(),
                };
                Some(format!("{name} missed {}: {why}", happened(event)))
            })
            .collect()
    })
}

/// What happened, as a note says it.
fn happened(event: &Event) -> String {
    match event {
        Event::WorkspaceFocused { workspace } => format!("workspace '{workspace}' being focused"),
        Event::WorkspaceCreated { workspace } => format!("workspace '{workspace}' being created"),
        Event::WorkspaceForked { workspace, from } => {
            format!("workspace '{workspace}' being forked from '{from}'")
        }
        Event::WorkspaceRemoved { workspace } => format!("workspace '{workspace}' being removed"),
        Event::HandoffCreated { workspace } => {
            format!("a handoff being created in workspace '{workspace}'")
        }
        Event::SessionStarted { workspace } => {
            format!("a chat starting in workspace '{workspace}'")
        }
        Event::PlaneSaved => "the plane being saved".to_owned(),
    }
}

/// The workspace folders a fork copies: the one each extension this machine approved declares,
/// **whether or not it is on** in the project — see the module docstring. Only an extension
/// whose bytes on disk are still the ones approved names one; the rest name nothing.
pub fn carried(config_root: &Path) -> Vec<String> {
    let loaded = super::read(config_root);
    if loaded.unreadable.is_some() {
        return Vec::new();
    }
    let mut folders: Vec<String> = loaded
        .registry
        .entries
        .iter()
        .filter(|(_, entry)| entry.approved.is_some())
        .filter_map(|(id, entry)| {
            // The manifest alone first, so an extension that keeps no folder costs no hash.
            let declared = super::manifest_at(&entry.path).ok()?;
            declared.events.as_ref()?.workspace_folder.as_ref()?;
            let found = super::read_at(&entry.path).ok()?;
            let approved = found.id() == id && loaded.standing(&found) == super::Standing::Approved;
            approved
                .then(|| found.manifest.events?.workspace_folder)
                .flatten()
        })
        .collect();
    folders.sort();
    folders.dedup();
    folders
}
