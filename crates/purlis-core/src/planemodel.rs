//! What the sidebar and the workspace panels draw, held in memory and kept current by what
//! changed (FD-10b, FD-10c).
//!
//! The sidebar is every workspace with its open todos, its vision and colour and whether it
//! is LIVE, and the plane's personas with the default one. It used to be read from the disk
//! whole on every ask: every workspace's `todos/` listed and every todo opened, for a change
//! that was a todo closed in one of them. Here it is read once, and each change the watcher
//! names ([`crate::planechange`]) re-reads only what that change is part of: a todo of `beta`
//! re-reads `beta/todos/` and nothing else.
//!
//! **The workspace panels, per section** ([`Sections`], FD-10c). A workspace's clones, todos,
//! memories and session records are read the first time its panels are asked for, and from
//! then on a change re-reads the one section it is part of: a memory an agent saves re-reads
//! that workspace's `memory/`, not its todos, its records or its clones. The personas' memory
//! counts the Personas panel draws are held the same way, one persona at a time.
//!
//! **In memory only.** Nothing here is written anywhere: the model is a reading of the plane,
//! made again from the disk when the app holds the plane, and dropped with it. It is not a
//! store, so it has no tier (ADR 0069). If it is ever persisted, it gets a tier line in
//! `docs/plane-format.md` as a rebuildable cache.
//!
//! **Not knowing rebuilds.** `None` for what changed — a batch the watcher could not place —
//! reads the whole model again, and what that gives equals a model read fresh ([`Model::read`]):
//! the model is never a second answer to "what is on disk" that nothing can correct.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::active::Place;
use crate::extension::project::theme;
use crate::planechange::{Change, Kind};
use crate::repos::{self, Clones};
use crate::sessionrecord::{self, Listed};
use crate::workspaces::{Entry, Plane, Workspace};

/// One workspace as the sidebar draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    /// Its directory.
    pub path: PathBuf,
    /// The `## Vision` body, or `""`.
    pub vision: String,
    /// Its open todos' titles, in the store's order.
    pub todos: Vec<String>,
    /// Its colour as `workspace.json` holds it, or `None`.
    pub colour: Option<String>,
}

/// What one workspace's panels are drawn from, section by section (FD-10c): everything
/// `workspace_panels` reads of the workspace itself. The personas and the default one are the
/// plane's, and the model holds them once for every workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sections {
    /// The clones on disk, and what charter would not look at. A `Workspace` change.
    pub clones: Clones,
    /// The repos `workspace.json` names. A `Workspace` change.
    pub declared: Vec<String>,
    /// Its open todos, or why the store could not be read. A `Todos` change.
    pub todos: Result<Vec<Entry>, String>,
    /// Its journal, oldest first as the store lists it, or why it could not be read. A
    /// `Memory` change.
    pub memories: Result<Vec<Entry>, String>,
    /// Its session records, newest first. A `Sessions` change.
    pub sessions: Vec<Listed>,
}

impl Sections {
    /// The sections of workspace `name` of the plane at `root`, read whole from the disk — or
    /// why the workspace could not be read at all.
    pub fn read(root: &Path, name: &str) -> Result<Self, String> {
        let plane = Plane::open(root);
        let clones = repos::clones(root, name).map_err(|why| why.to_string())?;
        let workspace = plane.workspace(name).map_err(|why| why.to_string())?;
        Ok(Self {
            clones,
            declared: repos::declared(&plane, name),
            todos: workspace.todos().map_err(|why| why.to_string()),
            memories: workspace.memories().map_err(|why| why.to_string()),
            sessions: sessions_of(root, name),
        })
    }
}

/// A workspace's session records, newest first.
fn sessions_of(root: &Path, name: &str) -> Vec<Listed> {
    sessionrecord::list(root, &Place::Workspace(name.to_owned()))
}

/// The plane's reading for the sidebar and the workspace panels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    rows: BTreeMap<String, Row>,
    /// Why `workspaces/` could not be listed, where it could not.
    rows_unread: Option<String>,
    personas: Result<Vec<String>, String>,
    default_persona: Option<String>,
    live: BTreeSet<String>,
    /// The sections of each workspace whose panels have been asked for, by name. Read on the
    /// first ask, not with the model: a plane of dozens of workspaces is drawn one at a time.
    sections: BTreeMap<String, Sections>,
    /// Each persona's memory count (`_shared` included) that has been asked for.
    memory_counts: BTreeMap<String, usize>,
}

impl Model {
    /// The model of the plane at `root`, read whole from the disk.
    pub fn read(root: &Path) -> Self {
        let plane = Plane::open(root);
        let mut model = Self {
            rows: BTreeMap::new(),
            rows_unread: None,
            personas: Ok(Vec::new()),
            default_persona: None,
            live: BTreeSet::new(),
            sections: BTreeMap::new(),
            memory_counts: BTreeMap::new(),
        };
        model.read_project(&plane);
        model.read_personas(&plane);
        model.read_names(&plane);
        model
    }

