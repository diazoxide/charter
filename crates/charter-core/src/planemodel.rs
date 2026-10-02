//! What the sidebar draws, held in memory and kept current by what changed (FD-10b).
//!
//! The sidebar is every workspace with its open todos, its vision and colour and whether it
//! is LIVE, and the plane's personas with the default one. It used to be read from the disk
//! whole on every ask: every workspace's `todos/` listed and every todo opened, for a change
//! that was a todo closed in one of them. Here it is read once, and each change the watcher
//! names ([`crate::planechange`]) re-reads only what that change is part of: a todo of `beta`
//! re-reads `beta/todos/` and nothing else.
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

use crate::extension::project::theme;
use crate::planechange::{Change, Kind};
use crate::workspaces::{Plane, Workspace};

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

/// The sidebar's reading of one plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    rows: BTreeMap<String, Row>,
    /// Why `workspaces/` could not be listed, where it could not.
    rows_unread: Option<String>,
    personas: Result<Vec<String>, String>,
    default_persona: Option<String>,
    live: BTreeSet<String>,
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
    }
}
