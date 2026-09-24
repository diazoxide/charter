//! What the app does when its window closes, when something asks it to quit, and when it is
//! started a second time.
//!
//! The shape of it: closing the window only hides it, because every session is a child of
//! this process and dies with it (ADR 0025) — a close that ended them would end the day's
//! work. Quitting goes to the window first, so the operator sees what is about to be ended,
//! and only the window's answer exits.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use std::sync::PoisonError;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use tauri::{AppHandle, Emitter, Manager, Runtime, Wry};

/// The event the window is sent when something asks the app to quit. The window answers by
/// calling the `quit` command, or by not calling it.
pub const QUIT_ASKED: &str = "quit-asked";

/// The event the window is sent when the menu's Preferences… is chosen (charter-app#283). The
/// window opens its Preferences tab, which is the palette row's verb too.
pub const PREFERENCES_ASKED: &str = "preferences-asked";

/// The window the app has. There is one (spec decision 1), and it is the one every part of
/// this module means.
pub const WINDOW: &str = "main";

/// The ids of the things that can be clicked. Written once here so the handler and the menu
/// cannot drift apart.
const QUIT: &str = "quit";
const SHOW: &str = "show";
const PREFERENCES: &str = "preferences";

/// How long an unanswered ask keeps the next one armed to quit outright.
///
/// It expires, and that is the point. Without a deadline one `quit-asked` the window never
/// received — pressed while it was still starting, say — would leave the app armed for the
/// rest of the day, and the next Cmd-Q would end every session with no warning at all.
/// Pressing quit twice in a few seconds is someone insisting; pressing it again an hour
/// later is someone quitting.
const INSISTING: Duration = Duration::from_secs(5);

/// Whether the window has already been asked to quit, and when.
///
/// A second ask while the first is unanswered exits without waiting. This is the way out of
/// a window that cannot answer — a wedged webview, a renderer that crashed — which would
/// otherwise leave an app that can only be killed. Pressing quit twice is a thing people
/// already do to an app that seems not to have heard.
#[derive(Default)]
pub struct Quitting {
    asked: Mutex<Option<Instant>>,
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
        self.asked_at(Instant::now())
    }

    /// `ask`, at a moment a test can choose.
    fn asked_at(&self, now: Instant) -> Ask {
        let mut asked = self.asked.lock().unwrap_or_else(PoisonError::into_inner);
        let insisting = asked.is_some_and(|before| now.duration_since(before) < INSISTING);
        *asked = Some(now);
        if insisting {
            Ask::QuitAnyway
        } else {
            Ask::AskTheWindow
        }
    }

    /// The window answered "not now", so the next ask is a first ask again.
    pub fn never_mind(&self) {
        *self.asked.lock().unwrap_or_else(PoisonError::into_inner) = None;
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

/// The menu-bar glyph: charter's mark redrawn as a macOS template image, and NOT the app
/// icon.
///
/// A template is drawn from its alpha channel alone — macOS throws the colours away and
/// paints the shape black on a light menu bar, white on a dark one, white again when the
/// menu is open. The app icon is full-bleed by design (#159, so the small sizes are not
/// shrunk by a margin only macOS wants), which means its alpha is the whole rectangle, so
/// handing it to a template is how the tray became a solid black square (#202).
///
/// `icons/tray-template.png` is the mark's winged quill in opaque black on transparent,
/// 42x36, redrawn from `icons/source-1024.png` for the size it is drawn at rather than
/// resized to it. tray-icon gives the tray's image a height of 18pt whatever its pixels are
/// (tray-icon 0.24.2, `platform_impl/macos`), so 36px is the Retina pixel size and the
/// glyph inside it lands at 15pt, the air Apple's own menu-bar icons leave.
///
/// The brackets around the quill are dropped, which is the one thing here that is a taste
/// call. Keeping them costs the bird a fifth of its height and takes the wing's feather
/// slots — 18px of a 1024 source — to 1.3px on a Retina menu bar and 0.7px off one, which
/// is a grey smudge inside a box. A menu bar wants a glyph, not a logo.
///
/// It is compiled in rather than read from disk: `include_image!` decodes it at build time,
/// so there is no file for an installer to lose and no runtime failure to handle.
const MENU_BAR_GLYPH: Image<'static> = tauri::include_image!("icons/tray-template.png");

/// What the tray is drawn from, and whether macOS is to treat it as a template.
struct TrayIcon {
    image: Image<'static>,
    as_template: bool,
}

/// Which image the tray gets. `macos` is passed in rather than read from `cfg!` inside, so
/// both answers can be tested from whichever platform the suite happens to run on.
///
/// macOS gets the template glyph and nothing else: a menu bar recolours what it draws, and
/// a coloured icon there is either invisible or a rectangle. Every other platform gets the
/// app's own icon in its own colours, which is what their trays have always shown and what
/// `icon_as_template` means nothing to. `None` is a tray without an icon, not a panic: the
/// app icon is bundled, but a build that lost it still has sessions to give back.
fn tray_icon(macos: bool, app_icon: Option<Image<'static>>) -> Option<TrayIcon> {
    if macos {
        return Some(TrayIcon {
            image: MENU_BAR_GLYPH,
            as_template: true,
        });
    }
    app_icon.map(|image| TrayIcon {
        image,
        as_template: false,
    })
}

/// The tray icon: what the app is while its window is hidden, and what brings it back.
pub fn tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, SHOW, "Show charter", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT, "Quit charter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut tray = TrayIconBuilder::with_id("charter")
        .tooltip("charter")
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
        });
    // Owned, because the tray outlives this call while the bundled icon is borrowed from
    // the app handle.
    let app_icon = app
        .default_window_icon()
        .map(|icon| icon.clone().to_owned());
    match tray_icon(cfg!(target_os = "macos"), app_icon) {
        Some(icon) => tray = tray.icon(icon.image).icon_as_template(icon.as_template),
        // Said out loud, because a tray with no icon is a tray that is hard to find, and
        // reaching the window from the dock is easier than hunting for an empty slot.
        None => eprintln!("charter: the tray has no icon; its menu is still on the click"),
    }
    tray.build(app)?;
    Ok(())
}

