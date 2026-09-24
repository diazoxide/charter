//! The Project settings tab's wire (charter-app#252): a plane's `charter.toml` and
//! `charter.local.toml`, read and written through [`charter_core::settings`].
//!
//! Thin by design, as `doctor.rs` is. Every refusal is the core's sentence — the same one the
//! next read of the file would say — and every write goes through the core's `toml_edit`
//! writer. What this adds is the shape the window reads a file in: its text for the raw view,
//! and every value in it by path for the forms, so the window needs no TOML parser of its own
//! and a key the forms do not know yet (#253's `[extensions]`) is already on the wire.

use charter_core::settings::{self, Edit, Found, Step, Value, Which};

use crate::planes::{PlaneId, Planes};

/// Which file: the committed one or this machine's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SettingsWhich {
    /// `charter.toml` — committed; the team sees it.
    Shared,
    /// `charter.local.toml` — gitignored; this machine only.
    Local,
}

impl From<SettingsWhich> for Which {
    fn from(which: SettingsWhich) -> Self {
        match which {
            SettingsWhich::Shared => Self::Shared,
            SettingsWhich::Local => Self::Local,
        }
    }
}

/// One step of the way to a key: a table's key, or a block's place in `[[forge]]`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SettingsStep {
    Key(String),
    Index(u32),
}

/// A value, as a form reads and writes it. `other` is one no form writes — a float, a date, a
/// list that is not all text — shown as TOML and changed only in the raw view.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum SettingsValue {
    Text(String),
    /// Whole numbers only; TOML's range is `i64` and a form writes no more than a JS number
    /// carries exactly, so it travels as one.
    Integer(f64),
    Bool(bool),
    List(Vec<String>),
    Other(String),
}

/// One value in a file, and where it is.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
pub struct SettingsField {
    pub path: Vec<SettingsStep>,
    pub value: SettingsValue,
}

/// One file, as the tab draws it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
pub struct SettingsFile {
    pub which: SettingsWhich,
    /// `charter.toml` or `charter.local.toml`.
    pub file: String,
    /// Whether it is there. A Local file that is not is created by the first save.
    pub exists: bool,
    /// Its text, for the raw view — and what a save is checked against, so an edit made
    /// elsewhere since is never written over.
    pub text: String,
    /// What charter refuses in it as it stands, in the core's words.
    pub refusals: Vec<String>,
    /// Whether it is TOML. When it is not, `fields` is empty and only the raw view can mend it.
    pub parsed: bool,
    /// Every value in it, in file order.
    pub fields: Vec<SettingsField>,
}

/// Both files.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
pub struct ProjectSettings {
    pub shared: SettingsFile,
    pub local: SettingsFile,
}

/// What a save is: the raw view's whole text, or a form's changes to the text it was read as.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SettingsChange {
    Raw { text: String },
    Edits { edits: Vec<SettingsEdit> },
}

/// Set the key at `path` to `value`, or remove it when `value` is `null`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, specta::Type)]
pub struct SettingsEdit {
    pub path: Vec<SettingsStep>,
    pub value: Option<SettingsValue>,
}

/// What a save answered: the file as it now stands, or every reason nothing was written.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SettingsSaved {
    Saved { file: SettingsFile },
    Refused { reasons: Vec<String> },
}

/// Both of this plane's settings files, and what charter says about each.
///
/// On a blocking thread: the Local file's check asks git whether it is ignored.
#[tauri::command]
#[specta::specta]
pub async fn project_settings(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<ProjectSettings, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(ProjectSettings {
            shared: file_of(&root, SettingsWhich::Shared)?,
            local: file_of(&root, SettingsWhich::Local)?,
        })
    })
    .await
    .map_err(|err| format!("reading the settings did not finish: {err}"))?
}

/// Write one file: checked with the core's rules, written with its `toml_edit` writer.
///
/// `base` is the text the window read (`null`: the file was not there), so a file changed on
/// disk since is refused rather than overwritten.
#[tauri::command]
#[specta::specta]
pub async fn save_project_settings(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    which: SettingsWhich,
    base: Option<String>,
    change: SettingsChange,
) -> Result<SettingsSaved, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || save(&root, which, base.as_deref(), change))
        .await
        .map_err(|err| format!("saving the settings did not finish: {err}"))?
}

