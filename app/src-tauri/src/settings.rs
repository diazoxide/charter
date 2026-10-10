//! The Settings tab's wire (charter-app#252): a plane's `charter.toml` and
//! `charter.local.toml`, read and written through [`purlis_core::settings`].
//!
//! Thin by design, as `doctor.rs` is. Every refusal is the core's sentence — the same one the
//! next read of the file would say — and every write goes through the core's `toml_edit`
//! writer. What this adds is the shape the window reads a file in: its text for Edit as TOML,
//! and every value in it by path for the forms, so the window needs no TOML parser of its own
//! and a key the forms do not know yet (#253's `[extensions]`) is already on the wire.

use purlis_core::settings::{self, Edit, Found, Step, Value, Which};

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
/// list that is not all text — shown as TOML and changed only under Edit as TOML.
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
    /// Its text, for Edit as TOML — and what a save is checked against, so an edit made
    /// elsewhere since is never written over.
    pub text: String,
    /// What charter refuses in it as it stands, in the core's words, each with the key it is
    /// about (#1292).
    pub refusals: Vec<SettingsRefusal>,
    /// Whether it is TOML. When it is not, `fields` is empty and only Edit as TOML can mend it.
    pub parsed: bool,
    /// Every value in it, in file order.
    pub fields: Vec<SettingsField>,
    /// The entries of each collection the file is the home of (ST-3): the Shared file's
    /// `[[forge]]` blocks, and the Local file's `[harness.<name>]` profiles (ST-4). `null` when
    /// it holds none, and left out by a caller that lists none.
    #[specta(optional)]
    pub entries: Option<Vec<SettingsEntry>>,
}

/// **One thing charter does not take from a settings file as it stands** (#1292): the core's
/// sentence, and the key it is about (`purlis_core::settings::Refusal`), so the window links it to
/// the setting that mends it without reading the sentence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SettingsRefusal {
    /// The reader's sentence, word for word.
    pub why: String,
    /// The key it is about, one step per table or key (an extension id with dots in it is one
    /// step); for a workspace's file, the key under `settings`. `null` when it is about the
    /// whole file, or about no key a setting is at.
    pub key: Option<Vec<String>>,
}

impl From<settings::Refusal> for SettingsRefusal {
    fn from(one: settings::Refusal) -> Self {
        Self {
            why: one.why,
            key: one.key,
        }
    }
}

/// One entry of a collection, as the core lists it (`purlis_core::settings::collection::Listed`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SettingsEntry {
    /// Which collection: `forges` or `hosts` (the project's sandbox hosts) in the Shared file,
    /// `profiles` or `myHosts` (yours) in the Local one.
    pub collection: String,
    /// Opaque: what a remove is sent by. A different one once the entry moved or changed.
    pub id: String,
    pub label: String,
    /// The path every key of the entry is under.
    pub keys: Vec<SettingsStep>,
    /// The Add form's values that would write it again, by the entry's key.
    pub values: Vec<EntryValue>,
}

/// One field of a collection entry, as the Add form holds it: a list is one entry per line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct EntryValue {
    pub field: String,
    pub value: String,
}

/// Both files.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
pub struct ProjectSettings {
    pub shared: SettingsFile,
    pub local: SettingsFile,
}

/// What a save is: Edit as TOML's whole text, or a form's changes to the text it was read as.
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
    // A preset turned on or off here, by you, is one this machine has seen (#1385), unless
    // another change was already waiting to be told: recorded as written (#1550).
    let shared = matches!(which, SettingsWhich::Shared);
    let pending = shared.then(|| Pending::now(root));
    match settings::save(root, which.into(), base, &text) {
        Ok(()) if shared => {
            if let Some(pending) = pending {
                pending.seen_by_you(root, &text);
            }
            Ok(SettingsSaved::Saved {
                file: file_of(root, which)?,
            })
        }
        Ok(()) => Ok(SettingsSaved::Saved {
            file: file_of(root, which)?,
        }),
        Err(reasons) => Ok(SettingsSaved::Refused { reasons }),
    }
}

/// Whether a change to the project's presets, and to its hosts, was waiting to be told before
/// this window wrote the shared file: one that was is told with the window's own, and the
/// window's own is not recorded as seen.
#[derive(Debug, Clone, Copy)]
struct Pending {
    presets: bool,
    hosts: bool,
}

impl Pending {
    fn now(root: &std::path::Path) -> Self {
        Self {
            presets: purlis_core::sandbox::local::presets_changed(root).is_some(),
            hosts: purlis_core::sandbox::local::hosts_changed(root).is_some(),
        }
    }

    /// Records the presets and the hosts `written`, the shared file as this window wrote it,
    /// as seen by the person who wrote it.
    fn seen_by_you(self, root: &std::path::Path, written: &str) {
        purlis_core::sandbox::local::presets_seen_by_you(root, self.presets, written);
        purlis_core::sandbox::local::hosts_seen_by_you(root, self.hosts, written);
    }
}

/// What a move answered: both files as they now stand, or every reason neither was written.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SettingsMoved {
    /// Boxed: both files are far larger than a refusal.
    Moved {
        settings: Box<ProjectSettings>,
    },
    Refused {
        reasons: Vec<String>,
    },
}

