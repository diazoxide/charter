//! **The Inbox's updates, for the window** (#1693, spec #1688): the app's half of
//! [`purlis_core::inboxupdates`], which argues what an update is, where it is kept and for how
//! long.
//!
//! Three commands, each about one project the window holds:
//!
//! - [`inbox_updates`] reads its updates, newest first;
//! - [`note_inbox_updates`] notes what the window derived from a source (a finished row, the
//!   doctor's report, a chat put back, a block, the away list, a Smart close's step): one whose
//!   key is kept already is left as it is, so noting is safe to repeat;
//! - [`settle_inbox_updates`] marks read or dismisses the ones named, or every one: Mark all
//!   read and Dismiss all (I-10), which act on updates only. **No ask is ever settled here**:
//!   an ask is answered through its own source (`asking`), one at a time.
//!
//! **The window's alone** (`purlis_session_protocol::ui::WINDOW_ONLY`): what happened to the
//! person's chats, by name, is no link's to read, and what they put away no link's to change.
//! Each answers off the window's thread, since a write flushes the file to the disk.

use std::path::PathBuf;

use purlis_core::inboxupdates::{Kind, Settle, Store, Update};

use crate::planes::{PlaneId, Planes};

/// What kind of thing happened, as the window names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateKind {
    /// A task finished: done, cancelled, or ended by the person.
    TaskDone,
    /// A task came to nothing: failed, blocked, ended without a report, or did not start.
    TaskFailed,
    /// The doctor found something.
    Doctor,
    /// A chat came back: resumed, or started fresh in its place.
    Resumed,
    /// A chat's sandbox refused something that is no ask.
    Sandbox,
    /// A dispatch was refused while nobody was at its chat (#1507).
    RefusedAway,
    /// A Smart close stopped without its record (SI-8f).
    SmartClose,
}

impl From<UpdateKind> for Kind {
    fn from(kind: UpdateKind) -> Self {
        match kind {
            UpdateKind::TaskDone => Kind::TaskDone,
            UpdateKind::TaskFailed => Kind::TaskFailed,
            UpdateKind::Doctor => Kind::Doctor,
            UpdateKind::Resumed => Kind::Resumed,
            UpdateKind::Sandbox => Kind::Sandbox,
            UpdateKind::RefusedAway => Kind::RefusedAway,
            UpdateKind::SmartClose => Kind::SmartClose,
        }
    }
}

impl From<Kind> for UpdateKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::TaskDone => UpdateKind::TaskDone,
            Kind::TaskFailed => UpdateKind::TaskFailed,
            Kind::Doctor => UpdateKind::Doctor,
            Kind::Resumed => UpdateKind::Resumed,
            Kind::Sandbox => UpdateKind::Sandbox,
            Kind::RefusedAway => UpdateKind::RefusedAway,
            Kind::SmartClose => UpdateKind::SmartClose,
        }
    }
}

/// One update, as the window draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct InboxUpdate {
    /// Its source's own name for it: what the window notes, settles and matches it by.
    pub key: String,
    pub kind: UpdateKind,
    /// When it happened, in seconds since 1970.
    pub at: u32,
    /// The chat it is about, while that number means one.
    pub session: Option<u32>,
    /// Who it is about, the session first: names, as data.
    pub chain: Vec<String>,
    /// What happened, in one line: data, never markup.
    pub says: String,
    /// The person has read it.
    pub read: bool,
}

impl From<Update> for InboxUpdate {
    fn from(one: Update) -> Self {
        Self {
            key: one.key,
            kind: one.kind.into(),
            at: u32::try_from(one.at).unwrap_or(u32::MAX),
            session: one.session,
            chain: one.chain,
            says: one.says,
            read: one.read,
        }
    }
}

/// An update the window derived from its source, to be noted.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
pub struct UpdateNoted {
    pub key: String,
    pub kind: UpdateKind,
    /// When it happened, by its source's own time, in seconds since 1970.
    pub at: u32,
    pub session: Option<u32>,
    pub chain: Vec<String>,
    pub says: String,
}

impl From<UpdateNoted> for Update {
    fn from(one: UpdateNoted) -> Self {
        Self {
            key: one.key,
            kind: one.kind.into(),
            at: u64::from(one.at),
            session: one.session,
            chain: one.chain,
            says: one.says,
            read: false,
            dismissed: false,
        }
    }
}

/// What the person did with updates: Mark read, or Dismiss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateSettled {
    Read,
    Dismissed,
}

impl From<UpdateSettled> for Settle {
    fn from(how: UpdateSettled) -> Self {
        match how {
            UpdateSettled::Read => Settle::Read,
            UpdateSettled::Dismissed => Settle::Dismissed,
        }
    }
}

/// What a write says where this machine keeps no data home.
const NOWHERE: &str =
    "purlis has nowhere to keep the Inbox's updates on this machine: it has no data home.";

/// The project's root, for a command about it.
fn root_of(planes: &Planes, plane: &PlaneId) -> Result<PathBuf, String> {
    Ok(planes.held(plane)?.root().to_path_buf())
}

/// Runs `work` off the window's thread, and says so where it did not finish.
async fn off_the_window<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| format!("reading the Inbox's updates did not finish: {err}"))?
}

/// [`Store::read`] as the window draws it.
pub fn read_in(store: Option<&Store>, root: &std::path::Path, now: u64) -> Vec<InboxUpdate> {
    store
        .map(|store| store.read(root, now))
        .unwrap_or_default()
        .into_iter()
        .map(InboxUpdate::from)
        .collect()
}

