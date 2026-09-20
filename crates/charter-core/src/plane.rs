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
/// **The redirect out of a linked WORKTREE is taken here too, and it used to be taken by
/// `save` alone.** `charter.toml` is tracked, so every linked worktree cut from a plane gets
/// its own copy checked out and looks like a plane of its own; [`plane_of`] follows identity
/// back to the MAIN working tree. `save` and `git-policy` asked for that step through a second
/// resolver (`command_root`) and every other command did not, so — standing in a worktree that
/// an enclosing plane's `workspaces/` does not contain — `charter save` named one plane and
/// `charter ws remember` wrote its memory into another, which `git worktree remove` then
/// deletes (M2.16). **Python has no such split**: `charter/root.py:find_root` is the one
/// resolver every Python command reaches through `config.ROOT`, and it has always taken
/// `_plane_of`. So this is not a behaviour charter chose twice — it is the port catching up,
/// and `command_root` is gone rather than kept in step by hand.
///
/// `start` is resolved first, as Python resolves it (`(start or Path.cwd()).resolve()`): a
/// path with a link in it names a tree by a route `git` will not echo back, and `save` reports
/// the tree it is about to commit.
///
/// **One step of Python's walk is still not taken here.** When NO marker is found above
/// `start`, Python asks a second time whether `start` sits in a linked worktree whose main
/// tree has a plane above it — the case of a worktree cut from a branch that predates
/// `charter.toml`, which is not tracked until someone commits it. That fallback needs its own
/// scenarios and is not M2.16's; a worktree that carries the marker, which is every worktree
/// of a committed plane, is answered above.
pub fn find_root(start: &Path) -> Result<PathBuf, PlaneError> {
    let here = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    marked_above(&here)
        .map(|marked| outermost(&plane_of(marked)))
        .ok_or_else(|| PlaneError::NotFound(start.to_path_buf()))
}

/// The nearest directory at or above `start` holding a `charter.toml`, or `None`.
///
/// **One walk, asked by every resolver.** [`find_root`] and [`place`] differ only in what they
/// do when nothing is found; writing the walk twice is how two functions that must agree about
/// a directory come to disagree about it — which is the defect M2.9 fixed between [`find_root`]
/// and [`place`] two milestones after it appeared, and M2.16 fixed again between [`find_root`]
/// and the `command_root` that `save` used to ask.
///
/// `is_file`, never `exists`: a DIRECTORY called `charter.toml` is not a marker.
fn marked_above(start: &Path) -> Option<&Path> {
    start.ancestors().find(|dir| dir.join(MANIFEST).is_file())
}

