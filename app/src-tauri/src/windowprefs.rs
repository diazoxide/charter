//! The window's layout and the operator's theme, **handed to the page as it is created** and
//! written back when the layout changes (`charter_core::windowprefs` holds the files and the
//! reasons).
//!
//! **Injected, never fetched.** A command is asynchronous, so a layout the window asked for
//! would land after the first paint and the window would paint the default arrangement and then
//! re-lay itself out — the flash ADR 0038 removed. So `lib.rs` builds the main window itself,
//! from its entry in `tauri.conf.json`, with [`creation_script`] as its initialization script:
//! Tauri runs that before the page's own scripts, so `main.tsx` finds both readings already in
//! the page and the first frame is drawn from them.
//!
//! **Main frame only.** `initialization_script` and not `…_for_all_frames`: an extension's panel
//! is a frame of its own, and the operator's arrangement is nothing it needs to be handed.

use std::path::Path;

use charter_core::windowprefs::{self, Reading};

/// The global the page reads both readings from. `app/src/windowprefs.ts` names it too; the
/// test below holds the script to it, and `windowprefs.test.ts` holds the page to it.
pub(crate) const GLOBAL: &str = "__CHARTER_AT_CREATION__";

/// Both readings, as the page receives them.
#[derive(Debug, serde::Serialize)]
struct AtCreation {
    layout: Reading,
    theme: Reading,
}

/// The script that puts both readings on the page before any of the page's own code runs.
///
/// **What is read from disk reaches the page as JSON data, never as code.** The readings are
/// serialised by `serde_json`, which escapes every quote and control character in a string, so
/// nothing in a hand-edited file can end the literal it sits in; and the value is frozen, so
/// nothing that runs later can quietly rewrite what the launch read.
///
/// A machine with no config home hands the page two empty readings: the default arrangement and
/// the built-in theme, which is what such a machine has always had.
pub(crate) fn creation_script(config_root: Option<&Path>) -> String {
    let at = match config_root {
        Some(root) => AtCreation {
            layout: windowprefs::read_layout(root),
            theme: windowprefs::read_theme(root),
        },
        None => AtCreation {
            layout: Reading::default(),
            theme: Reading::default(),
        },
    };
    let json = serde_json::to_string(&at).expect("a reading is plain data serde can always write");
    format!(
        "Object.defineProperty(window, {GLOBAL:?}, {{ value: Object.freeze({json}), \
         writable: false, configurable: false }});"
    )
}

/// Keeps the window's layout, replacing what the file held.
///
/// `text` is the document as JSON text; the core parses it and writes it back out itself, so
/// what lands on disk is always a document charter wrote.
#[tauri::command]
#[specta::specta]
pub async fn write_layout(text: String) -> Result<(), String> {
    let root = config_root()?;
    tauri::async_runtime::spawn_blocking(move || windowprefs::write_layout(&root, &text))
        .await
        .map_err(|err| format!("keeping the layout did not finish: {err}"))?
        .map_err(|err| err.to_string())
}

/// Moves the arrangement web storage held into the layout file — **only when there is no file
/// yet**. Answers whether it did, so the window knows the old key can go.
#[tauri::command]
#[specta::specta]
pub async fn adopt_layout(text: String) -> Result<bool, String> {
    let root = config_root()?;
    tauri::async_runtime::spawn_blocking(move || windowprefs::adopt_layout(&root, &text))
        .await
        .map_err(|err| format!("moving the layout did not finish: {err}"))?
        .map_err(|err| err.to_string())
}

/// The config home, or the reason charter keeps no layout on this machine.
fn config_root() -> Result<std::path::PathBuf, String> {
    charter_core::machine::config_root().ok_or_else(|| {
        "this machine has no config home, so charter cannot keep the layout".to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The object the script defines, read back the way a page would get it.
    fn defined(script: &str) -> serde_json::Value {
        let start = script.find("Object.freeze(").expect("frozen") + "Object.freeze(".len();
        let end = script.rfind("), writable").expect("closed");
        serde_json::from_str(&script[start..end]).expect("the value is JSON")
    }

    #[test]
    fn the_page_is_handed_the_layout_and_the_theme_on_disk() {
        let home = tempfile::tempdir().unwrap();
        let dir = charter_core::machine::dir(home.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(windowprefs::LAYOUT),
            r#"{"version":1,"regions":[{"id":"aside","side":"left","order":0}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join(windowprefs::THEME), r#"{"name":"Mine"}"#).unwrap();

        let script = creation_script(Some(home.path()));
        assert!(script.starts_with(&format!("Object.defineProperty(window, \"{GLOBAL}\"")));
        let at = defined(&script);
        assert_eq!(at["layout"]["found"], true);
        assert_eq!(at["layout"]["document"]["regions"][0]["side"], "left");
        assert_eq!(at["theme"]["document"]["name"], "Mine");
    }

    #[test]
    fn a_hand_edited_file_reaches_the_page_as_data_and_never_as_code() {
        // Whatever the operator — or anybody who can write the file — puts in a string, it is a
        // string when it arrives.
        let home = tempfile::tempdir().unwrap();
        let dir = charter_core::machine::dir(home.path());
        std::fs::create_dir_all(&dir).unwrap();
        let hostile = r#"\"}); alert(1); ({\" </script>"#;
        std::fs::write(
            dir.join(windowprefs::THEME),
            format!(r#"{{"name":"{hostile}"}}"#),
        )
        .unwrap();
        let at = defined(&creation_script(Some(home.path())));
        assert_eq!(
            at["theme"]["document"]["name"],
            "\"}); alert(1); ({\"\u{2028}</script>"
        );
    }

    #[test]
    fn a_broken_layout_reaches_the_page_as_a_reason_and_no_document() {
        let home = tempfile::tempdir().unwrap();
        let dir = charter_core::machine::dir(home.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(windowprefs::LAYOUT), "{ nope").unwrap();
        let at = defined(&creation_script(Some(home.path())));
        assert_eq!(at["layout"]["found"], true);
        assert_eq!(at["layout"]["document"], serde_json::Value::Null);
        assert!(
            at["layout"]["trouble"]
                .as_str()
                .is_some_and(|why| why.contains("is not JSON"))
        );
    }

    #[test]
    fn a_machine_with_no_config_home_is_handed_nothing_to_draw_from() {
        let at = defined(&creation_script(None));
        assert_eq!(at["layout"]["found"], false);
        assert_eq!(at["theme"]["found"], false);
    }
}
