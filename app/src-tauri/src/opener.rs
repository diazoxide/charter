//! The opener: how a plane gets into the app when no terminal handed one over.
//!
//! **The app used to have exactly one way in — the directory it was launched from.** A
//! double-clicked `.app` has `/` for a working directory, so the operator who opened charter
//! from the dock got a window that said `No plane` and offered nothing to do about it. ADR
//! 0033 is the decision that a project IS a plane and that the app opens one; this module is
//! the door.
//!
//! Four things live here and they are one story:
//!
//! - **what to offer** — the planes this machine remembers ([`recent_planes`]), and the folder
//!   picker for one it does not ([`pick_project`]);
//! - **what to ask** — a plane the operator has not approved is not opened, it is *described*,
//!   and the description is the dialog ([`Ask`]);
//! - **what an answer buys** — [`approve_plane`] records the yes and opens the plane, and it
//!   is the only way into `Planes`' second mint of `Approved`;
//! - **what a window holds** — [`window_holds_planes`] takes the tab strip and writes it into
//!   this machine's store, and [`planes_to_restore`] reads it back at the next cold launch
//!   (ADR 0033, decision 28). The restore says *which* projects; every one of them is opened
//!   through [`open_plane`] like any other, so it is not a way past the ask above.
//!
//! **Nothing here decides whether to ask.** `machine::Store::consent` decides, against the
//! plane as it is on this disk at this instant, inside `Planes::open_if_approved`; this module
//! carries the question to the window and the answer back. A second opinion about consent,
//! formed in the layer that draws it, is the shape of every gate that has later turned out not
//! to bite.

use std::collections::BTreeMap;

use charter_core::machine;
use tauri_plugin_dialog::DialogExt;

use crate::planes::{Asking, Holding, Opening, PlaneId, Planes, Restoring, Showing, restorable};

/// What a plane would contribute, as the trust prompt draws it — **and the exact value the
/// operator's approval is checked against.**
///
/// A mirror of [`machine::Contribution`] rather than the thing itself, because `charter-core`
/// never depends on the app and the app's wire types are generated into TypeScript. Each of
/// the four maps travels as pairs in the map's own order, which is `BTreeMap`'s and therefore
/// sorted, so a value that comes back from the window compares against one taken from disk
/// without either side having to sort anything.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PlaneContribution {
    /// Each plugin the plane's committed settings enable, and what they enable it as. These
    /// run inside the operator's harness.
    pub plugins: Vec<(String, String)>,
    /// Each environment variable those settings set, and its value. ADR 0022 measured where
    /// these land: a variable set on the harness process reaches the shell the model runs.
    pub env: Vec<(String, String)>,
    /// One line per chat the plane's reopen record would start on a program of its own — the
    /// program, its arguments and its directory, as the record names them.
    pub starts: Vec<(String, String)>,
    /// One line per chat that record would start on one of THIS machine's harness profiles.
    /// Drawn beside the rest and weighed differently: the record chooses which of the
    /// operator's own profiles runs, and `profiletrust` gates what any of them runs.
    pub profiles: Vec<(String, String)>,
}

impl PlaneContribution {
    /// What a plane contributes, as the window will draw it.
    fn of(what: &machine::Contribution) -> Self {
        Self {
            plugins: pairs(&what.plugins),
            env: pairs(&what.env),
            starts: pairs(&what.starts),
            profiles: pairs(&what.profiles),
        }
    }

    /// The same thing as the core's own shape, for the check against what is on disk now.
    ///
    /// **Duplicate keys collapse, and that is the safe direction.** A map cannot hold two of
    /// a name, so a value the window sent twice becomes one entry here and then fails to
    /// equal the plane's own contribution — which refuses the approval. The other direction,
    /// where a collapsed duplicate quietly matched, is the one that would let an approval
    /// cover something the operator never read.
    fn into_core(self) -> machine::Contribution {
        machine::Contribution {
            plugins: self.plugins.into_iter().collect(),
            env: self.env.into_iter().collect(),
            starts: self.starts.into_iter().collect(),
            profiles: self.profiles.into_iter().collect(),
        }
    }
}

