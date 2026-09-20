//! The control plane: the directory whose `charter.toml` everything else hangs off.

use std::path::{Path, PathBuf};

/// The file that marks a directory as a plane root.
pub const MANIFEST: &str = "charter.toml";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlaneError {
    #[error("no {MANIFEST} in {0} or any directory above it")]
    NotFound(PathBuf),
}

/// Finds the plane that `start` sits in: the nearest directory at or above it holding
/// `charter.toml`.
pub fn find_root(start: &Path) -> Result<PathBuf, PlaneError> {
    start
        .ancestors()
        .find(|dir| dir.join(MANIFEST).is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| PlaneError::NotFound(start.to_path_buf()))
}

/// The plane a process should act on: `$CHARTER_ROOT` if it is set, else the nearest
/// `charter.toml` at or above `start`.
///
/// The variable wins, as it does in Python charter, because it is how a caller PINS a plane
/// rather than inheriting whichever one its working directory happens to sit in — which is
/// what a test, a hook and a launched chat all need.
pub fn resolve(start: &Path) -> Result<PathBuf, PlaneError> {
    match std::env::var_os("CHARTER_ROOT") {
        Some(root) if !root.is_empty() => Ok(PathBuf::from(root)),
        _ => find_root(start),
    }
}

/// Where `charter init` and `charter reinit` act, and whether a plane is already there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The directory the command writes into, resolved.
    pub root: PathBuf,
    /// Whether a `charter.toml` marks it — Python's `config.HAS_CONTROL_PLANE`.
    pub is_plane: bool,
}

/// `charter/root.py:find_root_or_cwd`, for the two commands that may run where no plane is.
///
/// **Not [`resolve`], and the difference is the point.** [`resolve`] takes `$CHARTER_ROOT`
/// on trust; Python takes it only when it names a directory holding a `charter.toml` — and
/// when it does not, `init` falls back to the working directory rather than scaffolding
/// wherever the variable points. A stale `$CHARTER_ROOT` inherited from another shell would
/// otherwise put a plane into a directory the operator is not standing in.
///
/// The walk up from `cwd` hops OUTWARD through an enclosing plane's `workspaces/`
/// (`_outermost`), so a clone of a plane inside a workspace resolves to the plane that holds
/// it — the answer every other charter command gives there.
///
/// **One step of Python's walk is not ported:** a linked git worktree of a plane resolves,
/// in Python, to the plane in its MAIN worktree (`_plane_of`, `main_worktree_of`). Here the
/// worktree's own `charter.toml` answers, so `init` standing in a worktree of a plane
/// reports on the worktree's files.
pub fn place(cwd: &Path) -> Place {
    if let Some(named) = std::env::var_os("CHARTER_ROOT").filter(|v| !v.is_empty()) {
        let named = expand_user(Path::new(&named));
        if let Ok(root) = named.canonicalize()
            && root.join(MANIFEST).is_file()
        {
            return Place {
                root,
                is_plane: true,
            };
        }
    }
    let here = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    if let Some(found) = here.ancestors().find(|d| d.join(MANIFEST).is_file()) {
        return Place {
            root: outermost(found),
            is_plane: true,
        };
    }
    Place {
        root: here,
        is_plane: false,
    }
}

/// `~` and `~/…` against `$HOME`, as `Path.expanduser` reads them. `~user` is left alone.
///
/// `pub(crate)` because `doctor` reads the same `~` out of a committed `[plane] worktrees`,
/// and one spelling of "what does a tilde mean here" is the point of having one at all.
pub(crate) fn expand_user(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if text == "~" {
        return home().unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = text.strip_prefix("~/")
        && let Some(home) = home()
    {
        return home.join(rest);
    }
    path.to_path_buf()
}

/// The plane a process should ACT on, the way `charter/config.py` resolves it: the pinning
/// variable, else the nearest marker above `cwd`, redirected out of a linked worktree
/// ([`plane_of`]) and outward through any enclosing plane's `workspaces/` (`outermost`).
///
/// **Not [`resolve`], and the difference is deliberate.** [`resolve`] stops at the nearest
/// marker, which is right for the commands that only read the plane they are standing in. The
/// commands that COMMIT the plane — `save`, `git-policy` — have to agree with the vault, the
/// personas and the memory about which plane that is, or they act on one tree while every
/// other command acts on another. That disagreement is charter #806 and #809, and the two
/// refusals `save` carries are only reachable once this resolution is the one it uses.
pub fn command_root(cwd: &Path) -> Result<PathBuf, PlaneError> {
    if let Some(root) = std::env::var_os("CHARTER_ROOT").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(root));
    }
    let here = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let marked = here
        .ancestors()
        .find(|dir| dir.join(MANIFEST).is_file())
        .ok_or_else(|| PlaneError::NotFound(cwd.to_path_buf()))?;
    Ok(outermost(&plane_of(marked)))
}