/// [`Store::note`] as the window asks it.
pub fn note_in(
    store: Option<&Store>,
    root: &std::path::Path,
    noted: Vec<UpdateNoted>,
    now: u64,
) -> Result<Vec<InboxUpdate>, String> {
    let store = store.ok_or_else(|| NOWHERE.to_owned())?;
    store
        .note(root, noted.into_iter().map(Update::from).collect(), now)
        .map(|all| all.into_iter().map(InboxUpdate::from).collect())
        .map_err(|err| format!("purlis could not keep the Inbox's updates: {err}"))
}

/// [`Store::settle`] as the window asks it.
pub fn settle_in(
    store: Option<&Store>,
    root: &std::path::Path,
    keys: Option<Vec<String>>,
    how: UpdateSettled,
    now: u64,
) -> Result<Vec<InboxUpdate>, String> {
    let store = store.ok_or_else(|| NOWHERE.to_owned())?;
    store
        .settle(root, keys.as_deref(), how.into(), now)
        .map(|all| all.into_iter().map(InboxUpdate::from).collect())
        .map_err(|err| format!("purlis could not change the Inbox's updates: {err}"))
}

/// **A project's updates** (#1693): kept a day on this machine, newest first, dismissed ones
/// left out. None where this machine keeps no data home.
#[tauri::command]
#[specta::specta]
pub async fn inbox_updates(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<InboxUpdate>, String> {
    let root = root_of(&planes, &plane)?;
    off_the_window(move || {
        Ok(read_in(
            Store::here().as_ref(),
            &root,
            crate::sandboxing::now_secs(),
        ))
    })
    .await
}

/// **Notes what the window derived from a source** (#1693): each update whose key the project
/// does not keep yet is kept, unread; one it keeps, dismissed or not, is left as it is. One
/// older than a day is not kept. Answers the project's updates as [`inbox_updates`] does.
#[tauri::command]
#[specta::specta]
pub async fn note_inbox_updates(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    noted: Vec<UpdateNoted>,
) -> Result<Vec<InboxUpdate>, String> {
    let root = root_of(&planes, &plane)?;
    off_the_window(move || {
        note_in(
            Store::here().as_ref(),
            &root,
            noted,
            crate::sandboxing::now_secs(),
        )
    })
    .await
}

/// **Mark read, or Dismiss** (#1693, I-10): the updates named by `keys`, or every one the
/// project keeps where `keys` is null — Mark all read and Dismiss all. Updates only: no ask is
/// answered here. Answers the project's updates as [`inbox_updates`] does.
#[tauri::command]
#[specta::specta]
pub async fn settle_inbox_updates(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    keys: Option<Vec<String>>,
    how: UpdateSettled,
) -> Result<Vec<InboxUpdate>, String> {
    let root = root_of(&planes, &plane)?;
    off_the_window(move || {
        settle_in(
            Store::here().as_ref(),
            &root,
            keys,
            how,
            crate::sandboxing::now_secs(),
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn noted(key: &str, kind: UpdateKind, at: u64) -> UpdateNoted {
        UpdateNoted {
            key: key.to_owned(),
            kind,
            at: u32::try_from(at).expect("a time"),
            session: Some(4),
            chain: vec!["steward 12".to_owned()],
            says: "drill finished".to_owned(),
        }
    }

    #[test]
    fn the_window_notes_reads_and_settles_a_project_s_updates() {
        let dir = tempfile::tempdir().expect("a directory");
        let store = Store::in_data(&dir.path().join("data"));
        let root = dir.path().join("project");
        std::fs::create_dir_all(&root).expect("a project");

        let after = note_in(
            Some(&store),
            &root,
            vec![
                noted("task:a", UpdateKind::TaskDone, NOW - 60),
                noted("task:b", UpdateKind::TaskFailed, NOW - 30),
            ],
            NOW,
        )
        .expect("noted");
        assert_eq!(
            after.iter().map(|one| one.key.as_str()).collect::<Vec<_>>(),
            ["task:b", "task:a"]
        );
        assert_eq!(after[0].kind, UpdateKind::TaskFailed);
        assert!(!after[0].read);

        let read = settle_in(Some(&store), &root, None, UpdateSettled::Read, NOW).expect("read");
        assert!(read.iter().all(|one| one.read));

        let left = settle_in(
            Some(&store),
            &root,
            Some(vec!["task:a".to_owned()]),
            UpdateSettled::Dismissed,
            NOW,
        )
        .expect("dismissed");
        assert_eq!(
            left.iter().map(|one| one.key.as_str()).collect::<Vec<_>>(),
            ["task:b"]
        );
        assert_eq!(read_in(Some(&store), &root, NOW), left);
    }

    #[test]
    fn a_machine_with_no_data_home_reads_none_and_says_why_it_keeps_none() {
        let root = std::path::Path::new("/nowhere");
        assert!(read_in(None, root, NOW).is_empty());
        let refused = note_in(None, root, vec![], NOW).expect_err("nowhere to keep it");
        assert!(refused.contains("data home"), "{refused}");
    }

    #[test]
    fn every_kind_is_the_core_s_and_back() {
        for kind in [
            UpdateKind::TaskDone,
            UpdateKind::TaskFailed,
            UpdateKind::Doctor,
            UpdateKind::Resumed,
            UpdateKind::Sandbox,
            UpdateKind::RefusedAway,
            UpdateKind::SmartClose,
        ] {
            assert_eq!(UpdateKind::from(Kind::from(kind)), kind);
        }
    }

    #[test]
    fn the_updates_are_the_window_s_alone() {
        for command in [
            "inbox_updates",
            "note_inbox_updates",
            "settle_inbox_updates",
        ] {
            assert!(
                purlis_session_protocol::ui::WINDOW_ONLY.contains(&command),
                "{command} is the window's alone"
            );
        }
    }
}