fn pairs(map: &BTreeMap<String, String>) -> Vec<(String, String)> {
    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// One plane this machine remembers, as the opener draws it.
///
/// **When it was last opened is not here**, though the store holds it: it is what puts the
/// list in order, and the list is already in that order when it arrives. Carrying it would be
/// carrying a `u64` across the wire for nobody to draw — and specta refuses to export one at
/// all, to avoid the precision loss a JavaScript number would silently have. A row that says
/// "three days ago" can have it as a number this side of that limit, when there is a row that
/// says it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct RecentPlane {
    /// The plane's root: the path it was approved under and the path it will be opened by.
    pub path: String,
    /// What to call it in a list — the directory's own name. Two projects can share one, so
    /// the path is shown beside it and is what identifies the row.
    pub name: String,
    /// Whether the operator has approved this plane before.
    ///
    /// **Not a promise that opening it will not ask.** Whether it still contributes what they
    /// approved is a question about the plane's own files, and it is asked when the plane is
    /// opened. This is only what the store holds, which costs no disk at all — sixty-four
    /// rows each reading a settings file and a reopen record, to label a list, is a launch
    /// spent answering a question the next click answers properly.
    pub approved: bool,
}

/// What the opener has to offer, and everything it had to leave out to offer it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Recents {
    /// Most recently opened first.
    pub planes: Vec<RecentPlane>,
    /// One line per row charter would not offer: a plane that has moved or gone, and an entry
    /// the store itself would not take back.
    ///
    /// **Never an error, and never a dialog.** ADR 0034: the file is a convenience and the
    /// plane is the truth, so a launch that showed a modal about a memory stick that is not
    /// plugged in would be worse than the thing it was reporting.
    pub dropped: Vec<String>,
    /// Why this machine remembers nothing at all, in charter's own words — a platform charter
    /// keeps no store on (ADR 0031: `0600` has no expression on Windows, so the guard refuses
    /// rather than degrades), or a store charter would not read. Null when it does remember.
    ///
    /// The app is a working app either way. It just opens every project by picking it.
    pub forgetful: Option<String>,
}

/// What a cold launch puts back: the projects that were open, and what it would not take back.
///
/// **A project that has moved or is gone is dropped with a line saying so, never an error
/// dialog** (ADR 0033). The window draws those lines where the operator is standing, the way
/// the opener draws a recents row that went.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Restore {
    /// The projects to open again, left to right as the tabs were.
    pub planes: Vec<String>,
    /// Which of them was in front, as an index into `planes` after the drops. Null when there
    /// is nothing to put back.
    pub active: Option<u32>,
    /// One line per project charter would not take back.
    pub dropped: Vec<String>,
}

/// What a window is holding, as it says so itself.
///
/// One struct rather than two arguments, so that the tabs and the tab in front cannot be sent
/// separately and disagree — and because a `Vec<PlaneId>` keeps its own name here, where a
/// plane inside an `Option` argument does not.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct WindowTabs {
    /// The projects this window holds, left to right as its tabs show them.
    pub planes: Vec<PlaneId>,
    /// Which tab is in front. Null is the opener — the window is holding projects the
    /// operator is not looking at, or holding none at all.
    pub active: Option<u32>,
}

/// The trust ask: what this plane will put in force, in the words the operator reads.
///
/// **In the app the prompt IS the prompt** (ADR 0035). charter's CLI asks by printing a second
/// command to type, because `util.py` has nothing that reads stdin and a hook blocked on stdin
/// hangs a turn — a constraint about the CLI and about nothing else. Here there is a window
/// and a person looking at it, so the question is asked where the answer is given.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Ask {
    /// The plane charter resolved, which is what the approval is recorded against.
    ///
    /// Shown, and not merely carried: a picker pointed at a subdirectory opens the plane above
    /// it, and an operator approving a directory they did not choose is the whole failure this
    /// dialog exists to prevent, arrived at from the friendly end.
    pub path: String,
    /// What it contributes. It goes back to [`approve_plane`] untouched, and that is what
    /// makes the approval an answer to the question that was asked.
    pub contributes: PlaneContribution,
    /// Empty when nothing has approved this plane. Otherwise every way it now differs from
    /// what WAS approved, in charter's own words (`machine::Change`'s own `Display`).
    pub changes: Vec<String>,
    /// Whether this is a first approval rather than a re-ask, so the dialog can say which.
    pub first: bool,
}