/// The command above, without a runtime.
pub(crate) fn save(
    root: &std::path::Path,
    which: SettingsWhich,
    base: Option<&str>,
    change: SettingsChange,
) -> Result<SettingsSaved, String> {
    let text = match change {
        SettingsChange::Raw { text } => text,
        SettingsChange::Edits { edits } => {
            let edits: Vec<Edit> = edits.into_iter().map(edit_of).collect::<Result<_, _>>()?;
            match settings::edited(base.unwrap_or_default(), &edits) {
                Ok(text) => text,
                Err(why) => return Ok(SettingsSaved::Refused { reasons: vec![why] }),
            }
        }
    };
    match settings::save(root, which.into(), base, &text) {
        Ok(()) => Ok(SettingsSaved::Saved {
            file: file_of(root, which)?,
        }),
        Err(reasons) => Ok(SettingsSaved::Refused { reasons }),
    }
}

pub(crate) fn file_of(
    root: &std::path::Path,
    which: SettingsWhich,
) -> Result<SettingsFile, String> {
    let read = settings::read(root, which.into())?;
    let fields = settings::fields(&read.text);
    Ok(SettingsFile {
        which,
        file: read.file.to_owned(),
        exists: read.exists,
        refusals: read.refusals,
        parsed: fields.is_some(),
        fields: fields_on_the_wire(fields.unwrap_or_default()),
        text: read.text,
    })
}

// ---------------------------------------------------------------------------------------
// A workspace's settings (charter-app#280)
// ---------------------------------------------------------------------------------------

/// A workspace's settings — the `settings` of its `workspace.json` — as the Workspace settings
/// tab draws them. The same shape as a [`SettingsFile`], without a raw view: the manifest is
/// charter's and the team's, and a form is the one way into it here.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
pub struct WorkspaceSettings {
    pub workspace: String,
    /// `workspaces/<ws>/workspace.json`.
    pub file: String,
    /// Whether it is there. One that is not is created by the first save.
    pub exists: bool,
    /// Its text: what a save is checked against, so an edit made elsewhere since is never
    /// written over.
    pub text: String,
    /// What charter does not take from its settings as they stand, in the core's words.
    pub refusals: Vec<String>,
    /// Whether a form can change it: a JSON object, or no file yet.
    pub parsed: bool,
    /// Every value in its settings, by its path under `settings`.
    pub fields: Vec<SettingsField>,
    /// Whether the workspace is LIVE, so the file is committed and the team sees it.
    pub live: bool,
}

/// What a workspace settings save answered.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkspaceSettingsSaved {
    Saved { settings: WorkspaceSettings },
    Refused { reasons: Vec<String> },
}

/// One workspace's settings, and what charter says about them.
#[tauri::command]
#[specta::specta]
pub async fn workspace_settings(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<WorkspaceSettings, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || workspace_of(&root, &workspace))
        .await
        .map_err(|err| format!("reading the workspace's settings did not finish: {err}"))?
}

/// Change one workspace's settings: checked by the readers of the project's files, written into
/// its `workspace.json` with every other key kept (`charter_core::settings::workspace`).
#[tauri::command]
#[specta::specta]
pub async fn save_workspace_settings(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    base: Option<String>,
    edits: Vec<SettingsEdit>,
) -> Result<WorkspaceSettingsSaved, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        save_workspace(&root, &workspace, base.as_deref(), edits)
    })
    .await
    .map_err(|err| format!("saving the workspace's settings did not finish: {err}"))?
}

/// [`save_workspace_settings`], without a runtime.
pub(crate) fn save_workspace(
    root: &std::path::Path,
    workspace: &str,
    base: Option<&str>,
    edits: Vec<SettingsEdit>,
) -> Result<WorkspaceSettingsSaved, String> {
    let edits: Vec<Edit> = edits.into_iter().map(edit_of).collect::<Result<_, _>>()?;
    match settings::workspace::save(root, workspace, base, &edits) {
        Ok(()) => Ok(WorkspaceSettingsSaved::Saved {
            settings: workspace_of(root, workspace)?,
        }),
        Err(reasons) => Ok(WorkspaceSettingsSaved::Refused { reasons }),
    }
}

