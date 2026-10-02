//! A chat's work link, written by the host for the window (ADR 0088 §3, §4, §7; FW-5).
//!
//! A chat works on zero or one work item (V3). The link is a line in this device's work link log
//! of the workspace the chat is in, keyed by the chat's ULID, which V43 keeps stable across a
//! relaunch and a move and mints again in a copy (ADR 0066 as amended). The rules are
//! `charter_core::work::log`'s, and this module adds only what the host knows: which chat a
//! session is, where the window files it, and this device's id.
//!
//! - **Where a chat is** is where the window's sidebar files it: the workspace its directory is
//!   in ([`charter_core::workspaces::Plane::workspace_of`]). A chat filed at the project root
//!   cannot be linked (ADR 0088 §4), and neither can a shell.
//! - **The item is a tracker key in its normal form** (ADR 0088 §1), as the Work list (FW-9)
//!   will hand it over. One that is not is refused, never rewritten (#861's D-0007).
//!
//! **No control in the window calls these yet.** Where the window offers a chat's work link,
//! and in what words, is not fixed by ADR 0088, and is the operator's to rule (#914).

use std::path::Path;

use charter_core::work::TrackerKey;
use charter_core::work::log::{self, Place};

use crate::chats::Chats;
use crate::planes::{PlaneId, Planes};

/// Link the chat in `session` to `item`, and answer the item it now works on, read through its
/// aliases.
#[tauri::command]
#[specta::specta]
pub fn chat_work_link(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    item: String,
) -> Result<String, String> {
    let held = planes.held(&plane)?;
    link(
        held.root(),
        held.chats(),
        session,
        &item,
        chrono::Utc::now(),
    )
}

/// End the work link of the chat in `session`, and answer the item it worked on, or `null` when
/// it had none and nothing was written.
#[tauri::command]
#[specta::specta]
pub fn chat_work_unlink(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Option<String>, String> {
    let held = planes.held(&plane)?;
    unlink(held.root(), held.chats(), session, chrono::Utc::now())
}

/// The work item the chat in `session` works on, read through its aliases, or `null`.
#[tauri::command]
#[specta::specta]
pub fn chat_work_item(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Option<String>, String> {
    let held = planes.held(&plane)?;
    item_of(held.root(), held.chats(), session)
}

/// Who the chat in `session` is, and where it is: its ULID and its place.
struct Subject {
    chat: String,
    workspace: Option<String>,
}

impl Subject {
    fn of(root: &Path, chats: &Chats, session: u32) -> Result<Subject, String> {
        let Some((id, cwd)) = chats.identity_of(session) else {
            return Err(format!("charter has no chat {session} open."));
        };
        let Some(chat) = id else {
            return Err(format!(
                "chat {session} has no id yet, so it cannot have a work link."
            ));
        };
        let workspace = cwd
            .as_deref()
            .and_then(|cwd| charter_core::workspaces::Plane::open(root).workspace_of(cwd));
        Ok(Subject { chat, workspace })
    }

    fn place(&self) -> Place<'_> {
        self.workspace
            .as_deref()
            .map_or(Place::ProjectRoot, Place::Workspace)
    }
}

/// This device's id, which names the log a line goes in. None is refused, never replaced by a
/// hostname (ADR 0088 §7, #861's D-0009).
fn device(chats: &Chats) -> Result<&str, String> {
    chats.device().ok_or_else(|| {
        "this machine has no device id to name its work link log by, so no work link was written."
            .to_owned()
    })
}

fn link(
    root: &Path,
    chats: &Chats,
    session: u32,
    item: &str,
    ts: chrono::DateTime<chrono::Utc>,
) -> Result<String, String> {
    let item = TrackerKey::parse(item)?;
    let subject = Subject::of(root, chats, session)?;
    let device = device(chats)?;
    log::link_chat(root, subject.place(), device, ts, item, &subject.chat)
        .map_err(|why| why.to_string())?;
    log::fold(root)
        .chat_link(&subject.chat)
        .map(|item| item.as_str().to_owned())
        .ok_or_else(|| "the work link was written but does not read back.".to_owned())
}

fn unlink(
    root: &Path,
    chats: &Chats,
    session: u32,
    ts: chrono::DateTime<chrono::Utc>,
) -> Result<Option<String>, String> {
    let subject = Subject::of(root, chats, session)?;
    let device = device(chats)?;
    log::unlink_chat(root, subject.place(), device, ts, &subject.chat)
        .map(|ended| ended.map(|item| item.as_str().to_owned()))
        .map_err(|why| why.to_string())
}

fn item_of(root: &Path, chats: &Chats, session: u32) -> Result<Option<String>, String> {
    let subject = Subject::of(root, chats, session)?;
    Ok(log::fold(root)
        .chat_link(&subject.chat)
        .map(|item| item.as_str().to_owned()))
}

#[cfg(test)]
mod tests;