impl Ask {
    fn of(asking: Asking) -> Self {
        Self {
            path: asking.root.display().to_string(),
            contributes: PlaneContribution::of(&asking.contributes),
            changes: asking
                .consent
                .changes()
                .iter()
                .map(ToString::to_string)
                .collect(),
            first: asking.first(),
        }
    }
}

/// What came of asking to open a plane: it is open, or there is a question to answer first.
///
/// Two nullable fields rather than a tagged union, and told apart by shape the way
/// `plane_at_launch` is: exactly one of them is ever set, and a window that reads `plane`
/// first cannot accidentally treat an ask as an open.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Opened {
    /// The plane, once it is open. Null when the operator has to be asked first.
    pub plane: Option<PlaneId>,
    /// What to ask them. Null when the plane is open.
    pub ask: Option<Ask>,
}

impl From<Opening> for Opened {
    fn from(opening: Opening) -> Self {
        match opening {
            Opening::Open(plane) => Self {
                plane: Some(plane),
                ask: None,
            },
            Opening::Ask(asking) => Self {
                plane: None,
                ask: Some(Ask::of(asking)),
            },
        }
    }
}

/// The planes this machine remembers, each checked against the disk, with what was dropped.
///
/// **On a blocking thread, and this is not a micro-optimisation.** Every row costs a
/// `symlink_metadata` and a second `stat` for the manifest, and a remembered plane can be on a
/// network mount, an unplugged external disk or an automounted share. `machine::read` is
/// deliberately lexical for exactly this reason, and the disk question it leaves out is asked
/// here — off the thread that draws, so an opener waiting on a share that is not coming back
/// is an opener that is still on screen.
#[tauri::command]
#[specta::specta]
pub async fn recent_planes(planes: tauri::State<'_, Planes>) -> Result<Recents, String> {
    // One gated open, no `stat` of anything it names: safe on the thread that asked.
    let loaded = planes.remembered();
    tauri::async_runtime::spawn_blocking(move || offer(loaded))
        .await
        .map_err(|err| format!("reading the planes this machine remembers did not finish: {err}"))
}

/// [`recent_planes`] with the store already read, so the disk questions can be tested without
/// a Tauri runtime to run them on.
fn offer(loaded: machine::Loaded) -> Recents {
    let mut planes = Vec::new();
    // What the store itself would not take back — a path that is not absolute, one with a
    // `..` in it, one holding a NUL. Already dropped with a reason by the read; carried
    // through so the opener can say a row went rather than silently showing one fewer.
    let mut dropped: Vec<String> = loaded.dropped.iter().map(ToString::to_string).collect();
    for entry in loaded.store.recents {
        let shown = entry.plane.display().to_string();
        match machine::still_a_plane(&entry.plane) {
            Err(why) => dropped.push(format!("{} {why}", charter_core::shown::short(&shown))),
            Ok(()) => planes.push(RecentPlane {
                name: entry
                    .plane
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    // A root directory has no name of its own, so it is called by its path.
                    .unwrap_or_else(|| shown.clone()),
                path: shown,
                approved: entry.trust.is_some(),
            }),
        }
    }
    Recents {
        planes,
        dropped,
        forgetful: loaded.unreadable,
    }
}

