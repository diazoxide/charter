//! The extension registry, as the window asks about it.
//!
//! `charter_core::extension` is the whole of the thinking; this is the wire. Four commands:
//! what has contributed what ([`installed_extensions`]), pick a directory
//! ([`pick_extension`]), read one and ask about it ([`install_extension`]), and record the
//! yes ([`approve_extension`]).
//!
//! **There is no executor here, and that is still deliberate.** charter ADR 0041 staged it —
//! the registry first, the subprocess afterwards — so that the first extension runtime was not
//! also the thing that invented the list it runs against. The runtime now exists
//! (`charter_core::executor`, drawn by `crate::views`) and it was built against this list: it
//! re-reads the record and re-takes the fingerprint at every press, and nothing in this file
//! starts anything.
//!
//! **Installing is two clicks, and it has to be**, for the reason `approve_plane` is two: the
//! button that chooses a directory and the button that says yes to what was found in it are
//! different answers to different questions, and collapsing them means the operator approved
//! something before it was drawn. [`install_extension`] writes a row with no approval on it
//! and hands back the question; [`approve_extension`] takes the answer, carrying back the
//! fingerprint that was on screen (charter-app#123's shape).

use charter_core::extension;
use tauri_plugin_dialog::DialogExt;

/// The question charter asks before an extension contributes anything.
///
/// A mirror of [`extension::Prompt`] rather than the thing itself, because `charter-core` never
/// depends on the app and the app's wire types are generated into TypeScript.
///
/// **charter's own sentences travel with it rather than being written in the dialog.** That is
/// the point of carrying them: a window that composed its own words about what an extension can
/// reach could drift kinder than the truth one edit at a time, and the truth here is
/// uncomfortable enough that kinder is the likely direction. Since charter-app#152 there are
/// three of them, because the fingerprint note acquired an exception and an exception the window
/// worded itself would be the same drift through a smaller door.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ExtensionAsk {
    /// The id the record is keyed by, and the id the yes is recorded against.
    pub id: String,
    /// What to call it, as its own manifest does.
    pub name: String,
    /// Where it is. Shown and not merely carried: an extension is approved at a path, and one
    /// re-installed from another directory is a different question.
    pub path: String,
    /// Every contribution it declares, in charter's own words, one line each.
    pub declares: Vec<String>,
    /// The fingerprint of the bytes that were read to build this question. It goes back to
    /// [`approve_extension`] untouched, so the yes is for what was shown.
    pub fingerprint: String,
    /// Whether this is a first ask rather than a re-ask.
    pub first: bool,
    /// `extension::RUNS_AS_YOU` — that a plugin runs with the operator's own access and that
    /// the list above is a declaration, not a limit.
    pub runs_as_you: String,
    /// `extension::FINGERPRINTED` — that the fingerprint catches a change and is not a
    /// boundary.
    pub fingerprint_note: String,
    /// `extension::state_note` — which one directory charter does NOT read, when this
    /// extension declares one, and `null` when it declares none (charter-app#152).
    ///
    /// It travels for the same reason the other two do. The fingerprint note says charter read
    /// every file in the directory; the exception to that sentence belongs on the same screen
    /// as the sentence, in the core's words, or the wording has the defect #152 was opened over
    /// one carve-out later.
    pub state_note: Option<String>,
}

impl From<extension::Prompt> for ExtensionAsk {
    fn from(asked: extension::Prompt) -> Self {
        Self {
            id: asked.id,
            name: asked.name,
            path: asked.path,
            declares: asked.declares,
            fingerprint: asked.fingerprint,
            first: asked.first,
            runs_as_you: asked.runs_as_you,
            fingerprint_note: asked.fingerprint_note,
            state_note: asked.state_note,
        }
    }
}

/// One row of "what has contributed what to this window".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ExtensionRow {
    pub id: String,
    /// What to call it: its manifest's name when charter could read one, else its id.
    pub name: String,
    pub path: String,
    /// `approved`, `new` or `changed`.
    pub standing: String,
    /// The themes it is contributing **right now** — empty unless it is approved, so the row
    /// says what is in force rather than what was asked for.
    pub themes_in_force: Vec<String>,
    /// Every contribution it declares, whether or not any of it is in force.
    pub declares: Vec<String>,
    /// Why charter could not read it, when it could not. A row that contributes nothing says
    /// why rather than going quiet.
    pub refused: Option<String>,
    /// The question to ask about it, when there is one to ask.
    pub ask: Option<ExtensionAsk>,
}

