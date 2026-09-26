//! `nested plane` and `front door`: which plane answered, and whose identity it opens with.

use std::path::{Path, PathBuf};

use super::{Config, Doctor, NOT_CHECKED_HINT, Row, canonical, short_path};
use crate::plane::MANIFEST;

/// The plane whose `workspaces/` contains `root`, or `None`.
///
/// [`crate::plane::enclosing`], not a second walk of the same question: `place` asks it when
/// it hops outward for `init`, and two answers about one directory is the drift this repo
/// keeps paying for.
fn enclosing_plane(root: &Path) -> Option<PathBuf> {
    crate::plane::enclosing(root)
}

/// `root.standing_in_nested_plane`: the nested plane `cwd` stands in, when something other
/// than it answered — else `None`.
fn standing_in_nested_plane(cwd: &Path) -> Option<PathBuf> {
    let cur = canonical(cwd);
    let inner = cur
        .ancestors()
        .find(|d| d.join(MANIFEST).is_file())?
        .to_path_buf();
    enclosing_plane(&inner).map(|_| inner)
}

/// `nested plane`: is the plane charter acts on sitting inside ANOTHER plane's
/// `workspaces/` (#140)? Standing in one, every command operates on the inner plane — its own
/// vault registry, workspace pointers and `workspaces/` — and nothing says so.
///
/// **The gap this row was written around is closed, and the last arm below still describes
/// it.** When M2.5 wrote that arm, [`crate::plane::resolve`] stopped at the nearest
/// `charter.toml` while Python's `find_root` hopped outward, so standing in a nested clone was
/// the pinned case with nobody having pinned anything and the row had to say so. M2.9 gave
/// `resolve` the outward hop and M2.16 gave it the worktree redirect, so `enclosing_plane` of
/// the plane this binary resolved is now `None` by construction unless `$CHARTER_ROOT` put it
/// there — which makes that arm unreachable rather than wrong, and its words about "does not
/// hop outward" are no longer true of this binary.
///
/// Left standing on purpose: deleting a match arm is not a row's behaviour changing, it is a
/// row losing a case nobody re-derived, and which sentence an operator should read when a
/// resolver lands them inside a nested plane is its own ticket.
pub(super) fn nested(d: &Doctor) -> Row {
    const NAME: &str = "nested plane";
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    match enclosing_plane(&d.root) {
        None => match standing_in_nested_plane(&d.cwd) {
            Some(origin) if origin != d.root => Row::ok(
                NAME,
                format!(
                    "standing in {}, acting on {}",
                    short_path(&origin),
                    short_path(&d.root)
                ),
            ),
            _ => Row::ok(NAME, "not nested"),
        },
        Some(outer) if d.pinned => Row::warn(
            NAME,
            format!("pinned inside {}'s workspaces/", short_path(&outer)),
            format!(
                "$CHARTER_ROOT points at a plane nested in another one, so vaults and \
                 workspace pointers go to the inner plane and the outer never sees them. \
                 Without the override charter would resolve to {}.  → unset CHARTER_ROOT to \
                 use it",
                short_path(&outer)
            ),
        ),
        Some(outer) => Row::warn(
            NAME,
            format!("standing inside {}'s workspaces/", short_path(&outer)),
            format!(
                "This charter resolves the nearest charter.toml and does not hop outward \
                 through an enclosing plane's workspaces/ to {}, so it acts on this inner \
                 plane — its own vaults, personas and workspace pointers.  → run from {} or set \
                 CHARTER_ROOT to choose one on purpose",
                short_path(&outer),
                short_path(&outer)
            ),
        ),
    }
}

/// The bound on the one file this row reads, as plane data is bounded.
const LEGACY_LIMIT: u64 = 1_048_576;

