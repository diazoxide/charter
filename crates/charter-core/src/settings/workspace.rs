//! A workspace's settings: the `settings` object in `workspaces/<ws>/workspace.json`
//! (charter-app#280, ADR 0048).
//!
//! **One layer, read by the readers that read the project's two files.** A workspace refines its
//! project for the team, so its settings sit between Shared (`charter.toml`) and Local
//! (`charter.local.toml`) — and they have the same shape as those files' tables:
//! `settings.extensions.<id>.enabled` is `[extensions.<id>] enabled`. So the JSON is read as the
//! TOML table it mirrors ([`table_in`]) and handed to the same reader,
//! `extension::project`, which refuses it in the same words. A table a later reader takes
//! (a theme, harness plugins) is one more name in [`READ`] and one more reader asked here.
//!
//! **A save keeps the manifest the operator has.** Only `settings` changes; every other key keeps
//! its place and its value (`serde_json`'s `preserve_order`). A manifest charter wrote is stamped
//! again, so it stays charter's; one a hand wrote is written unstamped, so it stays the
//! operator's and the automatic writers keep leaving it alone (`crate::manifest`). A key removed
//! takes every object it leaves empty with it, so removing the last setting gives back the
//! manifest as it was.

use std::path::Path;

use serde_json::{Map, Value as Json};

use super::{Edit, Found, Step, Value};
use crate::extension::project;
use crate::manifest::Ownership;
use crate::workspaces::{Plane, Workspace};

/// The workspace's file.
pub const FILE: &str = "workspace.json";

/// The key in it that holds its settings.
pub const KEY: &str = "settings";

/// The tables a workspace's settings may hold: the ones something reads.
pub const READ: &[&str] = &[project::TABLE];

/// The file as a sentence names it: `workspaces/<ws>/workspace.json`.
pub fn named(workspace: &str) -> String {
    format!("workspaces/{workspace}/{FILE}")
}

/// The `settings` of the manifest whose text this is, as the TOML table it mirrors — or `None`
/// when it is not JSON or has none. A `null` is left out: it reads as not set.
pub fn table_in(manifest: &str) -> Option<toml::Table> {
    serde_json::from_str::<Json>(manifest)
        .ok()
        .as_ref()
        .and_then(table_of)
}

/// [`table_in`], of a manifest already read.
fn table_of(doc: &Json) -> Option<toml::Table> {
    match doc.get(KEY).and_then(to_toml)? {
        toml::Value::Table(table) => Some(table),
        _ => None,
    }
}

fn to_toml(value: &Json) -> Option<toml::Value> {
    Some(match value {
        Json::Null => return None,
        Json::Bool(b) => toml::Value::Boolean(*b),
        Json::String(text) => toml::Value::String(text.clone()),
        Json::Number(n) => match n.as_i64() {
            Some(n) => toml::Value::Integer(n),
            None => toml::Value::Float(n.as_f64()?),
        },
        Json::Array(items) => toml::Value::Array(items.iter().filter_map(to_toml).collect()),
        Json::Object(map) => toml::Value::Table(
            map.iter()
                .filter_map(|(key, value)| Some((key.clone(), to_toml(value)?)))
                .collect(),
        ),
    })
}

/// The settings of the workspace `workspace` of the plane at `root`, as [`table_in`] reads
/// them. `None` for a name that is not a workspace's, a manifest that is not there or cannot be
/// read, and one with no settings.
pub fn read(root: &Path, workspace: &str) -> Option<toml::Table> {
    let (doc, _) = Plane::open(root).workspace(workspace).ok()?.manifest();
    table_of(&doc?)
}