/// Move the values at `paths` into `to`, out of the other file: the Settings tab's "Shared /
/// Only on this machine" choice (SE-18). Both files are written or neither is
/// (`purlis_core::settings::move_keys`).
///
/// `shared_base` and `local_base` are the texts the window read (`null`: not there), so a file
/// changed on disk since is refused rather than overwritten.
#[tauri::command]
#[specta::specta]
pub async fn move_project_settings(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    to: SettingsWhich,
    shared_base: Option<String>,
    local_base: Option<String>,
    paths: Vec<Vec<SettingsStep>>,
) -> Result<SettingsMoved, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        move_keys(
            &root,
            to,
            shared_base.as_deref(),
            local_base.as_deref(),
            paths,
        )
    })
    .await
    .map_err(|err| format!("moving the setting did not finish: {err}"))?
}

/// [`move_project_settings`], without a runtime.
pub(crate) fn move_keys(
    root: &std::path::Path,
    to: SettingsWhich,
    shared_base: Option<&str>,
    local_base: Option<&str>,
    paths: Vec<Vec<SettingsStep>>,
) -> Result<SettingsMoved, String> {
    let paths: Vec<Vec<Step>> = paths
        .into_iter()
        .map(|path| path.into_iter().map(core_step).collect())
        .collect();
    let (from_base, to_base) = match to {
        SettingsWhich::Shared => (local_base, shared_base),
        SettingsWhich::Local => (shared_base, local_base),
    };
    // A move always writes the shared file, from or to: a preset or a host it changes there is
    // yours, as a save's is (#1385, #1550), unless another change was already waiting.
    let pending = Pending::now(root);
    match settings::move_keys(root, to.into(), from_base, to_base, &paths) {
        Ok(shared) => {
            pending.seen_by_you(root, &shared);
            Ok(SettingsMoved::Moved {
                settings: Box::new(ProjectSettings {
                    shared: file_of(root, SettingsWhich::Shared)?,
                    local: file_of(root, SettingsWhich::Local)?,
                }),
            })
        }
        Err(reasons) => Ok(SettingsMoved::Refused { reasons }),
    }
}

// ---------------------------------------------------------------------------------------
// Settings collections: adding and removing an entry (ST-3, #1227)
// ---------------------------------------------------------------------------------------

/// One field of an entry the core refused, and why: the field is the entry's own key.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct EntryFieldRefusal {
    pub field: String,
    pub why: String,
}

/// Something that uses an entry, which stops its removal. `group` is the Settings group it is
/// changed in (`project.saving`) when it is a setting; `follows`, whether a rename everywhere
/// (#1380) changes it too, in the one write that renames the entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct EntryReferrer {
    pub what: String,
    pub group: Option<String>,
    pub follows: bool,
}

/// What adding or removing a collection entry answered: the file as it now stands, or every
/// reason nothing was written — by field, by what uses the entry, and for the whole write
/// (`purlis_core::settings::collection::Refusal`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EntryWritten {
    /// Written. `added` is the entry's identity after an add or a rename (ST-4); `removed` the
    /// entry a remove took, as the Add form would write it again: what each one's Undo is made
    /// of (D-ST3-i).
    Saved {
        file: SettingsFile,
        added: Option<String>,
        removed: Option<Vec<EntryValue>>,
    },
    Refused {
        fields: Vec<EntryFieldRefusal>,
        referrers: Vec<EntryReferrer>,
        reasons: Vec<String>,
    },
}

/// A `[[forge]]` block as the Add form sends it, each field as typed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
pub struct ForgeEntry {
    pub kind: String,
    pub owner: String,
    pub host: String,
    pub exclude: Vec<String>,
}

/// Add a `[[forge]]` block to `charter.toml`, checked whole by the core
/// (`purlis_core::settings::forges::add`).
///
/// `base` is the text the window read (`null`: not there), so a file changed on disk since is
/// refused rather than overwritten.
#[tauri::command]
#[specta::specta]
pub async fn add_project_forge(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    base: Option<String>,
    entry: ForgeEntry,
) -> Result<EntryWritten, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || add_forge(&root, base.as_deref(), entry))
        .await
        .map_err(|err| format!("adding the forge did not finish: {err}"))?
}

/// [`add_project_forge`], without a runtime.
pub(crate) fn add_forge(
    root: &std::path::Path,
    base: Option<&str>,
    entry: ForgeEntry,
) -> Result<EntryWritten, String> {
    let entry = settings::forges::Entry {
        kind: entry.kind,
        owner: entry.owner,
        host: entry.host,
        exclude: entry.exclude,
    };
    match settings::forges::add(root, base, &entry) {
        Ok(id) => Ok(EntryWritten::Saved {
            file: file_of(root, SettingsWhich::Shared)?,
            added: Some(id),
            removed: None,
        }),
        Err(refusal) => Ok(refused(refusal)),
    }
}

/// Remove the `[[forge]]` block called `id` (as `charter.toml`'s `entries` list it) — refused,
/// naming them, while a repo or a setting uses it (`purlis_core::settings::forges::remove`).
///
/// `base` is the text the entries were drawn from, so an entry that moved or changed since is
/// refused rather than another removed.
#[tauri::command]
#[specta::specta]
pub async fn remove_project_forge(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    base: Option<String>,
    id: String,
) -> Result<EntryWritten, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || remove_forge(&root, base.as_deref(), &id))
        .await
        .map_err(|err| format!("removing the forge did not finish: {err}"))?
}

