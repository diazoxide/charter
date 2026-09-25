//! Views — what a tab holds when it does not hold a chat — as the window asks for them: which
//! are offered, and what one answers when the operator opens it.
//!
//! **A tab is a layout of panes, and a pane holds a session or a view** (ADR 0043, as
//! amended 2026-09-23). A view is named by data — who draws it, which of theirs, and what it is
//! about — and **charter's own views and an approved extension's come through the one command**
//! ([`open_view`]) and answer in the one vocabulary (`charter_core::panel`). The persona view
//! is the first built-in one (`panels::persona_view`); persona statistics is the first an
//! extension offers. The window draws both with the same code and cannot tell them apart except
//! by whose they are, which it says.
//!
//! `charter_core::executor` is the whole of the thinking for an extension's view — the gate
//! re-taken at the press, the one-question process, the bounds, the kill. This is the wire, and
//! two decisions that are the app's to make:
//!
//! - **The executor runs on a blocking thread and the window never waits on it.** A program
//!   that stalls for its whole deadline costs the tab that opened it a spinner and then a
//!   sentence; the command that draws the rest of the window is not in the same queue.
//! - **One question at a time per extension is queued here, not refused.** The executor holds
//!   one slot per extension and refuses a second ask while the first is running — which a
//!   window switching between two statistics tabs, or React's development double effect, used
//!   to meet as *"still answering"*. So this process takes a turn per extension before it asks:
//!   the second question waits for the first to finish (at most the executor's deadline) and is
//!   then asked, and the refusal is only ever seen by a caller outside this app.
//!
//! Two commands, and the split is the same one `extension_panels` / `workspace_panels` draws:
//!
//! - [`extension_views`] — what is offered, asked **once per window** after the first frame,
//!   because it is a survey and a survey re-hashes every installed extension's directory.
//! - [`open_view`] — what one view answers, asked **when the operator opens it** and never
//!   otherwise. For an extension's view it re-takes the gate itself, so a view that was offered
//!   at the survey and has changed since is refused at the press rather than run on the
//!   survey's word.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use charter_core::executor::Executor;
use charter_core::extension;
use charter_core::panel::Subject;

use crate::panels::PanelBlock;

/// The executor, held for as long as the app runs, so that its table of running programs is
/// one table and [`Views::stop_all`] reaches every one of them at exit — and the turns this
/// process takes before asking each extension anything.
#[derive(Default)]
pub(crate) struct Views {
    executor: Arc<Executor>,
    /// One lock per extension id, taken on the blocking thread around the whole ask. See this
    /// module's header: it is what turns the executor's *busy* refusal into a wait.
    turns: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl Views {
    /// The views of an app whose built-in extensions are `built_in` (charter-app#339): its
    /// executor starts them with no approval, and only where the app has them.
    pub(crate) fn with_built_in(built_in: extension::BuiltIn) -> Self {
        Self {
            executor: Arc::new(Executor::with_built_in(built_in)),
            ..Self::default()
        }
    }

    /// Kill every extension program still answering. The app's `Exit`, so that nothing an
    /// extension was asked to run outlives the window that asked.
    pub(crate) fn stop_all(&self) {
        self.executor.stop_all();
    }

    /// The lock that is `extension`'s turn.
    fn turn_of(&self, extension: &str) -> Arc<Mutex<()>> {
        let mut turns = self.turns.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(turns.entry(extension.to_owned()).or_default())
    }
}

/// Whether charter starts an extension's program on this platform at all.
///
/// **The same answer `charter_core::executor` gives**, asked here so that a platform where every
/// view would be refused does not offer one: a Statistics button that always answers *"charter
/// does not start an extension's program on this platform"* is a button that should not be
/// drawn. The executor keeps its own refusal as the gate; this only decides what is offered.
const RUNS_PROGRAMS: bool = cfg!(unix);

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
///
/// **`Gone` is not a refusal, and the window must keep them apart.** A tab that came back at a
/// launch can name a persona that has since been deleted, or an extension that has since been
/// uninstalled; that tab is drawn as a view whose source has gone — a sentence in the middle of
/// the tab, with nothing to repair — and not as an error. A refusal (`Err`) is something the
/// operator can act on: approve the extension again, make its program runnable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum ViewAnswer {
    Answered {
        /// The blocks, in the panel vocabulary, parsed and re-emitted by the core.
        blocks: Vec<PanelBlock>,
        /// How long it took, gate and round trip together, in milliseconds. Drawn quietly
        /// under the answer, because a producer that has become slow is worth noticing before
        /// it becomes one that times out.
        took_ms: u32,
    },
    /// What the view was about is not there any more, and why, in one sentence.
    Gone { why: String },
}

