//! Whether a string may name one entry inside a directory.
//!
//! charter mints its own workspace and persona names, so a value that does not match the
//! alphabet below cannot name one it produced — which is why a name read off disk, out of a
//! manifest, or off the command line is re-checked **before it is joined onto a path**.
//! `Path::join` throws the prefix away when handed an absolute path, and `..` walks out of
//! the plane, so a name that is not checked is a write anywhere on the filesystem.
//!
//! Deliberately a question about the *string*, never about the disk: asking the filesystem
//! would make a traversal succeed exactly when the attacker's target happens to exist, which
//! is the one case where the answer must not change.

/// The separators no single name may contain, on any platform charter runs on.
///
/// Both, always — not the running platform's. A plane is committed and travels, so a name
/// that is one segment here and two somewhere else is the same defect either way.
const SEPARATORS: [char; 2] = ['/', '\\'];

/// Could `name` name one entry inside some directory?
pub fn segment_ok(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    // A NUL terminates the string inside the C library, so the name charter checked and the
    // name the kernel opened would be two different strings.
    if name.contains('\0') {
        return false;
    }
    if name.contains(SEPARATORS) {
        return false;
    }
    // A Windows drive-qualified name (`C:x`) is rooted without starting with a separator.
    if std::path::Path::new(name).is_absolute() || drive_qualified(name) {
        return false;
    }
    true
}

fn drive_qualified(name: &str) -> bool {
    let mut chars = name.chars();
    matches!((chars.next(), chars.next()), (Some(c), Some(':')) if c.is_ascii_alphabetic())
}

/// Can `name` name a workspace this plane contains?
///
/// Containment first, then the alphabet: `^[A-Za-z0-9][A-Za-z0-9._-]*$`. The alphabet is the
/// right rule because charter mints these names itself.
pub fn workspace_name_ok(name: &str) -> bool {
    segment_ok(name) && alphabet_ok(name)
}

/// The one name under `personas/` that is not a persona: the store every persona reads.
pub const SHARED_PERSONA: &str = "_shared";

/// Can `name` name a persona?
///
/// **Lowercase only** — `charter/persona.py:47` is `^[a-z0-9][a-z0-9._-]*$`, a tighter
/// alphabet than a workspace's, and `Alpha` or `DevOps` is not a persona charter would
/// mint. On a case-insensitive filesystem accepting `DevOps` would reach `devops`'s files
/// through a name the plane does not have.
///
/// `_shared` is admitted by name and nothing else is: Python never validates it, reaching
/// that store through a `shared=True` flag instead, so this is where the two models meet.
pub fn persona_name_ok(name: &str) -> bool {
    if name == SHARED_PERSONA {
        return true;
    }
    segment_ok(name) && lowercase_alphabet_ok(name)
}

/// `^[A-Za-z0-9][A-Za-z0-9._-]*$`, hand-rolled rather than pulling in a regex engine for one
/// rule that never changes.
fn alphabet_ok(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// `^[a-z0-9][a-z0-9._-]*$` — a persona's alphabet, which admits no capital.
fn lowercase_alphabet_ok(name: &str) -> bool {
    let lower = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if lower(c) => {}
        _ => return false,
    }
    chars.all(|c| lower(c) || c == '-' || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_name_is_one_entry() {
        assert!(segment_ok("alpha"));
        assert!(segment_ok("my-repo.v2"));
    }

    #[test]
    fn nothing_that_walks_out_of_a_directory_is_one_entry() {
        for name in ["", ".", "..", "../alpha", "a/b", "a\\b", "/abs", "/", "C:x"] {
            assert!(!segment_ok(name), "{name:?} must not name one entry");
        }
    }

    #[test]
    fn a_name_holding_a_nul_is_refused_because_the_kernel_would_see_a_shorter_one() {
        assert!(!segment_ok("alpha\0evil"));
    }

    #[test]
    fn a_workspace_name_is_a_contained_name_in_charters_own_alphabet() {
        assert!(workspace_name_ok("alpha"));
        assert!(workspace_name_ok("alpha.2"));
        assert!(workspace_name_ok("a_b-c"));
        for name in [
            "../escape",
            "/abs",
            "-leading",
            ".hidden",
            "_shared",
            "a b",
            "é",
        ] {
            assert!(
                !workspace_name_ok(name),
                "{name:?} must not name a workspace"
            );
        }
    }

    #[test]
    fn a_persona_name_admits_no_capital_and_only_shared_leads_with_an_underscore() {
        // `charter/persona.py:47` is `^[a-z0-9][a-z0-9._-]*$`. On a case-insensitive
        // filesystem `DevOps` would otherwise reach `devops`'s files under a name the plane
        // does not have.
        assert!(persona_name_ok("devops"));
        assert!(persona_name_ok("dev-ops.2"));
        assert!(
            persona_name_ok("_shared"),
            "admitted by name, and only this one"
        );
        for name in [
            "Alpha",
            "ALPHA",
            "DevOps",
            "CON",
            "_a",
            "__evil",
            "_SHARED",
            "_",
            "A",
            "../escape",
            "/abs",
            "a/b",
            "",
        ] {
            assert!(!persona_name_ok(name), "{name:?} must not name a persona");
        }
    }
}

/// Where a plane keeps data charter writes. A path that resolves outside all of them is
/// refused, however legal its name.
/// The directories a control plane keeps data in, as `charter/contain.py:data_roots` lists
/// them: `personas/`, `workspaces/` and `.charter/persona-state`.
///
/// **`persona-state` and not `.charter`.** Ephemeral persona memory is data charter is
/// supposed to read and it lives under the secrets home, which is the whole reason this is a
/// list of data directories rather than "the plane, minus `.charter/`". Allowing `.charter`
/// wholesale would put the vaults and every other piece of plane state inside the allowlist.
const DATA_DIRS: [&str; 3] = ["personas", "workspaces", ".charter/persona-state"];

/// A write charter refused, with the reason the operator sees.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error(
    "'{path}' resolves to '{resolved}', outside the directories a control plane keeps its \
     data in (persona-state, personas, workspaces). A committed symlink there redirects the \
     {verb}, so charter follows a link that lands inside them and refuses one that leaves"
)]
pub struct Refused {
    pub path: String,
    pub resolved: String,
    /// `read` or `write` — the same refusal, named for what it stopped.
    pub verb: &'static str,
}

