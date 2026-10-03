//! One memory, read and written from the window's memory tab (SI-9b, ADR 0065): read it, edit
//! it in place, archive it (the window's Delete), put it back (its Undo), and make a new one.
//!
//! **Every command names its store**, as `todos.rs`'s name their workspace: a [`MemoryScope`] is a
//! workspace's journal, a persona's `memory/`, or `personas/_shared/memory/`, and a memory is a
//! scope and a slug. Nothing here resolves "the active one".
//!
//! Thin, as `todos.rs` is. The rules are the core's — `Workspace` and `Persona`'s
//! `open_memory`, `edit_memory`, `archive_memory`, `unarchive_memory` and `remember_titled`,
//! which `charter workspace edit|archive|unarchive` and `charter persona
//! edit-memory|archive-memory|unarchive-memory` call too — and the contract is
//! `docs/plane-format.md` → *Editing and archiving a memory*.

use std::io;
use std::path::{Path, PathBuf};

use charter_core::memstore::{Base, EditRefused};
use charter_core::personas::{Persona, SHARED};
use charter_core::workspaces::{Opened, Plane, Workspace};

use crate::planes::{PlaneId, Planes};

/// Which store a memory is in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum MemoryScope {
    /// A workspace's journal, `workspaces/<name>/memory/`.
    Workspace { name: String },
    /// A persona's own, `personas/<name>/memory/`.
    Persona { name: String },
    /// Every persona's, `personas/_shared/memory/`.
    Shared,
}

impl MemoryScope {
    /// The words the tab's badge says: the workspace's or the persona's name, or `shared`.
    fn word(&self) -> &str {
        match self {
            Self::Workspace { name } | Self::Persona { name } => name,
            Self::Shared => "shared",
        }
    }
}

/// What a memory's tab is keyed by, and what its catalogue rows are named after:
/// `workspace/<ws>/<slug>`, `persona/<name>/<slug>` or `shared/<slug>`.
///
/// **The window's `memories.memoryKey` spells the same thing**, and both are tested against the
/// same literals: a persona's memory row carries `memory.open:<this>`, and the window reads the
/// scope and the slug back out of it.
pub(crate) fn view_key(scope: &MemoryScope, slug: &str) -> String {
    match scope {
        MemoryScope::Workspace { name } => format!("workspace/{name}/{slug}"),
        MemoryScope::Persona { name } => format!("persona/{name}/{slug}"),
        MemoryScope::Shared => format!("shared/{slug}"),
    }
}

/// The slug of a new memory's tab, not written yet: `memories.DRAFT` in the window, which
/// spells the same thing (SI-9d).
///
/// **A slug the core refuses** (`memstore::slug_ok`), so it is not a memory any store can hold:
/// `+`, which it was, is a filename, and a hand-made `+.md` opened as a new memory's editor. A
/// file whose own name the core refuses — `\.md` on a Unix plane — is listed with a row that
/// runs nothing ([`crate::panels`]), so no row carries this key either.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const DRAFT: &str = "\\";

/// One memory, as its tab draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct MemoryView {
    pub scope: MemoryScope,
    /// The file's stem, which an edit never changes.
    pub slug: String,
    /// The `# ` heading.
    pub title: String,
    /// The stamp line's date and time, as written; empty for a file with none.
    pub stamp: String,
    /// The badge's word: the workspace's or the persona's name, or `shared`.
    pub place: String,
    /// Everything under the heading and the stamp: what the tab renders, and what an edit
    /// starts from.
    pub body: String,
    /// Plane-relative, with `/`.
    pub path: String,
    /// The file's whole text as it was read — what a save is checked against (`Base::Read`).
    pub text: String,
}

/// What a save came to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum MemoryEdited {
    /// Written. The memory as it is now, whose `text` the next save is checked against.
    Saved { memory: MemoryView },
    /// **Refused, and nothing was written**: the file changed on disk since the tab read it.
    /// `now` is the memory as it is now, or `null` when it is not there any more; the tab
    /// offers Reload or Overwrite.
    Stale { now: Option<MemoryView> },
}