pub(crate) fn workspace_of(
    root: &std::path::Path,
    workspace: &str,
) -> Result<WorkspaceSettings, String> {
    let read = settings::workspace::read_file(root, workspace)?;
    Ok(WorkspaceSettings {
        workspace: workspace.to_owned(),
        file: read.file,
        exists: read.exists,
        text: read.text,
        refusals: read.refusals,
        parsed: read.parsed,
        fields: fields_on_the_wire(read.fields),
        live: read.live,
    })
}

fn fields_on_the_wire(fields: Vec<(Vec<Step>, Found)>) -> Vec<SettingsField> {
    fields
        .into_iter()
        .map(|(path, value)| SettingsField {
            path: path.into_iter().map(step_of).collect(),
            value: value_of(value),
        })
        .collect()
}

fn step_of(step: Step) -> SettingsStep {
    match step {
        Step::Key(key) => SettingsStep::Key(key),
        Step::Index(at) => SettingsStep::Index(u32::try_from(at).unwrap_or(u32::MAX)),
    }
}

/// Exact up to 2^53, which is every number a form writes; beyond it the raw view shows the
/// digits and the form does not offer to change them.
const MOST_EXACT: u64 = 1 << 53;

fn value_of(found: Found) -> SettingsValue {
    match found {
        Found::Value(Value::Text(text)) => SettingsValue::Text(text),
        Found::Value(Value::Integer(n)) if n.unsigned_abs() <= MOST_EXACT => {
            SettingsValue::Integer(n as f64)
        }
        Found::Value(Value::Integer(n)) => SettingsValue::Other(n.to_string()),
        Found::Value(Value::Bool(b)) => SettingsValue::Bool(b),
        Found::Value(Value::List(items)) => SettingsValue::List(items),
        Found::Other(toml) => SettingsValue::Other(toml),
    }
}

fn edit_of(edit: SettingsEdit) -> Result<Edit, String> {
    Ok(Edit {
        path: edit
            .path
            .into_iter()
            .map(|step| match step {
                SettingsStep::Key(key) => Step::Key(key),
                SettingsStep::Index(at) => Step::Index(at as usize),
            })
            .collect(),
        value: edit.value.map(value_to_core).transpose()?,
    })
}