/// Every view an approved extension offers this window.
///
/// An extension that is new, changed or unreadable offers nothing — the registry's one job, as
/// for themes and panels. **What this returns is a list of buttons, not a list of permissions**:
/// [`open_view`] asks the gate again when one is pressed.
#[tauri::command]
#[specta::specta]
pub(crate) async fn extension_views() -> Result<Vec<ExtensionView>, String> {
    if !RUNS_PROGRAMS {
        return Ok(Vec::new());
    }
    let root = charter_core::machine::config_root().ok_or_else(|| {
        "this machine has no config home, so charter keeps no extensions".to_owned()
    })?;
    tauri::async_runtime::spawn_blocking(move || {
        offered(&extension::survey(&root, &crate::extensions::built_in()))
    })
    .await
    .map_err(|err| format!("reading this machine's views did not finish: {err}"))
}

/// Whether this platform runs extension programs at all
/// ([`charter_core::executor::RUNS_PROGRAMS`]). On one that does not, [`extension_views`]
/// offers nothing and the window should not draw a place for a view to go.
#[tauri::command]
#[specta::specta]
pub(crate) fn extension_programs_run() -> bool {
    charter_core::executor::RUNS_PROGRAMS
}

/// [`extension_views`] with the survey already taken, so the shaping is testable without a
/// Tauri runtime.
///
/// **Nothing on a platform that runs no programs**: a view's button there could only ever
/// answer [`charter_core::executor::REFUSED_HERE`], and a button that always refuses is a
/// button the operator should not have been shown.
fn offered(seen: &extension::Survey) -> Vec<ExtensionView> {
    if !charter_core::executor::RUNS_PROGRAMS {
        return Vec::new();
    }
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

/// What one view shows for this plane, now.
///
/// `from` is `None` for a view charter draws itself and an extension's id for one it offers;
/// `view` is which of theirs; `key` is what it is about inside that — a persona's name, or
/// empty for the whole plane. For an extension's view a non-empty key is handed to its program
/// as the persona it was opened from, and is checked as a persona name before it is.
///
/// **Every refusal comes back as the core's sentence**, which names what refused and says what
/// to do. The window draws it where the answer would have been.
#[tauri::command]
#[specta::specta]
pub(crate) async fn open_view(
    views: tauri::State<'_, Views>,
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    from: Option<String>,
    view: String,
    key: String,
    workspace: Option<String>,
) -> Result<ViewAnswer, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let Some(extension) = from else {
        // The state directory `charter persona list` reads the vault registry's local half from.
        let state = charter_core::personaverbs::state_dir(&root);
        return tauri::async_runtime::spawn_blocking(move || built_in(&root, &state, &view, &key))
            .await
            .map_err(|err| format!("reading that view did not finish: {err}"))?;
    };
    let config = charter_core::machine::config_root().ok_or_else(|| {
        "this machine has no config home, so charter keeps no extensions".to_owned()
    })?;
    if !key.is_empty() && !charter_core::personas::valid_name(&key) {
        return Err(format!(
            "{key:?} is not a persona name charter would hand anybody"
        ));
    }
    let executor = Arc::clone(&views.executor);
    let turn = views.turn_of(&extension);
    tauri::async_runtime::spawn_blocking(move || {
        // Uninstalled is gone, not refused: there is nothing to approve and nothing to repair,
        // and a tab that came back naming it says so and nothing else. The record is a small
        // file and this reads it without fingerprinting anything; the gate proper is the
        // executor's, below.
        if extension::read(&config, &crate::extensions::built_in())
            .entry(&extension)
            .is_none()
        {
            return Ok(ViewAnswer::Gone {
                why: format!(
                    "The extension '{extension}' is not installed on this machine any more, so \
                     this view has nothing to show."
                ),
            });
        }
        let _turn = turn.lock().unwrap_or_else(PoisonError::into_inner);
        // What the project this view is in says about the extension — and the workspace whose
        // strip it is on (charter-app#280) — read at the press like the rest of the gate
        // (ADR 0048).
        let project = extension::project::Choices::read_in(&root, workspace.as_deref());
        executor
            .ask(
                &config,
                &project,
                &extension,
                &view,
                Some(key.as_str()).filter(|key| !key.is_empty()),
                |about| match about {
                    Subject::Personas => {
                        charter_core::handed::personas(&root, chrono::Local::now().naive_local())
                    }
                },
            )
            .map(|answer| ViewAnswer::Answered {
                blocks: answer.blocks.iter().map(PanelBlock::from).collect(),
                took_ms: millis(answer.gate + answer.round_trip),
            })
    })
    .await
    .map_err(|err| format!("asking that view did not finish: {err}"))?
}