/// What a Delete did, and what its Undo hands back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct MemoryArchived {
    /// The memory's own slug, which Undo restores it under.
    pub slug: String,
    /// Its name in `archive/` now — its slug, or a numbered one when an earlier memory of
    /// that name was archived before (`memstore::archive_one`).
    pub archived: String,
}

/// A memory store, whichever kind: the two core types that answer the same five questions.
enum Store {
    Workspace(Workspace),
    Persona(Persona),
}

impl Store {
    /// The store `scope` names on the plane at `root`, when it is one the plane has.
    fn of(root: &Path, scope: &MemoryScope) -> Result<Self, String> {
        let plane = Plane::open(root);
        match scope {
            MemoryScope::Workspace { name } => {
                let ws = plane.workspace(name).map_err(|e| e.to_string())?;
                if !ws.dir().is_dir() {
                    return Err(format!("no workspace '{name}'"));
                }
                Ok(Self::Workspace(ws))
            }
            MemoryScope::Persona { name } => {
                let persona = plane.persona(name).map_err(|e| e.to_string())?;
                // The plane's personas, which `_shared` is not one of: its store has a scope of
                // its own.
                let on_plane = plane.personas().map_err(|e| e.to_string())?;
                if !on_plane.iter().any(|one| one == name) {
                    return Err(format!("no persona '{name}'"));
                }
                Ok(Self::Persona(persona))
            }
            MemoryScope::Shared => Ok(Self::Persona(
                plane.persona(SHARED).map_err(|e| e.to_string())?,
            )),
        }
    }

    fn open(&self, slug: &str) -> io::Result<Opened> {
        match self {
            Self::Workspace(ws) => ws.open_memory(slug),
            Self::Persona(p) => p.open_memory(slug),
        }
    }

    fn edit(
        &self,
        slug: &str,
        title: &str,
        text: &str,
        base: Base,
    ) -> Result<PathBuf, EditRefused> {
        match self {
            Self::Workspace(ws) => ws.edit_memory(slug, title, text, base),
            Self::Persona(p) => p.edit_memory(slug, title, text, base),
        }
    }

    fn archive(&self, slug: &str) -> io::Result<PathBuf> {
        match self {
            Self::Workspace(ws) => ws.archive_memory(slug),
            Self::Persona(p) => p.archive_memory(slug),
        }
    }

    fn unarchive(&self, archived: &str, restore_as: Option<&str>) -> io::Result<PathBuf> {
        match self {
            Self::Workspace(ws) => ws.unarchive_memory(archived, restore_as),
            Self::Persona(p) => p.unarchive_memory(archived, restore_as),
        }
    }

    fn remember(
        &self,
        text: &str,
        title: Option<&str>,
        stamp: chrono::NaiveDateTime,
    ) -> io::Result<PathBuf> {
        match self {
            Self::Workspace(ws) => ws.remember_titled(text, title, stamp),
            Self::Persona(p) => p.remember_titled(text, title, stamp),
        }
    }
}