/// Where charter keeps this plane's machine-local state — Python's `config.STATE_DIR`.
///
/// `$CHARTER_HOME` is taken verbatim, as Python takes it: an operator points it at a shared
/// directory to keep one vault and one state across several clones. It is here rather than in
/// either caller because `save` WRITES the push record and `doctor` READS it, and a state
/// directory the two disagree about is a record written where nothing looks for it.
pub fn state_dir(root: &Path) -> PathBuf {
    match std::env::var_os("CHARTER_HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home),
        _ => root.join(".charter"),
    }
}

/// `root.py:_plane_of`: the plane a found marker really belongs to.
///
/// `charter.toml` is a tracked file, so when the repo IS a plane every linked worktree cut
/// from it gets its own copy checked out and looks like a control plane of its own. Identity
/// follows the MAIN working tree — otherwise personas, the vault and every written memory
/// resolve into a directory `git worktree remove` deletes — and only when the marker is
/// present there too.
pub fn plane_of(marked: &Path) -> PathBuf {
    match main_worktree_of(marked) {
        Some(main) if main.join(MANIFEST).is_file() => main,
        _ => marked.to_path_buf(),
    }
}

/// The MAIN working tree behind `tree`, when `tree` is a linked worktree — else `None`.
///
/// A linked worktree's `.git` is a FILE reading `gitdir: <main>/.git/worktrees/<name>`, so the
/// main tree is the directory holding that `.git`. Pure path arithmetic, no subprocess,
/// because this sits under every command's plane resolution.
///
/// `None` for the main tree itself (`.git` is a directory), for a non-repo, and for a gitdir
/// with no `.git` component — a worktree of a BARE repo has no working tree to redirect to, so
/// the caller keeps what it had.
pub fn main_worktree_of(tree: &Path) -> Option<PathBuf> {
    let dot = tree.join(".git");
    let meta = std::fs::symlink_metadata(&dot).ok()?;
    if meta.is_dir() {
        return None;
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let named = text.trim().strip_prefix("gitdir:")?.trim();
    let mut path = PathBuf::from(named);
    if !path.is_absolute() {
        path = tree.join(path);
    }
    let path = path.canonicalize().ok()?;
    path.ancestors()
        .find(|a| a.file_name().is_some_and(|n| n == ".git"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// `root.py:_outermost`: follow `workspaces/` nesting outward until no plane encloses this
/// one. A plane is enclosed only through ANOTHER plane's own `workspaces/`, so a stray
/// `charter.toml` further up never swallows the planes beneath it.
pub(crate) fn outermost(marked: &Path) -> PathBuf {
    let mut seen = vec![marked.to_path_buf()];
    let mut cur = marked.to_path_buf();
    while let Some(outer) = enclosing(&cur) {
        if seen.contains(&outer) {
            break;
        }
        seen.push(outer.clone());
        cur = outer;
    }
    cur
}

/// `root.py:enclosing_plane`: the plane whose `workspaces/` contains `root`, or `None`.
///
/// `pub(crate)` for `doctor`s `nested plane` row, which asks this of the plane a command
/// resolved, and for `planegit`s refusal to commit a plane the caller is not standing in:
/// several walks for one question is how several answers about one directory arrive.
pub(crate) fn enclosing(root: &Path) -> Option<PathBuf> {
    let here = root.canonicalize().ok()?;
    here.ancestors().skip(1).find_map(|parent| {
        if !parent.join(MANIFEST).is_file() {
            return None;
        }
        let spaces = parent.join("workspaces").canonicalize().ok()?;
        here.starts_with(&spaces).then(|| parent.to_path_buf())
    })
}

#[cfg(test)]
mod place_tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_clone_of_a_plane_inside_a_workspace_resolves_to_the_plane_that_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().canonicalize().unwrap();
        fs::write(outer.join(MANIFEST), "").unwrap();
        let inner = outer.join("workspaces/ide/charter");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join(MANIFEST), "").unwrap();

        assert_eq!(outermost(&inner), outer);
    }

    #[test]
    fn a_plane_below_a_stray_marker_that_does_not_hold_it_in_workspaces_stands() {
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().canonicalize().unwrap();
        fs::write(outer.join(MANIFEST), "").unwrap();
        let inner = outer.join("projects/acme");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join(MANIFEST), "").unwrap();

        assert_eq!(outermost(&inner), inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_directory_holding_the_manifest_is_its_own_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), "").unwrap();

        assert_eq!(find_root(dir.path()), Ok(dir.path().to_path_buf()));
    }

    #[test]
    fn a_clone_deep_inside_a_workspace_finds_the_plane_above_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), "").unwrap();
        let clone = dir.path().join("workspaces/ide/charter-app/src");
        fs::create_dir_all(&clone).unwrap();

        assert_eq!(find_root(&clone), Ok(dir.path().to_path_buf()));
    }

    #[test]
    fn the_nearest_manifest_wins_over_one_further_up() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), "").unwrap();
        let nested = dir.path().join("inner");
        fs::create_dir_all(nested.join("deeper")).unwrap();
        fs::write(nested.join(MANIFEST), "").unwrap();

        assert_eq!(find_root(&nested.join("deeper")), Ok(nested));
    }

    #[test]
    fn a_directory_with_no_manifest_above_it_is_not_a_plane() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            find_root(dir.path()),
            Err(PlaneError::NotFound(dir.path().to_path_buf()))
        );
    }
}
