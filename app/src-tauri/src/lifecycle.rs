//! What the app does when its window closes, when something asks it to quit, and when it is
//! started a second time.
//!
//! The shape of it: closing the window only hides it, because every session is a child of
//! this process and dies with it (ADR 0025) — a close that ended them would end the day's
//! work. Quitting goes to the window first, so the operator sees what is about to be ended,
//! and only the window's answer exits.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, Wry};

/// The event the window is sent when something asks the app to quit. The window answers by
/// calling the `quit` command, or by not calling it.
pub const QUIT_ASKED: &str = "quit-asked";

/// The window the app has. There is one (spec decision 1), and it is the one every part of
/// this module means.
pub const WINDOW: &str = "main";

/// The ids of the things that can be clicked. Written once here so the handler and the menu
/// cannot drift apart.
const QUIT: &str = "quit";
const SHOW: &str = "show";

/// Whether the window has already been asked to quit.
///
/// The second ask exits without waiting for an answer. This is the way out of a window that
/// cannot answer — a wedged webview, a renderer that crashed — which would otherwise leave
/// an app that can only be killed. Pressing quit twice is a thing people already do to an
/// app that seems not to have heard.
#[derive(Default)]
pub struct Quitting {
    asked: AtomicBool,
}

/// What an ask to quit means this time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// The window is asked, and answers by calling `quit` or by leaving it.
    AskTheWindow,
    /// It has been asked once and has not answered. Go.
    QuitAnyway,
}

impl Quitting {
    pub fn ask(&self) -> Ask {
        if self.asked.swap(true, Ordering::SeqCst) {
            Ask::QuitAnyway
        } else {
            Ask::AskTheWindow
        }
    }

    /// The window answered "not now", so the next ask is a first ask again.
    pub fn never_mind(&self) {
        self.asked.store(false, Ordering::SeqCst);
    }
}

/// Brings the window back: shown, unminimised, and in front.
///
/// Each step can fail on its own and none of them is worth refusing to continue over — a
/// window that is shown but not focused is still the window back.
pub fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Asks the window to quit, or quits.
pub fn ask_to_quit<R: Runtime>(app: &AppHandle<R>) {
    match app.state::<Quitting>().ask() {
        Ask::AskTheWindow => {
            // Shown first: the ask is a dialog in the window, and a hidden window would put
            // the question somewhere nobody is looking.
            show(app);
            let _ = app.emit_to(WINDOW, QUIT_ASKED, ());
        }
        Ask::QuitAnyway => app.exit(0),
    }
}

/// Hides the window rather than closing it, so every session keeps running.
pub fn hide_rather_than_close<R: Runtime>(window: &tauri::Window<R>) {
    let _ = window.hide();
}

/// The tray icon: what the app is while its window is hidden, and what brings it back.
pub fn tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, SHOW, "Show charter", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT, "Quit charter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    TrayIconBuilder::with_id("charter")
        .tooltip("charter")
        .icon(
            app.default_window_icon()
                .expect("the app is bundled with its icon")
                .clone(),
        )
        // On macOS the icon is drawn as a template, so it follows a light or a dark menu bar.
        .icon_as_template(true)
        .menu(&menu)
        // The menu is for the right button. A left click is "give me the window back", which
        // is what the icon is mostly there for.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// The application menu, which is where Quit lives on every platform.
///
/// Tauri's own default menu has a predefined Quit that ends the process where it is clicked.
/// This replaces it with one of charter's own, so that quitting goes through the window and
/// the operator is told what is about to be ended.
pub fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let quit = MenuItem::with_id(app, QUIT, "Quit charter", true, Some("CmdOrCtrl+Q"))?;
    let app_menu = Submenu::with_items(
        app,
        "charter",
        true,
        &[
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    // Copy, paste and select-all are the predefined items a text field needs to behave; on
    // macOS nothing works without an Edit menu carrying them.
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &edit])
}

/// What a click on one of charter's own menu items does.
pub fn clicked<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        QUIT => ask_to_quit(app),
        SHOW => show(app),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_ask_to_quit_goes_to_the_window() {
        let quitting = Quitting::default();

        assert_eq!(quitting.ask(), Ask::AskTheWindow);
    }

    #[test]
    fn a_second_ask_quits_without_waiting_for_a_window_that_has_not_answered() {
        // The way out of a webview that cannot draw the dialog: press quit again.
        let quitting = Quitting::default();
        quitting.ask();

        assert_eq!(quitting.ask(), Ask::QuitAnyway);
    }

    #[test]
    fn an_ask_the_window_said_no_to_is_asked_again_from_the_start() {
        // Otherwise cancelling once would arm the next Cmd-Q to quit with no warning at all.
        let quitting = Quitting::default();
        quitting.ask();

        quitting.never_mind();

        assert_eq!(quitting.ask(), Ask::AskTheWindow);
    }
}