/// [`remove_project_forge`], without a runtime.
pub(crate) fn remove_forge(
    root: &std::path::Path,
    base: Option<&str>,
    id: &str,
) -> Result<EntryWritten, String> {
    match settings::forges::remove(root, base, id) {
        Ok(took) => Ok(EntryWritten::Saved {
            file: file_of(root, SettingsWhich::Shared)?,
            added: None,
            removed: Some(values_on_the_wire(vec![
                ("kind", took.kind),
                ("owner", took.owner),
                ("host", took.host),
                ("exclude", took.exclude.join("\n")),
            ])),
        }),
        Err(refusal) => Ok(refused(refusal)),
    }
}

/// A harness profile as the Add form sends it, each field as typed: the command one argument
/// per entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
pub struct ProfileEntry {
    pub name: String,
    pub kind: String,
    pub command: Vec<String>,
}

/// Add a `[harness.<name>]` profile to the project's local settings file, checked whole by the
/// core's profile rules (`purlis_core::settings::harness_profiles::add`). Its first run still
/// asks for approval.
///
/// `base` is the text the window read (`null`: not there), so a file changed on disk since is
/// refused rather than overwritten.
#[tauri::command]
#[specta::specta]
pub async fn add_project_profile(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    base: Option<String>,
    entry: ProfileEntry,
) -> Result<EntryWritten, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || add_profile(&root, base.as_deref(), entry))
        .await
        .map_err(|err| format!("adding the profile did not finish: {err}"))?
}

/// [`add_project_profile`], without a runtime.
pub(crate) fn add_profile(
    root: &std::path::Path,
    base: Option<&str>,
    entry: ProfileEntry,
) -> Result<EntryWritten, String> {
    let entry = settings::harness_profiles::Entry {
        name: entry.name,
        kind: entry.kind,
        command: entry.command,
    };
    match settings::harness_profiles::add(root, base, &entry) {
        Ok(id) => Ok(EntryWritten::Saved {
            file: file_of(root, SettingsWhich::Local)?,
            added: Some(id),
            removed: None,
        }),
        Err(refusal) => Ok(refused(refusal)),
    }
}

/// Remove the profile called `id` (as the local file's `entries` list it) — refused, naming
/// them, while a `[harness] default` uses it
/// (`purlis_core::settings::harness_profiles::remove`).
#[tauri::command]
#[specta::specta]
pub async fn remove_project_profile(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    base: Option<String>,
    id: String,
) -> Result<EntryWritten, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || remove_profile(&root, base.as_deref(), &id))
        .await
        .map_err(|err| format!("removing the profile did not finish: {err}"))?
}

/// [`remove_project_profile`], without a runtime.
pub(crate) fn remove_profile(
    root: &std::path::Path,
    base: Option<&str>,
    id: &str,
) -> Result<EntryWritten, String> {
    match settings::harness_profiles::remove(root, base, id) {
        Ok(took) => Ok(EntryWritten::Saved {
            file: file_of(root, SettingsWhich::Local)?,
            added: None,
            removed: Some(values_on_the_wire(vec![
                ("name", took.name),
                ("kind", took.kind),
                ("command", took.command.join("\n")),
            ])),
        }),
        Err(refusal) => Ok(refused(refusal)),
    }
}

/// Rename the profile called `id` to `to`. Plainly, only while nothing uses it — refused,
/// naming them, otherwise, each saying whether it `follows` a rename everywhere: that refusal is
/// what the window shows before anything changes. `everywhere` renames it and every user that
/// follows in one write, and is refused the same way while one does not (#1380;
/// `purlis_core::settings::harness_profiles::rename_everywhere`). Answers the renamed entry's
/// identity as `added`: what its Undo renames back, the same way.
#[tauri::command]
#[specta::specta]
pub async fn rename_project_profile(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    base: Option<String>,
    id: String,
    to: String,
    everywhere: bool,
) -> Result<EntryWritten, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        rename_profile(&root, base.as_deref(), &id, &to, everywhere)
    })
    .await
    .map_err(|err| format!("renaming the profile did not finish: {err}"))?
}

/// [`rename_project_profile`], without a runtime.
pub(crate) fn rename_profile(
    root: &std::path::Path,
    base: Option<&str>,
    id: &str,
    to: &str,
    everywhere: bool,
) -> Result<EntryWritten, String> {
    let renamed = if everywhere {
        settings::harness_profiles::rename_everywhere(root, base, id, to)
    } else {
        settings::harness_profiles::rename(root, base, id, to)
    };
    match renamed {
        Ok(id) => Ok(EntryWritten::Saved {
            file: file_of(root, SettingsWhich::Local)?,
            added: Some(id),
            removed: None,
        }),
        Err(refusal) => Ok(refused(refusal)),
    }
}

