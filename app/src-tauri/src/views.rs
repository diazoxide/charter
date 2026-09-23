//! An extension's views, as the window asks for them: which are offered, and what one answers
//! when the operator opens it.
//!
//! `charter_core::executor` is the whole of the thinking — the gate re-taken at the press, the
//! one-question process, the bounds, the kill. This is the wire, and one decision that is the
//! app's to make: **the executor runs on a blocking thread and the window never waits on it.**
//! A program that stalls for its whole five seconds costs the surface that opened it a spinner
//! and then a sentence; the command that draws the rest of the window is not in the same queue.
//!
//! Two commands, and the split is the same one `extension_panels` / `workspace_panels` draws:
//!
//! - [`extension_views`] — what is offered, asked **once per window** after the first frame,
//!   because it is a survey and a survey re-hashes every installed extension's directory.
//! - [`open_view`] — what one view answers, asked **when the operator opens it** and never
//!   otherwise. It re-takes the gate itself, so a view that was offered at the survey and has
//!   changed since is refused at the press rather than run on the survey's word.

use std::sync::Arc;

use charter_core::executor::Executor;
use charter_core::extension;
use charter_core::panel::Subject;

use crate::panels::PanelBlock;

/// The executor, held for as long as the app runs, so that its table of running programs is
/// one table and [`Views::stop_all`] reaches every one of them at exit.
#[derive(Default)]
pub(crate) struct Views(Arc<Executor>);

impl Views {
    /// Kill every extension program still answering. The app's `Exit`, so that nothing an
    /// extension was asked to run outlives the window that asked.
    pub(crate) fn stop_all(&self) {
        self.0.stop_all();
    }
}

/// One view an approved extension offers, as the window draws its button.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct ExtensionView {
    /// The extension's id, which is what the view is asked through and what the surface says
    /// it came from (ADR 0041 item 5 — what is in force is shown after approval, not only at
    /// it).
    pub extension: String,
    /// The view's id within it.
    pub id: String,
    /// What the button and the surface are called.
    pub title: String,
    /// What it is about (`panel::Subject`) — which decides where the window offers it.
    pub about: String,
}

/// What a view answered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct ViewAnswer {
    /// The blocks, in the panel vocabulary, parsed and re-emitted by the core.
    pub blocks: Vec<PanelBlock>,
    /// How long it took, gate and round trip together, in milliseconds. Drawn quietly under
    /// the answer, because a producer that has become slow is worth noticing before it becomes
    /// one that times out.
    pub took_ms: u32,
}

/// Every view an approved extension offers this window.
///
/// An extension that is new, changed or unreadable offers nothing — the registry's one job, as
/// for themes and panels. **What this returns is a list of buttons, not a list of permissions**:
/// [`open_view`] asks the gate again when one is pressed.
#[tauri::command]
#[specta::specta]
pub(crate) async fn extension_views() -> Result<Vec<ExtensionView>, String> {
    let root = charter_core::machine::config_root().ok_or_else(|| {
        "this machine has no config home, so charter keeps no extensions".to_owned()
    })?;
    tauri::async_runtime::spawn_blocking(move || offered(&extension::survey(&root)))
        .await
        .map_err(|err| format!("reading this machine's views did not finish: {err}"))
}

/// [`extension_views`] with the survey already taken, so the shaping is testable without a
/// Tauri runtime.
fn offered(seen: &extension::Survey) -> Vec<ExtensionView> {
    seen.installed
        .iter()
        .flat_map(|row| {
            row.views_in_force().iter().map(|view| ExtensionView {
                extension: row.id.clone(),
                id: view.id.clone(),
                title: view.title.clone(),
                about: view.about.as_str().to_owned(),
            })
        })
        .collect()
}

/// Ask `extension`'s program what `view` shows for this plane, now.
///
/// `focus` is the persona whose card it was opened from, when it was — a name charter's own
/// panel put on screen, checked here as a persona name before it is handed to anybody.
///
/// **Every refusal comes back as the core's sentence**, which names the extension and says what
/// to do. The window draws it where the answer would have been.
#[tauri::command]
#[specta::specta]
pub(crate) async fn open_view(
    views: tauri::State<'_, Views>,
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    extension: String,
    view: String,
    focus: Option<String>,
) -> Result<ViewAnswer, String> {
    let config = charter_core::machine::config_root().ok_or_else(|| {
        "this machine has no config home, so charter keeps no extensions".to_owned()
    })?;
    let root = planes.held(&plane)?.root().to_path_buf();
    if let Some(name) = &focus
        && !charter_core::personas::valid_name(name)
    {
        return Err(format!(
            "{name:?} is not a persona name charter would hand anybody"
        ));
    }
    let executor = Arc::clone(&views.0);
    tauri::async_runtime::spawn_blocking(move || {
        executor
            .ask(
                &config,
                &extension,
                &view,
                focus.as_deref(),
                |about| match about {
                    Subject::Personas => {
                        charter_core::handed::personas(&root, chrono::Local::now().naive_local())
                    }
                },
            )
            .map(|answer| ViewAnswer {
                blocks: answer.blocks.iter().map(PanelBlock::from).collect(),
                took_ms: u32::try_from((answer.gate + answer.round_trip).as_millis())
                    .unwrap_or(u32::MAX),
            })
    })
    .await
    .map_err(|err| format!("asking that view did not finish: {err}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made(approve: bool) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let at = dir.path().join("ext");
        std::fs::create_dir_all(at.join("bin")).expect("the extension's directory");
        std::fs::write(
            at.join(extension::MANIFEST),
            r#"{"version":1,"id":"stats","name":"Stats","contributes":{"runs":"bin/run",
                "views":[{"id":"statistics","title":"Statistics","about":"personas"}]}}"#,
        )
        .expect("a manifest");
        std::fs::write(at.join("bin/run"), "#!/bin/sh\n").expect("a program");
        let config = dir.path().join("config");
        let found = extension::install(&config, &at).expect("installed");
        if approve {
            extension::approve(&config, found.id(), &found.path, &found.fingerprint)
                .expect("approved");
        }
        (dir, config)
    }

    #[test]
    fn an_approved_extension_s_view_is_offered_with_what_it_is_about() {
        let (_dir, config) = made(true);
        assert_eq!(
            offered(&extension::survey(&config)),
            vec![ExtensionView {
                extension: "stats".into(),
                id: "statistics".into(),
                title: "Statistics".into(),
                about: "personas".into(),
            }]
        );
    }

    #[test]
    fn an_unapproved_extension_offers_no_view() {
        let (_dir, config) = made(false);
        assert!(offered(&extension::survey(&config)).is_empty());
    }
}