/// Everything charter would not read in the settings of the manifest whose text this is, one
/// sentence each. Empty when there is nothing to refuse, and when it has no settings.
pub fn refusals(text: &str, workspace: &str) -> Vec<String> {
    let file = named(workspace);
    let Ok(Json::Object(doc)) = serde_json::from_str::<Json>(text) else {
        return vec![format!(
            "{file} is not a JSON object, so charter reads no settings from it — mend it by hand"
        )];
    };
    let Some(settings) = doc.get(KEY) else {
        return Vec::new();
    };
    let Some(settings) = settings.as_object() else {
        return vec![format!(
            "{KEY} in {file} is not an object — a workspace's settings are \
             {{\"{}\": {{\"<id>\": {{\"{}\": …, \"{}\": {{…}}}}}}}}",
            project::TABLE,
            project::ENABLED,
            project::SETTINGS
        )];
    };
    let mut out: Vec<String> = settings
        .keys()
        .filter(|key| !READ.contains(&key.as_str()))
        .map(|key| {
            format!(
                "{KEY}.{key} in {file} is not read — a workspace's settings hold {} and nothing \
                 else",
                READ.join(", ")
            )
        })
        .collect();
    if let Some(table) = to_toml(&Json::Object(settings.clone())).and_then(|t| match t {
        toml::Value::Table(table) => Some(table),
        _ => None,
    }) {
        out.extend(project::refusals_in(&table, &file));
    }
    out
}

/// One workspace's manifest as the Workspace settings tab reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    /// `workspaces/<ws>/workspace.json`.
    pub file: String,
    /// Whether it is there. One that is not is created by the first save.
    pub exists: bool,
    /// Its text, or empty — what a save is checked against.
    pub text: String,
    /// What charter does not take from its settings as they stand.
    pub refusals: Vec<String>,
    /// Whether it is a JSON object, which is what a form can change.
    pub parsed: bool,
    /// Every value in its settings and the path to it, as `settings::fields` gives a TOML
    /// file's.
    pub fields: Vec<(Vec<Step>, Found)>,
    /// Whether the workspace is LIVE, so its manifest is committed and the team sees it.
    pub live: bool,
}

/// The workspace `workspace` of the plane at `root`, or the sentence for a name that is not one
/// — or for one that is not there any more, whose directory a save must never make again.
fn workspace_of(root: &Path, workspace: &str) -> Result<Workspace, String> {
    let ws = Plane::open(root)
        .workspace(workspace)
        .map_err(|_| format!("'{workspace}' is not a workspace charter can name"))?;
    if !ws.dir().is_dir() {
        return Err(format!(
            "there is no workspace '{workspace}' in this plane any more, so it has no settings"
        ));
    }
    Ok(ws)
}

/// The manifest's text, `None` when it is not there, or why it could not be read.
fn on_disk(ws: &Workspace) -> Result<Option<String>, String> {
    let file = named(ws.name());
    ws.manifest_text().map_err(|e| {
        format!(
            "{file} could not be read ({})",
            crate::shown::short(&e.to_string())
        )
    })
}

/// The manifest of `workspace`, and what charter says about its settings now.
pub fn read_file(root: &Path, workspace: &str) -> Result<Read, String> {
    let ws = workspace_of(root, workspace)?;
    let text = on_disk(&ws)?;
    let exists = text.is_some();
    let text = text.unwrap_or_default();
    let doc = serde_json::from_str::<Json>(&text).ok();
    let parsed = !exists || doc.as_ref().is_some_and(Json::is_object);
    let fields = doc
        .as_ref()
        .and_then(table_of)
        .map(|table| super::fields_of(&table))
        .unwrap_or_default();
    Ok(Read {
        file: named(workspace),
        refusals: if exists {
            refusals(&text, workspace)
        } else {
            Vec::new()
        },
        exists,
        text,
        parsed,
        fields,
        live: Plane::open(root).is_live(workspace),
    })
}