/// Add a host to the sandbox's Internet access (#1341): `shared` is the project's, in
/// `charter.toml`, which every teammate follows; `local` is yours, in `charter.local.toml`, on
/// this machine only. The core checks it and says why it refuses one
/// (`purlis_core::settings::hosts::add`).
///
/// `base` is the text the window read (`null`: not there), so a file changed on disk since is
/// refused rather than overwritten.
#[tauri::command]
#[specta::specta]
pub async fn add_sandbox_host(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    which: SettingsWhich,
    base: Option<String>,
    host: String,
) -> Result<EntryWritten, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || add_host(&root, which, base.as_deref(), &host))
        .await
        .map_err(|err| format!("adding the host did not finish: {err}"))?
}

/// **The host field, checked as it is typed** (#1405): why `host` is not a host the sandbox
/// takes, or `null` when it is one. The core's own parser
/// (`purlis_core::sandbox::hosts::Host::parse`), so the field and Add refuse the same text with
/// the same sentence; reads nothing and writes nothing. Add still asks the core again, which
/// also checks the file and an administrator's policy.
#[tauri::command]
#[specta::specta]
pub fn check_sandbox_host(host: String) -> Option<String> {
    purlis_core::sandbox::hosts::Host::parse(&host).err()
}

/// [`add_sandbox_host`], without a runtime.
pub(crate) fn add_host(
    root: &std::path::Path,
    which: SettingsWhich,
    base: Option<&str>,
    host: &str,
) -> Result<EntryWritten, String> {
    match settings::hosts::add(root, which.into(), base, host) {
        Ok(id) => Ok(EntryWritten::Saved {
            file: file_of(root, which)?,
            added: Some(id),
            removed: None,
        }),
        Err(refusal) => Ok(refused(refusal)),
    }
}

/// Remove the host called `id` (as the file's `entries` list it) from the sandbox's Internet
/// access at `which`'s level (`purlis_core::settings::hosts::remove`).
#[tauri::command]
#[specta::specta]
pub async fn remove_sandbox_host(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    which: SettingsWhich,
    base: Option<String>,
    id: String,
) -> Result<EntryWritten, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || remove_host(&root, which, base.as_deref(), &id))
        .await
        .map_err(|err| format!("removing the host did not finish: {err}"))?
}

/// [`remove_sandbox_host`], without a runtime.
pub(crate) fn remove_host(
    root: &std::path::Path,
    which: SettingsWhich,
    base: Option<&str>,
    id: &str,
) -> Result<EntryWritten, String> {
    match settings::hosts::remove(root, which.into(), base, id) {
        Ok(took) => Ok(EntryWritten::Saved {
            file: file_of(root, which)?,
            added: None,
            removed: Some(values_on_the_wire(vec![("host", took)])),
        }),
        Err(refusal) => Ok(refused(refusal)),
    }
}

/// Confirm your own host called `id` in `charter.local.toml` (#1341): one Settings did not add
/// on this machine (a chat's edit, or a file from elsewhere) reaches nothing until it is
/// confirmed here (`purlis_core::settings::hosts::confirm`).
#[tauri::command]
#[specta::specta]
pub async fn confirm_sandbox_host(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    base: Option<String>,
    id: String,
) -> Result<EntryWritten, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || confirm_host(&root, base.as_deref(), &id))
        .await
        .map_err(|err| format!("confirming the host did not finish: {err}"))?
}

/// [`confirm_sandbox_host`], without a runtime.
pub(crate) fn confirm_host(
    root: &std::path::Path,
    base: Option<&str>,
    id: &str,
) -> Result<EntryWritten, String> {
    match settings::hosts::confirm(root, base, id) {
        Ok(id) => Ok(EntryWritten::Saved {
            file: file_of(root, SettingsWhich::Local)?,
            added: Some(id),
            removed: None,
        }),
        Err(refusal) => Ok(refused(refusal)),
    }
}

/// A collection write's refusal, for the wire.
fn refused(refusal: settings::collection::Refusal) -> EntryWritten {
    EntryWritten::Refused {
        fields: refusal
            .fields
            .into_iter()
            .map(|one| EntryFieldRefusal {
                field: one.field.to_owned(),
                why: one.why,
            })
            .collect(),
        referrers: refusal
            .referrers
            .into_iter()
            .map(|one| EntryReferrer {
                what: one.what,
                group: one.group.map(|group| group.id().to_owned()),
                follows: one.follows,
            })
            .collect(),
        reasons: refusal.file,
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
        refusals: read.refusals.into_iter().map(Into::into).collect(),
        parsed: fields.is_some(),
        fields: fields_on_the_wire(fields.unwrap_or_default()),
        entries: entries_of(root, which, &read.text),
        text: read.text,
    })
}

/// The collections `which` is the home of, listed by the core: the Shared file's forges and
/// the project's sandbox hosts, and the Local file's harness profiles (ST-4) and your own
/// sandbox hosts (#1341).
fn entries_of(
    root: &std::path::Path,
    which: SettingsWhich,
    text: &str,
) -> Option<Vec<SettingsEntry>> {
    let core: Which = which.into();
    let own = match which {
        SettingsWhich::Shared => ("forges", settings::forges::listed(text)),
        SettingsWhich::Local => ("profiles", settings::harness_profiles::listed(text)),
    };
    let hosts = (
        settings::hosts::collection(core),
        settings::hosts::listed_at(root, core, text),
    );
    let entries: Vec<SettingsEntry> = [own, hosts]
        .into_iter()
        .flat_map(|(collection, listed)| {
            listed.into_iter().map(move |one| SettingsEntry {
                collection: collection.to_owned(),
                id: one.id,
                label: one.label,
                keys: one.keys.into_iter().map(step_of).collect(),
                values: values_on_the_wire(one.values),
            })
        })
        .collect();
    (!entries.is_empty()).then_some(entries)
}

