//! What changed in a plane on disk, by kind and path (FD-10).
//!
//! The app watches a plane and used to say only *that* it moved (`plane-changed`), so every
//! panel read its whole answer again on every write: a memory an agent saved made the sidebar
//! list every workspace's todos once more. This module names what a changed path IS — a todo
//! of one workspace, a memory of one persona, a session record, a manifest — so a reader can
//! ask again only when the change is one its answer is made of.
//!
//! **It is a reading of the plane format's paths, nothing more** (`docs/plane-format.md`): no
//! file is opened, and a path is classified by where it is, never by what is in it.
//!
//! **Not knowing is said, never guessed.** A batch holding a path this cannot place — one
//! outside the plane, which is what a root spelled through a link looks like — is answered
//! `None`, which a reader takes as "anything may have moved" and reads everything again, as
//! it did before this module. A reader that misses a change draws a stale panel; one told too
//! much only reads again.

use std::collections::BTreeSet;
use std::path::{Component, Path};

/// What a changed path is part of, as the readers of the plane divide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A file at the plane root — `charter.toml`, `charter.local.toml`, `CLAUDE.md` — or the
    /// root itself.
    Project,
    /// The harness settings and sub-agents a chat reads at its start: `.claude/`.
    Harness,
    /// A workspace's own files: `workspace.json`, a clone arriving or going, the workspace
    /// directory itself; or `workspaces/` itself, with no workspace named.
    Workspace,
    /// A workspace's `todos/`.
    Todos,
    /// A memory store: a workspace's `memory/` or a persona's (`_shared` included).
    Memory,
    /// Session records: a workspace's `sessions/`, or the plane root's.
    Sessions,
    /// A persona's own files (`persona.md` and the rest), the persona directory itself; or
    /// `personas/` itself, with no persona named.
    Persona,
}

/// One changed path, and what it is part of.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct Change {
    pub kind: Kind,
    /// The workspace it is in, where it is in one.
    pub workspace: Option<String>,
    /// The persona it belongs to, where it belongs to one (`_shared` for the shared store).
    pub persona: Option<String>,
    /// The path, relative to the plane root, with `/` between its parts.
    pub path: String,
}

/// What one path under `root` is, or `None` when it is not under `root`.
pub fn classify(root: &Path, path: &Path) -> Option<Change> {
    let inside = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for part in inside.components() {
        match part {
            Component::Normal(name) => parts.push(name.to_str()?.to_owned()),
            Component::CurDir => {}
            // `..` or a second root would put the path somewhere this cannot name.
            _ => return None,
        }
    }
    let named = |at: usize| parts.get(at).cloned();
    let (kind, workspace, persona) = match parts.first().map(String::as_str) {
        None => (Kind::Project, None, None),
        Some(".claude") => (Kind::Harness, None, None),
        Some("sessions") => (Kind::Sessions, None, None),
        Some("workspaces") => {
            let kind = match parts.get(2).map(String::as_str) {
                Some("todos") => Kind::Todos,
                Some("memory") => Kind::Memory,
                Some("sessions") => Kind::Sessions,
                _ => Kind::Workspace,
            };
            (kind, named(1), None)
        }
        Some("personas") => {
            let kind = match parts.get(2).map(String::as_str) {
                Some("memory") => Kind::Memory,
                _ => Kind::Persona,
            };
            (kind, None, named(1))
        }
        Some(_) => (Kind::Project, None, None),
    };
    Some(Change {
        kind,
        workspace,
        persona,
        path: parts.join("/"),
    })
}