    /// Brings the model up to date with `changes` in the plane at `root`, reading only what
    /// they are part of — or all of it again for `None`, "anything may have moved".
    pub fn apply(&mut self, root: &Path, changes: Option<&[Change]>) {
        let Some(changes) = changes else {
            *self = Self::read(root);
            return;
        };
        // The plane's own files hold the default persona and which workspaces are LIVE, and
        // the root itself moving may be `workspaces/` or `personas/` arriving: read again.
        if changes.iter().any(|change| change.kind == Kind::Project) {
            *self = Self::read(root);
            return;
        }
        let plane = Plane::open(root);
        if changes.iter().any(|change| change.kind == Kind::Persona) {
            self.read_personas(&plane);
        }
        let workspace_moved: BTreeSet<&str> = changes
            .iter()
            .filter(|change| change.kind == Kind::Workspace)
            .filter_map(|change| change.workspace.as_deref())
            .collect();
        // A workspace made, removed or renamed is `workspaces/` moving, and so is a todo of a
        // workspace this has not met: the names are listed again, which reads only the ones
        // that are new.
        let names_moved = changes.iter().any(|change| {
            change.kind == Kind::Workspace
                || (change.kind == Kind::Todos
                    && change
                        .workspace
                        .as_deref()
                        .is_some_and(|name| !self.rows.contains_key(name)))
        });
        let met = if names_moved {
            self.read_names(&plane)
        } else {
            BTreeSet::new()
        };
        // A workspace just met was read whole by the listing.
        for name in workspace_moved.iter().filter(|name| !met.contains(**name)) {
            if self.rows.contains_key(*name)
                && let Some(row) = row_of(&plane, name)
            {
                self.rows.insert((*name).to_owned(), row);
            }
        }
        let todos_moved: BTreeSet<&str> = changes
            .iter()
            .filter(|change| change.kind == Kind::Todos)
            .filter_map(|change| change.workspace.as_deref())
            .filter(|name| !workspace_moved.contains(name))
            .collect();
        for name in todos_moved {
            if let (Some(row), Ok(workspace)) = (self.rows.get_mut(name), plane.workspace(name)) {
                row.todos = todos_of(&workspace);
            }
        }
        self.apply_to_sections(root, &plane, changes);
        self.apply_to_counts(root, changes);
    }

    /// Brings each held workspace's sections up to date with `changes`, re-reading only the
    /// section each one is part of.
    fn apply_to_sections(&mut self, root: &Path, plane: &Plane, changes: &[Change]) {
        for change in changes {
            let Some(name) = change.workspace.as_deref() else {
                // `workspaces/` itself moved: any of them may be gone or another one.
                if change.kind == Kind::Workspace {
                    self.sections.clear();
                }
                continue;
            };
            let Some(held) = self.sections.get_mut(name) else {
                // Read whole when its panels are first asked for.
                continue;
            };
            match change.kind {
                // The workspace's directory itself made, removed or renamed: everything in it
                // may have moved, so it is read again on the next ask.
                Kind::Workspace if change.path == format!("workspaces/{name}") => {
                    self.sections.remove(name);
                }
                Kind::Workspace => match repos::clones(root, name) {
                    Ok(clones) => {
                        held.clones = clones;
                        held.declared = repos::declared(plane, name);
                    }
                    Err(_) => {
                        self.sections.remove(name);
                    }
                },
                Kind::Todos | Kind::Memory => match plane.workspace(name) {
                    Ok(workspace) if change.kind == Kind::Todos => {
                        held.todos = workspace.todos().map_err(|why| why.to_string());
                    }
                    Ok(workspace) => {
                        held.memories = workspace.memories().map_err(|why| why.to_string());
                    }
                    Err(_) => {
                        self.sections.remove(name);
                    }
                },
                Kind::Sessions => held.sessions = sessions_of(root, name),
                Kind::Project | Kind::Harness | Kind::Persona | Kind::Git | Kind::Chats => {}
            }
        }
    }

    /// Brings the held memory counts up to date with `changes`: a persona's memory, or the
    /// persona itself, counts it again; `personas/` itself moving forgets every count.
    fn apply_to_counts(&mut self, root: &Path, changes: &[Change]) {
        for change in changes {
            if !matches!(change.kind, Kind::Memory | Kind::Persona) {
                continue;
            }
            match change.persona.as_deref() {
                Some(persona) => {
                    if let Some(count) = self.memory_counts.get_mut(persona) {
                        *count = crate::personas::memory_count(root, persona);
                    }
                }
                None if change.kind == Kind::Persona => self.memory_counts.clear(),
                None => {}
            }
        }
    }