/// charter's own views that answer in panel blocks, by id. **One today**, the persona view, and
/// the match is the whole of the registry: a built-in view is code in this process, so there is
/// nothing to discover. The vault view (`{ view: "vault", key: <vault> }`, charter-app#235) is
/// charter's too, but the window draws it itself from the `vault_*` commands — a table the
/// operator writes to is not something blocks can say — so it is never asked here, and a caller
/// that did would be told this charter has no such block view.
///
/// `state` is the plane's state directory, where this machine's half of the vault registry is
/// — passed in rather than looked up so a test names its own and never reads `$CHARTER_HOME`.
fn built_in(
    root: &std::path::Path,
    state: &std::path::Path,
    view: &str,
    key: &str,
) -> Result<ViewAnswer, String> {
    let began = std::time::Instant::now();
    match view {
        "persona" => Ok(match crate::panels::persona_view(root, state, key)? {
            Some(blocks) => ViewAnswer::Answered {
                blocks: blocks.iter().map(PanelBlock::from).collect(),
                took_ms: millis(began.elapsed()),
            },
            None => ViewAnswer::Gone {
                why: format!("This plane has no persona called {key} any more."),
            },
        }),
        other => Ok(ViewAnswer::Gone {
            why: format!("This version of charter has no view called '{other}'."),
        }),
    }
}

/// A tab that holds a view, as the window and the record both know it (`reopen::View`).
///
/// **The window says these and the record keeps them**; nothing in the core decides one. They
/// travel as a whole list every time it changes, because a tab strip is small and a list that
/// is the window's arrangement is easier to keep true than a stream of edits to one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub(crate) struct ViewTab {
    /// The extension's id, or `null` for charter's own view.
    pub from: Option<String>,
    pub view: String,
    /// A persona's name, or empty for the whole plane.
    pub key: String,
    /// What the tab says.
    pub title: String,
    /// The strip it is on, or `null` for the strip of chats outside every workspace.
    pub workspace: Option<String>,
    /// Where it is on the strip, counted over chats and views together from the left.
    pub at: u32,
    pub active: bool,
    pub pinned: bool,
}

impl From<charter_core::reopen::View> for ViewTab {
    fn from(view: charter_core::reopen::View) -> Self {
        Self {
            from: view.from,
            view: view.view,
            key: view.key,
            title: view.title,
            workspace: view.workspace,
            at: view.at,
            active: view.active,
            pinned: view.pinned,
        }
    }
}

impl From<ViewTab> for charter_core::reopen::View {
    fn from(tab: ViewTab) -> Self {
        Self {
            from: tab.from,
            view: tab.view,
            key: tab.key,
            title: tab.title,
            workspace: tab.workspace,
            at: tab.at,
            active: tab.active,
            pinned: tab.pinned,
        }
    }
}