/// `front door`: the plane's declared default persona still names a persona that exists.
///
/// Both rungs that declare one — `[persona] default` and the legacy `personas/.default` —
/// resolve to no identity when the persona was renamed or deleted. That is right, and it used
/// to be the whole response: the plane silently lost its front door. WARN, never FAIL: a
/// plane with no persona still clones, still reaches its forge, still runs.
pub(super) fn front_door(d: &Doctor) -> Row {
    const NAME: &str = "front door";
    let cfg = match &d.config {
        Config::Read(cfg) => cfg,
        Config::Malformed(why) | Config::Refused(why) => return Row::not_checked(NAME, why),
    };
    let declared = match cfg.get("persona") {
        None => String::new(),
        Some(v) if !super::config::truthy(v) => String::new(),
        Some(toml::Value::Table(section)) => section
            .get("default")
            .map(|v| crate::memstore::py_strip(&crate::profiles::py_str(v)).to_owned())
            .unwrap_or_default(),
        Some(other) => {
            return Row::warn(
                NAME,
                format!(
                    "not checked ('{}' object has no attribute 'get')",
                    super::config::py_type(other)
                ),
                NOT_CHECKED_HINT,
            );
        }
    };
    let personas = d.root.join("personas");
    let legacy = personas.join(".default");
    let legacy_name = match std::fs::metadata(&legacy) {
        Err(_) => String::new(),
        Ok(meta) => {
            // Gated as plane data is: a committed link out of the plane is not read, and a
            // FIFO or a directory there is not a name.
            if let Err(refused) = crate::contain::readable(&d.root, &legacy) {
                return Row::not_checked(NAME, refused);
            }
            // A directory is left to the read below, which refuses it with the errno Python
            // quotes; anything else that is not a file would block that read or never end it.
            if !meta.is_dir() && (!meta.is_file() || meta.len() > LEGACY_LIMIT) {
                return Row::not_checked(
                    NAME,
                    format!(
                        "{} is not a file charter reads",
                        super::fsx::path_field(&legacy)
                    ),
                );
            }
            match std::fs::read_to_string(&legacy) {
                Ok(text) => crate::memstore::py_strip(&text).to_owned(),
                Err(e) => return Row::not_checked(NAME, super::fsx::py_os_error(&e, &legacy)),
            }
        }
    };
    // `charter.toml` first: it is the rung that wins, so its breakage is the one that costs
    // the plane its identity.
    for (value, place) in [
        (&declared, "charter.toml [persona] default"),
        (&legacy_name, "personas/.default"),
    ] {
        if value.is_empty() {
            continue;
        }
        if is_persona(&personas, value) {
            return Row::ok(NAME, format!("'{value}' via {place}"));
        }
        return Row::warn(
            NAME,
            format!(
                "{place} names '{value}', which is not a persona — this plane has no front door \
                 and every session starts with no identity"
            ),
            format!("charter persona default <name>  (or `charter persona create {value}`)"),
        );
    }
    let others = super::memory::list_personas(&d.root)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| !n.starts_with('_'))
        .count();
    if others > 0 {
        return Row {
            name: NAME.to_owned(),
            status: super::Status::Ok,
            detail: format!(
                "none declared — {others} persona(s) exist, and a session started with none \
                 of them has no identity"
            ),
            hint: "charter persona default <name>".to_owned(),
        };
    }
    Row::ok(NAME, "none declared")
}

/// `routing:` in a persona's frontmatter, which is retired (charter#369): read without error,
/// acted on by nothing, and said so here. Personas reach the harness as sub-agents, which is
/// where a request is routed now.
///
/// **No row where no persona declares it**, and green where one does: a plane the Python
/// scaffolded carries `routing: advise` on its front door, and a yellow row on every such plane
/// would be a warning about a line that does no harm. The row is there so the key is not
/// silently meaningless.
pub(super) fn routing(d: &Doctor) -> Option<Row> {
    let declaring: Vec<String> = crate::personagrant::list_personas(&d.root)
        .into_iter()
        .filter(|name| {
            crate::personas::load(&d.root, name)
                .is_some_and(|pairs| pairs.iter().any(|(key, _)| key == "routing"))
        })
        .collect();
    (!declaring.is_empty()).then(|| {
        Row::ok(
            "routing",
            format!(
                "ignored — `routing:` is retired; personas are offered to the harness as \
                 sub-agents (declared by {})",
                declaring.join(", ")
            ),
        )
    })
}

/// `persona.def_path(name).exists()`: the directory layout or the legacy flat file.
///
/// Asked only of a name that can name one entry in `personas/` — a committed `default =
/// "../elsewhere"` is not a persona, and joining it would ask the filesystem about a path
/// outside the directory this is a question about.
fn is_persona(personas: &Path, name: &str) -> bool {
    crate::contain::segment_ok(name)
        && (personas.join(name).join("persona.md").exists()
            || personas.join(format!("{name}.md")).exists())
}
