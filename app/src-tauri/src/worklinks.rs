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
//!   cannot be linked (ADR 0088 §4), and one working outside the project is told that instead.
//! - **The item is a tracker key in its normal form** (ADR 0088 §1), as the Work list (FW-9)
//!   will hand it over. One that is not is refused, never rewritten (#861's D-0007).
//!
//! The window offers them as **Link to work item…** and **Unlink work item** on a chat's tab menu
//! and in the palette, and shows **Work item: `<key>`** on a linked chat (V60, #914).

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

/// The chat a work link is about: its ULID and where the window files it.
struct LinkedChat {
    id: String,
    place: Filed,
}

/// Where the window files a chat, for its work link.
enum Filed {
    Workspace(String),
    /// At the project root: in no workspace on purpose (ADR 0088 §4).
    ProjectRoot,
    /// Working in a directory outside the project altogether.
    Outside,
}

impl LinkedChat {
    fn of(root: &Path, chats: &Chats, session: u32) -> Result<LinkedChat, String> {
        let Some(at) = chats.chat_at(session) else {
            return Err(format!("charter has no chat {session} open."));
        };
        let Some(id) = at.id else {
            return Err(format!(
                "chat {session} has no id yet, so it cannot have a work link."
            ));
        };
        let place = match at.cwd.as_deref() {
            None => Filed::ProjectRoot,
            Some(cwd) => match charter_core::workspaces::Plane::open(root).workspace_of(cwd) {
                Some(ws) => Filed::Workspace(ws),
                None if charter_core::active::inside_plane(root, cwd) => Filed::ProjectRoot,
                None => Filed::Outside,
            },
        };
        Ok(LinkedChat { id, place })
    }

    /// The core's place for this chat, or the refusal for one outside the project.
    fn place(&self, act: Act) -> Result<Place<'_>, String> {
        match &self.place {
            Filed::Workspace(ws) => Ok(Place::Workspace(ws)),
            Filed::ProjectRoot => Ok(Place::ProjectRoot),
            Filed::Outside => Err(format!(
                "this chat works outside the project, so it is in no workspace and cannot be \
                 {} a work item: open a chat in the workspace the item belongs to.",
                act.could_not_be()
            )),
        }
    }
}

/// What a write does to a chat's work link, for its refusal.
#[derive(Debug, Clone, Copy)]
enum Act {
    Link,
    Unlink,
}

impl Act {
    fn could_not_be(self) -> &'static str {
        match self {
            Act::Link => "linked to",
            Act::Unlink => "unlinked from",
        }
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

/// A key as the window is handed it.
fn key_text(item: TrackerKey) -> String {
    item.as_str().to_owned()
}

fn link(
    root: &Path,
    chats: &Chats,
    session: u32,
    item: &str,
    ts: chrono::DateTime<chrono::Utc>,
) -> Result<String, String> {
    let item = TrackerKey::parse(item)?;
    let chat = LinkedChat::of(root, chats, session)?;
    let device = device(chats)?;
    log::link_chat(root, chat.place(Act::Link)?, device, ts, item, &chat.id)
        .map_err(|why| why.to_string())?;
    item_of(root, chats, session)?
        .ok_or_else(|| "the work link was written but does not read back.".to_owned())
}

fn unlink(
    root: &Path,
    chats: &Chats,
    session: u32,
    ts: chrono::DateTime<chrono::Utc>,
) -> Result<Option<String>, String> {
    let chat = LinkedChat::of(root, chats, session)?;
    let device = device(chats)?;
    log::unlink_chat(root, chat.place(Act::Unlink)?, device, ts, &chat.id)
        .map(|ended| ended.map(key_text))
        .map_err(|why| why.to_string())
}

fn item_of(root: &Path, chats: &Chats, session: u32) -> Result<Option<String>, String> {
    let chat = LinkedChat::of(root, chats, session)?;
    Ok(log::fold(root).chat_link(&chat.id).map(key_text))
}

#[cfg(test)]
mod tests;
