//! ⌘P's files (FM-7, #1110): a file found by its fuzzy name, in the scope the window's focus
//! picks — one branch, the project in front, or every project open in charter.
//!
//! Thin, as `piecefiles.rs` is: what is offered, how it ranks and what never appears are
//! `charter_core::files::find`'s answers. This holds one `Finder` per window for one palette
//! session, so each branch is listed once while the operator types, and drops it when the
//! palette closes or the window goes.
//!
//! **The window names projects and branches, never directories.** A project is a `PlaneId` the
//! registry vouches for, and "every open project" is the registry's own list, not one the
//! window hands in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use charter_core::files::{Finder, Named, Place};

use crate::planes::{PlaneId, Planes};

/// The most files one keystroke answers with: more than a palette shows without scrolling, few
/// enough to cross the IPC at every keystroke.
const MOST: usize = 50;

/// Where ⌘P and ⌘⇧F look: for ⌘P the scope follows the window's focus, and Tab widens it; the
/// Search tab (FM-8) offers the four as a choice.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FileScope {
    /// One branch: a piece, or no piece for the repo's own folder.
    Branch {
        plane: PlaneId,
        workspace: String,
        repo: String,
        piece: Option<String>,
    },
    /// Every branch of one workspace of a project, `near` first (FM-8).
    Workspace {
        plane: PlaneId,
        workspace: String,
        near: Option<NearBranch>,
    },
    /// Every branch of one project, `near` first: the branch the window's focus is on, so the
    /// nearest of equal hits leads.
    Project {
        plane: PlaneId,
        near: Option<NearBranch>,
    },
    /// Every branch of every project open in charter: the project in front first, and `near`
    /// first within it.
    OpenProjects {
        front: Option<PlaneId>,
        near: Option<NearBranch>,
    },
}

/// The branch the window's focus is on, by name, in the project a scope puts first. It only
/// orders the scope: a name no branch of the project has moves nothing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize, specta::Type)]
pub struct NearBranch {
    pub workspace: String,
    pub repo: String,
    /// No piece is the repo's own folder.
    pub piece: Option<String>,
}

/// One file ⌘P found: its project, its branch and its path in that branch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FoundFile {
    pub plane: PlaneId,
    pub workspace: String,
    pub repo: String,
    /// No piece is the repo's own folder.
    pub piece: Option<String>,
    pub path: String,
}

/// What one keystroke found.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FilesFound {
    /// Best first, at most fifty.
    pub files: Vec<FoundFile>,
    /// How many branches the scope covers.
    pub branches: u32,
    /// Each branch that could not be listed, in the core's sentence.
    pub refused: Vec<String>,
    /// Each branch with more files than ⌘P lists, in the core's sentence.
    pub partial: Vec<String>,
}

/// Each window's palette session: which one, and the listings it has read.
#[derive(Default)]
pub struct FileFinder {
    windows: Mutex<Windows>,
}

#[derive(Default)]
struct Windows {
    sessions: HashMap<String, Arc<Mutex<Session>>>,
    /// The session each window last ended: a find for it that arrives after the end is
    /// answered from a session nobody keeps, never one that would outlive the palette.
    ended: HashMap<String, u32>,
}

struct Session {
    id: u32,
    finder: Finder,
}

impl FileFinder {
    /// The window's session `id`, a fresh one when the window has moved on to a new palette.
    fn session(&self, window: &str, id: u32) -> Arc<Mutex<Session>> {
        let fresh = || {
            Arc::new(Mutex::new(Session {
                id,
                finder: Finder::default(),
            }))
        };
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        if windows.ended.get(window) == Some(&id) {
            return fresh();
        }
        let session = windows
            .sessions
            .entry(window.to_string())
            .or_insert_with(|| {
                Arc::new(Mutex::new(Session {
                    id,
                    finder: Finder::default(),
                }))
            });
        let stale = session.lock().unwrap_or_else(PoisonError::into_inner).id != id;
        if stale {
            *session = Arc::new(Mutex::new(Session {
                id,
                finder: Finder::default(),
            }));
        }
        Arc::clone(session)
    }

    /// The window's palette session `id` closed: its listings are let go of, and a find for
    /// it still on its way keeps nothing.
    fn end(&self, window: &str, id: u32) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        windows.sessions.remove(window);
        windows.ended.insert(window.to_string(), id);
    }

    /// The window went: everything kept for it is let go of.
    pub fn forget(&self, window: &str) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        windows.sessions.remove(window);
        windows.ended.remove(window);
    }
}