/// Asks the operating system for a folder, and answers with the one the operator chose.
///
/// Null is a cancelled dialog, which is not a failure and says nothing. The path is not
/// resolved here and not checked here: [`open_plane`] does both, because it is also what a
/// recents row and a second launch go through, and three callers resolving a path three times
/// is three answers to one question — the defect `plane.rs` records twice in its own
/// docstrings.
#[tauri::command]
#[specta::specta]
pub async fn pick_project(app: tauri::AppHandle) -> Result<Option<String>, String> {
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

/// Opens a plane, **or answers with the question that has to be asked first**.
///
/// The one way in for every caller that is not the launch: a recents row, a picked folder, a
/// typed path, and a second launch handing its directory to this process. All four go through
/// the same resolution and the same consent read, in `Planes::open_if_approved`.
///
/// A plane that is already open is answered with the id it already has, and nothing is bound
/// or started a second time — which is `Planes::open`'s rule and the reason it exists.
#[tauri::command]
#[specta::specta]
pub fn open_plane(planes: tauri::State<'_, Planes>, path: String) -> Result<Opened, String> {
    planes
        .open_if_approved(std::path::Path::new(&path))
        .map(Opened::from)
}

/// The operator's answer to [`Ask`]: yes, open it.
///
/// `contributes` is the value the dialog drew, handed straight back. It is checked against the
/// plane again before anything is approved or opened — see `Planes::approve_and_open`, where
/// the check and the reason for it live.
///
/// **This is a separate click from the one that opened the dialog, and it has to be.** A
/// command that both asked and opened would be asking nothing.
#[tauri::command]
#[specta::specta]
pub fn approve_plane(
    planes: tauri::State<'_, Planes>,
    path: String,
    contributes: PlaneContribution,
) -> Result<PlaneId, String> {
    planes.approve_and_open(std::path::Path::new(&path), &contributes.into_core())
}

/// The projects a cold launch has to put back, and every one it would not take back.
///
/// **Answered to the window rather than acted on here, and that is the whole shape of it.**
/// Opening a project starts the programs its reopen record names, so every open goes through
/// the trust gate — and the gate ends in a dialog, which only something with a window can
/// carry through. A restore that opened these itself would be a third mint of `Approved`
/// covering every project the operator had ever had open at once. So this says *which*, the
/// window opens each one through `open_plane` like a recents row, and a project that has
/// started doing more than what was approved is asked about exactly as it would have been.
///
/// **On a blocking thread**, for `recent_planes`' reason: every row costs a
/// `symlink_metadata` and a `stat`, and a remembered project can be on a share that is not
/// coming back.
#[tauri::command]
#[specta::specta]
pub async fn planes_to_restore(
    planes: tauri::State<'_, Planes>,
    restoring: tauri::State<'_, Restoring>,
) -> Result<Restore, String> {
    // `--no-restore` starts clean (spec decision 28). Answered with nothing rather than with
    // a reason: the operator asked for this, so there is no news in it.
    if !restoring.wanted() {
        return Ok(Restore {
            planes: Vec::new(),
            active: None,
            dropped: Vec::new(),
        });
    }
    // One gated open, no `stat` of anything it names: safe on the thread that asked.
    let loaded = planes.remembered();
    tauri::async_runtime::spawn_blocking(move || {
        let back = restorable(loaded);
        Restore {
            planes: back
                .planes
                .iter()
                .map(|plane| plane.display().to_string())
                .collect(),
            active: back.active.and_then(|at| u32::try_from(at).ok()),
            dropped: back.dropped,
        }
    })
    .await
    .map_err(|err| format!("reading the projects this window had open did not finish: {err}"))
}

/// A window says what it is holding: its projects as tabs, and which one is in front.
///
/// Two things, in one call, because they are one fact. A notification about a chat in a
/// project the operator is NOT looking at is sent rather than suppressed — see [`Showing`],
/// where the gap this closes is written down — and the tab strip is written into this
/// machine's store so the next cold launch puts it back (ADR 0033).
///
/// **The window says; nothing asks it.** The window is the only thing that knows what it
/// draws, and a core that inferred the arrangement from its own registry would be answering
/// "what is open in this process", which is a different question the moment a project is held
/// but not on screen.
#[tauri::command]
#[specta::specta]
pub fn window_holds_planes(
    window: tauri::Window,
    showing: tauri::State<'_, Showing>,
    planes: tauri::State<'_, Planes>,
    held: WindowTabs,
) {
    showing.in_window(
        window.label(),
        Holding {
            planes: held.planes,
            active: held.active.map(|at| at as usize),
        },
    );
    // Every window's, not this one's: the store holds the arrangement of the whole app, and
    // writing one window's tabs over it would forget the others.
    planes.remember_arrangement(&showing.arrangement());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A plane on disk, with nothing in it but the marker that makes it one.
    fn a_plane(at: &Path) -> PathBuf {
        std::fs::create_dir_all(at).expect("the plane's directory");
        std::fs::write(at.join(charter_core::plane::MANIFEST), "").expect("its charter.toml");
        at.to_path_buf()
    }

    fn remembering(planes: &[&Path]) -> machine::Loaded {
        let mut store = machine::Store::default();
        for (age, plane) in planes.iter().enumerate() {
            store.remember(plane, 100 - u64::try_from(age).unwrap_or(0));
        }
        machine::Loaded {
            store,
            ..machine::Loaded::default()
        }
    }

    #[test]
    fn a_remembered_plane_that_is_gone_is_dropped_with_a_line_and_never_an_error() {
        // ADR 0034: the file is a convenience and the plane is the truth. An opener that
        // shows one fewer row and says why is usable; a dialog before there is a window is
        // not, and an error would take the rest of the list with it.
        let dir = tempfile::tempdir().expect("a directory");
        let there = a_plane(&dir.path().join("there"));
        let gone = dir.path().join("gone");

        let offered = offer(remembering(&[&there, &gone]));

        assert_eq!(
            offered
                .planes
                .iter()
                .map(|row| &row.path)
                .collect::<Vec<_>>(),
            vec![&there.display().to_string()],
        );
        assert_eq!(offered.dropped.len(), 1, "{:?}", offered.dropped);
        assert!(
            offered.dropped[0].contains("no longer there"),
            "{}",
            offered.dropped[0]
        );
    }

    #[test]
    fn a_remembered_directory_that_stopped_being_a_plane_is_dropped_and_says_so() {
        let dir = tempfile::tempdir().expect("a directory");
        let was = a_plane(&dir.path().join("was"));
        std::fs::remove_file(was.join(charter_core::plane::MANIFEST)).expect("the manifest goes");

        let offered = offer(remembering(&[&was]));

        assert!(offered.planes.is_empty());
        assert!(
            offered.dropped[0].contains(charter_core::plane::MANIFEST),
            "{}",
            offered.dropped[0]
        );
    }

    #[test]
    fn a_row_says_whether_the_operator_has_approved_that_plane_before() {
        let dir = tempfile::tempdir().expect("a directory");
        let asked = a_plane(&dir.path().join("asked"));
        let never = a_plane(&dir.path().join("never"));
        let mut store = machine::Store::default();
        store.remember(&never, 1);
        store.approve(&asked, 2, machine::Contribution::of(&asked));
        let loaded = machine::Loaded {
            store,
            ..machine::Loaded::default()
        };

        let offered = offer(loaded);

        let by_name = |name: &str| {
            offered
                .planes
                .iter()
                .find(|row| row.name == name)
                .unwrap_or_else(|| panic!("a row for {name}"))
                .approved
        };
        assert!(by_name("asked"));
        assert!(!by_name("never"));
    }

    #[test]
    fn a_machine_that_keeps_no_store_says_so_and_still_answers_with_a_list() {
        // Windows has no store at all (ADR 0031), and the app there must be a working app
        // that simply cannot remember planes — never an opener that refuses to draw.
        let offered = offer(machine::Loaded {
            unreadable: Some("charter keeps no machine store on this platform".to_owned()),
            ..machine::Loaded::default()
        });

        assert!(offered.planes.is_empty());
        assert!(offered.forgetful.is_some());
    }

    #[test]
    fn what_the_window_sends_back_is_the_contribution_it_was_shown() {
        // The approval is checked by comparing these, so the trip through the wire types has
        // to be lossless. A field that did not survive it would make every approval refuse —
        // or, far worse, make two different contributions compare equal.
        let what = machine::Contribution {
            plugins: [("a@market".to_owned(), "true".to_owned())]
                .into_iter()
                .collect(),
            env: [("PATH".to_owned(), "/x".to_owned())].into_iter().collect(),
            starts: [("{\"program\":\"/bin/sh\"}".to_owned(), String::new())]
                .into_iter()
                .collect(),
            profiles: [("{\"profile\":\"work\"}".to_owned(), String::new())]
                .into_iter()
                .collect(),
        };

        assert_eq!(PlaneContribution::of(&what).into_core(), what);
    }

    #[test]
    fn a_first_ask_is_told_apart_from_a_plane_that_has_started_doing_more() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));

        let first = Ask::of(Asking {
            root: root.clone(),
            contributes: machine::Contribution::default(),
            consent: machine::Consent::New,
        });
        let again = Ask::of(Asking {
            root,
            contributes: machine::Contribution::default(),
            consent: machine::Consent::Grew(vec![machine::Change::PluginAdded("a".to_owned())]),
        });

        assert!(first.first);
        assert!(first.changes.is_empty());
        assert!(!again.first);
        assert_eq!(again.changes.len(), 1);
        assert!(
            again.changes[0].contains("plugin a"),
            "{}",
            again.changes[0]
        );
    }
}
