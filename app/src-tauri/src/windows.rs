//! The OS windows charter draws, and which of them a project's events go to (ADR 0033,
//! amended 2026-09-26; charter#126).
//!
//! **A window holds projects, and a project is held by one window.** The main window is the one
//! `tauri.conf.json` declares. Every other window is a *split window*: a project tab moved into
//! a window of its own, labelled `window-<n>`. Both run the same page, so a split window has
//! its own palette, its own `F2` listener and its own title bar, and it draws only the projects
//! it holds.
//!
//! What each window holds is [`Showing`]'s, and every question about window identity is asked
//! there: this module is the Tauri side of it. It makes and closes the windows, carries a
//! project from one to another, and sends a project's events to the window holding it and to
//! no other.
//!
//! The capabilities grant the same commands to every label [`is_charter_window`] accepts, and to
//! no other label (ADR 0052, amended 2026-09-26). A window's label is Tauri's, read from the
//! invoking window, so a page cannot claim to be another window.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::opener::WindowTabs;
use crate::planes::{Holding, PlaneId, Planes, Showing};

/// The window `tauri.conf.json` declares: the one a launch opens, the one the tray and the dock
/// bring back, and the one a closed split window hands its projects to.
pub const MAIN: &str = "main";

/// What every split window's label starts with.
pub const SPLIT_PREFIX: &str = "window-";

/// The split windows' labels as the capabilities grant them: `window-` and a digit. Every
/// label [`split_label`] makes matches it, and `ipc.rs` holds the two together against Tauri's
/// real access-control list. Read only by those tests: the capabilities are JSON.
#[cfg(test)]
pub const SPLIT_GLOB: &str = "window-[0-9]*";

/// The event a window is sent when projects are moved into it, or handed back by a split
/// window that closed: [`ProjectsArrived`].
pub const PROJECTS_ARRIVED: &str = "projects-arrived";

/// The event every window is sent when a window is made or goes away: the labels of the
/// windows there are now. A window reads it to know which others to hear from before a quit.
pub const WINDOWS_CHANGED: &str = "windows-changed";

/// The label of the `n`th split window this process made.
pub fn split_label(n: u32) -> String {
    format!("{SPLIT_PREFIX}{n}")
}

/// Whether `label` is one of charter's windows: the main window, or a split window.
///
/// **The same test as the capabilities' `window-[0-9]*`**, `window-` and then a digit, so the
/// windows charter treats as its own and the windows the access-control list grants are one
/// set (`ipc.rs` holds them together). charter only ever makes `window-<n>`; the page cannot
/// make a window at all.
pub fn is_charter_window(label: &str) -> bool {
    label == MAIN
        || label
            .strip_prefix(SPLIT_PREFIX)
            .is_some_and(|rest| rest.starts_with(|first: char| first.is_ascii_digit()))
}

/// The order windows are listed and remembered in: the main window, then the split windows by
/// number (so `window-2` before `window-10`), then anything else by name.
pub(crate) fn order_key(label: &str) -> (u8, u64, String) {
    if label == MAIN {
        return (0, 0, String::new());
    }
    match label
        .strip_prefix(SPLIT_PREFIX)
        .and_then(|n| n.parse::<u64>().ok())
    {
        Some(n) => (1, n, String::new()),
        None => (2, 0, label.to_owned()),
    }
}

/// Projects moved into a window, and the one it is to bring to the front — `None` when they go
/// in behind what it is showing, which is what a closed split window's projects do.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectsArrived {
    pub planes: Vec<PlaneId>,
    pub front: Option<PlaneId>,
}

/// Sends `event` to the window holding `plane`, **and only to it**.
///
/// A project no window has said it holds yet — one being opened, whose window has not drawn it
/// — is sent to every window, and each one's listener checks the plane as it always has. That
/// is the cheap direction: an event a window ignores costs a comparison, and one that no window
/// heard is a chat whose state nobody drew.
pub fn emit_for_plane<R: Runtime, S: Serialize + Clone>(
    app: &AppHandle<R>,
    plane: &PlaneId,
    event: &str,
    payload: S,
) {
    let holder = app
        .try_state::<Showing>()
        .and_then(|showing| showing.holder(plane));
    let _ = match holder {
        Some(label) => app.emit_to(label.as_str(), event, payload),
        None => app.emit(event, payload),
    };
}