/// The stem of a path the core answered with.
fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// `opened` as its tab draws it.
fn view_of(root: &Path, scope: &MemoryScope, opened: Opened) -> MemoryView {
    let path = opened
        .path
        .strip_prefix(root)
        .unwrap_or(&opened.path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    MemoryView {
        scope: scope.clone(),
        slug: opened.entry.slug,
        title: opened.entry.title,
        stamp: opened.entry.stamp,
        place: scope.word().to_owned(),
        body: opened.entry.body,
        path,
        text: opened.text,
    }
}

/// The memory `slug` in `store`, or `None` when it is not there.
fn read_in(
    root: &Path,
    scope: &MemoryScope,
    store: &Store,
    slug: &str,
) -> Result<Option<MemoryView>, String> {
    match store.open(slug) {
        Ok(opened) => Ok(Some(view_of(root, scope, opened))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Now, as the store stamps a file it writes.
fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// What a memory command says when its blocking thread ended without an answer. Every one of
/// them runs on such a thread: finding a memory by its slug lists its store, and a store holds
/// thousands (SC-2).
const READING: &str = "reading the memory";
const WRITING: &str = "writing the memory";

/// One memory, for its tab. `null` is a memory that is not there any more — a tab put back at a
/// launch can name one archived since — which the tab draws as a view whose source has gone.
#[tauri::command]
#[specta::specta]
pub async fn memory_read(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    scope: MemoryScope,
    slug: String,
) -> Result<Option<MemoryView>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window(READING, move || read(&root, &scope, &slug)).await
}

fn read(root: &Path, scope: &MemoryScope, slug: &str) -> Result<Option<MemoryView>, String> {
    let store = Store::of(root, scope)?;
    read_in(root, scope, &store, slug)
}

/// Save an edit: new title and body, same slug, the stamp kept, the index line retitled.
///
/// `read` is the file's whole text as the tab read it (`MemoryView.text`); a file that differs
/// on disk now is answered as `stale` and nothing is written. `overwrite` is the tab's
/// Overwrite, after a stale answer: it writes whatever the file holds now.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn memory_edit(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    scope: MemoryScope,
    slug: String,
    title: String,
    text: String,
    read: String,
    overwrite: bool,
) -> Result<MemoryEdited, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window(WRITING, move || {
        edit(&root, &scope, &slug, &title, &text, &read, overwrite)
    })
    .await
}

fn edit(
    root: &Path,
    scope: &MemoryScope,
    slug: &str,
    title: &str,
    text: &str,
    read: &str,
    overwrite: bool,
) -> Result<MemoryEdited, String> {
    let store = Store::of(root, scope)?;
    let base = if overwrite {
        Base::Overwrite
    } else {
        Base::Read(read)
    };
    match store.edit(slug, title, text, base) {
        Ok(_) => read_in(root, scope, &store, slug)?
            .map(|memory| MemoryEdited::Saved { memory })
            .ok_or_else(|| format!("'{slug}' was saved and is not there now")),
        Err(EditRefused::Stale) => Ok(MemoryEdited::Stale {
            now: read_in(root, scope, &store, slug)?,
        }),
        Err(EditRefused::Io(e)) => Err(e.to_string()),
    }
}

/// The window's Delete: the memory moves to its store's `archive/` and its index line goes.
/// What it answers is what Undo hands back to `memory_unarchive`.
#[tauri::command]
#[specta::specta]
pub async fn memory_archive(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    scope: MemoryScope,
    slug: String,
) -> Result<MemoryArchived, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window(WRITING, move || archive(&root, &scope, &slug)).await
}

fn archive(root: &Path, scope: &MemoryScope, slug: &str) -> Result<MemoryArchived, String> {
    let store = Store::of(root, scope)?;
    let at = store.archive(slug).map_err(|e| e.to_string())?;
    Ok(MemoryArchived {
        slug: slug.to_owned(),
        archived: stem(&at),
    })
}

/// The window's Undo: the memory named `archived` in `archive/` moves back — under
/// `restore_as` when given, which is how Undo puts back one that archiving had to number — and
/// its index line is appended. The memory as it is back.
#[tauri::command]
#[specta::specta]
pub async fn memory_unarchive(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    scope: MemoryScope,
    archived: String,
    restore_as: Option<String>,
) -> Result<MemoryView, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window(WRITING, move || {
        unarchive(&root, &scope, &archived, restore_as.as_deref())
    })
    .await
}

fn unarchive(
    root: &Path,
    scope: &MemoryScope,
    archived: &str,
    restore_as: Option<&str>,
) -> Result<MemoryView, String> {
    let store = Store::of(root, scope)?;
    let back = store
        .unarchive(archived, restore_as)
        .map_err(|e| e.to_string())?;
    let slug = stem(&back);
    read_in(root, scope, &store, &slug)?.ok_or_else(|| format!("'{slug}' is not back"))
}

/// A new memory, through the store's own remember: the file `charter … remember --title`
/// writes. An empty `title` is the text's first line.
#[tauri::command]
#[specta::specta]
pub async fn memory_create(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    scope: MemoryScope,
    title: String,
    text: String,
) -> Result<MemoryView, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window(WRITING, move || create(&root, &scope, &title, &text, now())).await
}

