//! A workspace's settings: the `settings` object in `workspaces/<ws>/workspace.json`
//! (charter-app#280, ADR 0048).
//!
//! **One layer, read by the readers that read the project's two files.** A workspace refines its
//! project for the team, so its settings sit between Shared (`charter.toml`) and Local
//! (`charter.local.toml`) — and they have the same shape as those files' tables:
//! `settings.extensions.<id>.enabled` is `[extensions.<id>] enabled`. So the JSON is read as the
//! TOML table it mirrors ([`table_in`]) and handed to the same reader,
//! `extension::project`, which refuses it in the same words. A table a later reader takes is
//! one more name in [`READ`] and one more reader asked here: `harness_plugins`
//! (charter-app#282) is read by `harness_plugin`, whose `Choices::read_in` takes this layer, and
//! `theme` (charter-app#281) by `extension::project::theme`, which also reads the workspace's
//! colour from it.
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
pub const READ: &[&str] = &[
    project::TABLE,
    crate::harness_plugin::TABLE,
    project::theme::TABLE,
];

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
pub(crate) fn table_of(doc: &Json) -> Option<toml::Table> {
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
            "{file} is not a JSON object, so purlis reads no settings from it — mend it by hand"
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
        out.extend(project::refusals_in(&table, &file, "settings."));
        out.extend(crate::harness_plugin::refusals_in(
            &table,
            &file,
            "settings.",
        ));
        // And `theme` (charter-app#281), by the one reader of a theme.
        out.extend(project::theme::refusals_in_workspace(&table, &file));
    }
    out
}

