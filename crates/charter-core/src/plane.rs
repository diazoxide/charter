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
/// `charter.toml`, then **outward** through any enclosing plane's `workspaces/`.
///
/// **The hop is the whole of `charter/root.py:find_root`'s second half, and leaving it out
/// was a defect two reviews found independently.** `charter.toml` is a tracked file, so every
/// clone of a plane is itself a plane — and `charter clone` puts clones at
/// `workspaces/<ws>/<repo>`. Stopping at the nearest marker means that, standing in one,
/// `charter root`, `charter ws remember` and `charter sync` all acted on the INNER plane:
/// different personas, no vault, a memory written into the cloned repo's git index instead of
/// the operator's plane (charter#200). [`place`] already hopped — it is `find_root_or_cwd`'s
/// port and has carried [`outermost`] since it was written — so `charter init` and
/// `charter reinit` answered one plane and every other command answered another, in the same
/// directory. One ladder, one answer.
///
/// **Not "outermost marker wins".** The hop is allowed only through an enclosing plane's own
/// `workspaces/`, so a stray `charter.toml` in `~` never swallows the planes beneath it.
///
/// **One step of Python's walk is still not taken here**, and it is the one
/// [`command_root`] adds: a linked git worktree of a plane resolves, in Python, to the plane
/// in its MAIN worktree ([`plane_of`]). Here the worktree's own `charter.toml` answers. Left
/// alone deliberately — giving every command that redirect would move where a chat in a
/// worktree writes its memory, which is a behaviour change with its own tests to write, and
/// the commands that genuinely need it already ask [`command_root`].
pub fn find_root(start: &Path) -> Result<PathBuf, PlaneError> {
    marked_above(start)
        .map(outermost)
        .ok_or_else(|| PlaneError::NotFound(start.to_path_buf()))
}

/// The nearest directory at or above `start` holding a `charter.toml`, or `None`.
///
/// **One walk, asked by both resolvers.** [`find_root`] and [`command_root`] differ by a
/// single step ([`plane_of`]) and by nothing else; writing the walk twice is how two
/// functions that must agree about a directory come to disagree about it — which is the
/// defect M2.9 fixed between [`find_root`] and [`place`] two milestones after it appeared.
///
/// `is_file`, never `exists`: a DIRECTORY called `charter.toml` is not a marker.
fn marked_above(start: &Path) -> Option<&Path> {
    start.ancestors().find(|dir| dir.join(MANIFEST).is_file())
}

/// The plane a process should act on: `$CHARTER_ROOT` if it is set, else [`find_root`] from
/// `start` — the nearest `charter.toml` at or above it, hopped outward through any enclosing
/// plane's `workspaces/`.
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
/// **Not [`resolve`], and since M2.9 the difference is exactly one step: [`plane_of`].**
///
/// This paragraph said "[`resolve`] stops at the nearest marker" when M2.5 wrote it, and that
/// was true of the day's code and is no longer: M2.9 gave [`find_root`] the outward hop it was
/// missing, because `place` had always had it and the two answered differently in the same
/// directory (charter#200). Both walks now share [`marked_above`] and [`outermost`]; what is
/// left here is the redirect out of a linked WORKTREE, which every other command deliberately
/// does not take.
///
/// It belongs to these two because the commands that COMMIT the plane — `save`, `git-policy` —
/// have to agree with the vault, the personas and the memory about which plane that is, or
/// they act on one tree while every other command acts on another. That disagreement is
/// charter #806 and #809, and the two refusals `save` carries are only reachable once this
/// resolution is the one it uses.
pub fn command_root(cwd: &Path) -> Result<PathBuf, PlaneError> {
    if let Some(root) = std::env::var_os("CHARTER_ROOT").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(root));
    }
    // Resolved first, where [`find_root`] takes the caller's path as it stands: `save` reports
    // the tree it is about to commit, and a path with a link in it names that tree by a route
    // `git` will not echo back.
    let here = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    marked_above(&here)
        .map(|marked| outermost(&plane_of(marked)))
        .ok_or_else(|| PlaneError::NotFound(cwd.to_path_buf()))
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

// --------------------------------------------------------------------------------------- //
// writing charter's own state                                                               //
// --------------------------------------------------------------------------------------- //

/// Create a directory under charter's own state at 0700 — `charter/config.py:private_mkdir`.
///
/// **The umask must not decide the mode of the plane's state directory.** A plain
/// `create_dir_all` makes each level at `0o777 & ~umask`, which on the default `umask 022` is
/// 0755 — and `.charter/` is the directory the vault registry lives in. Rust's `DirBuilder`
/// applies its mode to every level it creates, which is the part CPython's `pathlib` does not.
///
/// A directory that already exists is left exactly as it is, which is Python's rule too:
/// charter tightens what it creates and reports what it did not, because `$CHARTER_HOME` can
/// point the state directory at a home or a shared team directory.
pub fn private_dir(plane: &Path, dir: &Path) -> std::io::Result<()> {
    crate::contain::no_link_on_the_way(plane, dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Write charter's own state at 0600, settling the mode on the **inode** before any content
/// reaches it — `charter/config.py:_private_fd`, including its ordering.
///
/// `OpenOptions::mode` applies **only when the call creates the inode**, so a file written by
/// an older charter, restored from a tarball or made by hand keeps whatever mode it had and
/// every byte written after it sits at that mode. The permission is therefore set on the
/// descriptor this call holds.
///
/// **`O_TRUNC` is deliberately not in the flags.** Truncating first would empty the file while
/// it is still at its old mode; the truncate happens after, so there is no window in which new
/// content is readable by an account the finished file is not.
///
/// A failed chmod is swallowed rather than raised, as Python's is: filesystems with fixed
/// permissions (exFAT, many network mounts) cannot hold a mode, and refusing to write state to
/// protect a mode the filesystem was never going to keep helps nobody.
pub fn write_private(plane: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    crate::contain::no_link_on_the_way(plane, path)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = crate::contain::nofollow(&mut options).open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    file.set_len(0)?;
    file.write_all(bytes)
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
    fn a_clone_that_is_itself_a_plane_resolves_to_the_plane_holding_it() {
        // `charter.toml` is tracked, so a clone of a plane carries one. Stopping at the
        // nearest marker acted on the inner plane — different personas, no vault, memory
        // written into the clone's git index (charter#200). `place` already hopped; this is
        // the same ladder, and the two answered differently in the same directory.
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().canonicalize().unwrap();
        fs::write(outer.join(MANIFEST), "").unwrap();
        let inner = outer.join("workspaces/ide/charter");
        fs::create_dir_all(inner.join("crates/src")).unwrap();
        fs::write(inner.join(MANIFEST), "").unwrap();

        assert_eq!(find_root(&inner.join("crates/src")), Ok(outer.clone()));
        // The same answer `place` has always given here, which is the point of the fix: the
        // two entry points disagreed in this directory. Asked through `outermost` rather than
        // `place` because `place` reads `$CHARTER_ROOT`, and a test must not depend on the
        // environment the suite happens to run in.
        assert_eq!(outermost(&inner), outer);
    }

    #[test]
    fn a_stray_marker_above_a_plane_that_does_not_hold_it_in_workspaces_never_swallows_it() {
        // "Outermost marker wins" would let one `charter.toml` in `~` take every plane
        // beneath it. The hop is only ever through an enclosing plane's own `workspaces/`.
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().canonicalize().unwrap();
        fs::write(outer.join(MANIFEST), "").unwrap();
        let inner = outer.join("projects/acme");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join(MANIFEST), "").unwrap();

        assert_eq!(find_root(&inner), Ok(inner));
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