fn value_to_core(value: SettingsValue) -> Result<Value, String> {
    Ok(match value {
        SettingsValue::Text(text) => Value::Text(text),
        SettingsValue::Integer(n) if n.fract() == 0.0 && n.abs() <= MOST_EXACT as f64 => {
            Value::Integer(n as i64)
        }
        SettingsValue::Integer(n) => {
            return Err(format!("{n} is not a whole number a form can write"));
        }
        SettingsValue::Bool(b) => Value::Bool(b),
        SettingsValue::List(items) => Value::List(items),
        SettingsValue::Other(_) => {
            return Err("that value is only changed in the raw view".to_owned());
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(shared: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), shared).unwrap();
        dir
    }

    #[test]
    fn every_value_is_on_the_wire_by_its_path_including_a_forge_block_by_its_place() {
        let dir = plane(
            "schema = 1\n[[forge]]\nkind = \"github\"\nexclude = [\"a\"]\n[memory]\nshare = \"commit\"\n",
        );
        let file = file_of(dir.path(), SettingsWhich::Shared).unwrap();
        assert!(file.parsed && file.exists);
        assert_eq!(
            file.fields,
            [
                SettingsField {
                    path: vec![SettingsStep::Key("schema".into())],
                    value: SettingsValue::Integer(1.0),
                },
                SettingsField {
                    path: vec![
                        SettingsStep::Key("forge".into()),
                        SettingsStep::Index(0),
                        SettingsStep::Key("kind".into()),
                    ],
                    value: SettingsValue::Text("github".into()),
                },
                SettingsField {
                    path: vec![
                        SettingsStep::Key("forge".into()),
                        SettingsStep::Index(0),
                        SettingsStep::Key("exclude".into()),
                    ],
                    value: SettingsValue::List(vec!["a".into()]),
                },
                SettingsField {
                    path: vec![
                        SettingsStep::Key("memory".into()),
                        SettingsStep::Key("share".into()),
                    ],
                    value: SettingsValue::Text("commit".into()),
                },
            ]
        );
    }

    #[test]
    fn a_file_that_is_not_toml_is_on_the_wire_as_text_with_no_fields() {
        let dir = plane("[memory\n");
        let file = file_of(dir.path(), SettingsWhich::Shared).unwrap();
        assert!(!file.parsed);
        assert!(file.fields.is_empty());
        assert_eq!(file.text, "[memory\n");
        assert_eq!(file.refusals.len(), 1);
    }

    #[test]
    fn a_form_edit_is_saved_through_the_cores_writer_and_answers_the_new_file() {
        let body = "# keep me\n[memory]\nshare = \"local\" # why\n";
        let dir = plane(body);
        let saved = save(
            dir.path(),
            SettingsWhich::Shared,
            Some(body),
            SettingsChange::Edits {
                edits: vec![SettingsEdit {
                    path: vec![
                        SettingsStep::Key("memory".into()),
                        SettingsStep::Key("share".into()),
                    ],
                    value: Some(SettingsValue::Text("push".into())),
                }],
            },
        )
        .unwrap();
        let SettingsSaved::Saved { file } = saved else {
            panic!("saved: {saved:?}")
        };
        assert_eq!(file.text, "# keep me\n[memory]\nshare = \"push\" # why\n");
    }

    fn with_workspace(manifest: &str) -> tempfile::TempDir {
        let dir = plane("schema = 1\n");
        let ws = dir.path().join("workspaces/alpha");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("workspace.json"), manifest).unwrap();
        dir
    }

    fn enabled(on: bool) -> SettingsEdit {
        SettingsEdit {
            path: ["extensions", "stats", "enabled"]
                .into_iter()
                .map(|key| SettingsStep::Key(key.into()))
                .collect(),
            value: Some(SettingsValue::Bool(on)),
        }
    }

    #[test]
    fn a_workspace_settings_edit_is_saved_through_the_core_and_answers_the_new_settings() {
        let manifest = "{\n  \"name\": \"alpha\"\n}\n";
        let dir = with_workspace(manifest);
        let before = workspace_of(dir.path(), "alpha").unwrap();
        assert_eq!(before.file, "workspaces/alpha/workspace.json");
        assert!(before.exists && before.parsed && before.fields.is_empty());
        let saved =
            save_workspace(dir.path(), "alpha", Some(manifest), vec![enabled(false)]).unwrap();
        let WorkspaceSettingsSaved::Saved { settings } = saved else {
            panic!("saved: {saved:?}")
        };
        assert_eq!(
            settings.fields,
            [SettingsField {
                path: ["extensions", "stats", "enabled"]
                    .into_iter()
                    .map(|key| SettingsStep::Key(key.into()))
                    .collect(),
                value: SettingsValue::Bool(false),
            }]
        );
    }

    #[test]
    fn a_refused_workspace_save_answers_the_cores_sentences() {
        let manifest = "{\n  \"name\": \"alpha\"\n}\n";
        let dir = with_workspace(manifest);
        let saved = save_workspace(dir.path(), "alpha", Some("{}"), vec![enabled(true)]).unwrap();
        let WorkspaceSettingsSaved::Refused { reasons } = saved else {
            panic!("refused: {saved:?}")
        };
        assert!(reasons[0].contains("changed on disk"), "{reasons:?}");
    }

    #[test]
    fn a_refused_save_answers_the_cores_sentences_and_writes_nothing() {
        let dir = plane("schema = 1\n");
        let saved = save(
            dir.path(),
            SettingsWhich::Shared,
            Some("schema = 1\n"),
            SettingsChange::Raw {
                text: "schema = 9\n".into(),
            },
        )
        .unwrap();
        let SettingsSaved::Refused { reasons } = saved else {
            panic!("refused: {saved:?}")
        };
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].contains("declares schema 9"), "{reasons:?}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("charter.toml")).unwrap(),
            "schema = 1\n"
        );
    }
}