fn values_on_the_wire(values: Vec<(&'static str, String)>) -> Vec<EntryValue> {
    values
        .into_iter()
        .map(|(field, value)| EntryValue {
            field: field.to_owned(),
            value,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// How far a save goes, as the two files decide it (charter-app#300, ADR 0051)
// ---------------------------------------------------------------------------------------

/// One save setting as the project has it: its value, and the file that decided it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct InForce {
    /// As the files write it — a mode's word, a branch, `on` or `off`, a quiet period as `30s`
    /// or `2m` — or `null` where no value is the answer: a mode the Saving view asks for, a
    /// branch the plane or repo already has.
    pub value: Option<String>,
    /// `default`, `shared` or `local`.
    pub source: String,
}

/// `[plane]`, as `planesave::Settings` resolves it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct PlaneInForce {
    pub mode: InForce,
    /// Whether the mode is `charter.toml`'s `[memory] share`, the deprecated alias.
    pub from_share: bool,
    pub branch: InForce,
    pub save_branch: InForce,
    pub sign: InForce,
    pub autosave: InForce,
    pub autosave_after: InForce,
}

/// `[repos.<name>]` for one repo, as `planesave::Settings::repo` resolves it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct RepoInForce {
    /// Its name in `inventory/repos.json`, or in a file's `[repos]`.
    pub name: String,
    pub mode: InForce,
    pub branch: InForce,
    pub sign: InForce,
    pub autosave: InForce,
    pub autosave_after: InForce,
}

/// How far a save of the plane and of each repo goes in this project, and which file decided
/// each key: what the Settings tab's Saving group says beside each control.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SavingInForce {
    pub plane: PlaneInForce,
    /// One per repo `inventory/repos.json` catalogues, in its order, then one per repo only a
    /// file's `[repos]` names.
    pub repos: Vec<RepoInForce>,
    /// Why `charter.local.toml` had no say in `[plane]`, when git would carry it and it set a
    /// save key there (charter-app#319): what the Plane group says.
    pub plane_left_out: Option<String>,
    /// The same for `[repos]`: what the Repos group says.
    pub repos_left_out: Option<String>,
}

/// The plane's and each repo's save settings in force — `planesave::Settings`, the one
/// resolver every save asks, shaped for the wire.
///
/// On a blocking thread: the Local file's check asks git whether it is ignored.
#[tauri::command]
#[specta::specta]
pub async fn project_saving_in_force(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<SavingInForce, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || saving_in_force(&root))
        .await
        .map_err(|err| format!("reading how this project saves did not finish: {err}"))
}

/// [`project_saving_in_force`], without a runtime.
pub(crate) fn saving_in_force(root: &std::path::Path) -> SavingInForce {
    use purlis_core::planesave::{Mode, Resolved, Settings};

    fn said<T>(resolved: &Resolved<T>, value: Option<String>) -> InForce {
        InForce {
            value,
            source: resolved.source.as_str().to_owned(),
        }
    }
    let mode = |r: &Resolved<Mode>| said(r, Some(r.value.as_str().to_owned()));
    let branch = |r: &Resolved<Option<String>>| said(r, r.value.clone());
    let on = |r: &Resolved<bool>| said(r, Some(if r.value { "on" } else { "off" }.to_owned()));
    let quiet = |r: &Resolved<std::time::Duration>| said(r, Some(quiet_period(r.value)));

    let settings = Settings::read(root);
    let plane = &settings.plane;
    SavingInForce {
        plane: PlaneInForce {
            mode: said(&plane.mode, plane.mode.value.map(|m| m.as_str().to_owned())),
            from_share: plane.from_share,
            branch: branch(&plane.branch),
            save_branch: branch(&plane.save_branch),
            sign: on(&plane.sign),
            autosave: on(&plane.autosave),
            autosave_after: quiet(&plane.autosave_after),
        },
        repos: settings
            .repo_names(root)
            .into_iter()
            .map(|name| {
                let repo = settings.repo(&name);
                RepoInForce {
                    mode: mode(&repo.mode),
                    branch: branch(&repo.branch),
                    sign: on(&repo.sign),
                    autosave: on(&repo.autosave),
                    autosave_after: quiet(&repo.autosave_after),
                    name,
                }
            })
            .collect(),
        plane_left_out: settings.plane_left_out.clone(),
        repos_left_out: settings.repos_left_out.clone(),
    }
}