/// One entry of the application menu, before it is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    /// charter's own Quit, with the accelerator it carries.
    Quit(&'static str),
    /// Preferences… (charter-app#283), with the accelerator it carries: the window's text
    /// sizes, which are this machine's.
    Preferences(&'static str),
    Hide,
    Separator,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// The application menu's submenus, by title, for macOS or for everything else.
///
/// **On macOS it is the `⌘` menu every Mac app has**, and it has to be: without an Edit menu
/// carrying the predefined items, `⌘C` and `⌘V` do nothing in a text field. No `⌘` chord is a
/// key a terminal encodes, so nothing here is taken from a chat.
///
/// **Everywhere else, only Quit, on `Ctrl+Shift+Q` (#187).** muda's predefined items there are
/// `Ctrl` chords it does not let charter change, and each is a key the chat's shell owns: Copy
/// is `Ctrl-C`, the interrupt; Undo `Ctrl-Z`, suspend; Select all `Ctrl-A`, beginning of line;
/// Hide `Ctrl-H`, backspace. A terminal app keeps its own keys off them by adding `Shift`
/// (GNOME Terminal and Konsole: `Ctrl+Shift+C`, `V`, `Q`), which xterm encodes as nothing. The
/// Edit items could not be moved there — a predefined item's accelerator is fixed — so they are
/// left out rather than left on the chat's keys. A text field in the webview keeps its own
/// `Ctrl+C` and `Ctrl+V`, which the webview handles without any menu.
///
/// **Preferences… is on both, on `⌘,` and `Ctrl+,`** (charter-app#283) — the key every Mac app
/// opens its settings on, and VS Code's everywhere else. Neither takes anything from a chat:
/// xterm.js 6.0.0 sends nothing for a `⌘` chord but `⌘A`, and with `Ctrl` it encodes a letter,
/// space, `3`–`8`, `[`, `\` and `]` — not `,`.
///
/// What leaving them out costs, **read from muda 0.19.3's source and not measured in a window**
/// (nobody here has a Linux or Windows desktop to press the key in): on Windows the Edit items
/// were real accelerators in the window's accelerator table, so there they really did take the
/// key before the webview saw it, and the menu loses Undo to Select all. On Linux
/// (`platform_impl/gtk`) Copy, Cut, Paste and Select all draw the chord as a label only and act
/// through `libxdo` when clicked, a feature this build does not enable, and Undo, Redo and Hide
/// are not drawn at all — so there the Edit menu did little but claim keys it did not use.
fn layout(macos: bool) -> Vec<(&'static str, Vec<Item>)> {
    if !macos {
        return vec![(
            "charter",
            vec![
                Item::Preferences("Ctrl+,"),
                Item::Separator,
                Item::Quit("Ctrl+Shift+Q"),
            ],
        )];
    }
    vec![
        (
            "charter",
            vec![
                Item::Preferences("CmdOrCtrl+,"),
                Item::Separator,
                Item::Hide,
                Item::Separator,
                Item::Quit("CmdOrCtrl+Q"),
            ],
        ),
        // Copy, paste and select-all are the predefined items a text field needs to behave;
        // on macOS nothing works without an Edit menu carrying them.
        (
            "Edit",
            vec![
                Item::Undo,
                Item::Redo,
                Item::Separator,
                Item::Cut,
                Item::Copy,
                Item::Paste,
                Item::SelectAll,
            ],
        ),
    ]
}

/// The application menu, which is where Quit lives on every platform.
///
/// Tauri's own default menu has a predefined Quit that ends the process where it is clicked.
/// This replaces it with one of charter's own, so that quitting goes through the window and
/// the operator is told what is about to be ended.
pub fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let mut submenus = Vec::new();
    for (title, items) in layout(cfg!(target_os = "macos")) {
        let mut built: Vec<Box<dyn tauri::menu::IsMenuItem<Wry>>> = Vec::new();
        for item in items {
            built.push(match item {
                Item::Quit(accelerator) => Box::new(MenuItem::with_id(
                    app,
                    QUIT,
                    "Quit charter",
                    true,
                    Some(accelerator),
                )?),
                Item::Preferences(accelerator) => Box::new(MenuItem::with_id(
                    app,
                    PREFERENCES,
                    "Preferences…",
                    true,
                    Some(accelerator),
                )?),
                Item::Hide => Box::new(PredefinedMenuItem::hide(app, None)?),
                Item::Separator => Box::new(PredefinedMenuItem::separator(app)?),
                Item::Undo => Box::new(PredefinedMenuItem::undo(app, None)?),
                Item::Redo => Box::new(PredefinedMenuItem::redo(app, None)?),
                Item::Cut => Box::new(PredefinedMenuItem::cut(app, None)?),
                Item::Copy => Box::new(PredefinedMenuItem::copy(app, None)?),
                Item::Paste => Box::new(PredefinedMenuItem::paste(app, None)?),
                Item::SelectAll => Box::new(PredefinedMenuItem::select_all(app, None)?),
            });
        }
        let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = built.iter().map(|b| &**b).collect();
        submenus.push(Submenu::with_items(app, title, true, &refs)?);
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = submenus
        .iter()
        .map(|s| s as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();
    Menu::with_items(app, &refs)
}

/// What a click on one of charter's own menu items does.
pub fn clicked<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        QUIT => ask_to_quit(app),
        SHOW => show(app),
        // Shown first, for Quit's reason: the tab opens in the window, and a hidden window
        // would put it where nobody is looking.
        PREFERENCES => {
            show(app);
            let _ = app.emit_to(WINDOW, PREFERENCES_ASKED, ());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full-bleed app icon, the shape of the one #159 generated: opaque corner to corner.
    fn full_bleed_icon() -> Image<'static> {
        Image::new_owned(vec![255; 4 * 8 * 8], 8, 8)
    }

    fn alpha(image: &Image<'_>) -> Vec<u8> {
        image.rgba().iter().skip(3).step_by(4).copied().collect()
    }

    /// The property that failed in #202, and the only one macOS reads: a template image is
    /// its alpha channel, so it has to have both a background that is not there and a shape
    /// that is. The app icon passes the second half and fails the first.
    fn reads_as_a_template(image: &Image<'_>) -> bool {
        let alpha = alpha(image);
        let clear = alpha.iter().filter(|&&a| a == 0).count();
        let opaque = alpha.iter().filter(|&&a| a == u8::MAX).count();
        clear * 2 > alpha.len() && opaque * 10 > alpha.len()
    }

    /// The accelerator `item` carries once built, written the way muda parses one.
    ///
    /// A predefined item's is not charter's to choose: muda 0.19.3
    /// (`src/items/predefined.rs::accelerator`) gives each a fixed one, `CmdOrCtrl` on every
    /// platform, and that table is copied here so a test can hold the menu to it.
    fn accelerator(item: Item, macos: bool) -> Option<String> {
        let chord = |key: &str| Some(format!("CmdOrCtrl+{key}"));
        match item {
            Item::Quit(accelerator) | Item::Preferences(accelerator) => {
                Some(accelerator.to_owned())
            }
            Item::Separator => None,
            Item::Hide => chord("H"),
            Item::Undo => chord("Z"),
            Item::Redo if macos => Some("CmdOrCtrl+Shift+Z".to_owned()),
            Item::Redo => chord("Y"),
            Item::Cut => chord("X"),
            Item::Copy => chord("C"),
            Item::Paste => chord("V"),
            Item::SelectAll => chord("A"),
        }
    }

    /// Whether a pane's terminal turns `accelerator` into bytes for the shell, on a platform
    /// that is macOS or not — so a menu that claims it takes a key from the chat.
    ///
    /// xterm (`@xterm/xterm` 6.0.0, `Keyboard.ts`) encodes `Ctrl` with a letter, space, `3`–`8`,
    /// `[`, `\` or `]` as a control character only when neither `Shift`, `Alt` nor `Meta` is
    /// held; with `Shift` it sends nothing, which is the room GNOME Terminal and Konsole put
    /// their own keys in. `CmdOrCtrl` is `Ctrl` off macOS and `⌘` on it, and a `⌘` chord xterm
    /// sends nothing for either.
    fn a_terminal_encodes(accelerator: &str, macos: bool) -> bool {
        let words: Vec<String> = accelerator.split('+').map(str::to_uppercase).collect();
        let (key, modifiers) = words.split_last().expect("an accelerator has a key");
        let ctrl = modifiers.iter().any(|m| {
            m == "CTRL"
                || m == "CONTROL"
                || (!macos && (m == "CMDORCTRL" || m == "COMMANDORCONTROL"))
        });
        let other = modifiers
            .iter()
            .any(|m| m == "SHIFT" || m == "ALT" || m == "OPTION" || m == "SUPER" || m == "CMD");
        let encoded = match key.as_str() {
            "SPACE" | "[" | "\\" | "]" | "BRACKETLEFT" | "BRACKETRIGHT" | "BACKSLASH" => true,
            one if one.len() == 1 => one
                .chars()
                .all(|c| c.is_ascii_alphabetic() || ('3'..='8').contains(&c)),
            _ => false,
        };
        ctrl && !other && encoded
    }

    fn accelerators(macos: bool) -> Vec<(Item, String)> {
        layout(macos)
            .into_iter()
            .flat_map(|(_, items)| items)
            .filter_map(|item| accelerator(item, macos).map(|a| (item, a)))
            .collect()
    }

    #[test]
    fn a_ctrl_letter_is_the_terminals_and_a_ctrl_shift_letter_or_a_cmd_chord_is_not() {
        assert!(a_terminal_encodes("CmdOrCtrl+C", false));
        assert!(a_terminal_encodes("Ctrl+Q", false));
        assert!(!a_terminal_encodes("CmdOrCtrl+C", true));
        assert!(!a_terminal_encodes("Ctrl+Shift+Q", false));
        assert!(!a_terminal_encodes("Alt+F4", false));
        // Not only letters: xterm sends `^[` for `Ctrl+[` and `^\` for `Ctrl+\`.
        assert!(a_terminal_encodes("Ctrl+[", false));
        assert!(a_terminal_encodes("Ctrl+5", false));
        // And not a comma, which is why Preferences can be on `Ctrl+,` (charter-app#283).
        assert!(!a_terminal_encodes("Ctrl+,", false));
        assert!(!a_terminal_encodes("Ctrl+0", false));
    }

    #[test]
    fn off_macos_no_menu_accelerator_is_a_key_the_chats_terminal_encodes() {
        // #187: off macOS muda's predefined Edit items are `Ctrl` chords, and every one of
        // them is a key a shell owns — Copy was `Ctrl-C`, the interrupt.
        let taken: Vec<(Item, String)> = accelerators(false)
            .into_iter()
            .filter(|(_, a)| a_terminal_encodes(a, false))
            .collect();

        assert_eq!(taken, Vec::new());
    }

    #[test]
    fn off_macos_quit_is_ctrl_shift_q_as_it_is_in_a_terminal_app() {
        assert_eq!(
            accelerators(false),
            vec![
                (Item::Preferences("Ctrl+,"), "Ctrl+,".to_owned()),
                (Item::Quit("Ctrl+Shift+Q"), "Ctrl+Shift+Q".to_owned())
            ]
        );
    }

    #[test]
    fn on_macos_the_menu_is_the_cmd_menu_it_always_was() {
        assert_eq!(
            layout(true),
            vec![
                (
                    "charter",
                    vec![
                        Item::Preferences("CmdOrCtrl+,"),
                        Item::Separator,
                        Item::Hide,
                        Item::Separator,
                        Item::Quit("CmdOrCtrl+Q")
                    ]
                ),
                (
                    "Edit",
                    vec![
                        Item::Undo,
                        Item::Redo,
                        Item::Separator,
                        Item::Cut,
                        Item::Copy,
                        Item::Paste,
                        Item::SelectAll,
                    ]
                ),
            ]
        );
        assert!(
            accelerators(true)
                .iter()
                .all(|(_, a)| !a_terminal_encodes(a, true))
        );
    }

    #[test]
    fn the_menu_bar_glyph_is_a_shape_in_alpha_and_not_a_filled_rectangle() {
        // #202: the tray was the app icon drawn as a template, and a template is painted
        // from its alpha alone, so an opaque image is a black rectangle in the menu bar.
        // Nothing noticed, because the PNG looks right in any viewer.
        let glyph = MENU_BAR_GLYPH;

        let alpha = alpha(&glyph);
        assert_eq!(
            alpha.len(),
            (glyph.width() * glyph.height()) as usize,
            "the glyph is RGBA, so there is one alpha byte per pixel"
        );
        assert!(
            reads_as_a_template(&glyph),
            "the glyph must be transparent around an opaque shape, not opaque throughout"
        );
        let corners = [
            0,
            (glyph.width() - 1) as usize,
            (glyph.width() * (glyph.height() - 1)) as usize,
            (glyph.width() * glyph.height() - 1) as usize,
        ];
        for corner in corners {
            assert_eq!(alpha[corner], 0, "a menu bar shows through every corner");
        }
    }

    #[test]
    fn macos_gets_the_template_glyph_however_opaque_the_app_icon_is() {
        let icon = tray_icon(true, Some(full_bleed_icon())).expect("macOS carries its glyph");

        assert!(icon.as_template, "a menu bar recolours what it draws");
        assert!(
            reads_as_a_template(&icon.image),
            "only an image with a real alpha channel may be drawn as a template (#202)"
        );
    }

    #[test]
    fn every_other_platform_gets_the_app_icon_in_its_own_colours() {
        // `icon_as_template` is a macOS concept. Linux and Windows draw the image as it is,
        // and theirs was never the thing that broke.
        let app_icon = full_bleed_icon();

        let icon = tray_icon(false, Some(app_icon.clone())).expect("the app icon is bundled");

        assert!(!icon.as_template);
        assert_eq!(icon.image.rgba(), app_icon.rgba());
    }

    #[test]
    fn a_build_that_lost_its_app_icon_still_gets_a_tray() {
        // The tray was built with `.expect("the app is bundled with its icon")` while the
        // code around it says a tray is never worth the app: "the sessions are the work".
        assert!(tray_icon(false, None).is_none(), "and no panic");
        assert!(
            tray_icon(true, None).is_some(),
            "macOS never asks the bundle for it — the glyph is compiled in"
        );
    }

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
    fn an_ask_the_window_never_answered_stops_arming_the_next_one() {
        // A `quit-asked` the window never received — pressed while it was still starting —
        // would otherwise leave the app armed for the rest of the day, and the next Cmd-Q
        // would end every session with no warning at all.
        let quitting = Quitting::default();
        let when = Instant::now();
        quitting.asked_at(when);

        let later = quitting.asked_at(when + INSISTING + Duration::from_millis(1));

        assert_eq!(later, Ask::AskTheWindow);
    }

    #[test]
    fn asking_twice_in_a_moment_is_someone_insisting() {
        let quitting = Quitting::default();
        let when = Instant::now();
        quitting.asked_at(when);

        let again = quitting.asked_at(when + Duration::from_millis(500));

        assert_eq!(again, Ask::QuitAnyway);
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