/// What has contributed what to this window — charter ADR 0041's item 2, and the thing every
/// later decision about extensions is read off.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct InstalledExtensions {
    /// charter's own themes, which are compiled into the window and are never asked about.
    /// Listed so that the answer to "what is contributing" is one list rather than one per
    /// source.
    pub built_in_themes: Vec<String>,
    pub extensions: Vec<ExtensionRow>,
    /// Why the record could not be read, when it could not — in which case nothing an
    /// extension declares is in force and `extensions` is empty.
    pub unreadable: Option<String>,
    /// Rows the record held and charter dropped, with the reason for each.
    pub dropped: Vec<String>,
}

/// One theme an approved extension is contributing, as the window receives it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ExtensionTheme {
    /// The extension it came from, so the window can say whose theme it is drawing.
    pub extension: String,
    /// What to call it, as the manifest does.
    pub name: String,
    /// The file's **text**, unparsed.
    ///
    /// **The vocabulary is `theme.ts`'s and the parser is `theme.ts`'s**, and this crate
    /// deliberately does not grow a second one. `TOKENS` is the closed vocabulary charter owns
    /// (ADR 0041 property 1) and `load` is the parse-and-re-emit that keeps a theme's bytes out
    /// of a stylesheet (property 4); both are already written and already tested. A Rust parser
    /// here would be a second vocabulary, and two vocabularies drift — which is property 1
    /// turning into a lie by accident rather than by decision.
    ///
    /// It is the text the fingerprint was taken over, handed on rather than fetched again.
    pub text: String,
}

/// Every theme in force: the ones approved extensions contribute, and nothing else.
///
/// An extension that is new, changed or unreadable contributes nothing here. That is the
/// registry doing its one job — [`extension::Surveyed::themes_in_force`] is what answers, and
/// this command only shapes it.
#[tauri::command]
#[specta::specta]
pub async fn extension_themes() -> Result<Vec<ExtensionTheme>, String> {
    let root = config_root()?;
    tauri::async_runtime::spawn_blocking(move || in_force(&extension::survey(&root)))
        .await
        .map_err(|err| format!("reading this machine's themes did not finish: {err}"))
}

/// Every panel an approved extension contributes to this window's side region.
///
/// **Its own command, asked once per window and never per workspace focus.** A survey reads
/// every installed extension's whole directory to re-take the fingerprint — ADR 0041's named
/// cost of fingerprinting code — and folding that into `workspace_panels` would put it on the
/// path that has 100 ms to draw, once per click on the workspace strip.
///
/// It is the same shape as [`extension_themes`] one row down, and for the same reason: an
/// extension that is new, changed or unreadable contributes nothing here. That is the registry
/// doing its one job, and a contributed panel rests on it entirely.
#[tauri::command]
#[specta::specta]
pub async fn extension_panels() -> Result<Vec<crate::panels::PanelView>, String> {
    let root = config_root()?;
    tauri::async_runtime::spawn_blocking(move || panels_in_force(&extension::survey(&root)))
        .await
        .map_err(|err| format!("reading this machine's panels did not finish: {err}"))
}

/// [`extension_panels`] with the survey already taken, so the shaping is testable without a
/// Tauri runtime to run it on.
fn panels_in_force(seen: &extension::Survey) -> Vec<crate::panels::PanelView> {
    let mut found: Vec<charter_core::panel::Panel> = seen
        .installed
        .iter()
        .flat_map(|row| row.panels_in_force().iter().cloned())
        .collect();
    // Sorted here rather than in the window, so that two extensions declaring the same `order`
    // land in the same place at every launch instead of the order the record was read in.
    charter_core::panel::Panel::sort(&mut found);
    found.iter().map(crate::panels::PanelView::from).collect()
}

/// [`extension_themes`] with the survey already taken.
fn in_force(seen: &extension::Survey) -> Vec<ExtensionTheme> {
    let mut themes = Vec::new();
    for row in &seen.installed {
        let Some(found) = &row.found else { continue };
        for theme in row.themes_in_force() {
            // A theme whose text is somehow not there is dropped rather than sent empty: an
            // empty theme would load as the built-in with a complaint for every token, which
            // looks like a theme that did nothing wrong.
            if let Some(text) = found.theme_text(theme) {
                themes.push(ExtensionTheme {
                    extension: row.id.clone(),
                    name: theme.name.clone(),
                    text: text.to_owned(),
                });
            }
        }
    }
    themes
}

/// The config home, or the reason charter has no machine-level state.
fn config_root() -> Result<std::path::PathBuf, String> {
    charter_core::machine::config_root()
        .ok_or_else(|| "this machine has no config home, so charter keeps no extensions".to_owned())
}