/// Apply `edits` to the settings of `workspace`'s manifest and write it — or say every reason it
/// was not written.
///
/// `base` is the manifest as the caller read it (`None`: it was not there). One that changed
/// since is not written over. A save is refused for what the reader would refuse in the settings
/// it would write, except what they already held, and for a secret-shaped value, whatever they
/// held.
pub fn save(
    root: &Path,
    workspace: &str,
    base: Option<&str>,
    edits: &[Edit],
) -> Result<(), Vec<String>> {
    let ws = workspace_of(root, workspace).map_err(|why| vec![why])?;
    let file = named(workspace);
    let now = on_disk(&ws).map_err(|why| vec![why])?;
    if now.as_deref() != base {
        return Err(vec![format!(
            "{file} changed on disk since this tab read it, so nothing was saved. Read it again, \
             then make the change again."
        )]);
    }
    let (mut doc, standing) = match &now {
        Some(text) => match serde_json::from_str::<Json>(text) {
            Ok(Json::Object(doc)) => (doc, refusals(text, workspace)),
            _ => return Err(refusals(text, workspace)),
        },
        None => (birth(&ws), Vec::new()),
    };
    let mut settings = match doc.get(KEY) {
        Some(Json::Object(settings)) => settings.clone(),
        None => Map::new(),
        Some(_) => return Err(refusals(now.as_deref().unwrap_or_default(), workspace)),
    };
    for edit in edits {
        apply(&mut settings, edit)?;
    }
    // In the place it had — `insert` keeps an existing key's — and `shift_remove`, not
    // `remove`, which would swap the last key into its place.
    if settings.is_empty() {
        doc.shift_remove(KEY);
    } else {
        doc.insert(KEY.to_owned(), Json::Object(settings.clone()));
    }
    let doc = Json::Object(doc);
    let written = crate::pyjson::dumps_indent2(&doc);
    let mut refused: Vec<String> = refusals(&written, workspace)
        .into_iter()
        .filter(|why| !standing.contains(why))
        .collect();
    if let Some(kind) =
        crate::secretshape::secret_kind(&crate::pyjson::dumps_indent2(&Json::Object(settings)))
    {
        refused.push(format!(
            "{file} looks like it holds a secret ({kind}), so nothing was saved — it is \
             committed with a LIVE workspace, so every clone of this plane would carry the \
             secret. Keep the value in a vault and name it where it is needed as \
             vault:<vault>/<key>."
        ));
    }
    if !refused.is_empty() {
        return Err(refused);
    }
    // A manifest a hand wrote stays the hand's: it is written without charter's stamp.
    let stamped = crate::manifest::ownership(now.as_deref()) != Ownership::Operator;
    ws.write_manifest_as(&doc, stamped).map_err(|e| {
        vec![format!(
            "{file} could not be written ({}), so nothing was saved.",
            crate::shown::short(&e.to_string())
        )]
    })
}

/// The manifest a workspace gets when it has none, as charter's own scaffold writes it.
fn birth(ws: &Workspace) -> Map<String, Json> {
    let author = std::env::var("USER")
        .ok()
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    match ws.birth_manifest(chrono::Utc::now(), &author) {
        Json::Object(doc) => doc,
        _ => Map::new(),
    }
}

/// One edit, applied to `settings` by its path of keys. A key removed takes every object it
/// leaves empty with it.
fn apply(settings: &mut Map<String, Json>, edit: &Edit) -> Result<(), Vec<String>> {
    let keys: Vec<&str> = edit
        .path
        .iter()
        .map(|step| match step {
            Step::Key(key) => Ok(key.as_str()),
            Step::Index(_) => Err(vec![format!(
                "a workspace's {KEY} hold no lists of tables, so a form cannot change one"
            )]),
        })
        .collect::<Result<_, _>>()?;
    let Some((last, way)) = keys.split_last() else {
        return Err(vec![format!("an edit to {KEY} names no key")]);
    };
    match &edit.value {
        Some(value) => {
            let mut at = settings;
            for key in way {
                let next = at
                    .entry((*key).to_owned())
                    .or_insert_with(|| Json::Object(Map::new()));
                at = next.as_object_mut().ok_or_else(|| {
                    vec![format!(
                        "{KEY}.{} is not an object, so it has no keys",
                        way.join(".")
                    )]
                })?;
            }
            at.insert((*last).to_owned(), to_json(value));
        }
        None => remove(settings, way, last),
    }
    Ok(())
}

/// Remove `last` under `way`, and every object on the way it leaves empty.
fn remove(at: &mut Map<String, Json>, way: &[&str], last: &str) {
    match way.split_first() {
        None => {
            at.shift_remove(last);
        }
        Some((key, rest)) => {
            if let Some(Json::Object(inner)) = at.get_mut(*key) {
                remove(inner, rest, last);
                if inner.is_empty() {
                    at.shift_remove(*key);
                }
            }
        }
    }
}

fn to_json(value: &Value) -> Json {
    match value {
        Value::Text(text) => Json::String(text.clone()),
        Value::Integer(n) => Json::from(*n),
        Value::Bool(b) => Json::Bool(*b),
        Value::List(items) => Json::Array(items.iter().cloned().map(Json::String).collect()),
    }
}

#[cfg(test)]
mod tests;