/// A quiet period as the files write it: whole minutes as `2m`, anything else as seconds.
fn quiet_period(period: std::time::Duration) -> String {
    let secs = period.as_secs();
    if secs > 0 && secs.is_multiple_of(60) {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

// ---------------------------------------------------------------------------------------
// A workspace's settings (charter-app#280)
// ---------------------------------------------------------------------------------------

/// A workspace's settings — the `settings` of its `workspace.json` — as Settings draws them at
/// the Workspace level. The same shape as a [`SettingsFile`]; its whole text is edited under Edit
/// as JSON (NO-7, #1232), as a project's files are under Edit as TOML.
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
    /// What charter does not take from its settings as they stand, in the core's words, each
    /// with its key under `settings` (#1292).
    pub refusals: Vec<SettingsRefusal>,
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

/// Change one workspace's `workspace.json`: a form's changes to its settings, written with every
/// other key kept, or Edit as JSON's whole text (NO-7, #1232) — each checked by the readers of
/// the project's files (`purlis_core::settings::workspace`).
///
/// `base` is the text the window read (`null`: the file was not there), so a file changed on
/// disk since is refused rather than overwritten.
#[tauri::command]
#[specta::specta]
pub async fn save_workspace_settings(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    base: Option<String>,
    change: SettingsChange,
) -> Result<WorkspaceSettingsSaved, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        save_workspace(&root, &workspace, base.as_deref(), change)
    })
    .await
    .map_err(|err| format!("saving the workspace's settings did not finish: {err}"))?
}

/// [`save_workspace_settings`], without a runtime.
pub(crate) fn save_workspace(
    root: &std::path::Path,
    workspace: &str,
    base: Option<&str>,
    change: SettingsChange,
) -> Result<WorkspaceSettingsSaved, String> {
    let saved = match change {
        SettingsChange::Raw { text } => {
            settings::workspace::save_text(root, workspace, base, &text)
        }
        SettingsChange::Edits { edits } => {
            let edits: Vec<Edit> = edits.into_iter().map(edit_of).collect::<Result<_, _>>()?;
            settings::workspace::save(root, workspace, base, &edits)
        }
    };
    match saved {
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
        refusals: read.refusals.into_iter().map(Into::into).collect(),
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

/// Exact up to 2^53, which is every number a form writes; beyond it Edit as TOML shows the
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
        path: edit.path.into_iter().map(core_step).collect(),
        value: edit.value.map(value_to_core).transpose()?,
    })
}