/// What has contributed what to this window.
///
/// It reads the disk — every installed extension's whole directory, to re-take the fingerprint
/// — because an approval is of bytes and the bytes are what may have changed since. ADR 0041
/// names that cost: *the fingerprint is a hash of code, checked at each launch*, and
/// charter-app#152 widened it from the declared list to the tree. Off the UI thread for exactly
/// that reason, and the cost is measured in #152's PR body rather than left as an estimate.
#[tauri::command]
#[specta::specta]
pub async fn installed_extensions() -> Result<InstalledExtensions, String> {
    let root = config_root()?;
    tauri::async_runtime::spawn_blocking(move || listed(&extension::survey(&root)))
        .await
        .map_err(|err| format!("reading this machine's extensions did not finish: {err}"))
}

/// [`installed_extensions`] with the survey already taken, so the shaping is testable without
/// a Tauri runtime to run it on.
fn listed(seen: &extension::Survey) -> InstalledExtensions {
    InstalledExtensions {
        built_in_themes: seen.built_in_themes.clone(),
        extensions: seen
            .installed
            .iter()
            .map(|row| {
                let asked = row.found.as_ref().and_then(|found| {
                    // A row that is already approved has nothing to ask, and a question with
                    // no risk in it is one an operator learns to answer yes to without
                    // reading.
                    (!row.standing.may_contribute())
                        .then(|| ExtensionAsk::from(extension::prompt(found, row.standing)))
                });
                ExtensionRow {
                    id: row.id.clone(),
                    name: row
                        .found
                        .as_ref()
                        .map_or_else(|| row.id.clone(), |found| found.manifest.name.clone()),
                    path: row.path.display().to_string(),
                    standing: row.standing.as_str().to_owned(),
                    themes_in_force: row
                        .themes_in_force()
                        .iter()
                        .map(|theme| theme.name.clone())
                        .collect(),
                    declares: asked.as_ref().map_or_else(
                        || {
                            row.found.as_ref().map_or_else(Vec::new, |found| {
                                extension::prompt(found, row.standing).declares
                            })
                        },
                        |ask| ask.declares.clone(),
                    ),
                    refused: row.refused.clone(),
                    ask: asked,
                }
            })
            .collect(),
        unreadable: seen.unreadable.clone(),
        dropped: seen.dropped.clone(),
    }
}

/// The folder picker, for an extension this machine does not have.
///
/// **By path, by the operator, from nowhere.** ADR 0041 rejects a registry and a fetch by name
/// — *the moment charter resolves an extension name over the network it owns a supply chain* —
/// so this is the only way one arrives, and it is the same picker `pick_project` uses.
#[tauri::command]
#[specta::specta]
pub async fn pick_extension(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (chose, chosen) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |picked| {
        // The window may have gone while the dialog was up; then there is nobody to tell.
        let _ = chose.send(picked);
    });
    tauri::async_runtime::spawn_blocking(move || chosen.recv().ok().flatten())
        .await
        .map(|picked| picked.map(|path| path.to_string()))
        .map_err(|err| format!("the folder picker did not finish: {err}"))
}

/// Read the extension at `path`, write it into the record **unapproved**, and hand back the
/// question to ask about it.
///
/// An extension charter cannot read is an error here rather than a silent nothing: the
/// operator pointed at a directory and is owed the reason it is not one.
#[tauri::command]
#[specta::specta]
pub async fn install_extension(path: String) -> Result<ExtensionAsk, String> {
    let root = config_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        let at = std::path::PathBuf::from(path);
        let found = extension::install(&root, &at).map_err(|why| why.to_string())?;
        let standing = extension::read(&root).standing(&found);
        Ok(ExtensionAsk::from(extension::prompt(&found, standing)))
    })
    .await
    .map_err(|err| format!("reading that extension did not finish: {err}"))?
}

/// The operator's answer to [`ExtensionAsk`]: yes.
///
/// `fingerprint` is the one the dialog drew, handed straight back — charter-app#123's fix
/// applied before the defect can be copied. What is recorded is the bytes that were on screen,
/// so a write between the drawing and the click is not consented to; it makes the next launch
/// ask again, which is the whole of the mechanism.
///
/// **This is a separate click from the one that installed it, and it has to be.**
#[tauri::command]
#[specta::specta]
pub async fn approve_extension(
    id: String,
    path: String,
    fingerprint: String,
) -> Result<(), String> {
    let root = config_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        extension::approve(&root, &id, std::path::Path::new(&path), &fingerprint)
            .map_err(|why| why.to_string())
    })
    .await
    .map_err(|err| format!("recording that approval did not finish: {err}"))?
}

