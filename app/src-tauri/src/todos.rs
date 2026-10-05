//! A workspace's todos, written from the window's Todos panel (SI-3): record one, close one as
//! done, forget one — and read one, for its view tab (#1214).
//!
//! **Every command names its workspace.** The panel is about the focused workspace and says
//! which, and what it sends is that name — never "the active one", which is a terminal's idea
//! (`charter ws todo` resolves it from a session lock and the environment) and would let a
//! todo typed in one workspace's panel land in another.
//!
//! Thin, as `workspaces.rs` is. The rules are `charter_core::workspaces::Workspace`'s, which is
//! what `charter ws todo` calls too: a todo with no words or about work already on the list is
//! refused ([`Workspace::record_todo`]), a close writes the journal first and then deletes
//! ([`Workspace::close_todo`]), and a forget journals nothing and refuses a slug that is not one
//! path segment ([`Workspace::forget_todo`]).

use std::path::Path;

use charter_core::workspaces::{Plane, Workspace};

use crate::planes::{PlaneId, Planes};

/// The workspace `name` on the plane at `root`, when it is one that exists.
fn workspace(root: &Path, name: &str) -> Result<Workspace, String> {
    let ws = Plane::open(root)
        .workspace(name)
        .map_err(|e| e.to_string())?;
    if !ws.dir().is_dir() {
        return Err(format!("no workspace '{name}'"));
    }
    Ok(ws)
}

/// The title of the open todo `slug`, or the slug when charter cannot read one.
fn title_of(ws: &Workspace, slug: &str) -> String {
    ws.todos()
        .ok()
        .and_then(|open| open.into_iter().find(|todo| todo.slug == slug))
        .map_or_else(|| slug.to_owned(), |todo| todo.title)
}

/// What a todo command says when its blocking thread ended without an answer. Each one runs on
/// such a thread: it takes the store's lock, and then the plane model's, which a change nobody
/// could name holds while it reads every workspace again (SC-2, FD-10c).
const WRITING: &str = "writing the todo";

/// Now, as the store stamps a file it writes.
fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// One open todo, for its view tab (#1214): the workspace it is in, its slug, and what its file
/// says — the title, the stamp it was written with, and the text under it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct TodoView {
    pub workspace: String,
    pub slug: String,
    pub title: String,
    /// When it was opened: the date and time its stamp line records, as written.
    pub stamp: String,
    /// Everything under the stamp line, trimmed. Empty for a todo that is only a title.
    pub body: String,
}

/// One open todo of `workspace`, for its tab. `null` is a todo that is not open any more —
/// closed as done or forgotten since its tab opened, here or in a terminal — which the tab
/// draws as a view whose source has gone, not as a failure.
#[tauri::command]
#[specta::specta]
pub async fn todo_read(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    slug: String,
) -> Result<Option<TodoView>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window("reading the todo", move || {
        read_in(&root, &workspace, &slug)
    })
    .await
}

fn read_in(root: &Path, name: &str, slug: &str) -> Result<Option<TodoView>, String> {
    let ws = workspace(root, name)?;
    let open = ws.todos().map_err(|e| e.to_string())?;
    Ok(open
        .into_iter()
        .find(|todo| todo.slug == slug)
        .map(|todo| TodoView {
            workspace: name.to_owned(),
            slug: todo.slug,
            title: todo.title,
            stamp: todo.stamp,
            body: todo.body,
        }))
}

/// Record a todo in `workspace`, and answer with what was said.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127). Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn todo_add(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    text: String,
) -> Result<String, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window(WRITING, move || {
        let said = add_in(held.root(), &workspace, &text, now());
        // Told whether or not it was written, as a persona's write is: a refusal moved
        // nothing, and the store is read once more.
        held.wrote(&[format!("workspaces/{workspace}/todos")]);
        said
    })
    .await
}

fn add_in(
    root: &Path,
    name: &str,
    text: &str,
    stamp: chrono::NaiveDateTime,
) -> Result<String, String> {
    let ws = workspace(root, name)?;
    ws.record_todo(text.trim(), stamp)
        .map_err(|refused| refused.to_string())?;
    Ok(format!("Todo recorded in '{name}'."))
}

/// Close a todo as done: the journal records it, then the todo goes.
#[tauri::command]
#[specta::specta]
pub async fn todo_done(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    slug: String,
) -> Result<String, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window(WRITING, move || {
        let said = done_in(held.root(), &workspace, &slug, now());
        // The journal records it, then the todo goes.
        held.wrote(&[
            format!("workspaces/{workspace}/memory"),
            format!("workspaces/{workspace}/todos"),
        ]);
        said
    })
    .await
}

fn done_in(
    root: &Path,
    name: &str,
    slug: &str,
    stamp: chrono::NaiveDateTime,
) -> Result<String, String> {
    let ws = workspace(root, name)?;
    let title = title_of(&ws, slug);
    ws.close_todo(slug, stamp).map_err(|e| e.to_string())?;
    Ok(format!(
        "Closed '{}' in '{name}' — the journal has the trace.",
        charter_core::personas::one_line(&title)
    ))
}