/// The view tabs this plane had open when it was last recorded — at a launch, the ones the
/// record put back. The window opens a tab for each; nothing in one is asked until it is drawn.
#[tauri::command]
#[specta::specta]
pub(crate) fn reopened_views(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
) -> Result<Vec<ViewTab>, String> {
    Ok(planes
        .held(&plane)?
        .chats()
        .views()
        .into_iter()
        .map(ViewTab::from)
        .collect())
}

/// What view tabs the window has open now, so the record brings them back at the next launch.
#[tauri::command]
#[specta::specta]
pub(crate) fn window_views(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    views: Vec<ViewTab>,
) -> Result<(), String> {
    planes
        .held(&plane)?
        .chats()
        .hold_views(views.into_iter().map(Into::into).collect());
    Ok(())
}

fn millis(took: std::time::Duration) -> u32 {
    u32::try_from(took.as_millis()).unwrap_or(u32::MAX)
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
        let found =
            extension::install(&config, &extension::BuiltIn::none(), &at).expect("installed");
        if approve {
            extension::approve(&config, found.id(), &found.path, &found.fingerprint)
                .expect("approved");
        }
        (dir, config)
    }

    #[test]
    fn an_approved_extension_s_view_is_offered_with_what_it_is_about() {
        let (_dir, config) = made(true);
        let offered = offered(&extension::survey(&config, &extension::BuiltIn::none()));
        if !charter_core::executor::RUNS_PROGRAMS {
            assert!(
                offered.is_empty(),
                "a view was offered where it could only refuse"
            );
            return;
        }
        assert_eq!(
            offered,
            vec![ExtensionView {
                extension: "stats".into(),
                id: "statistics".into(),
                title: "Statistics".into(),
                about: "personas".into(),
            }]
        );
    }

    #[test]
    fn the_persona_view_is_answered_in_the_panel_vocabulary_with_its_memories_as_a_list() {
        let plane = tempfile::tempdir().expect("a plane");
        let dir = plane.path().join("personas/steward");
        std::fs::create_dir_all(dir.join("memory")).expect("a persona");
        std::fs::write(
            dir.join("persona.md"),
            "---\nname: steward\nrole: The steward\ndelegate-when: routing\n---\nbody\n",
        )
        .expect("a definition");
        std::fs::write(
            dir.join("memory/a-fact.md"),
            "---\ntitle: A fact\n---\nThe whole of it.\n",
        )
        .expect("a memory");

        let ViewAnswer::Answered { blocks, .. } = built_in(
            plane.path(),
            &plane.path().join(".charter"),
            "persona",
            "steward",
        )
        .expect("an answer") else {
            panic!("the persona view was not answered");
        };

        let notes: Vec<&str> = blocks
            .iter()
            .filter_map(|block| match block {
                PanelBlock::Note { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(notes.contains(&"It remembers 1 thing."), "{notes:?}");
        // The definition is label and value — the two columns the card drew — not sentences.
        let facts: Vec<(&str, &str)> = blocks
            .iter()
            .filter_map(|block| match block {
                PanelBlock::Facts { facts } => Some(facts),
                _ => None,
            })
            .flatten()
            .map(|fact| (fact.label.as_str(), fact.value.as_str()))
            .collect();
        assert!(
            facts.contains(&("Delegate to it for", "routing")),
            "{facts:?}"
        );
        assert!(
            facts.iter().any(|(label, _)| *label == "Vault"),
            "{facts:?}"
        );
        let Some(PanelBlock::List { rows, .. }) = blocks.last() else {
            panic!("the memories are not a list: {blocks:?}");
        };
        assert_eq!(rows.len(), 1);
    }

    /// The persona view's `Vault` fact for `name`, asked with `state` as the state directory.
    fn vault_fact(root: &std::path::Path, state: &std::path::Path, name: &str) -> String {
        let ViewAnswer::Answered { blocks, .. } =
            built_in(root, state, "persona", name).expect("an answer")
        else {
            panic!("the persona view was not answered");
        };
        blocks
            .iter()
            .filter_map(|block| match block {
                PanelBlock::Facts { facts } => Some(facts),
                _ => None,
            })
            .flatten()
            .find(|fact| fact.label == "Vault")
            .map(|fact| fact.value.clone())
            .expect("a Vault fact")
    }

    fn persona_on(root: &std::path::Path, name: &str, definition: &str) {
        let dir = root.join("personas").join(name);
        std::fs::create_dir_all(&dir).expect("a persona");
        std::fs::write(dir.join("persona.md"), definition).expect("a definition");
    }

    /// charter-app#185: a vault only the registry tags to the persona is named on the card, and
    /// it is the vault `charter persona list` names — not "not declared in its definition".
    #[test]
    fn the_persona_view_names_the_vault_the_registry_tags_as_the_cli_does() {
        let plane = tempfile::tempdir().expect("a plane");
        let state = plane.path().join(".charter");
        persona_on(plane.path(), "release", "---\nrole: Release\n---\n");
        std::fs::write(
            plane.path().join("vaults.json"),
            r#"{"vaults": {"release-kr": {"provider": "keyring", "persona": "release"}}}"#,
        )
        .expect("a registry");

        let shown = vault_fact(plane.path(), &state, "release");

        let cli = charter_core::personaverbs::vault_of(plane.path(), &state, "release")
            .expect("the CLI names one");
        assert!(shown.starts_with(&format!("{cli} — ")), "{shown}");
        assert!(shown.contains("vault registry"), "{shown}");
        assert!(!shown.contains("not declared"), "{shown}");
    }

    /// The other answers the card gives: a definition's own name, `vault: none`, nobody naming
    /// one, and a registry that does not read — each its own words.
    #[test]
    fn the_persona_view_says_where_a_vault_came_from_or_why_there_is_none() {
        let plane = tempfile::tempdir().expect("a plane");
        let state = plane.path().join(".charter");
        persona_on(plane.path(), "devops", "---\nvault: devops\n---\n");
        persona_on(plane.path(), "steward", "---\nvault: none\n---\n");
        persona_on(plane.path(), "release", "---\nrole: Release\n---\n");

        assert_eq!(
            vault_fact(plane.path(), &state, "devops"),
            "devops — the name; what is in it is never shown here"
        );
        assert_eq!(
            vault_fact(plane.path(), &state, "steward"),
            "none; this persona holds no credentials of its own"
        );
        assert_eq!(
            vault_fact(plane.path(), &state, "release"),
            "none — neither its definition nor the vault registry names one"
        );

        std::fs::write(plane.path().join("vaults.json"), "{not json").expect("a registry");
        let unread = vault_fact(plane.path(), &state, "release");
        assert!(
            unread.starts_with(
                "unknown — its definition names none, and the vault registry does not read: "
            ),
            "{unread}"
        );
        assert!(unread.contains("vaults.json is corrupt"), "{unread}");
    }

    #[test]
    fn a_persona_the_plane_no_longer_has_is_a_view_whose_source_has_gone() {
        let plane = tempfile::tempdir().expect("a plane");
        std::fs::create_dir_all(plane.path().join("personas")).expect("a personas directory");

        assert!(matches!(
            built_in(
                plane.path(),
                &plane.path().join(".charter"),
                "persona",
                "steward"
            ),
            Ok(ViewAnswer::Gone { .. })
        ));
    }

    #[test]
    fn a_built_in_view_this_charter_does_not_have_is_gone_and_not_an_error() {
        // A record written by a newer charter can name one.
        let plane = tempfile::tempdir().expect("a plane");

        assert!(matches!(
            built_in(plane.path(), &plane.path().join(".charter"), "timeline", ""),
            Ok(ViewAnswer::Gone { .. })
        ));
    }

    #[test]
    fn a_persona_view_is_never_asked_about_a_name_that_is_not_a_persona_name() {
        let plane = tempfile::tempdir().expect("a plane");

        assert!(
            built_in(
                plane.path(),
                &plane.path().join(".charter"),
                "persona",
                "../etc"
            )
            .is_err()
        );
    }

    #[test]
    fn an_unapproved_extension_offers_no_view() {
        let (_dir, config) = made(false);
        assert!(offered(&extension::survey(&config, &extension::BuiltIn::none())).is_empty());
    }
}
