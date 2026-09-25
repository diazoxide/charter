//! Events the window's own actions report to the extensions that hear them (charter-app#343).
//!
//! `charter_core::extension::events::deliver` is the whole of the thinking — who hears what,
//! the gate, the deadline. This is the app's half: **when** it is asked, and where what it says
//! goes.
//!
//! - **After the command has its answer, on a thread of its own.** A command that did something
//!   an extension hears — created or removed a workspace, saved the plane, the operator focusing
//!   a workspace — first works out its own result, then hands the event to [`Heard::tell`], which
//!   starts a thread and returns at once. The command answers the window without waiting for
//!   any extension, so a slow or broken one costs the window nothing and cannot change what the
//!   command answered.
//! - **A failure is a note, drawn with the facts file's notes.** Each note names the extension
//!   and is kept, a few per project, for `extension_facts` to hand the window beside its own
//!   ("N extension notes" on the status line). Then the window is told ([`HEARD`]) so it reads
//!   the facts again — which is also how a badge an extension refreshed from the event shows.
//! - **Its own executor**, stopped at exit with the views' one. An event and a view may each
//!   have one question in flight to the same extension; two events wait their turn
//!   (`Executor::tell`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use charter_core::executor::Executor;
use charter_core::extension::events::{self, Event};
use charter_core::extension::project::Choices;
use tauri::Emitter;

use crate::planes::PlaneId;

/// What the window is told once the extensions that hear an event have been told: the plane it
/// happened in, so only that project's window parts read their facts again.
pub(crate) const HEARD: &str = "extension-heard";

/// The most notes kept per project. The status line shows a count and the list in its tooltip;
/// the newest are the ones worth reading.
const MOST_NOTES: usize = 8;

/// The executor events are delivered with, and the notes they left.
#[derive(Default)]
pub(crate) struct Heard {
    executor: Arc<Executor>,
    notes: Arc<Mutex<HashMap<PathBuf, Vec<String>>>>,
}

/// What `extension-heard` carries: the project whose extensions were just told something.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct ExtensionHeard {
    pub plane: PlaneId,
}

impl Heard {
    /// Kill every extension program still being told something. The app's `Exit`.
    pub(crate) fn stop_all(&self) {
        self.executor.stop_all();
    }

    /// Tell the extensions that hear it that `event` happened in the plane at `root`, on a
    /// thread started here — the caller returns at once. See the module's header.
    pub(crate) fn tell(&self, app: &tauri::AppHandle, plane: PlaneId, root: PathBuf, event: Event) {
        let Some(config) = charter_core::machine::config_root_if_there() else {
            return;
        };
        let executor = Arc::clone(&self.executor);
        let notes = Arc::clone(&self.notes);
        let app = app.clone();
        let _ = std::thread::Builder::new()
            .name(format!("extension event {}", event.kind().as_str()))
            .spawn(move || {
                deliver(&executor, &notes, &config, &root, &event);
                let _ = app.emit(HEARD, ExtensionHeard { plane });
            });
    }

    /// The notes events left for the plane at `root`, oldest first.
    pub(crate) fn notes_for(&self, root: &Path) -> Vec<String> {
        self.notes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(root)
            .cloned()
            .unwrap_or_default()
    }
}

/// [`Heard::tell`]'s work, on the thread it starts: deliver, and keep what went wrong.
fn deliver(
    executor: &Executor,
    notes: &Mutex<HashMap<PathBuf, Vec<String>>>,
    config: &Path,
    root: &Path,
    event: &Event,
) {
    let choices = Choices::read_in(root, event.workspace());
    let said = events::deliver(executor, config, &choices, event);
    if said.is_empty() {
        return;
    }
    let mut notes = notes.lock().unwrap_or_else(PoisonError::into_inner);
    let kept = notes.entry(root.to_path_buf()).or_default();
    kept.extend(said);
    let over = kept.len().saturating_sub(MOST_NOTES);
    kept.drain(..over);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// An approved extension at `<dir>/ext` that hears a workspace being created and answers
    /// `answer` to it, writing down every event it is told.
    fn hearing(dir: &Path, answer: &str) -> PathBuf {
        use charter_core::extension;
        let ext = dir.join("ext");
        std::fs::create_dir_all(ext.join("bin")).expect("the extension's directory");
        std::fs::write(
            ext.join(extension::MANIFEST),
            r#"{"version": 2, "id": "script", "name": "Script", "state": "state",
                "capabilities": ["events"],
                "contributes": {"runs": "bin/script",
                                "events": {"hears": ["workspace-created"]}}}"#,
        )
        .expect("a manifest");
        let program = ext.join("bin/script");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nIFS= read -r line\nmkdir -p \"$CHARTER_EXTENSION_STATE\"\n\
                 printf '%s\\n' \"$line\" >> \"$CHARTER_EXTENSION_STATE/heard\"\n\
                 printf '%s\\n' '{answer}'\n"
            ),
        )
        .expect("a program");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
            .expect("runnable");
        let config = dir.join("config");
        let found = extension::install(&config, &ext).expect("installed");
        extension::approve(&config, found.id(), &found.path, &found.fingerprint).expect("approved");
        config
    }

    fn created() -> Event {
        Event::WorkspaceCreated {
            workspace: "beta".into(),
        }
    }

    #[test]
    fn an_extension_that_hears_it_is_told_and_leaves_no_note() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = hearing(dir.path(), r#"{"charter":2}"#);
        let heard = Heard::default();
        let root = dir.path().join("plane");
        std::fs::create_dir_all(&root).expect("a plane");

        deliver(&heard.executor, &heard.notes, &config, &root, &created());

        assert!(heard.notes_for(&root).is_empty());
        let told = std::fs::read_to_string(dir.path().join("ext/state/heard")).expect("told");
        assert!(told.contains(r#""event":"workspace-created""#), "{told}");
    }

    #[test]
    fn a_failure_is_kept_as_a_note_for_that_project_and_no_other() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = hearing(dir.path(), r#"{"charter":2,"error":"broken"}"#);
        let heard = Heard::default();
        let root = dir.path().join("plane");
        std::fs::create_dir_all(&root).expect("a plane");

        for _ in 0..(MOST_NOTES + 3) {
            deliver(&heard.executor, &heard.notes, &config, &root, &created());
        }

        let notes = heard.notes_for(&root);
        assert_eq!(notes.len(), MOST_NOTES, "{notes:#?}");
        assert!(
            notes[0].starts_with("Script missed workspace 'beta' being created"),
            "{notes:#?}"
        );
        assert!(heard.notes_for(&dir.path().join("elsewhere")).is_empty());
    }
}
