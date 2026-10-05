//! A memory's scope, and moving a memory from one scope to another (KN-3, the follow-up ADR
//! 0065 Q13 left out).
//!
//! A memory is in one of three kinds of store: a workspace's journal
//! (`workspaces/<ws>/memory/`), a persona's own (`personas/<name>/memory/`), or the one every
//! persona reads (`personas/_shared/memory/`). [`move_memory`] is the one move, which the
//! window's Move and `charter workspace move` / `charter persona move-memory` both call; the
//! file rules are [`crate::memstore::move_one`]'s, and `docs/plane-format.md` → *Moving a memory
//! between scopes* is the contract.

use std::io;
use std::path::PathBuf;

use crate::memstore;
use crate::personas::SHARED;
use crate::workspaces::Plane;

/// Which store a memory is in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    /// A workspace's journal.
    Workspace(String),
    /// A persona's own memory.
    Persona(String),
    /// The memory every persona reads, `personas/_shared/memory/`.
    Shared,
}

impl Scope {
    /// The store's directory, relative to the project, with `/`.
    pub fn store(&self) -> String {
        match self {
            Self::Workspace(name) => format!("workspaces/{name}/memory"),
            Self::Persona(name) => format!("personas/{name}/memory"),
            Self::Shared => format!("personas/{SHARED}/memory"),
        }
    }

    /// How a sentence names it: `workspace 'alpha'`, `persona 'devops'`, `shared memory`.
    pub fn said(&self) -> String {
        match self {
            Self::Workspace(name) => format!("workspace '{name}'"),
            Self::Persona(name) => format!("persona '{name}'"),
            Self::Shared => "shared memory".to_owned(),
        }
    }

    /// Whether the store's files carry the journal's `YYYYMMDD-HHMMSS-` prefix.
    fn timestamped(&self) -> bool {
        matches!(self, Self::Workspace(_))
    }

    /// The header an index this store does not have yet is made with.
    fn header(&self) -> String {
        match self {
            Self::Workspace(name) => crate::workspaces::WS_MEMORY_HEADER.replace("{name}", name),
            // What a persona's `remember` makes one with: it scaffolds no header of its own.
            Self::Persona(_) | Self::Shared => "# Memory Index\n\n".to_owned(),
        }
    }

    /// The store's directory on `plane`, when the plane has this scope: a workspace that is
    /// there, a persona of the plane's, or `_shared`. `InvalidInput` for a name that is not
    /// one, `NotFound` for one the plane does not have; `PermissionDenied` for a store a link
    /// takes out of the project.
    pub fn dir(&self, plane: &Plane) -> io::Result<PathBuf> {
        let named = |e: crate::workspaces::NameError| {
            io::Error::new(io::ErrorKind::InvalidInput, e.to_string())
        };
        let dir = match self {
            Self::Workspace(name) => {
                let ws = plane.workspace(name).map_err(named)?;
                if !ws.dir().is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("no workspace '{name}'"),
                    ));
                }
                ws.dir().join("memory")
            }
            Self::Persona(name) => {
                let persona = plane.persona(name).map_err(named)?;
                // The plane's personas, which `_shared` is not one of: its store is a scope of
                // its own.
                if !plane.personas()?.iter().any(|one| one == name) {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("no persona '{name}'"),
                    ));
                }
                persona.dir().join("memory")
            }
            Self::Shared => plane.root().join("personas").join(SHARED).join("memory"),
        };
        crate::contain::writable(plane.root(), &dir).map_err(|refused| {
            io::Error::new(io::ErrorKind::PermissionDenied, refused.to_string())
        })?;
        Ok(dir)
    }
}

/// Move the memory `slug` (its exact name) from `from` to `to` on `plane`; the path it is at
/// now. Its file is renamed, its text kept — title and stamp — and its name is its slug in a
/// persona's store and `<its stamp>-<slug>` in a journal ([`memstore::move_one`]). `now` stamps
/// the name of a memory with no stamp line moved into a journal.
///
/// Moved out of a journal and back, a memory has its first name again **to the minute**: its
/// stamp line holds minutes, so the seconds of its first name come back as `00`
/// ([`memstore::move_one`]).
///
/// Refused, with nothing moved: the same scope twice (`InvalidInput`), a scope the plane does
/// not have ([`Scope::dir`]), a target holding a memory of that name (`AlreadyExists`), and a
/// store charter may not write.
pub fn move_memory(
    plane: &Plane,
    from: &Scope,
    slug: &str,
    to: &Scope,
    now: chrono::NaiveDateTime,
) -> io::Result<PathBuf> {
    if from == to {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{slug}' is in {} already", from.said()),
        ));
    }
    let source = from.dir(plane)?;
    let target = to.dir(plane)?;
    memstore::move_one(
        plane.root(),
        &source,
        slug,
        &target,
        to.timestamped(),
        &to.header(),
        now,
    )
}

/// The scopes of `plane` a memory can be moved to, in the order a picker lists them: every
/// workspace, every persona, then shared memory.
pub fn scopes(plane: &Plane) -> io::Result<Vec<Scope>> {
    let mut out: Vec<Scope> = plane
        .workspaces()?
        .into_iter()
        .filter(|name| crate::contain::workspace_name_ok(name))
        .map(Scope::Workspace)
        .collect();
    out.extend(
        plane
            .personas()?
            .into_iter()
            .filter(|name| crate::contain::persona_name_ok(name))
            .map(Scope::Persona),
    );
    out.push(Scope::Shared);
    Ok(out)
}

/// The exact name a slug typed on the command line names in `scope`'s store — an exact name,
/// else the one file whose name ends `-<slug>.md` ([`memstore::typed_name`]) — or the slug
/// itself when nothing matches, so the move answers for it.
pub fn typed(plane: &Plane, scope: &Scope, slug: &str) -> io::Result<String> {
    let dir = scope.dir(plane)?;
    match memstore::typed_name(plane.root(), &dir, slug, memstore::Typed::Stored) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(slug.to_owned()),
        answered => answered,
    }
}