/// Forget a todo: it goes, and nothing is journalled.
#[tauri::command]
#[specta::specta]
pub async fn todo_forget(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    slug: String,
) -> Result<String, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window(WRITING, move || {
        let said = forget_in(held.root(), &workspace, &slug);
        held.wrote(&[format!("workspaces/{workspace}/todos")]);
        said
    })
    .await
}

fn forget_in(root: &Path, name: &str, slug: &str) -> Result<String, String> {
    let ws = workspace(root, name)?;
    let title = title_of(&ws, slug);
    ws.forget_todo(slug).map_err(|e| e.to_string())?;
    Ok(format!(
        "Dropped '{}' from '{name}' — abandoned, so nothing was journalled.",
        charter_core::personas::one_line(&title)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> chrono::NaiveDateTime {
        "2026-05-04T11:32:17".parse().unwrap()
    }

    /// A plane with the workspaces `alpha` and `beta`.
    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
        for ws in ["alpha", "beta"] {
            std::fs::create_dir_all(dir.path().join("workspaces").join(ws)).unwrap();
        }
        dir
    }

    fn open(root: &Path, name: &str) -> Vec<(String, String)> {
        workspace(root, name)
            .unwrap()
            .todos()
            .unwrap()
            .into_iter()
            .map(|todo| (todo.slug, todo.title))
            .collect()
    }

    #[test]
    fn a_todo_lands_in_the_workspace_named_and_in_no_other() {
        let dir = plane();

        add_in(dir.path(), "beta", "Cut the 0.63 release", at()).unwrap();

        let beta = open(dir.path(), "beta");
        assert_eq!(beta.len(), 1, "{beta:?}");
        assert_eq!(beta[0].1, "Cut the 0.63 release");
        assert!(open(dir.path(), "alpha").is_empty());
    }

    #[test]
    fn a_todo_already_on_the_list_is_refused_in_the_cores_words() {
        let dir = plane();
        add_in(dir.path(), "alpha", "Review the rollout plan", at()).unwrap();

        let refused = add_in(dir.path(), "alpha", "review the rollout plan", at()).unwrap_err();

        assert_eq!(refused, "already on the list: Review the rollout plan");
        assert_eq!(open(dir.path(), "alpha").len(), 1);
    }

    #[test]
    fn a_workspace_that_is_not_there_gets_no_todo() {
        let dir = plane();
        let refused = add_in(dir.path(), "gamma", "Anything at all", at()).unwrap_err();
        assert!(refused.contains("gamma"), "{refused}");
        assert!(!dir.path().join("workspaces/gamma").exists());
    }

    #[test]
    fn a_todo_closed_as_done_is_gone_and_the_journal_names_it() {
        let dir = plane();
        add_in(dir.path(), "alpha", "Drop the old importer", at()).unwrap();
        let slug = open(dir.path(), "alpha")[0].0.clone();

        let said = done_in(dir.path(), "alpha", &slug, at()).unwrap();

        assert!(said.contains("Drop the old importer"), "{said}");
        assert!(open(dir.path(), "alpha").is_empty());
        let journal = workspace(dir.path(), "alpha").unwrap().memories().unwrap();
        assert_eq!(journal.len(), 1, "{journal:?}");
        assert!(journal[0].title.contains("Drop the old importer"));
    }

    #[test]
    fn a_forgotten_todo_is_gone_and_nothing_is_journalled() {
        let dir = plane();
        add_in(dir.path(), "alpha", "Drop the old importer", at()).unwrap();
        let slug = open(dir.path(), "alpha")[0].0.clone();

        forget_in(dir.path(), "alpha", &slug).unwrap();

        assert!(open(dir.path(), "alpha").is_empty());
        let journal = workspace(dir.path(), "alpha").unwrap().memories().unwrap();
        assert!(journal.is_empty(), "{journal:?}");
    }

    #[test]
    fn a_todo_is_read_whole_for_its_tab_and_a_closed_one_is_none() {
        let dir = plane();
        add_in(dir.path(), "alpha", "Drop the old importer", at()).unwrap();
        let slug = open(dir.path(), "alpha")[0].0.clone();

        let view = read_in(dir.path(), "alpha", &slug)
            .unwrap()
            .expect("an open todo");
        assert_eq!(view.workspace, "alpha");
        assert_eq!(view.slug, slug);
        assert_eq!(view.title, "Drop the old importer");
        assert_eq!(view.stamp, "2026-05-04 11:32");

        // The same slug in another workspace is not this todo.
        assert_eq!(read_in(dir.path(), "beta", &slug).unwrap(), None);

        done_in(dir.path(), "alpha", &slug, at()).unwrap();
        assert_eq!(read_in(dir.path(), "alpha", &slug).unwrap(), None);
        assert!(read_in(dir.path(), "gamma", &slug).is_err());
    }

    #[test]
    fn a_slug_that_walks_out_of_the_todo_store_is_refused() {
        let dir = plane();
        let charter = dir.path().join("workspaces/beta/workspace.md");
        std::fs::write(&charter, "# beta\n").unwrap();

        assert!(forget_in(dir.path(), "alpha", "../../beta/workspace").is_err());
        assert!(done_in(dir.path(), "alpha", "../../beta/workspace", at()).is_err());
        assert!(charter.exists());
    }
}