    /// Workspace `name`'s sections, read on the first ask and kept current from then on — or
    /// why the workspace could not be read, which is never held: the next ask reads again. A
    /// workspace with a store that could not be read is read again on every ask too.
    pub fn sections(&mut self, root: &Path, name: &str) -> Result<Sections, String> {
        // A store that could not be read is not held either: what refused it — a link out of
        // the plane, a permission — can change with nothing the watch hears.
        if let Some(held) = self.sections.get(name)
            && held.todos.is_ok()
            && held.memories.is_ok()
        {
            return Ok(held.clone());
        }
        let read = Sections::read(root, name)?;
        self.sections.insert(name.to_owned(), read.clone());
        Ok(read)
    }

    /// How many memories `persona` holds (`_shared` for the shared store), counted on the
    /// first ask and kept current from then on.
    pub fn memory_count(&mut self, root: &Path, persona: &str) -> usize {
        *self
            .memory_counts
            .entry(persona.to_owned())
            .or_insert_with(|| crate::personas::memory_count(root, persona))
    }

    /// The workspaces, sorted by name — or why `workspaces/` could not be listed.
    pub fn rows(&self) -> Result<impl Iterator<Item = &Row>, String> {
        match &self.rows_unread {
            Some(why) => Err(why.clone()),
            None => Ok(self.rows.values()),
        }
    }

    /// The plane's personas, sorted — or why `personas/` could not be listed.
    pub fn personas(&self) -> Result<Vec<String>, String> {
        self.personas.clone()
    }

    /// `[persona] default` in `charter.toml`, or `None`.
    pub fn default_persona(&self) -> Option<&str> {
        self.default_persona.as_deref()
    }

    /// Whether `name` is LIVE.
    pub fn is_live(&self, name: &str) -> bool {
        self.live.contains(name)
    }

    fn read_project(&mut self, plane: &Plane) {
        self.default_persona = plane.default_persona();
        self.live = crate::wscmd::live_workspaces(plane.root());
    }

    fn read_personas(&mut self, plane: &Plane) {
        self.personas = plane.personas().map_err(|err| err.to_string());
    }

    /// Lists `workspaces/` again: a name that is new is read, one that is gone is dropped,
    /// and one already held is left as it is. Answers the names it read.
    fn read_names(&mut self, plane: &Plane) -> BTreeSet<String> {
        let mut met = BTreeSet::new();
        let names = match plane.workspaces() {
            Ok(names) => names,
            Err(err) => {
                self.rows.clear();
                self.rows_unread = Some(err.to_string());
                return met;
            }
        };
        self.rows_unread = None;
        let names: BTreeSet<String> = names.into_iter().collect();
        self.rows.retain(|name, _| names.contains(name));
        for name in names {
            if self.rows.contains_key(&name) {
                continue;
            }
            // A name off disk is re-checked before it is joined onto a path; one that cannot
            // be a workspace is left out rather than drawn.
            if let Some(row) = row_of(plane, &name) {
                met.insert(name.clone());
                self.rows.insert(name, row);
            }
        }
        met
    }
}

/// One workspace read whole, or `None` for a name that cannot be one.
fn row_of(plane: &Plane, name: &str) -> Option<Row> {
    let workspace = plane.workspace(name).ok()?;
    Some(Row {
        name: name.to_owned(),
        path: workspace.dir().to_path_buf(),
        vision: workspace.vision(),
        todos: todos_of(&workspace),
        colour: theme::colour_of(&workspace)
            .as_ref()
            .map(theme::Colour::value),
    })
}