fn core_step(step: SettingsStep) -> Step {
    match step {
        SettingsStep::Key(key) => Step::Key(key),
        SettingsStep::Index(at) => Step::Index(at as usize),
    }
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
            return Err("that value is only changed under Edit as TOML".to_owned());
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1405: the field's check is the core's parser, word for word.
    #[test]
    fn the_host_field_is_checked_by_the_cores_parser() {
        assert_eq!(check_sandbox_host("api.example.com:8443".to_owned()), None);
        assert_eq!(check_sandbox_host("*.example.com".to_owned()), None);
        for typed in ["https://api.example.com/v1", "10.0.0.0/8", "a b", ""] {
            let said = check_sandbox_host(typed.to_owned());
            assert_eq!(
                said,
                purlis_core::sandbox::hosts::Host::parse(typed).err(),
                "{typed}"
            );
            assert!(said.is_some(), "{typed}");
        }
    }

    fn plane(shared: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), shared).unwrap();
        dir
    }

    #[test]
    fn a_forge_is_added_and_its_field_refusals_and_users_reach_the_wire_by_name() {
        let text = "schema = 1\n";
        let dir = plane(text);
        let entry = |kind: &str, host: &str| ForgeEntry {
            kind: kind.into(),
            owner: "acme".into(),
            host: host.into(),
            exclude: vec![],
        };
        let EntryWritten::Refused { fields, .. } =
            add_forge(dir.path(), Some(text), entry("svn", "")).unwrap()
        else {
            panic!("an unknown kind is refused");
        };
        assert_eq!(fields[0].field, "kind");

        let EntryWritten::Saved { file, .. } =
            add_forge(dir.path(), Some(text), entry("gitlab", "git.acme.dev")).unwrap()
        else {
            panic!("a good forge is written");
        };
        assert!(
            file.text.contains("host = \"git.acme.dev\""),
            "{}",
            file.text
        );

        std::fs::create_dir_all(dir.path().join("inventory")).unwrap();
        std::fs::write(
            dir.path().join("inventory/repos.json"),
            r#"{"repos": [{"name": "billing", "ssh_url": "git@git.acme.dev:acme/billing.git"}]}"#,
        )
        .unwrap();
        let EntryWritten::Refused { referrers, .. } =
            remove_forge(dir.path(), Some(&file.text), &file.entries.unwrap()[0].id).unwrap()
        else {
            panic!("a forge a repo is on is not removed");
        };
        assert_eq!(
            referrers,
            [EntryReferrer {
                what: "The repo billing (inventory/repos.json) is on git.acme.dev.".into(),
                group: None,
                follows: false,
            }]
        );
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
        assert_eq!(file.refusals[0].key, None, "the whole file names no key");
    }

    /// #1292: each standing refusal reaches the wire with the key it is about, so the window
    /// links it without reading the sentence, an extension id with dots in it as one step.
    #[test]
    fn a_refusal_is_on_the_wire_with_its_key() {
        let dir = plane("[extensions.\"my.ext\"]\nenabled = 1\n");
        let file = file_of(dir.path(), SettingsWhich::Shared).unwrap();
        assert_eq!(
            file.refusals,
            vec![SettingsRefusal {
                why: "extensions.my.ext.enabled in charter.toml is not true or false".into(),
                key: Some(vec!["extensions".into(), "my.ext".into(), "enabled".into()]),
            }]
        );
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
            save_workspace(dir.path(), "alpha", Some(manifest), edits(enabled(false))).unwrap();
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

    fn edits(one: SettingsEdit) -> SettingsChange {
        SettingsChange::Edits { edits: vec![one] }
    }

    #[test]
    fn a_workspace_json_saved_as_text_is_written_through_the_core_and_read_back() {
        let manifest = "{\n  \"name\": \"alpha\"\n}\n";
        let dir = with_workspace(manifest);
        let text = "{\"name\": \"alpha\", \"settings\": {\"extensions\": {\"stats\": {\"enabled\": true}}}}\n";
        let saved = save_workspace(
            dir.path(),
            "alpha",
            Some(manifest),
            SettingsChange::Raw { text: text.into() },
        )
        .unwrap();
        let WorkspaceSettingsSaved::Saved { settings } = saved else {
            panic!("saved: {saved:?}")
        };
        assert_eq!(settings.text, text);
        assert_eq!(settings.fields.len(), 1, "{:?}", settings.fields);
        let refused = save_workspace(
            dir.path(),
            "alpha",
            Some(text),
            SettingsChange::Raw { text: "{".into() },
        )
        .unwrap();
        assert!(
            matches!(&refused, WorkspaceSettingsSaved::Refused { reasons } if reasons[0].contains("would not be a JSON object")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_refused_workspace_save_answers_the_cores_sentences() {
        let manifest = "{\n  \"name\": \"alpha\"\n}\n";
        let dir = with_workspace(manifest);
        let saved = save_workspace(dir.path(), "alpha", Some("{}"), edits(enabled(true))).unwrap();
        let WorkspaceSettingsSaved::Refused { reasons } = saved else {
            panic!("refused: {saved:?}")
        };
        assert!(reasons[0].contains("changed on disk"), "{reasons:?}");
    }

    #[test]
    fn a_profile_is_added_renamed_and_refused_on_the_wire_by_field_and_by_its_users() {
        let dir = plane_with_local("schema = 1\n[harness]\ndefault = \"work\"\n", "");
        let root = dir.path();
        let entry = |name: &str| ProfileEntry {
            name: name.into(),
            kind: "claude".into(),
            command: vec!["claude".into()],
        };
        let EntryWritten::Refused { fields, .. } =
            add_profile(root, Some(""), entry("a.b")).unwrap()
        else {
            panic!("a name the rules refuse is not written");
        };
        assert_eq!(fields[0].field, "name");

        let EntryWritten::Saved { file, added, .. } =
            add_profile(root, Some(""), entry("spare")).unwrap()
        else {
            panic!("a good profile is written");
        };
        let entries = file.entries.clone().unwrap();
        assert_eq!(entries[0].collection, "profiles");
        assert_eq!(added.as_deref(), Some(entries[0].id.as_str()));

        let EntryWritten::Saved { file, .. } =
            rename_profile(root, Some(&file.text), &entries[0].id, "work", false).unwrap()
        else {
            panic!("a profile nothing uses is renamed");
        };
        let EntryWritten::Refused { referrers, .. } =
            remove_profile(root, Some(&file.text), &file.entries.unwrap()[0].id).unwrap()
        else {
            panic!("the profile the default harness names is not removed");
        };
        assert_eq!(referrers[0].group.as_deref(), Some("project.harness"));
        // The project's default is every teammate's: a rename everywhere does not change it.
        assert!(!referrers[0].follows);
    }

    /// A plane that is a git repo whose `charter.local.toml` is ignored, so the Local layer is
    /// read. git for a fixture never reads the developer's global config.
    fn plane_with_local(shared: &str, local: &str) -> tempfile::TempDir {
        let dir = plane(shared);
        let root = dir.path();
        std::fs::write(root.join(".gitignore"), "/charter.local.toml\n").unwrap();
        std::fs::write(root.join("charter.local.toml"), local).unwrap();
        let mut git = std::process::Command::new("git");
        git.arg("-C")
            .arg(root)
            .args(["init", "-q"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        let out = purlis_core::forklock::output(&mut git).expect("git runs");
        assert!(out.status.success());
        dir
    }

    fn in_force(value: Option<&str>, source: &str) -> InForce {
        InForce {
            value: value.map(str::to_owned),
            source: source.to_owned(),
        }
    }

    #[test]
    fn the_planes_save_settings_in_force_say_the_file_that_decided_each() {
        let dir = plane_with_local(
            "[plane]\nmode = \"pr\"\nsign = true\nautosave_after = \"120s\"\n",
            "[plane]\nmode = \"push\"\nbranch = \"trunk\"\n",
        );
        let got = saving_in_force(dir.path()).plane;
        assert_eq!(
            got,
            PlaneInForce {
                mode: in_force(Some("push"), "local"),
                from_share: false,
                branch: in_force(Some("trunk"), "local"),
                save_branch: in_force(None, "default"),
                sign: in_force(Some("on"), "shared"),
                autosave: in_force(Some("on"), "default"),
                autosave_after: in_force(Some("2m"), "shared"),
            }
        );
    }

    #[test]
    fn a_mode_from_memory_share_says_so() {
        let dir = plane("[memory]\nshare = \"commit\"\n");
        let got = saving_in_force(dir.path()).plane;
        assert_eq!(got.mode, in_force(Some("commit"), "shared"));
        assert!(got.from_share);
        let unset = saving_in_force(plane("schema = 1\n").path()).plane;
        assert_eq!(unset.mode, in_force(None, "default"));
        assert!(!unset.from_share);
    }

    #[test]
    fn every_catalogued_repo_has_a_row_with_its_settings_in_force() {
        let dir = plane_with_local(
            "[repos.api]\nmode = \"push\"\nautosave_after = \"45s\"\n",
            "[repos.api]\nautosave = true\n",
        );
        std::fs::create_dir(dir.path().join("inventory")).unwrap();
        std::fs::write(
            dir.path().join("inventory/repos.json"),
            r#"{"group": "acme", "count": 2, "repos": [{"name": "web"}, {"name": "api"}]}"#,
        )
        .unwrap();
        let got = saving_in_force(dir.path());
        assert_eq!(
            got.repos,
            [
                RepoInForce {
                    name: "web".into(),
                    mode: in_force(Some("off"), "default"),
                    branch: in_force(None, "default"),
                    sign: in_force(Some("off"), "default"),
                    autosave: in_force(Some("off"), "default"),
                    autosave_after: in_force(Some("30s"), "default"),
                },
                RepoInForce {
                    name: "api".into(),
                    mode: in_force(Some("push"), "shared"),
                    branch: in_force(None, "default"),
                    sign: in_force(Some("off"), "default"),
                    autosave: in_force(Some("on"), "local"),
                    autosave_after: in_force(Some("45s"), "shared"),
                },
            ]
        );
        assert_eq!((got.plane_left_out, got.repos_left_out), (None, None));
    }

    #[test]
    fn a_move_answers_both_files_as_they_now_stand() {
        let shared = "[plane]\nmode = \"push\"\n";
        let dir = plane_with_local(shared, "");
        let moved = move_keys(
            dir.path(),
            SettingsWhich::Local,
            Some(shared),
            Some(""),
            vec![vec![
                SettingsStep::Key("plane".into()),
                SettingsStep::Key("mode".into()),
            ]],
        )
        .unwrap();
        let SettingsMoved::Moved { settings } = moved else {
            panic!("moved: {moved:?}")
        };
        assert!(settings.shared.fields.is_empty(), "{:?}", settings.shared);
        assert_eq!(
            settings.local.fields,
            [SettingsField {
                path: vec![
                    SettingsStep::Key("plane".into()),
                    SettingsStep::Key("mode".into()),
                ],
                value: SettingsValue::Text("push".into()),
            }]
        );
    }

    /// #1550: the project's hosts a move brings into the shared file are the mover's own change
    /// too, and the hosts Notice does not tell them back.
    #[test]
    fn hosts_moved_into_the_shared_file_are_seen_by_whoever_moved_them() {
        let shared = "[sandbox]\nmode = \"on\"\n";
        let local = "[sandbox]\nhosts = [\"api.example.com\"]\n";
        let dir = plane_with_local(shared, local);
        assert_eq!(purlis_core::sandbox::local::hosts_changed(dir.path()), None);
        let moved = move_keys(
            dir.path(),
            SettingsWhich::Shared,
            Some(shared),
            Some(local),
            vec![vec![
                SettingsStep::Key("sandbox".into()),
                SettingsStep::Key("hosts".into()),
            ]],
        )
        .unwrap();
        assert!(matches!(moved, SettingsMoved::Moved { .. }), "{moved:?}");
        assert!(
            std::fs::read_to_string(dir.path().join("charter.toml"))
                .unwrap()
                .contains("api.example.com"),
            "the host is in the shared file now"
        );
        assert_eq!(purlis_core::sandbox::local::hosts_changed(dir.path()), None);
    }

    /// #1550: presets a move brings into the shared file are this machine's own change, as a
    /// save's are, and are not told back to the person who moved them.
    #[test]
    fn presets_moved_into_the_shared_file_are_seen_by_whoever_moved_them() {
        let shared = "[sandbox]\nmode = \"on\"\n";
        let local = "[sandbox]\negress = [\"forge\"]\n";
        let dir = plane_with_local(shared, local);
        assert_eq!(
            purlis_core::sandbox::local::presets_changed(dir.path()),
            None,
            "a project on the defaults tells nothing"
        );
        let moved = move_keys(
            dir.path(),
            SettingsWhich::Shared,
            Some(shared),
            Some(local),
            vec![vec![
                SettingsStep::Key("sandbox".into()),
                SettingsStep::Key("egress".into()),
            ]],
        )
        .unwrap();
        assert!(matches!(moved, SettingsMoved::Moved { .. }), "{moved:?}");
        assert!(
            std::fs::read_to_string(dir.path().join("charter.toml"))
                .unwrap()
                .contains("egress"),
            "the presets are in the shared file now"
        );
        assert_eq!(
            purlis_core::sandbox::local::presets_changed(dir.path()),
            None
        );
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