/// Take an extension out of the record.
///
/// Its own files are not charter's and are not deleted: ADR 0041's *an extension is
/// re-installable by name; its state is its own problem*. What goes is the path and the
/// approval, which is exactly what ADR 0034 says deleting this machine's state must cost.
#[tauri::command]
#[specta::specta]
pub async fn forget_extension(id: String) -> Result<(), String> {
    let root = config_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        extension::forget(&root, &id).map_err(|why| why.to_string())
    })
    .await
    .map_err(|err| format!("forgetting that extension did not finish: {err}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let at = dir.path().join("ext");
        std::fs::create_dir_all(&at).expect("the extension's directory");
        std::fs::write(
            at.join(extension::MANIFEST),
            r#"{"version":1,"id":"solarized","name":"Solarized",
                "contributes":{"themes":[{"name":"Solarized Dark","file":"dark.json"}]}}"#,
        )
        .expect("a manifest");
        std::fs::write(at.join("dark.json"), "{}").expect("a theme");
        let config = dir.path().join("config");
        (dir, at, config)
    }

    #[test]
    fn a_row_that_is_not_approved_carries_the_question_to_ask_about_it() {
        let (_dir, at, config) = made();
        extension::install(&config, &at).expect("installed");

        let listed = listed(&extension::survey(&config));
        let row = &listed.extensions[0];
        assert_eq!(row.standing, "new");
        assert!(
            row.themes_in_force.is_empty(),
            "it contributed before it was asked about"
        );
        let ask = row.ask.as_ref().expect("a question");
        assert_eq!(ask.runs_as_you, extension::RUNS_AS_YOU);
        assert_eq!(ask.fingerprint_note, extension::FINGERPRINTED);
        assert_eq!(
            ask.state_note, None,
            "an extension with no state directory has no exception to state"
        );
    }

    #[test]
    fn a_state_directory_is_carried_to_the_window_as_the_core_words_it() {
        // charter-app#152. `fingerprint_note` says charter read every file in the directory;
        // the one directory it did not read has to reach the same screen, in the core's own
        // words, or the window is left to word the exception itself.
        let (_dir, at, config) = made();
        std::fs::write(
            at.join(extension::MANIFEST),
            r#"{"version":1,"id":"solarized","name":"Solarized","state":"cache",
                "contributes":{"themes":[{"name":"Solarized Dark","file":"dark.json"}]}}"#,
        )
        .expect("a manifest with a state directory");
        extension::install(&config, &at).expect("installed");

        let listed = listed(&extension::survey(&config));
        let ask = listed.extensions[0].ask.as_ref().expect("a question");
        assert_eq!(
            ask.state_note.as_deref(),
            Some(&*extension::state_note("cache"))
        );
    }

    #[test]
    fn an_approved_row_has_nothing_left_to_ask() {
        // A question that never carries risk is one an operator learns to answer without
        // reading, which is `profiletrust::approval_needed`'s reason for never asking about a
        // built-in.
        let (_dir, at, config) = made();
        let found = extension::install(&config, &at).expect("installed");
        extension::approve(&config, found.id(), &found.path, &found.fingerprint).expect("approved");

        let listed = listed(&extension::survey(&config));
        let row = &listed.extensions[0];
        assert_eq!(row.standing, "approved");
        assert_eq!(row.themes_in_force, vec!["Solarized Dark".to_owned()]);
        assert!(row.ask.is_none());
    }

    #[test]
    fn a_row_charter_could_not_read_says_why_and_still_lists_what_it_declared() {
        let (_dir, at, config) = made();
        extension::install(&config, &at).expect("installed");
        std::fs::remove_file(at.join(extension::MANIFEST)).expect("the manifest");

        let listed = listed(&extension::survey(&config));
        let row = &listed.extensions[0];
        assert!(row.refused.is_some(), "it went quiet");
        assert!(row.themes_in_force.is_empty());
        assert!(
            row.ask.is_none(),
            "charter asked about something it could not read"
        );
    }

    #[test]
    fn only_an_approved_extensions_theme_is_ever_in_force() {
        let (_dir, at, config) = made();
        let found = extension::install(&config, &at).expect("installed");
        assert!(
            in_force(&extension::survey(&config)).is_empty(),
            "an unapproved extension's theme reached the window"
        );

        extension::approve(&config, found.id(), &found.path, &found.fingerprint).expect("approved");
        let themes = in_force(&extension::survey(&config));
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].extension, "solarized");
        assert_eq!(themes[0].name, "Solarized Dark");
        assert_eq!(themes[0].text, "{}", "the text is the file's, unparsed");

        std::fs::write(at.join("dark.json"), r#"{"tokens":{}}"#).expect("a changed theme");
        assert!(
            in_force(&extension::survey(&config)).is_empty(),
            "a theme that changed after approval was still drawn"
        );
    }

    #[test]
    fn the_window_is_told_charters_own_themes_as_well() {
        let (_dir, _at, config) = made();
        let listed = listed(&extension::survey(&config));

        assert_eq!(listed.built_in_themes, extension::BUILT_IN_THEMES.to_vec());
    }
}