/// What a batch of changed paths under `root` is, each change once, in a stable order — or
/// `None` when any one of them cannot be placed (see the module's header).
pub fn of_batch<'a>(root: &Path, paths: impl IntoIterator<Item = &'a Path>) -> Option<Vec<Change>> {
    let mut changes = BTreeSet::new();
    for path in paths {
        changes.insert(classify(root, path)?);
    }
    Some(changes.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/home/dev/plane";

    fn at(path: &str) -> Change {
        classify(Path::new(ROOT), &Path::new(ROOT).join(path)).expect("inside the plane")
    }

    fn change(kind: Kind, workspace: Option<&str>, persona: Option<&str>, path: &str) -> Change {
        Change {
            kind,
            workspace: workspace.map(str::to_owned),
            persona: persona.map(str::to_owned),
            path: path.to_owned(),
        }
    }

    #[test]
    fn a_todo_is_a_todo_of_its_workspace() {
        assert_eq!(
            at("workspaces/alpha/todos/m8-1.md"),
            change(
                Kind::Todos,
                Some("alpha"),
                None,
                "workspaces/alpha/todos/m8-1.md"
            )
        );
    }

    #[test]
    fn a_workspace_memory_and_a_persona_memory_are_both_memory_of_their_owner() {
        assert_eq!(
            at("workspaces/alpha/memory/20260919-120000-note.md"),
            change(
                Kind::Memory,
                Some("alpha"),
                None,
                "workspaces/alpha/memory/20260919-120000-note.md"
            )
        );
        assert_eq!(
            at("personas/steward/memory/MEMORY.md"),
            change(
                Kind::Memory,
                None,
                Some("steward"),
                "personas/steward/memory/MEMORY.md"
            )
        );
        assert_eq!(
            at("personas/_shared/memory/fact.md").persona.as_deref(),
            Some("_shared")
        );
    }

    #[test]
    fn session_records_are_sessions_of_their_workspace_or_of_the_root() {
        assert_eq!(
            at("workspaces/alpha/sessions/2026-10-02-1650-x.md"),
            change(
                Kind::Sessions,
                Some("alpha"),
                None,
                "workspaces/alpha/sessions/2026-10-02-1650-x.md"
            )
        );
        assert_eq!(
            at("sessions/2026-10-02-1650-x.md"),
            change(Kind::Sessions, None, None, "sessions/2026-10-02-1650-x.md")
        );
    }

    #[test]
    fn a_manifest_a_clone_or_the_workspace_itself_is_the_workspace() {
        for path in [
            "workspaces/alpha",
            "workspaces/alpha/workspace.json",
            "workspaces/alpha/svc",
        ] {
            assert_eq!(at(path), change(Kind::Workspace, Some("alpha"), None, path));
        }
        assert_eq!(
            at("workspaces"),
            change(Kind::Workspace, None, None, "workspaces")
        );
    }

    #[test]
    fn a_persona_charter_is_its_persona_and_the_root_files_are_the_project() {
        assert_eq!(
            at("personas/steward/persona.md"),
            change(
                Kind::Persona,
                None,
                Some("steward"),
                "personas/steward/persona.md"
            )
        );
        assert_eq!(
            at("personas"),
            change(Kind::Persona, None, None, "personas")
        );
        assert_eq!(at(".claude/agents/x.md").kind, Kind::Harness);
        assert_eq!(
            at("charter.toml"),
            change(Kind::Project, None, None, "charter.toml")
        );
        assert_eq!(at("CLAUDE.md").kind, Kind::Project);
        assert_eq!(at(""), change(Kind::Project, None, None, ""));
    }

    #[test]
    fn a_path_outside_the_plane_is_not_placed() {
        assert_eq!(
            classify(Path::new(ROOT), Path::new("/home/dev/elsewhere/x.md")),
            None
        );
    }

    #[test]
    fn a_batch_is_each_change_once_and_unknown_as_a_whole_if_one_path_is_unplaced() {
        let root = Path::new(ROOT);
        let todo = root.join("workspaces/alpha/todos/a.md");
        let memory = root.join("workspaces/alpha/memory/b.md");
        let batch = [todo.as_path(), memory.as_path(), todo.as_path()];
        assert_eq!(
            of_batch(root, batch),
            Some(vec![
                change(
                    Kind::Todos,
                    Some("alpha"),
                    None,
                    "workspaces/alpha/todos/a.md"
                ),
                change(
                    Kind::Memory,
                    Some("alpha"),
                    None,
                    "workspaces/alpha/memory/b.md"
                ),
            ])
        );
        let outside = Path::new("/elsewhere/c.md");
        assert_eq!(of_batch(root, [todo.as_path(), outside]), None);
    }
}