/// Every charter window there is, by label, the main window first — less `going`, a window
/// that is being destroyed and may still be listed while it is.
fn labels<R: Runtime>(app: &AppHandle<R>, going: Option<&str>) -> Vec<String> {
    let mut labels: Vec<String> = app
        .webview_windows()
        .into_keys()
        .filter(|label| is_charter_window(label) && Some(label.as_str()) != going)
        .collect();
    labels.sort_by_key(|label| order_key(label));
    labels
}

/// Tells every window which windows there are now.
fn tell_windows_changed<R: Runtime>(app: &AppHandle<R>, going: Option<&str>) {
    let _ = app.emit(WINDOWS_CHANGED, labels(app, going));
}

/// Writes the arrangement down, as a window's own report of its tabs does.
fn remember<R: Runtime>(app: &AppHandle<R>) {
    if let (Some(planes), Some(showing)) = (app.try_state::<Planes>(), app.try_state::<Showing>()) {
        planes.remember_arrangement(&showing.arrangement());
    }
}

/// Brings a window back: shown, unminimised, and in front. Each step can fail on its own and
/// none is worth refusing to continue over.
pub fn raise<R: Runtime>(app: &AppHandle<R>, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Makes a split window, from the main window's own entry in `tauri.conf.json` so it has the
/// same size, minimum width, title bar and page — and the same creation script, so it draws
/// the operator's layout and theme from its first frame (`windowprefs.rs`).
fn make_split<R: Runtime>(app: &AppHandle<R>, label: &str) -> Result<(), String> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .cloned()
        .ok_or("tauri.conf.json declares no main window")?;
    config.label = label.to_owned();
    tauri::WebviewWindowBuilder::from_config(app, &config)
        .map_err(|err| format!("charter could not make a window: {err}"))?
        .initialization_script(crate::windowprefs::creation_script(
            charter_core::machine::config_root().as_deref(),
        ))
        .build()
        .map(|_| ())
        .map_err(|err| format!("charter could not make a window: {err}"))
}

/// Moves projects into another window — **a new one when `to` is null** — and answers the
/// label of the window they are now in.
///
/// Nothing is closed and nothing is started: the chats go on running in the core, and the
/// window they arrive in draws them from what the core already has open, as a window that
/// reloaded would. The window they left takes their tabs out itself, because it asked.
///
/// **Refused**, in charter's own words, for a project this process is not holding, for a
/// project another window holds (a window cannot take a project out from under a different
/// window), and for a window that is not one of charter's.
///
/// Async, so Tauri runs it off the main thread: making a window from a synchronous command
/// deadlocks on Windows.
#[tauri::command]
#[specta::specta]
pub async fn move_projects(
    app: AppHandle,
    window: tauri::Window,
    showing: tauri::State<'_, Showing>,
    planes: tauri::State<'_, Planes>,
    projects: Vec<PlaneId>,
    front: Option<PlaneId>,
    to: Option<String>,
) -> Result<String, String> {
    let from = window.label().to_owned();
    for plane in &projects {
        planes.held(plane)?;
    }
    let made = to.is_none();
    let to = match to {
        Some(to) if to == from => return Err("Those projects are already in this window.".into()),
        Some(to) if is_charter_window(&to) && app.get_webview_window(&to).is_some() => to,
        Some(to) => return Err(format!("charter has no window called {to}.")),
        None => showing.fresh_label(),
    };
    // A new window has something in front from its first frame; a window projects join keeps
    // what it was showing unless the move names a front.
    let in_front = front
        .as_ref()
        .or(if made { projects.first() } else { None });
    showing
        .move_into(&from, &projects, in_front, &to)
        .map_err(|theirs| {
            format!(
                "{} is in another window, and only that window can move it.",
                charter_core::shown::short(theirs.as_str())
            )
        })?;
    if made {
        // What it holds is in `Showing` before the window exists, so the window's first ask
        // (`projects_handed`) finds it. A window that could not be made hands them back.
        if let Err(why) = make_split(&app, &to) {
            let _ = showing.move_into(&to, &projects, None, &from);
            return Err(why);
        }
        tell_windows_changed(&app, None);
    } else {
        let _ = app.emit_to(
            to.as_str(),
            PROJECTS_ARRIVED,
            ProjectsArrived {
                planes: projects,
                front: front.clone(),
            },
        );
        raise(&app, &to);
    }
    remember(&app);
    Ok(to)
}