/// A workspace's open todos' titles. A store charter cannot read costs that workspace its
/// todo list, not the sidebar its workspaces.
fn todos_of(workspace: &Workspace) -> Vec<String> {
    workspace
        .todos()
        .unwrap_or_default()
        .into_iter()
        .map(|todo| todo.title)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspaces::Entry;

    fn todo(root: &Path, workspace: &str, slug: &str, title: &str) {
        let store = root.join("workspaces").join(workspace).join("todos");
        std::fs::create_dir_all(&store).expect("a todo store");
        std::fs::write(store.join(format!("{slug}.md")), format!("# {title}\n")).expect("todo");
    }

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a scratch plane");
        let root = dir.path();
        todo(root, "alpha", "a1", "Alpha one");
        todo(root, "beta", "b1", "Beta one");
        std::fs::create_dir_all(root.join("personas/steward")).expect("a persona");
        std::fs::write(root.join("personas/steward/persona.md"), "# steward\n").expect("charter");
        dir
    }

    fn change(kind: Kind, workspace: Option<&str>, path: &str) -> Change {
        Change {
            kind,
            workspace: workspace.map(str::to_owned),
            persona: None,
            path: path.to_owned(),
        }
    }

    fn todos(model: &Model, workspace: &str) -> Vec<String> {
        model
            .rows()
            .expect("listed")
            .find(|row| row.name == workspace)
            .map(|row| row.todos.clone())
            .unwrap_or_else(|| panic!("no workspace {workspace}"))
    }

    #[test]
    fn a_model_read_fresh_holds_each_workspace_with_its_todos_and_the_personas() {
        let plane = plane();
        let model = Model::read(plane.path());
        let names: Vec<_> = model
            .rows()
            .expect("listed")
            .map(|row| row.name.clone())
            .collect();
        assert_eq!(names, ["alpha", "beta"]);
        assert_eq!(todos(&model, "alpha"), ["Alpha one"]);
        assert_eq!(todos(&model, "beta"), ["Beta one"]);
        assert_eq!(model.personas(), Ok(vec!["steward".to_owned()]));
    }

    #[test]
    fn a_change_of_one_workspaces_todos_re_reads_that_store_and_no_other() {
        let plane = plane();
        let root = plane.path();
        let mut model = Model::read(root);
        // Both stores move on disk, and the model is told about beta's only: what it then
        // holds for alpha is what it held before, so alpha's store was not read again.
        todo(root, "alpha", "a2", "Alpha two");
        todo(root, "beta", "b2", "Beta two");

        model.apply(
            root,
            Some(&[change(
                Kind::Todos,
                Some("beta"),
                "workspaces/beta/todos/b2.md",
            )]),
        );

        assert_eq!(todos(&model, "beta"), ["Beta one", "Beta two"]);
        assert_eq!(todos(&model, "alpha"), ["Alpha one"]);
    }

    #[test]
    fn not_knowing_what_changed_rebuilds_the_model_into_one_equal_to_a_fresh_read() {
        let plane = plane();
        let root = plane.path();
        let mut model = Model::read(root);
        todo(root, "alpha", "a2", "Alpha two");
        todo(root, "gamma", "g1", "Gamma one");
        std::fs::remove_dir_all(root.join("workspaces/beta")).expect("beta gone");
        std::fs::write(
            root.join("charter.toml"),
            "[persona]\ndefault = \"steward\"\n",
        )
        .expect("a default persona");

        model.apply(root, None);

        assert_eq!(model, Model::read(root));
        assert_eq!(todos(&model, "alpha"), ["Alpha one", "Alpha two"]);
        assert_eq!(model.default_persona(), Some("steward"));
    }

    #[test]
    fn a_workspace_made_or_removed_and_its_charter_and_colour_follow_their_own_changes() {
        let plane = plane();
        let root = plane.path();
        let mut model = Model::read(root);
        todo(root, "gamma", "g1", "Gamma one");
        std::fs::remove_dir_all(root.join("workspaces/beta")).expect("beta gone");
        std::fs::write(
            root.join("workspaces/alpha/workspace.md"),
            "# alpha\n\n## Vision\n\nShip the model\n",
        )
        .expect("a vision");
        std::fs::write(
            root.join("workspaces/alpha/workspace.json"),
            r##"{"settings":{"theme":{"colour":"#112233"}}}"##,
        )
        .expect("a colour");

        model.apply(
            root,
            Some(&[
                change(Kind::Workspace, Some("gamma"), "workspaces/gamma"),
                change(Kind::Workspace, Some("beta"), "workspaces/beta"),
                change(
                    Kind::Workspace,
                    Some("alpha"),
                    "workspaces/alpha/workspace.md",
                ),
                change(
                    Kind::Workspace,
                    Some("alpha"),
                    "workspaces/alpha/workspace.json",
                ),
            ]),
        );

        assert_eq!(model, Model::read(root));
        let alpha = model
            .rows()
            .expect("listed")
            .find(|row| row.name == "alpha")
            .cloned()
            .expect("alpha");
        assert_eq!(alpha.vision, "Ship the model");
        assert_eq!(alpha.colour.as_deref(), Some("#112233"));
        assert_eq!(todos(&model, "gamma"), ["Gamma one"]);
    }

    #[test]
    fn a_persona_change_lists_the_personas_again_and_a_project_change_reads_it_all() {
        let plane = plane();
        let root = plane.path();
        let mut model = Model::read(root);
        std::fs::create_dir_all(root.join("personas/scribe")).expect("a persona");
        std::fs::write(root.join("personas/scribe/persona.md"), "# scribe\n").expect("charter");
        model.apply(
            root,
            Some(&[Change {
                kind: Kind::Persona,
                workspace: None,
                persona: Some("scribe".to_owned()),
                path: "personas/scribe/persona.md".to_owned(),
            }]),
        );
        assert_eq!(
            model.personas(),
            Ok(vec!["scribe".to_owned(), "steward".to_owned()])
        );

        std::fs::write(
            root.join("charter.toml"),
            "[persona]\ndefault = \"scribe\"\n",
        )
        .expect("a default persona");
        std::fs::write(
            root.join(".gitignore"),
            "# >>> charter live workspaces (managed by `charter workspace live`) >>>\n\
             !/workspaces/alpha/workspace.json\n\
             # <<< charter live workspaces <<<\n",
        )
        .expect("alpha is live");
        model.apply(root, Some(&[change(Kind::Project, None, "charter.toml")]));
        assert_eq!(model.default_persona(), Some("scribe"));
        assert!(model.is_live("alpha"));
        assert!(
            !model.is_live("beta"),
            "only the workspaces the block names are live"
        );
        assert_eq!(model, Model::read(root));
    }

    #[test]
    fn a_memory_a_session_record_the_harness_settings_or_a_save_read_nothing() {
        let plane = plane();
        let root = plane.path();
        let mut model = Model::read(root);
        let before = model.clone();
        todo(root, "alpha", "a2", "Alpha two");

        model.apply(
            root,
            Some(&[
                change(Kind::Memory, Some("alpha"), "workspaces/alpha/memory/m.md"),
                change(
                    Kind::Sessions,
                    Some("alpha"),
                    "workspaces/alpha/sessions/s.md",
                ),
                change(Kind::Harness, None, ".claude/settings.json"),
                crate::planechange::saved(),
            ]),
        );

        assert_eq!(model, before);
    }

    #[test]
    fn a_todo_of_a_workspace_the_model_has_not_met_brings_the_workspace_in() {
        let plane = plane();
        let root = plane.path();
        let mut model = Model::read(root);
        todo(root, "gamma", "g1", "Gamma one");

        model.apply(
            root,
            Some(&[change(
                Kind::Todos,
                Some("gamma"),
                "workspaces/gamma/todos/g1.md",
            )]),
        );

        assert_eq!(todos(&model, "gamma"), ["Gamma one"]);
        assert_eq!(model, Model::read(root));
    }

    // ---- the workspace panels' sections (FD-10c) ------------------------------------------

    fn memory(root: &Path, workspace: &str, slug: &str, title: &str) {
        let store = root.join("workspaces").join(workspace).join("memory");
        std::fs::create_dir_all(&store).expect("a memory store");
        std::fs::write(store.join(format!("{slug}.md")), format!("# {title}\n")).expect("memory");
    }

    fn persona_memory(root: &Path, persona: &str, slug: &str) {
        let store = root.join("personas").join(persona).join("memory");
        std::fs::create_dir_all(&store).expect("a persona's store");
        std::fs::write(store.join(format!("{slug}.md")), format!("# {slug}\n")).expect("memory");
    }

    fn clone(root: &Path, workspace: &str, repo: &str) {
        std::fs::create_dir_all(
            root.join("workspaces")
                .join(workspace)
                .join(repo)
                .join(".git"),
        )
        .expect("a clone");
    }

    fn session(root: &Path, workspace: &str, minute: u32, title: &str) {
        let facts = crate::sessionrecord::Facts {
            place: crate::active::Place::Workspace(workspace.to_owned()),
            at: chrono::NaiveDate::from_ymd_opt(2026, 10, 4)
                .and_then(|day| day.and_hms_opt(9, minute, 0))
                .expect("a time"),
            chat: None,
            persona: None,
            pieces: Vec::new(),
        };
        let body = crate::sessionrecord::SECTIONS
            .iter()
            .map(|section| format!("## {section}\n\nSomething.\n"))
            .collect::<Vec<_>>()
            .join("\n");
        crate::sessionrecord::record(
            root,
            &crate::sessionrecord::New {
                title,
                body: &body,
                facts: &facts,
            },
        )
        .expect("a session record");
    }

    fn titles(read: &Result<Vec<Entry>, String>) -> Vec<String> {
        read.as_ref()
            .expect("read")
            .iter()
            .map(|entry| entry.title.clone())
            .collect()
    }

    fn session_titles(sections: &Sections) -> Vec<String> {
        sections
            .sessions
            .iter()
            .map(|record| record.title.clone())
            .collect()
    }

    fn repo_names(sections: &Sections) -> Vec<String> {
        sections
            .clones
            .repos
            .iter()
            .map(|repo| repo.name.clone())
            .collect()
    }

    /// A plane whose `alpha` has a todo, a memory, a session record and a clone.
    fn full_plane() -> tempfile::TempDir {
        let plane = plane();
        let root = plane.path();
        memory(root, "alpha", "m1", "Memory one");
        session(root, "alpha", 1, "Session one");
        clone(root, "alpha", "svc");
        plane
    }

    #[test]
    fn a_workspaces_sections_read_through_the_model_equal_a_fresh_read() {
        let plane = full_plane();
        let root = plane.path();
        let mut model = Model::read(root);

        let sections = model.sections(root, "alpha").expect("alpha's sections");

        assert_eq!(sections, Sections::read(root, "alpha").expect("read fresh"));
        assert_eq!(titles(&sections.todos), ["Alpha one"]);
        assert_eq!(titles(&sections.memories), ["Memory one"]);
        assert_eq!(session_titles(&sections), ["Session one"]);
        assert_eq!(repo_names(&sections), ["svc"]);
    }

    #[test]
    fn a_memory_written_re_reads_that_workspaces_memory_store_only() {
        let plane = full_plane();
        let root = plane.path();
        let mut model = Model::read(root);
        model.sections(root, "alpha").expect("alpha's sections");
        // Every section moves on disk, and the model is told about the memory only: what it
        // then holds for the others is what it held before, so they were not read again.
        memory(root, "alpha", "m2", "Memory two");
        todo(root, "alpha", "a2", "Alpha two");
        session(root, "alpha", 2, "Session two");
        clone(root, "alpha", "web");

        model.apply(
            root,
            Some(&[change(
                Kind::Memory,
                Some("alpha"),
                "workspaces/alpha/memory/m2.md",
            )]),
        );
        let sections = model.sections(root, "alpha").expect("alpha's sections");

        assert_eq!(titles(&sections.memories), ["Memory one", "Memory two"]);
        assert_eq!(titles(&sections.todos), ["Alpha one"]);
        assert_eq!(session_titles(&sections), ["Session one"]);
        assert_eq!(repo_names(&sections), ["svc"]);
    }

    #[test]
    fn a_session_record_written_re_reads_that_workspaces_sessions_only() {
        let plane = full_plane();
        let root = plane.path();
        let mut model = Model::read(root);
        model.sections(root, "alpha").expect("alpha's sections");
        memory(root, "alpha", "m2", "Memory two");
        todo(root, "alpha", "a2", "Alpha two");
        session(root, "alpha", 2, "Session two");
        clone(root, "alpha", "web");

        model.apply(
            root,
            Some(&[change(
                Kind::Sessions,
                Some("alpha"),
                "workspaces/alpha/sessions/x.md",
            )]),
        );
        let sections = model.sections(root, "alpha").expect("alpha's sections");

        // Newest first, as the Sessions panel lists them.
        assert_eq!(session_titles(&sections), ["Session two", "Session one"]);
        assert_eq!(titles(&sections.memories), ["Memory one"]);
        assert_eq!(titles(&sections.todos), ["Alpha one"]);
        assert_eq!(repo_names(&sections), ["svc"]);
    }

    #[test]
    fn a_todo_re_reads_its_todos_and_a_clone_arriving_re_reads_the_clones() {
        let plane = full_plane();
        let root = plane.path();
        let mut model = Model::read(root);
        model.sections(root, "alpha").expect("alpha's sections");
        memory(root, "alpha", "m2", "Memory two");
        todo(root, "alpha", "a2", "Alpha two");
        clone(root, "alpha", "web");

        model.apply(
            root,
            Some(&[
                change(Kind::Todos, Some("alpha"), "workspaces/alpha/todos/a2.md"),
                change(Kind::Workspace, Some("alpha"), "workspaces/alpha/web"),
            ]),
        );
        let sections = model.sections(root, "alpha").expect("alpha's sections");

        assert_eq!(titles(&sections.todos), ["Alpha one", "Alpha two"]);
        assert_eq!(repo_names(&sections), ["svc", "web"]);
        assert_eq!(titles(&sections.memories), ["Memory one"]);
    }

    #[test]
    fn a_change_in_another_workspace_leaves_the_held_sections_as_they_are() {
        let plane = full_plane();
        let root = plane.path();
        let mut model = Model::read(root);
        let before = model.sections(root, "alpha").expect("alpha's sections");
        memory(root, "alpha", "m2", "Memory two");

        model.apply(
            root,
            Some(&[change(
                Kind::Memory,
                Some("beta"),
                "workspaces/beta/memory/m.md",
            )]),
        );

        assert_eq!(model.sections(root, "alpha").expect("alpha's"), before);
    }

    #[test]
    fn not_knowing_what_changed_reads_every_section_again() {
        let plane = full_plane();
        let root = plane.path();
        let mut model = Model::read(root);
        model.sections(root, "alpha").expect("alpha's sections");
        memory(root, "alpha", "m2", "Memory two");
        todo(root, "alpha", "a2", "Alpha two");
        session(root, "alpha", 2, "Session two");
        clone(root, "alpha", "web");

        model.apply(root, None);

        assert_eq!(
            model.sections(root, "alpha").expect("alpha's"),
            Sections::read(root, "alpha").expect("read fresh")
        );
    }

    #[test]
    fn a_store_that_could_not_be_read_is_read_again_on_the_next_ask() {
        // What refused it can change with nothing the watch hears: a link out of the plane
        // replaced by the store itself.
        let plane = full_plane();
        let root = plane.path();
        let elsewhere = tempfile::tempdir().expect("outside the plane");
        let store = root.join("workspaces/alpha/todos");
        std::fs::remove_dir_all(&store).expect("the store goes");
        std::os::unix::fs::symlink(elsewhere.path(), &store).expect("a link out");
        let mut model = Model::read(root);
        assert!(model.sections(root, "alpha").expect("alpha").todos.is_err());

        std::fs::remove_file(&store).expect("the link goes");
        todo(root, "alpha", "a1", "Alpha one");

        assert_eq!(
            titles(&model.sections(root, "alpha").expect("alpha").todos),
            ["Alpha one"]
        );
    }

    #[test]
    fn a_workspace_that_is_not_there_has_no_sections() {
        let plane = plane();
        let mut model = Model::read(plane.path());
        assert!(model.sections(plane.path(), "nobody").is_err());
    }

    #[test]
    fn a_personas_memory_count_follows_its_own_memory_and_no_other() {
        let plane = plane();
        let root = plane.path();
        std::fs::create_dir_all(root.join("personas/scribe")).expect("a persona");
        let mut model = Model::read(root);
        assert_eq!(model.memory_count(root, "steward"), 0);
        assert_eq!(model.memory_count(root, "scribe"), 0);
        persona_memory(root, "steward", "s1");
        persona_memory(root, "scribe", "c1");

        model.apply(
            root,
            Some(&[Change {
                kind: Kind::Memory,
                workspace: None,
                persona: Some("steward".to_owned()),
                path: "personas/steward/memory/s1.md".to_owned(),
            }]),
        );

        assert_eq!(model.memory_count(root, "steward"), 1);
        assert_eq!(model.memory_count(root, "scribe"), 0);
        model.apply(root, None);
        assert_eq!(model.memory_count(root, "scribe"), 1);
    }

    /// One thing done to a plane on disk, as the CLI, an editor or another chat does it.
    #[derive(Debug, Clone)]
    enum Write {
        Make(usize),
        Remove(usize),
        Rename(usize, usize),
        AddTodo(usize, usize),
        CloseTodo(usize, usize),
        Vision(usize, usize),
        Persona(usize),
        Default(usize),
        Remember(usize, usize),
        ForgetMemory(usize, usize),
        Record(usize, usize),
        Clone(usize, usize),
        PersonaMemory(usize, usize),
        Declare(usize, usize),
        RemoveClone(usize, usize),
    }

    const NAMES: [&str; 3] = ["alpha", "beta", "gamma"];

    fn a_write() -> impl proptest::strategy::Strategy<Value = Write> {
        use proptest::prelude::*;
        prop_oneof![
            (0..3usize).prop_map(Write::Make),
            (0..3usize).prop_map(Write::Remove),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::Rename(a, b)),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::AddTodo(a, b)),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::CloseTodo(a, b)),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::Vision(a, b)),
            (0..3usize).prop_map(Write::Persona),
            (0..3usize).prop_map(Write::Default),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::Remember(a, b)),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::ForgetMemory(a, b)),
            (0..3usize, 0..30usize).prop_map(|(a, b)| Write::Record(a, b)),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::Clone(a, b)),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::PersonaMemory(a, b)),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::Declare(a, b)),
            (0..3usize, 0..3usize).prop_map(|(a, b)| Write::RemoveClone(a, b)),
        ]
    }

    /// Does `write` to the plane at `root`, and answers the paths it touched — what the watch
    /// reports for it.
    fn done(root: &Path, write: &Write) -> Vec<PathBuf> {
        let ws = |i: usize| root.join("workspaces").join(NAMES[i]);
        match *write {
            Write::Make(i) => {
                std::fs::create_dir_all(ws(i).join("todos")).expect("made");
                vec![ws(i), ws(i).join("todos")]
            }
            Write::Remove(i) => {
                let _ = std::fs::remove_dir_all(ws(i));
                vec![ws(i)]
            }
            Write::Rename(a, b) => {
                if a == b || !ws(a).is_dir() || ws(b).exists() {
                    return Vec::new();
                }
                std::fs::rename(ws(a), ws(b)).expect("renamed");
                vec![ws(a), ws(b)]
            }
            Write::AddTodo(i, k) => {
                if !ws(i).is_dir() {
                    return Vec::new();
                }
                todo(
                    root,
                    NAMES[i],
                    &format!("t{k}"),
                    &format!("{} {k}", NAMES[i]),
                );
                vec![ws(i).join("todos").join(format!("t{k}.md"))]
            }
            Write::CloseTodo(i, k) => {
                let file = ws(i).join("todos").join(format!("t{k}.md"));
                if std::fs::remove_file(&file).is_err() {
                    return Vec::new();
                }
                vec![file]
            }
            Write::Vision(i, k) => {
                if !ws(i).is_dir() {
                    return Vec::new();
                }
                let file = ws(i).join("workspace.md");
                std::fs::write(&file, format!("# x\n\n## Vision\n\nVision {k}\n")).expect("vision");
                vec![file]
            }
            Write::Persona(k) => {
                let dir = root.join("personas").join(format!("p{k}"));
                std::fs::create_dir_all(&dir).expect("a persona");
                std::fs::write(dir.join("persona.md"), "# p\n").expect("charter");
                vec![dir.join("persona.md")]
            }
            Write::Default(k) => {
                let file = root.join("charter.toml");
                std::fs::write(&file, format!("[persona]\ndefault = \"p{k}\"\n")).expect("toml");
                vec![file]
            }
            Write::Remember(i, k) => {
                if !ws(i).is_dir() {
                    return Vec::new();
                }
                memory(
                    root,
                    NAMES[i],
                    &format!("m{k}"),
                    &format!("{} memory {k}", NAMES[i]),
                );
                vec![ws(i).join("memory").join(format!("m{k}.md"))]
            }
            Write::ForgetMemory(i, k) => {
                let file = ws(i).join("memory").join(format!("m{k}.md"));
                if std::fs::remove_file(&file).is_err() {
                    return Vec::new();
                }
                vec![file]
            }
            Write::Record(i, minute) => {
                let store = ws(i).join("sessions");
                if !ws(i).is_dir()
                    || store.is_dir()
                        && std::fs::read_dir(&store).is_ok_and(|mut it| {
                            it.any(|entry| {
                                entry.is_ok_and(|e| {
                                    e.file_name()
                                        .to_string_lossy()
                                        .contains(&format!("-09{minute:02}-"))
                                })
                            })
                        })
                {
                    return Vec::new();
                }
                session(
                    root,
                    NAMES[i],
                    minute as u32,
                    &format!("{} record {minute}", NAMES[i]),
                );
                vec![store]
            }
            Write::Clone(i, k) => {
                if !ws(i).is_dir() {
                    return Vec::new();
                }
                clone(root, NAMES[i], &format!("r{k}"));
                vec![ws(i).join(format!("r{k}"))]
            }
            Write::Declare(i, k) => {
                if !ws(i).is_dir() {
                    return Vec::new();
                }
                let file = ws(i).join("workspace.json");
                let repos: Vec<String> = (0..=k).map(|n| format!(r#"{{"name":"r{n}"}}"#)).collect();
                std::fs::write(&file, format!(r#"{{"repos":[{}]}}"#, repos.join(",")))
                    .expect("a manifest");
                vec![file]
            }
            Write::RemoveClone(i, k) => {
                let repo = ws(i).join(format!("r{k}"));
                if std::fs::remove_dir_all(&repo).is_err() {
                    return Vec::new();
                }
                vec![repo]
            }
            Write::PersonaMemory(p, k) => {
                let persona = ["steward", "p0", "_shared"][p];
                persona_memory(root, persona, &format!("pm{k}"));
                vec![
                    root.join("personas")
                        .join(persona)
                        .join("memory")
                        .join(format!("pm{k}.md")),
                ]
            }
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(48))]

        /// Whatever the plane goes through, a model told each change as the watch names it is
        /// the model a fresh read gives.
        #[test]
        fn a_model_told_every_change_equals_one_read_fresh(
            writes in proptest::collection::vec(a_write(), 1..12)
        ) {
            let plane = plane();
            let root = plane.path();
            let mut model = Model::read(root);
            for write in &writes {
                let touched = done(root, write);
                let changes = crate::planechange::of_batch(root, touched.iter().map(PathBuf::as_path))
                    .expect("inside the plane");
                model.apply(root, Some(&changes));
                proptest::prop_assert_eq!(&model, &Model::read(root), "after {:?}", write);
            }
        }

        /// The same for the panels (FD-10c): each workspace's sections, asked for before and
        /// after every write so the model holds them, and each persona's memory count, equal
        /// what a fresh read of the disk gives.
        #[test]
        fn the_panels_told_every_change_equal_the_panels_read_fresh(
            writes in proptest::collection::vec(a_write(), 1..12)
        ) {
            let plane = plane();
            let root = plane.path();
            let mut model = Model::read(root);
            let personas = ["steward", "p0", "_shared"];
            for name in NAMES {
                let _ = model.sections(root, name);
            }
            for persona in personas {
                model.memory_count(root, persona);
            }
            for write in &writes {
                let touched = done(root, write);
                let changes = crate::planechange::of_batch(root, touched.iter().map(PathBuf::as_path))
                    .expect("inside the plane");
                model.apply(root, Some(&changes));
                for name in NAMES {
                    proptest::prop_assert_eq!(
                        model.sections(root, name),
                        Sections::read(root, name),
                        "{} after {:?}", name, write
                    );
                }
                for persona in personas {
                    proptest::prop_assert_eq!(
                        model.memory_count(root, persona),
                        crate::personas::memory_count(root, persona),
                        "{} after {:?}", persona, write
                    );
                }
            }
        }
    }
}