/// What the manifest's own readers would take leniently from the manifest whose text this is,
/// one sentence each (#1292): a `name` that is not the workspace's folder, and `repos` that is
/// not a list of `{"name": …}` records, each `branch` one git would take ([`a_branch_name`]). Edit as JSON refuses them, unless the file already held
/// them, so a raw edit cannot write what `clone`, `restore` and the repo list would read past
/// or drop. In the readers' `<key> in <file> …` shape, so the window can name the key. Empty for
/// text that is not a JSON object, which is refused on its own.
///
/// **What `held` (the file on disk) already held is let through by its VALUE**, never by the
/// sentence: the same odd `name`, the same `repos` that is no list, an odd entry equal to one
/// the file held. A sentence names an entry by its place, so matching sentences would let a raw
/// edit put a new odd entry where an old one stood.
pub fn manifest_refusals(text: &str, workspace: &str, held: Option<&str>) -> Vec<String> {
    let file = named(workspace);
    let Ok(Json::Object(doc)) = serde_json::from_str::<Json>(text) else {
        return Vec::new();
    };
    let before = held
        .and_then(|held| serde_json::from_str::<Json>(held).ok())
        .and_then(|held| match held {
            Json::Object(held) => Some(held),
            _ => None,
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    match doc.get("name") {
        None => {}
        Some(Json::String(name)) if name == workspace => {}
        Some(name) if before.get("name") == Some(name) => {}
        Some(_) => out.push(format!(
            "name in {file} is not \"{workspace}\" — a workspace is named by its folder, so its \
             manifest's name is the folder's; rename the workspace to change it"
        )),
    }
    let shape = "a workspace's repos are a list of {\"name\": \"<repo>\"} records, each with an \
                 optional \"branch\" git would take as a branch's name";
    match doc.get("repos") {
        None => {}
        Some(Json::Array(rows)) => {
            let held_rows = match before.get("repos") {
                Some(Json::Array(held)) => held.as_slice(),
                _ => &[],
            };
            for (at, row) in rows.iter().enumerate() {
                let ok = row.as_object().is_some_and(|row| {
                    row.get("name")
                        .and_then(Json::as_str)
                        .is_some_and(crate::contain::repo_name_ok)
                        && row.get("branch").is_none_or(|branch| match branch {
                            Json::String(branch) => a_branch_name(branch),
                            other => other.is_null(),
                        })
                });
                if !ok && !held_rows.contains(row) {
                    out.push(format!(
                        "repos in {file} has an entry ({}) that is not a repo's record — {shape}",
                        at + 1
                    ));
                }
            }
        }
        Some(repos) if before.get("repos") == Some(repos) => {}
        Some(_) => out.push(format!("repos in {file} is not a list — {shape}")),
    }
    out
}

/// Whether `branch` is a name a manifest's row may carry: what `git check-ref-format --branch`
/// takes, checked in-process by gix's own reference rules (#1292). Not starting with `-`, which
/// a git command line would read as an option; not a full `refs/…` name, which would be read
/// as `refs/heads/refs/…`; not `@`, which git reads as `HEAD`; and `refs/heads/<branch>` a
/// valid branch reference. `HEAD` is let through: it is what `snapshot` records for a repo
/// with no branch checked out.
fn a_branch_name(branch: &str) -> bool {
    if branch == "HEAD" {
        return true;
    }
    !branch.starts_with('-')
        && branch != "@"
        && !branch.starts_with("refs/")
        && gix::validate::reference::branch_name(format!("refs/heads/{branch}").as_str().into())
            .is_ok()
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
        .map_err(|_| format!("'{workspace}' is not a workspace purlis can name"))?;
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
    // From the base check to the write, one lock (#1292): a check alone leaves the window
    // between them open to a clone or a removal writing the same file.
    let _held = ws
        .manifest_lock()
        .map_err(|why| vec![format!("{why}, so nothing was saved.")])?;
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
    // As the project save reads the manifest: as written, then through its escapes (#1315).
    if let Some(kind) = crate::secretshape::kind_as_read(
        Some(crate::secretshape::Structured::Json),
        &crate::pyjson::dumps_indent2(&Json::Object(settings)),
    ) {
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

/// **Write `text` as the whole manifest of `workspace`** (Edit as JSON, NO-7 #1232) — or say
/// every reason it was not written. The Workspace level's counterpart of Edit as TOML
/// (`settings::save`), under the same rules:
///
/// - `base` is the manifest as the caller read it (`None`: not there), and one that changed
///   since is not written over;
/// - text that is not a JSON object is refused, so a half-mended manifest is never written;
/// - what the settings reader would refuse in it is refused, except what the manifest already
///   held — an edit is not refused for a key it did not touch;
/// - a secret-shaped value anywhere in it is refused, whatever it held before, as typed and as
///   the parsed document holds it: a LIVE workspace's manifest is committed.
///
/// **It stays whose it was**, as a form's save leaves it: a manifest a hand wrote (charter's
/// digest does not match) is written as typed, byte for byte, and stays the operator's; one
/// charter wrote, or a first one, is stamped again, so charter's writers go on keeping it.
pub fn save_text(
    root: &Path,
    workspace: &str,
    base: Option<&str>,
    text: &str,
) -> Result<(), Vec<String>> {
    let ws = workspace_of(root, workspace).map_err(|why| vec![why])?;
    let file = named(workspace);
    // From the base check to the write, one lock (#1292): a check alone leaves the window
    // between them open to a clone or a removal writing the same file.
    let _held = ws
        .manifest_lock()
        .map_err(|why| vec![format!("{why}, so nothing was saved.")])?;
    let now = on_disk(&ws).map_err(|why| vec![why])?;
    if now.as_deref() != base {
        return Err(vec![format!(
            "{file} changed on disk since this tab read it, so nothing was saved. Read it again, \
             then make the change again."
        )]);
    }
    let doc = match serde_json::from_str::<Json>(text) {
        Ok(doc @ Json::Object(_)) => doc,
        Ok(_) => {
            return Err(vec![format!(
                "{file} would not be a JSON object, so nothing was saved: a workspace's manifest \
                 is one object, {{…}}"
            )]);
        }
        Err(e) => {
            return Err(vec![format!(
                "{file} would not be a JSON object, so nothing was saved: {e}"
            )]);
        }
    };
    let standing: Vec<String> = now
        .as_deref()
        .filter(|now| serde_json::from_str::<Json>(now).is_ok_and(|doc| doc.is_object()))
        .map_or_else(Vec::new, |now| refusals(now, workspace));
    let mut refused: Vec<String> = refusals(text, workspace)
        .into_iter()
        .filter(|why| !standing.contains(why))
        .chain(manifest_refusals(text, workspace, now.as_deref()))
        .collect();
    // As typed, and as charter reads it: a string can spell a character as an escape, and the
    // document — which is also what charter's own writer puts on disk — holds the character.
    // The one answer the project save gives a staged workspace.json (#1304).
    if let Some(kind) =
        crate::secretshape::kind_as_read(Some(crate::secretshape::Structured::Json), text)
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
    let not_written = |e: std::io::Error| {
        vec![format!(
            "{file} could not be written ({}), so nothing was saved.",
            crate::shown::short(&e.to_string())
        )]
    };
    if crate::manifest::ownership(now.as_deref()) == Ownership::Operator {
        ws.write_manifest_text(text).map_err(not_written)
    } else {
        ws.write_manifest_as(&doc, true).map_err(not_written)
    }
}

/// The manifest a workspace gets when it has none, as charter's own scaffold writes it.
fn birth(ws: &Workspace) -> Map<String, Json> {
    match ws.birth_manifest(chrono::Utc::now(), &crate::wscmd::ensure::author()) {
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