/// The plane a process should act on: `$CHARTER_ROOT` if it is set, else [`find_root`] from
/// `start` — the nearest `charter.toml` at or above it, out of a linked worktree and outward
/// through any enclosing plane's `workspaces/`.
///
/// The variable wins, as it does in Python charter, because it is how a caller PINS a plane
/// rather than inheriting whichever one its working directory happens to sit in — which is
/// what a test, a hook and a launched chat all need.
///
/// **Every command asks this one, `save` and `git-policy` included.** There was a second
/// resolver (`command_root`) for exactly those two until M2.16, and the one step it added is
/// now [`find_root`]'s. Two functions whose whole contract is to name the same directory are
/// kept in step by being one function; the differential scenarios that prove it are
/// `remember-from-a-worktree-…` and `save-from-a-worktree-…`, which run the two commands from
/// the same directory and compare both answers against Python's.
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
/// The walk up from `cwd` is [`marked_above`] — the same one [`find_root`] takes, and asked
/// through the same two steps: out of a linked worktree (`_plane_of`) and outward through an
/// enclosing plane's `workspaces/` (`_outermost`). So a clone of a plane inside a workspace,
/// and a worktree cut from a plane, both resolve to the plane that holds them — the answer
/// every other charter command gives there. `init` used to be a third answer in the same
/// directory (M2.16); `charter init` standing in a worktree of a plane now reports on the
/// plane, which is what `charter doctor` standing beside it reports on.
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
    if let Some(found) = marked_above(&here) {
        return Place {
            root: outermost(&plane_of(found)),
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

/// The nested plane the caller is STANDING IN, when [`find_root`] redirected past it — else
/// `None`. `charter/root.py:standing_in_nested_plane`.
///
/// Without this the redirect is invisible. Once the resolver answers with the OUTER plane,
/// [`enclosing`] of that answer is `None` by construction — the outer plane is not nested —
/// so every surface that would name the nesting falls silent and charter acts, quietly, on a
/// plane the operator cannot see it choose. `charter status` is the surface that says it
/// (ADR 0013's second rule applies to charter's own corrections too).
///
/// Never raises: it is read on a render path. A directory that cannot be resolved, or a walk
/// that meets an unreadable parent, answers `None` — the same as "nothing nested here",
/// because a notice charter cannot substantiate is one it does not print.
pub fn standing_in_nested_plane(start: &Path) -> Option<PathBuf> {
    let here = start.canonicalize().ok()?;
    let marked = marked_above(&here)?;
    let inner = plane_of(marked);
    enclosing(&inner).map(|_| inner)
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

    /// A linked worktree of `main`, as git lays one out: a `.git` FILE naming the main tree's
    /// administrative directory, and the tracked `charter.toml` checked out into it.
    ///
    /// Built by hand rather than by `git worktree add`, because this function is pure path
    /// arithmetic on purpose ([`main_worktree_of`]) and a unit test that shells out to git
    /// would stop testing that. The differential scenarios run the real thing.
    fn worktree_of(main: &Path, name: &str, at: &Path) -> PathBuf {
        fs::create_dir_all(main.join(".git/worktrees").join(name)).unwrap();
        fs::create_dir_all(at).unwrap();
        fs::write(
            at.join(".git"),
            format!("gitdir: {}/.git/worktrees/{name}\n", main.display()),
        )
        .unwrap();
        fs::write(at.join(MANIFEST), "").unwrap();
        at.to_path_buf()
    }

    #[test]
    fn a_directory_holding_the_manifest_is_its_own_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join(MANIFEST), "").unwrap();

        assert_eq!(find_root(&root), Ok(root));
    }

    #[test]
    fn a_clone_deep_inside_a_workspace_finds_the_plane_above_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join(MANIFEST), "").unwrap();
        let clone = root.join("workspaces/ide/charter-app/src");
        fs::create_dir_all(&clone).unwrap();

        assert_eq!(find_root(&clone), Ok(root));
    }

    #[test]
    fn a_worktree_cut_from_a_plane_resolves_to_the_plane_it_was_cut_from() {
        // M2.16. `charter.toml` is tracked, so the worktree carries one and reads as a plane
        // of its own. Standing here, `save` resolved to the main tree and every other command
        // resolved to this directory — so `charter ws remember` wrote a memory into a tree
        // `git worktree remove` deletes, while `charter save` said it was committing another.
        // Placed OUTSIDE the plane's `workspaces/`, because `outermost` already hops out of
        // charter's own `workspaces/<ws>/.worktrees/<name>` layout and would hide the step
        // this test is about.
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().canonicalize().unwrap().join("plane");
        fs::create_dir_all(&main).unwrap();
        fs::write(main.join(MANIFEST), "").unwrap();
        let tree = worktree_of(&main, "feature", &main.parent().unwrap().join("feature"));

        assert_eq!(find_root(&tree), Ok(main.clone()));
        // And the step `init` takes in the same directory, which is the other half of the
        // defect: three resolvers, three planes. Asked through `plane_of`/`outermost` rather
        // than through `place`, because `place` reads `$CHARTER_ROOT` and a test must not
        // depend on the environment the suite happens to run in.
        assert_eq!(outermost(&plane_of(&tree)), main);
    }

    #[test]
    fn a_worktree_whose_main_tree_is_not_a_plane_stays_where_it_is() {
        // `plane_of` redirects only when the main tree carries the marker too. A worktree of
        // an ordinary repo that someone ran `charter init` inside is a plane, and following
        // its `.git` back would answer with a directory that has no `charter.toml` at all.
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().canonicalize().unwrap().join("repo");
        fs::create_dir_all(&main).unwrap();
        let tree = worktree_of(&main, "feature", &main.parent().unwrap().join("feature"));

        assert_eq!(find_root(&tree), Ok(tree));
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
        let top = dir.path().canonicalize().unwrap();
        fs::write(top.join(MANIFEST), "").unwrap();
        let nested = top.join("inner");
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