/// The files of the scope whose path matches `query`, best first. `session` is the palette's:
/// within one session each branch is listed once, so typing pays only for the match. An
/// empty query finds nothing and lists the scope, which the window asks as the palette opens.
// The projects are resolved here, against the registry, and the listing and the match run on a
// blocking thread, never the one that draws (SC-2): a cold listing of a large branch takes a
// git call of a few hundred milliseconds. Not a doc comment, because the generated bindings
// carry those.
#[tauri::command]
#[specta::specta]
pub async fn find_files(
    window: tauri::Window,
    planes: tauri::State<'_, Planes>,
    finder: tauri::State<'_, FileFinder>,
    session: u32,
    scope: FileScope,
    query: String,
) -> Result<FilesFound, String> {
    let projects = projects_of(&scope, &planes)?;
    let session = finder.session(window.label(), session);
    tauri::async_runtime::spawn_blocking(move || {
        let mut session = session.lock().unwrap_or_else(PoisonError::into_inner);
        found_in(&mut session.finder, &scope, &projects, &query)
    })
    .await
    .map_err(|err| format!("finding files did not finish: {err}"))
}

/// The palette session `session` closed: this window's listings are let go of.
#[tauri::command]
#[specta::specta]
pub fn find_files_end(window: tauri::Window, finder: tauri::State<'_, FileFinder>, session: u32) {
    finder.end(window.label(), session);
}

/// [`find_files`], once its projects are vouched for.
fn found_in(
    finder: &mut Finder,
    scope: &FileScope,
    projects: &[PlaneId],
    query: &str,
) -> FilesFound {
    let named = branches_in(scope, projects, &mut |root| finder.branches(root).to_vec());
    let roots: Vec<PathBuf> = named.iter().map(|(plane, _)| plane.root().into()).collect();
    let places: Vec<Place<'_>> = named
        .iter()
        .zip(&roots)
        .map(|((_, one), root)| Place {
            plane: root,
            branch: one.branch(),
        })
        .collect();
    let found = finder.find(&places, query, MOST);
    FilesFound {
        files: found
            .hits
            .into_iter()
            .map(|hit| {
                let (plane, one) = &named[hit.at];
                FoundFile {
                    plane: plane.clone(),
                    workspace: one.ws.clone(),
                    repo: one.repo.clone(),
                    piece: one.piece.clone(),
                    path: hit.path,
                }
            })
            .collect(),
        branches: u32::try_from(named.len()).unwrap_or(u32::MAX),
        refused: found.refused.into_iter().map(|(_, why)| why).collect(),
        partial: found.partial.into_iter().map(|(_, said)| said).collect(),
    }
}

/// The projects `scope` looks in, vouched for by the registry: the one it names, which must be
/// open, or every project open now. ⌘P's and ⌘⇧F's one check (ADR 0052).
pub(crate) fn projects_of(scope: &FileScope, planes: &Planes) -> Result<Vec<PlaneId>, String> {
    Ok(match scope {
        FileScope::OpenProjects { .. } => planes.open_now(),
        FileScope::Branch { plane, .. }
        | FileScope::Workspace { plane, .. }
        | FileScope::Project { plane, .. } => {
            planes.held(plane)?;
            vec![plane.clone()]
        }
    })
}

/// Every branch `scope` covers, with its project, nearest first: what ⌘P ranks and what ⌘⇧F
/// walks, in this order. `projects` are the projects the registry vouched for, and
/// `branches_of` answers a project's branches in the explorer's order.
pub(crate) fn branches_in(
    scope: &FileScope,
    projects: &[PlaneId],
    branches_of: &mut dyn FnMut(&Path) -> Vec<Named>,
) -> Vec<(PlaneId, Named)> {
    match scope {
        FileScope::Branch {
            plane,
            workspace,
            repo,
            piece,
        } => vec![(
            plane.clone(),
            Named {
                ws: workspace.clone(),
                repo: repo.clone(),
                piece: piece.clone(),
            },
        )],
        FileScope::Workspace {
            plane,
            workspace,
            near,
        } => nearest_first(branches_of, projects, Some(plane), near)
            .into_iter()
            .filter(|(_, one)| &one.ws == workspace)
            .collect(),
        FileScope::Project { plane, near } => {
            nearest_first(branches_of, projects, Some(plane), near)
        }
        FileScope::OpenProjects { front, near } => {
            nearest_first(branches_of, projects, front.as_ref(), near)
        }
    }
}