/// What this window was made to hold — for a split window, the projects moved into it; for any
/// window, what it last said it holds. Null for a window that holds nothing.
#[tauri::command]
#[specta::specta]
pub fn projects_handed(
    window: tauri::Window,
    showing: tauri::State<'_, Showing>,
) -> Option<WindowTabs> {
    showing.holding(window.label()).map(|held| WindowTabs {
        planes: held.planes,
        active: held.active.and_then(|at| u32::try_from(at).ok()),
    })
}

/// Every charter window there is, by label, the main window first — as [`WINDOWS_CHANGED`]
/// says it whenever that changes.
#[tauri::command]
#[specta::specta]
pub fn charter_windows(app: AppHandle) -> Vec<String> {
    labels(&app, None)
}

/// Brings the window holding `plane` to the front and answers its label — or null when no
/// window holds it. A window that is asked to open a project another window holds, or pressed
/// on a chat that is in another window, asks this rather than taking the project in twice.
#[tauri::command]
#[specta::specta]
pub fn show_window_holding(
    app: AppHandle,
    showing: tauri::State<'_, Showing>,
    plane: PlaneId,
) -> Option<String> {
    let label = showing.holder(&plane)?;
    raise(&app, &label);
    Some(label)
}

/// A window said what it holds. **A split window that holds nothing goes**: it was made to hold
/// the projects moved into it, and with the last of them moved or closed it has no reason to
/// be there. The main window stays, and draws the opener.
pub fn said_it_holds<R: Runtime>(window: &tauri::Window<R>, showing: &Showing, holding: Holding) {
    let empty = holding.planes.is_empty();
    showing.in_window(window.label(), holding);
    if empty && window.label() != MAIN {
        let _ = window.destroy();
    }
}

/// Hands whatever `window` still held to the main window, behind what it is showing, and tells
/// the main window. Answers whether there was anything to hand back.
fn hand_back<R: Runtime>(app: &AppHandle<R>, window: &str) -> bool {
    let Some(showing) = app.try_state::<Showing>() else {
        return false;
    };
    let handed = showing.close_into(window, MAIN);
    if handed.is_empty() {
        return false;
    }
    let _ = app.emit_to(
        MAIN,
        PROJECTS_ARRIVED,
        ProjectsArrived {
            planes: handed,
            front: None,
        },
    );
    remember(app);
    true
}

/// The close button on a window.
///
/// The main window hides, as it always has: every session is a child of this process (ADR
/// 0025), and the way out is Quit, which says what it is about to end. **A split window closes,
/// and its projects go back to the main window**, behind what that window is showing, with
/// every chat still running (ADR 0033, amended 2026-09-26). The main window is brought back if
/// it was hidden, so the projects are somewhere the operator can see.
pub fn close_requested<R: Runtime>(window: &tauri::Window<R>, api: &tauri::CloseRequestApi) {
    if window.label() == MAIN || !is_charter_window(window.label()) {
        api.prevent_close();
        let _ = window.hide();
        return;
    }
    let app = window.app_handle();
    if hand_back(app, window.label())
        && !app
            .get_webview_window(MAIN)
            .is_some_and(|main| main.is_visible().unwrap_or(false))
    {
        raise(app, MAIN);
    }
}

/// A window has gone. It is forgotten, and every other window is told. Whatever it still held
/// goes to the main window rather than nowhere: a window that went without a close must not
/// take projects with it.
pub fn destroyed<R: Runtime>(window: &tauri::Window<R>) {
    let app = window.app_handle();
    hand_back(app, window.label());
    tell_windows_changed(app, Some(window.label()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charters_windows_are_the_main_window_and_the_numbered_split_windows() {
        assert!(is_charter_window(MAIN));
        assert!(is_charter_window("window-1"));
        assert!(is_charter_window(&split_label(42)));
        // The capabilities' glob reads `window-` and a digit, and so does this.
        assert!(is_charter_window("window-1a"));
        for stray in ["window-", "window-x", "windows-1", "other", "", "Main"] {
            assert!(!is_charter_window(stray), "{stray}");
        }
    }

    #[test]
    fn windows_are_ordered_main_first_then_by_number() {
        let mut labels = vec!["window-10", "other", "window-2", MAIN];
        labels.sort_by_key(|label| order_key(label));
        assert_eq!(labels, [MAIN, "window-2", "window-10", "other"]);
    }
}
