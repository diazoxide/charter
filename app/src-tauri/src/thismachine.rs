//! **Settings › You › This machine** (ST-2, #1226): what this machine's store holds — its
//! recent projects, its pins, its approvals — drawn as one list, and each taken back from here.
//!
//! **Keyed by path, never by an open project.** Most of what the store remembers is not open:
//! a recent from last month, a pin in a project on a disk that is not plugged in. So every
//! command here names the project by the path the store holds it under, and acts on the store
//! alone — the same entry points `charter-core` gives the CLI.
//!
//! **Nothing here grants anything.** Forget and Revoke only take away, and an approval comes
//! back only by the operator answering the ask at the next open; an Undo is offered only for an
//! unpin, which puts back a pin and nothing else.

use std::path::Path;

use charter_core::machine;

use crate::planes::Planes;

/// One workspace pin, as This machine lists it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct MachineWorkspacePin {
    /// The workspace's name, as it was pinned.
    pub name: String,
    /// Whether the project no longer has a workspace by that name. Such a pin is kept
    /// **dormant** (V91c as amended): hidden from the strip, and back in its place when the
    /// workspace returns; This machine lists it so it can be forgotten for good. False where
    /// the project itself cannot be read, since then nothing is known about its workspaces.
    pub gone: bool,
}

/// One project this machine remembers, with what it holds about it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct MachineProject {
    /// The path the store holds it under: what every command here is handed back.
    pub path: String,
    /// The directory's own name.
    pub name: String,
    /// Why it is no longer a project here — moved, or gone — or null when it is one.
    pub gone: Option<String>,
    /// Whether this machine approved it (the opener's trust ask).
    pub approved: bool,
    /// Whether the project itself is pinned.
    pub pinned: bool,
    /// Its workspace pins, in the order they were pinned in.
    pub workspace_pins: Vec<MachineWorkspacePin>,
}

/// Everything This machine lists.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ThisMachine {
    /// Most recently opened first.
    pub projects: Vec<MachineProject>,
    /// What the store would not take back, one line each.
    pub dropped: Vec<String>,
    /// Why this machine keeps no store at all, or null when it does.
    pub forgetful: Option<String>,
}

/// What this machine's store holds, each project checked against the disk.
///
/// **On a blocking thread**, for `recent_planes`' reason: each row costs a `stat`, and a
/// remembered project can be on a share that is not coming back.
#[tauri::command]
#[specta::specta]
pub async fn this_machine(planes: tauri::State<'_, Planes>) -> Result<ThisMachine, String> {
    let loaded = planes.remembered();
    tauri::async_runtime::spawn_blocking(move || listed(loaded))
        .await
        .map_err(|err| format!("reading what this machine remembers did not finish: {err}"))
}

/// [`this_machine`] with the store already read.
fn listed(loaded: machine::Loaded) -> ThisMachine {
    let projects = loaded
        .store
        .recents
        .iter()
        .map(|entry| {
            let shown = entry.plane.display().to_string();
            let gone = machine::still_a_plane(&entry.plane).err();
            // The project's own list says which pins still resolve; one that cannot be read
            // says nothing, and no pin is called gone on a guess.
            let there = if gone.is_none() {
                charter_core::workspaces::Plane::open(&entry.plane)
                    .workspaces()
                    .ok()
            } else {
                None
            };
            MachineProject {
                name: entry
                    .plane
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| shown.clone()),
                path: shown,
                gone,
                approved: entry.trust.is_some(),
                pinned: entry.pinned,
                workspace_pins: entry
                    .pinned_workspaces
                    .iter()
                    .map(|name| MachineWorkspacePin {
                        name: name.clone(),
                        gone: there.as_ref().is_some_and(|all| !all.contains(name)),
                    })
                    .collect(),
            }
        })
        .collect();
    ThisMachine {
        projects,
        dropped: loaded.dropped.iter().map(ToString::to_string).collect(),
        forgetful: loaded.unreadable,
    }
}

/// Forgets a project on this machine: its recent, its approval, its pins and any tab a launch
/// would put back. A project that is gone from the disk is forgotten the same way.
#[tauri::command]
#[specta::specta]
pub fn forget_project(planes: tauri::State<'_, Planes>, path: String) -> Result<(), String> {
    planes.forget(Path::new(&path))
}

/// Re-points a remembered project that is gone (moved, or on a disk that is not here) at a
/// folder the operator picked, and answers the project found there (NO-5). The folder is
/// checked first, as an open checks it, and the approval does not travel: the next open asks.
#[tauri::command]
#[specta::specta]
pub fn locate_project(
    planes: tauri::State<'_, Planes>,
    gone: String,
    picked: String,
) -> Result<String, String> {
    planes
        .locate(Path::new(&gone), Path::new(&picked))
        .map(|found| found.display().to_string())
}

/// Revokes this machine's approval of a project. It stays remembered and pinned; the next open
/// asks again, as a first open does.
#[tauri::command]
#[specta::specta]
pub fn revoke_approval(planes: tauri::State<'_, Planes>, path: String) -> Result<(), String> {
    planes.revoke(Path::new(&path))
}

/// Pins or unpins a project, or one workspace in it, by the path the store holds it under —
/// whether or not it is open. `at` puts a workspace pin back at that place among the project's
/// workspace pins (the Undo of an unpin); without it a pin goes last.
#[tauri::command]
#[specta::specta]
pub fn pin_on_this_machine(
    planes: tauri::State<'_, Planes>,
    path: String,
    workspace: Option<String>,
    pinned: bool,
    at: Option<u32>,
) -> Result<(), String> {
    let root = Path::new(&path);
    match (workspace, at) {
        (Some(name), Some(at)) if pinned => {
            planes.pin_workspace_at(root, &name, usize::try_from(at).unwrap_or(usize::MAX))
        }
        (workspace, _) => planes.pin(root, workspace.as_deref(), pinned),
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_to_a_workspace_the_project_lost_is_listed_as_gone_and_one_it_has_is_not() {
        let planes = tempfile::tempdir().unwrap();
        let root = planes.path().join("p");
        std::fs::create_dir_all(root.join("workspaces/ide")).unwrap();
        std::fs::write(root.join(charter_core::plane::MANIFEST), "").unwrap();
        let away = planes.path().join("away");
        let mut store = machine::Store::default();
        store.remember(&away, 1);
        store.pin_workspace(&away, "old", true).unwrap();
        store.approve(&root, 2, machine::Contribution::default());
        store.pin(&root, true).unwrap();
        store.pin_workspace(&root, "ide", true).unwrap();
        store.pin_workspace(&root, "renamed", true).unwrap();

        let shown = listed(machine::Loaded {
            store,
            ..machine::Loaded::default()
        });

        let here = &shown.projects[0];
        assert_eq!(here.name, "p");
        assert!(here.gone.is_none() && here.approved && here.pinned);
        let pins: Vec<(&str, bool)> = here
            .workspace_pins
            .iter()
            .map(|pin| (pin.name.as_str(), pin.gone))
            .collect();
        assert_eq!(pins, [("ide", false), ("renamed", true)]);
        let there = &shown.projects[1];
        assert!(there.gone.is_some(), "a project not on the disk says so");
        assert!(
            !there.workspace_pins[0].gone,
            "nothing is known of a missing project's workspaces, so no pin is called gone"
        );
    }
}