/// Check that `path` still lands inside `root`'s data directories once every symlink on the
/// way is followed.
///
/// The name rule alone is not containment. A **committed** symlink at
/// `workspaces/<legal-name>` travels with the plane to every machine that clones it, and
/// pointing it out of the plane redirects every write to that workspace. Python refuses this
/// in `contain.writable`; the name check cannot see it, because the name is fine.
///
/// The path itself need not exist — charter creates workspaces and stores on demand — so the
/// deepest ancestor that DOES exist is resolved and the rest appended. That is the part a
/// symlink can lie about; the remainder is names charter is about to create.
pub fn writable(root: &std::path::Path, path: &std::path::Path) -> Result<(), Refused> {
    contained(root, path, "write")
}

/// The same check before a READ.
///
/// charter gates its reads too, for a reason the write side does not cover: a committed
/// `workspaces/evil -> ../../elsewhere` with a legal name made `workspace vision` PRINT a
/// file from outside the plane (charter #442). Containing the name does not contain that —
/// the name was never the wrong part.
pub fn readable(root: &std::path::Path, path: &std::path::Path) -> Result<(), Refused> {
    contained(root, path, "read")
}

fn contained(
    root: &std::path::Path,
    path: &std::path::Path,
    verb: &'static str,
) -> Result<(), Refused> {
    let resolved = resolve_existing(path);
    let base = resolve_existing(root);
    let inside = DATA_DIRS
        .iter()
        .map(|dir| base.join(dir))
        .any(|allowed| resolved.starts_with(&allowed));
    if inside {
        Ok(())
    } else {
        Err(Refused {
            path: path.display().to_string(),
            resolved: resolved.display().to_string(),
            verb,
        })
    }
}

/// Whether `path` resolves anywhere inside `root`, both ends resolved.
///
/// Used where the plane's own state lives rather than its data directories. **Both ends**:
/// on macOS a temp plane is under `/var/folders/...`, itself a link to `/private/var/...`,
/// so comparing a resolved path against an unresolved root refuses everything.
pub fn within_plane(root: &std::path::Path, path: &std::path::Path) -> bool {
    resolve_existing(path).starts_with(resolve_existing(root))
}

/// `path` with its deepest existing ancestor canonicalised.
fn resolve_existing(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    let mut rest = Vec::new();
    let mut walk = path;
    while let Some(parent) = walk.parent() {
        if let Some(name) = walk.file_name() {
            rest.push(name.to_os_string());
        }
        if let Ok(real) = parent.canonicalize() {
            let mut out = real;
            for name in rest.iter().rev() {
                out.push(name);
            }
            return out;
        }
        walk = parent;
    }
    path.to_path_buf()
}

#[cfg(test)]
mod writable_tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        dir
    }

    #[test]
    fn a_path_inside_the_workspaces_is_writable() {
        let dir = plane();

        assert_eq!(
            writable(
                dir.path(),
                &dir.path().join("workspaces/alpha/workspace.md")
            ),
            Ok(())
        );
    }

    #[test]
    fn a_workspace_that_is_a_symlink_out_of_the_plane_is_refused() {
        // A COMMITTED symlink travels with the plane, so this is not a local mistake: it
        // redirects every write to that workspace on every machine that clones it.
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("workspaces/escape")).unwrap();

        let refusal = writable(
            dir.path(),
            &dir.path().join("workspaces/escape/workspace.md"),
        )
        .expect_err("a link that leaves the plane is refused");

        assert!(
            refusal.to_string().contains("outside the directories"),
            "{refusal}"
        );
    }

    #[test]
    fn a_symlink_that_lands_back_inside_the_plane_is_followed() {
        // charter follows a link that stays inside, and only refuses one that leaves.
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("workspaces/real")).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("workspaces/real"),
            dir.path().join("workspaces/alias"),
        )
        .unwrap();

        assert_eq!(
            writable(
                dir.path(),
                &dir.path().join("workspaces/alias/workspace.md")
            ),
            Ok(())
        );
    }

    #[test]
    fn a_path_outside_every_data_directory_is_refused() {
        let dir = plane();

        assert!(writable(dir.path(), &dir.path().join("docs/topology.md")).is_err());
        assert!(writable(dir.path(), std::path::Path::new("/etc/passwd")).is_err());
    }
}