/// Every branch of `projects`, nearest first: the `front` project before the others, and in it
/// the `near` branch before its others. Ties in the ranking go to the place nearer the front of
/// the scope, so this is what makes the focused branch's copy of a file beat the clone's.
fn nearest_first(
    branches_of: &mut dyn FnMut(&Path) -> Vec<Named>,
    projects: &[PlaneId],
    front: Option<&PlaneId>,
    near: &Option<NearBranch>,
) -> Vec<(PlaneId, Named)> {
    let mut ordered: Vec<&PlaneId> = projects.iter().collect();
    // Stable, so the others keep the registry's order.
    ordered.sort_by_key(|plane| Some(*plane) != front);
    let mut named = Vec::new();
    for plane in ordered {
        let mut of: Vec<(PlaneId, Named)> = branches_of(plane.root())
            .into_iter()
            .map(|one| (plane.clone(), one))
            .collect();
        if Some(plane) == front
            && let Some(near) = near
        {
            of.sort_by_key(|(_, one)| {
                !(one.ws == near.workspace && one.repo == near.repo && one.piece == near.piece)
            });
        }
        named.extend(of);
    }
    named
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ran = charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "user.email=t@e.invalid", "-c", "user.name=t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args),
        )
        .unwrap();
        assert!(ran.status.success(), "git {args:?}: {ran:?}");
    }

    /// A project with a clone and one piece cut from it, holding `docs/guide.md`.
    fn project() -> (tempfile::TempDir, PlaneId) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let clone = root.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main", "."]);
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "one"]);
        let piece = charter_core::worktree::add(&root, "alpha", "thing", "piece", None)
            .unwrap()
            .path;
        std::fs::create_dir_all(piece.join("docs")).unwrap();
        std::fs::write(piece.join("docs/guide.md"), "\n").unwrap();
        (dir, PlaneId::for_tests(&root))
    }

    #[test]
    fn a_hit_names_its_project_and_branch_whichever_scope_found_it() {
        let (_one, first) = project();
        let (_two, second) = project();
        let mut finder = Finder::default();
        let branch = FileScope::Branch {
            plane: first.clone(),
            workspace: "alpha".into(),
            repo: "thing".into(),
            piece: Some("piece".into()),
        };

        let narrow = found_in(&mut finder, &branch, std::slice::from_ref(&first), "guide");
        let wide = found_in(
            &mut finder,
            &FileScope::OpenProjects {
                front: None,
                near: None,
            },
            &[first.clone(), second.clone()],
            "guide",
        );

        let guide = |plane: &PlaneId| FoundFile {
            plane: plane.clone(),
            workspace: "alpha".into(),
            repo: "thing".into(),
            piece: Some("piece".into()),
            path: "docs/guide.md".into(),
        };
        assert_eq!(narrow.files, [guide(&first)]);
        assert_eq!(narrow.branches, 1);
        assert_eq!(wide.files, [guide(&first), guide(&second)]);
        // Each project's own folder of the repo, and its piece.
        assert_eq!(wide.branches, 4);
        assert!(wide.refused.is_empty(), "{:?}", wide.refused);
    }

    fn piece() -> Option<NearBranch> {
        Some(NearBranch {
            workspace: "alpha".into(),
            repo: "thing".into(),
            piece: Some("piece".into()),
        })
    }

    /// Where each hit for `README.md` is: the clone's own folder has it, and so does the piece.
    fn readmes(found: &FilesFound) -> Vec<(PlaneId, Option<String>)> {
        found
            .files
            .iter()
            .filter(|one| one.path == "README.md")
            .map(|one| (one.plane.clone(), one.piece.clone()))
            .collect()
    }

    #[test]
    fn the_focused_branchs_copy_of_a_file_leads_the_project_scope() {
        let (_dir, plane) = project();
        let mut finder = Finder::default();
        let scope = |near| FileScope::Project {
            plane: plane.clone(),
            near,
        };

        let near = found_in(
            &mut finder,
            &scope(piece()),
            std::slice::from_ref(&plane),
            "readme",
        );
        let none = found_in(
            &mut finder,
            &scope(None),
            std::slice::from_ref(&plane),
            "readme",
        );

        assert_eq!(
            readmes(&near),
            [(plane.clone(), Some("piece".into())), (plane.clone(), None)]
        );
        assert_eq!(
            readmes(&none),
            [(plane.clone(), None), (plane.clone(), Some("piece".into()))],
            "with no focus, the explorer's order"
        );
    }

    #[test]
    fn the_project_in_front_leads_every_open_project_and_its_focused_branch_leads_it() {
        let (_one, first) = project();
        let (_two, second) = project();
        let mut finder = Finder::default();
        let scope = FileScope::OpenProjects {
            front: Some(second.clone()),
            near: piece(),
        };

        let found = found_in(
            &mut finder,
            &scope,
            &[first.clone(), second.clone()],
            "readme",
        );

        assert_eq!(
            readmes(&found),
            [
                (second.clone(), Some("piece".into())),
                (second.clone(), None),
                (first.clone(), None),
                (first.clone(), Some("piece".into())),
            ]
        );
    }

    #[test]
    fn a_find_that_arrives_after_its_session_ended_keeps_nothing() {
        let finder = FileFinder::default();
        let open = finder.session("main", 7);

        finder.end("main", 7);
        let late = finder.session("main", 7);

        assert!(!Arc::ptr_eq(&open, &late));
        assert!(
            finder.windows.lock().unwrap().sessions.is_empty(),
            "the late find kept a session the palette has closed"
        );
        let next = finder.session("main", 8);
        assert!(Arc::ptr_eq(&next, &finder.session("main", 8)));
    }

    #[test]
    fn a_new_session_lists_again_and_the_same_one_does_not() {
        let finder = FileFinder::default();
        let one = finder.session("main", 1);
        let same = finder.session("main", 1);
        let next = finder.session("main", 2);

        assert!(Arc::ptr_eq(&one, &same));
        assert!(!Arc::ptr_eq(&one, &next));
        finder.forget("main");
        assert!(!Arc::ptr_eq(&next, &finder.session("main", 2)));
    }
}