fn create(
    root: &Path,
    scope: &MemoryScope,
    title: &str,
    text: &str,
    stamp: chrono::NaiveDateTime,
) -> Result<MemoryView, String> {
    if title.contains(['\n', '\r']) {
        // The edit's rule, so the two writes of a title refuse the same thing.
        return Err("a memory's title is one line".to_owned());
    }
    let store = Store::of(root, scope)?;
    let title = title.trim();
    let path = store
        .remember(text, (!title.is_empty()).then_some(title), stamp)
        .map_err(|e| e.to_string())?;
    let slug = stem(&path);
    read_in(root, scope, &store, &slug)?
        .ok_or_else(|| format!("'{slug}' was written and is not there"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> chrono::NaiveDateTime {
        "2026-05-04T11:32:17".parse().unwrap()
    }

    /// A plane with the workspace `alpha`, the persona `steward` and the shared store.
    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
        std::fs::create_dir_all(root.join("personas/steward/memory")).unwrap();
        std::fs::write(
            root.join("personas/steward/persona.md"),
            "---\nrole: Steward\n---\n# Steward\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("personas/_shared/memory")).unwrap();
        dir
    }

    fn steward() -> MemoryScope {
        MemoryScope::Persona {
            name: "steward".into(),
        }
    }

    fn alpha() -> MemoryScope {
        MemoryScope::Workspace {
            name: "alpha".into(),
        }
    }

    #[test]
    fn a_memory_is_keyed_by_its_store_and_its_slug() {
        // The literals `memories.test.ts` spells too: one format, two spellers.
        assert_eq!(view_key(&alpha(), "x"), "workspace/alpha/x");
        assert_eq!(view_key(&steward(), "x"), "persona/steward/x");
        assert_eq!(view_key(&MemoryScope::Shared, "x"), "shared/x");
    }

    #[test]
    fn a_new_memorys_tab_is_keyed_by_a_slug_no_memory_can_have() {
        // The literal `memories.test.ts` spells too.
        assert_eq!(view_key(&MemoryScope::Shared, DRAFT), "shared/\\");
        let dir = plane();
        // Refused as a slug, not merely absent: no file the store holds can be it.
        assert!(read(dir.path(), &MemoryScope::Shared, DRAFT).is_err());
    }

    #[test]
    fn a_memory_made_in_the_window_is_the_file_remember_writes_and_reads_back() {
        let dir = plane();

        let made = create(
            dir.path(),
            &steward(),
            "Where prod-1 is",
            "It is in eu-west-1.",
            at(),
        )
        .unwrap();

        assert_eq!(made.slug, "where-prod-1-is");
        assert_eq!(made.title, "Where prod-1 is");
        assert_eq!(made.stamp, "2026-05-04 11:32");
        assert_eq!(made.place, "steward");
        assert_eq!(made.body, "It is in eu-west-1.");
        assert_eq!(made.path, "personas/steward/memory/where-prod-1-is.md");
        assert_eq!(
            made.text,
            "# Where prod-1 is\n\n_2026-05-04 11:32 · persistent_\n\nIt is in eu-west-1.\n"
        );
        assert_eq!(
            read(dir.path(), &steward(), "where-prod-1-is").unwrap(),
            Some(made)
        );
    }

    #[test]
    fn each_scope_writes_to_its_own_store_and_to_no_other() {
        let dir = plane();

        let journal = create(dir.path(), &alpha(), "", "The API is slow on Mondays", at()).unwrap();
        let shared = create(
            dir.path(),
            &MemoryScope::Shared,
            "Freeze",
            "No deploys on Friday",
            at(),
        )
        .unwrap();

        assert!(
            journal
                .path
                .starts_with("workspaces/alpha/memory/20260504-113217-"),
            "{}",
            journal.path
        );
        assert_eq!(journal.title, "The API is slow on Mondays");
        assert_eq!(journal.place, "alpha");
        assert_eq!(shared.path, "personas/_shared/memory/freeze.md");
        assert_eq!(shared.place, "shared");
        assert_eq!(read(dir.path(), &steward(), "freeze").unwrap(), None);
    }

    #[test]
    fn a_memory_that_is_not_there_reads_as_none_and_a_store_that_is_not_there_is_refused() {
        let dir = plane();

        assert_eq!(read(dir.path(), &steward(), "never-written").unwrap(), None);
        let refused = read(
            dir.path(),
            &MemoryScope::Persona {
                name: "ghost".into(),
            },
            "x",
        )
        .unwrap_err();
        assert!(refused.contains("ghost"), "{refused}");
        let refused = read(
            dir.path(),
            &MemoryScope::Workspace {
                name: "gamma".into(),
            },
            "x",
        )
        .unwrap_err();
        assert!(refused.contains("gamma"), "{refused}");
    }

    #[test]
    fn the_shared_store_is_not_reached_as_a_persona_named_shared() {
        let dir = plane();
        create(
            dir.path(),
            &MemoryScope::Shared,
            "Freeze",
            "No deploys on Friday",
            at(),
        )
        .unwrap();

        let refused = read(
            dir.path(),
            &MemoryScope::Persona {
                name: "_shared".into(),
            },
            "freeze",
        )
        .unwrap_err();

        assert!(refused.contains("_shared"), "{refused}");
    }

    #[test]
    fn a_save_against_what_the_tab_read_rewrites_it_in_place() {
        let dir = plane();
        let made = create(dir.path(), &steward(), "Where prod-1 is", "eu-west-1", at()).unwrap();

        let saved = edit(
            dir.path(),
            &steward(),
            &made.slug,
            "Where prod-1 lives",
            "eu-west-2 since May",
            &made.text,
            false,
        )
        .unwrap();

        let MemoryEdited::Saved { memory } = saved else {
            panic!("refused: {saved:?}");
        };
        assert_eq!(memory.slug, "where-prod-1-is", "the slug never changes");
        assert_eq!(memory.title, "Where prod-1 lives");
        assert_eq!(
            memory.stamp, made.stamp,
            "an edit does not re-date a memory"
        );
        assert_eq!(memory.body, "eu-west-2 since May");
        let index =
            std::fs::read_to_string(dir.path().join("personas/steward/memory/MEMORY.md")).unwrap();
        assert!(
            index.contains("- [Where prod-1 lives](where-prod-1-is.md)"),
            "{index}"
        );
    }

    #[test]
    fn a_save_over_a_change_the_tab_did_not_see_is_stale_and_writes_nothing() {
        let dir = plane();
        let made = create(dir.path(), &steward(), "Where prod-1 is", "eu-west-1", at()).unwrap();
        let file = dir.path().join(&made.path);
        let theirs = made
            .text
            .replace("eu-west-1", "us-east-1 (changed by a chat)");
        std::fs::write(&file, &theirs).unwrap();

        let refused = edit(
            dir.path(),
            &steward(),
            &made.slug,
            "Mine",
            "mine",
            &made.text,
            false,
        )
        .unwrap();

        let MemoryEdited::Stale { now: Some(now) } = refused else {
            panic!("not stale: {refused:?}");
        };
        assert_eq!(
            now.text, theirs,
            "the tab is handed what is there now, to reload"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
    }

    #[test]
    fn an_overwrite_after_a_stale_answer_writes_over_what_is_there() {
        let dir = plane();
        let made = create(dir.path(), &steward(), "Where prod-1 is", "eu-west-1", at()).unwrap();
        std::fs::write(
            dir.path().join(&made.path),
            made.text.replace("eu-west-1", "theirs"),
        )
        .unwrap();

        let saved = edit(dir.path(), &steward(), &made.slug, "Mine", "mine", "", true).unwrap();

        let MemoryEdited::Saved { memory } = saved else {
            panic!("refused: {saved:?}");
        };
        assert_eq!(memory.body, "mine");
    }

    #[test]
    fn a_save_of_a_memory_archived_meanwhile_is_stale_with_nothing_to_reload() {
        let dir = plane();
        let made = create(dir.path(), &steward(), "Gone soon", "text", at()).unwrap();
        archive(dir.path(), &steward(), &made.slug).unwrap();

        let refused = edit(
            dir.path(),
            &steward(),
            &made.slug,
            "T",
            "b",
            &made.text,
            false,
        );

        // Either answer tells the tab the truth; what it must never be is a write.
        assert!(
            matches!(refused, Ok(MemoryEdited::Stale { now: None }) | Err(_)),
            "{refused:?}"
        );
        assert_eq!(read(dir.path(), &steward(), &made.slug).unwrap(), None);
    }

    #[test]
    fn a_delete_archives_it_and_undo_puts_it_back_under_its_own_slug() {
        let dir = plane();
        let made = create(dir.path(), &alpha(), "", "Deploys freeze on Fridays", at()).unwrap();

        let gone = archive(dir.path(), &alpha(), &made.slug).unwrap();

        assert_eq!(gone.slug, made.slug);
        assert_eq!(read(dir.path(), &alpha(), &made.slug).unwrap(), None);
        assert!(
            dir.path()
                .join("workspaces/alpha/memory/archive")
                .join(format!("{}.md", gone.archived))
                .is_file()
        );

        let back = unarchive(dir.path(), &alpha(), &gone.archived, Some(&gone.slug)).unwrap();

        assert_eq!(back.slug, made.slug);
        assert_eq!(back.text, made.text);
        let index =
            std::fs::read_to_string(dir.path().join("workspaces/alpha/memory/MEMORY.md")).unwrap();
        assert!(index.contains(&format!("({}.md)", made.slug)), "{index}");
    }

    #[test]
    fn undo_of_a_memory_archiving_had_to_number_restores_its_own_slug() {
        let dir = plane();
        // An earlier memory of the same name is already in the archive.
        let first = create(dir.path(), &MemoryScope::Shared, "Freeze", "first", at()).unwrap();
        archive(dir.path(), &MemoryScope::Shared, &first.slug).unwrap();
        let second = create(dir.path(), &MemoryScope::Shared, "Freeze", "second", at()).unwrap();

        let gone = archive(dir.path(), &MemoryScope::Shared, &second.slug).unwrap();
        assert_ne!(gone.archived, gone.slug, "numbered in the archive");

        let back = unarchive(
            dir.path(),
            &MemoryScope::Shared,
            &gone.archived,
            Some(&gone.slug),
        )
        .unwrap();

        assert_eq!(back.slug, "freeze");
        assert_eq!(back.body, "second");
    }

    #[test]
    fn a_title_that_breaks_a_line_is_refused_and_nothing_is_written() {
        let dir = plane();

        assert!(create(dir.path(), &steward(), "one\ntwo", "text", at()).is_err());

        let listed = std::fs::read_dir(dir.path().join("personas/steward/memory"))
            .unwrap()
            .count();
        assert_eq!(listed, 0);
    }

    /// `deploy` and `prod-deploy` in the shared store, and `deploy` archived by something else —
    /// a chat, or another window — while its tab stays open (SI-9d).
    fn deploy_archived_beside_prod_deploy(dir: &tempfile::TempDir) -> (MemoryView, String) {
        let deploy = create(dir.path(), &MemoryScope::Shared, "Deploy", "ours", at()).unwrap();
        let prod = create(
            dir.path(),
            &MemoryScope::Shared,
            "Prod deploy",
            "theirs",
            at(),
        )
        .unwrap();
        assert_eq!(prod.slug, "prod-deploy");
        archive(dir.path(), &MemoryScope::Shared, &deploy.slug).unwrap();
        (deploy, prod.text)
    }

    #[test]
    fn a_tab_whose_memory_was_archived_reads_as_gone_and_not_as_the_memory_its_name_ends_with() {
        let dir = plane();
        let (deploy, _) = deploy_archived_beside_prod_deploy(&dir);

        assert_eq!(
            read(dir.path(), &MemoryScope::Shared, &deploy.slug).unwrap(),
            None
        );
    }

    #[test]
    fn a_save_in_a_tab_whose_memory_was_archived_writes_no_other_memory() {
        let dir = plane();
        let (deploy, prod) = deploy_archived_beside_prod_deploy(&dir);
        let prod_file = dir.path().join("personas/_shared/memory/prod-deploy.md");

        let saved = edit(
            dir.path(),
            &MemoryScope::Shared,
            &deploy.slug,
            "Deploy v2",
            "mine",
            &deploy.text,
            false,
        );
        let overwritten = edit(
            dir.path(),
            &MemoryScope::Shared,
            &deploy.slug,
            "Deploy v2",
            "mine",
            "",
            true,
        );

        assert!(
            !matches!(saved, Ok(MemoryEdited::Saved { .. })),
            "{saved:?}"
        );
        assert!(overwritten.is_err(), "{overwritten:?}");
        assert_eq!(std::fs::read_to_string(prod_file).unwrap(), prod);
    }

    #[test]
    fn delete_and_undo_in_a_tab_whose_memory_was_archived_move_no_other_memory() {
        let dir = plane();
        let (deploy, prod) = deploy_archived_beside_prod_deploy(&dir);
        let store = dir.path().join("personas/_shared/memory");

        let gone = archive(dir.path(), &MemoryScope::Shared, &deploy.slug).unwrap();
        assert_eq!(
            gone.archived, "deploy",
            "already archived, under its own name"
        );
        let back = unarchive(
            dir.path(),
            &MemoryScope::Shared,
            &gone.archived,
            Some(&gone.slug),
        )
        .unwrap();

        assert_eq!(back.slug, "deploy");
        assert_eq!(back.body, "ours");
        assert_eq!(
            std::fs::read_to_string(store.join("prod-deploy.md")).unwrap(),
            prod
        );
    }

    #[test]
    fn the_index_is_not_a_memory_the_window_can_open_or_delete() {
        let dir = plane();
        create(dir.path(), &steward(), "A", "a", at()).unwrap();
        let index = dir.path().join("personas/steward/memory/MEMORY.md");
        let before = std::fs::read_to_string(&index).unwrap();

        assert!(read(dir.path(), &steward(), "MEMORY").is_err());
        assert!(archive(dir.path(), &steward(), "MEMORY").is_err());
        assert!(edit(dir.path(), &steward(), "MEMORY", "t", "b", "", true).is_err());
        assert_eq!(std::fs::read_to_string(&index).unwrap(), before);
    }

    #[test]
    fn a_slug_that_walks_out_of_the_store_is_refused() {
        let dir = plane();
        let charter = dir.path().join("personas/steward/persona.md");

        assert!(read(dir.path(), &steward(), "../persona").is_err());
        assert!(archive(dir.path(), &steward(), "../persona").is_err());
        assert!(edit(dir.path(), &steward(), "../persona", "t", "b", "", true).is_err());
        assert!(charter.is_file());
    }

    /// Every file under `dir` with its bytes, and every link with its target.
    #[cfg(unix)]
    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let kind = std::fs::symlink_metadata(&path).unwrap().file_type();
            if kind.is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                out.push((path, target.to_string_lossy().as_bytes().to_vec()));
            } else if kind.is_dir() {
                out.push((path.clone(), Vec::new()));
                out.extend(snapshot(&path));
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
        out.sort();
        out
    }

    #[cfg(unix)]
    #[test]
    fn a_journal_a_chat_linked_to_a_persona_is_refused_by_every_memory_command() {
        // V74 (#1064): the window runs outside every chat's sandbox, so a journal a chat swapped
        // for a link to a persona's memory must not carry the window's writes, or its reads,
        // there — although the link stays inside the plane.
        let dir = plane();
        let theirs = dir.path().join("personas/steward/memory");
        std::fs::create_dir_all(theirs.join("archive")).unwrap();
        std::fs::write(
            theirs.join("MEMORY.md"),
            "# Theirs\n\n- [Theirs](theirs.md)\n",
        )
        .unwrap();
        std::fs::write(
            theirs.join("theirs.md"),
            "# Theirs\n\n_2026-10-01 09:00 · persistent_\n\nTheirs.\n",
        )
        .unwrap();
        std::fs::write(theirs.join("archive/gone.md"), "# Gone\n\nGone.\n").unwrap();
        std::os::unix::fs::symlink(
            "../../personas/steward/memory",
            dir.path().join("workspaces/alpha/memory"),
        )
        .unwrap();
        let before = snapshot(dir.path());

        assert!(create(dir.path(), &alpha(), "", "Planted through a link", at()).is_err());
        assert!(edit(dir.path(), &alpha(), "theirs", "Mine", "Mine.", "", true).is_err());
        assert!(archive(dir.path(), &alpha(), "theirs").is_err());
        assert!(unarchive(dir.path(), &alpha(), "gone", None).is_err());
        assert!(!matches!(read(dir.path(), &alpha(), "theirs"), Ok(Some(_))));

        assert_eq!(snapshot(dir.path()), before, "something was written");
    }
}
