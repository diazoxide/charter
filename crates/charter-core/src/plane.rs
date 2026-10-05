//! The control plane: the directory whose `purlis.toml` or `charter.toml` everything else
//! hangs off.

use std::path::{Path, PathBuf};

use crate::names::{self, Found};

/// The OLD name of the file that marks a directory as a plane root, which the call sites not yet
/// moved onto [`crate::names`] still join. A plane is recognised by either name ([`is_plane`]);
/// [`manifest`] says which file a plane's manifest is.
pub const MANIFEST: &str = "charter.toml";

/// The manifest's name for a WRITER at `root`: whichever is there, `purlis.toml` first, and
/// `charter.toml` where neither is — so `init` keeps writing what it always wrote, and never
/// writes a second manifest beside a `purlis.toml`.
pub fn manifest_to_keep(root: &Path) -> &'static str {
    match names::PLANE_MANIFEST.file_in(root).found {
        Found::Purlis => names::PLANE_MANIFEST.write,
        Found::Old | Found::Neither => MANIFEST,
    }
}

/// Whether `dir` is a plane root: it holds a `purlis.toml` or a `charter.toml` FILE.
pub fn is_plane(dir: &Path) -> bool {
    names::PLANE_MANIFEST.file_in(dir).found != Found::Neither
}

/// The manifest of the plane at `root`: `purlis.toml` when it is there, else `charter.toml`
/// when that is, else `purlis.toml` — the purlis name wins (V93e).
pub fn manifest(root: &Path) -> PathBuf {
    names::PLANE_MANIFEST.file_in(root).name
}

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
/// **When nothing is marked above `start` at all, [`worktree_plane_above`] asks the second
/// question Python asks**, and M2.23 is what brought it over.
pub fn find_root(start: &Path) -> Result<PathBuf, PlaneError> {
    let here = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    walk(&here).ok_or_else(|| PlaneError::NotFound(start.to_path_buf()))
}

/// The whole of `charter/root.py:find_root`'s walk, from an already-resolved directory: the
/// marked ancestor if there is one, else the plane of the main tree this linked worktree was
/// cut from.
///
/// **One function, because `find_root` and [`place`] are `find_root` and `find_root_or_cwd`
/// in Python and Python writes the walk once.** They differ only in what they do with
/// `None` — raise, or fall back to the working directory — and every time a step has been
/// added to one of them and not the other, two commands standing in one directory have named
/// two different planes (M2.9, then M2.16, then this).
/// **The fence is held here and not in [`find_root`] and [`place`] separately**, for the
/// reason this function exists at all: two copies of the walk are two answers, and two
/// copies of the guard on the walk would be one guarded answer and one unguarded one. This
/// is the single line at which a directory becomes "the plane this process acts on", so it
/// is the single line a fenced build has to survive (charter-app#129).
fn walk(here: &Path) -> Option<PathBuf> {
    let found = match marked_above(here) {
        Some(marked) => Some(outermost(&plane_of(marked))),
        None => worktree_plane_above(here),
    };
    if let Some(root) = &found {
        crate::fence::hold(crate::fence::Act::Resolve, root);
    }
    found
}

/// `root.py:find_root`'s SECOND walk: this directory sits in a linked worktree whose plane
/// lives in the repo it was cut from.
///
/// **[`plane_of`] cannot reach this case and that is the point.** It redirects a marker it
/// FOUND, and there is often none to find: `charter init` writes `charter.toml` and never
/// stages it, so a worktree cut from `main` does not contain one — the common shape, not the
/// exotic one. A worktree branched from before the plane was committed has it too. Charter's
/// own `enter:` line then landed a session in a plane-less directory — no personas, no vault,
/// memory written where `git worktree remove --force` deletes it — while `doctor` reported
/// everything green, because every surface it asked resolved to that same directory and found
/// it internally consistent (charter's `root.py` comment, and M2.23 here).
///
/// **The main tree's PARENTS are walked too, deliberately.** A worktree cut from a fleet
/// clone has its main tree at `workspaces/<ws>/<repo>`, so the plane is ABOVE the clone and
/// not at it.
///
/// **The first linked worktree above `start` decides, and nothing below it is tried again.**
/// That is Python's `break`: once a directory has been identified as a worktree, its main
/// tree either has a plane or this is not a plane-less worktree landing at all. Walking on
/// would let a worktree nested inside another worktree answer with the outer one's plane —
/// an answer neither implementation has ever given.
fn worktree_plane_above(here: &Path) -> Option<PathBuf> {
    let main = here.ancestors().find_map(main_worktree_of)?;
    marked_above(&main).map(|marked| outermost(&plane_of(marked)))
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
    start.ancestors().find(|dir| is_plane(dir))
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
    match crate::steer::var_os("PURLIS_ROOT") {
        Some(root) if !root.is_empty() => {
            let root = PathBuf::from(root);
            // The variable is a pin, not an exemption: a fenced build that is pointed at a
            // plane outside its fence is a run acting on somebody else's plane just as
            // surely as one that walked to it (charter-app#129).
            crate::fence::hold(crate::fence::Act::Resolve, &root);
            Ok(root)
        }
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
/// The walk up from `cwd` is [`walk`] — [`find_root`]'s own, and the whole of it: out of a
/// linked worktree (`_plane_of`), outward through an enclosing plane's `workspaces/`
/// (`_outermost`), and — when nothing above `cwd` is marked at all — the main tree this
/// worktree was cut from ([`worktree_plane_above`]). So a clone of a plane inside a
/// workspace, a worktree cut from a plane, and a worktree cut from a plane whose marker was
/// never committed all resolve to the plane that holds them — the answer every other charter
/// command gives there. `init` used to be a third answer in the same directory (M2.16), and
/// in a plane-less worktree it scaffolded a SECOND plane into a tree `git worktree remove`
/// deletes (M2.23); `charter init` standing in a worktree of a plane now reports on the
/// plane, which is what `charter doctor` standing beside it reports on.
pub fn place(cwd: &Path) -> Place {
    if let Some(named) = crate::steer::var_os("PURLIS_ROOT").filter(|v| !v.is_empty()) {
        let named = expand_user(Path::new(&named));
        if let Ok(root) = named.canonicalize()
            && is_plane(&root)
        {
            return Place {
                root,
                is_plane: true,
            };
        }
    }
    let here = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    match walk(&here) {
        Some(root) => Place {
            root,
            is_plane: true,
        },
        None => Place {
            root: here,
            is_plane: false,
        },
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
///
/// **Always `.charter/` for now, even beside a `.purlis/`** (D-RN1-11). The guards that keep
/// the state folder out of reads, searches and commits know only `.charter/`, so the state
/// folder moves in the same change that teaches them `.purlis/` (RN-2a). The doctor's
/// `renamed leftovers` row still names a `.purlis/` beside it.
pub fn state_dir(root: &Path) -> PathBuf {
    match crate::steer::var_os("PURLIS_HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home),
        _ => root.join(names::STATE_DIR.newest_old()),
    }
}

/// A project clone's key (ADR 0079 §8): the first 16 hex characters of the SHA-256 of the
/// clone's canonical root path, over the path's own bytes (`OsStr::as_encoded_bytes`).
///
/// **The one hash of a clone's identity.** The reopen record says which clone wrote it by it
/// (V43), the search index will key its directory on it (ADR 0079), and a save branch's default
/// name is its first six characters for every UTF-8 root
/// ([`crate::planesave::Plane::save_branch_or_default`]), short because a person reads it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CloneKey(String);

impl CloneKey {
    /// The key of the clone at `root`, canonicalised here. A root that cannot be resolved keys
    /// as spelled.
    pub fn of(root: &Path) -> Self {
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Self::of_canonical(&root)
    }

    /// The key of `root`, which the caller has already canonicalised.
    pub fn of_canonical(root: &Path) -> Self {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(root.as_os_str().as_encoded_bytes());
        let mut key = crate::extension::hex(&digest);
        key.truncate(16);
        Self(key)
    }

    /// `word` where it is a key charter would write: 16 lowercase hex characters.
    pub fn parse(word: &str) -> Option<Self> {
        let ok = word.len() == 16
            && word
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        ok.then(|| Self(word.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CloneKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
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
        Some(main) if is_plane(&main) => main,
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
        if !is_plane(parent) {
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

/// Write charter's own state at 0600, whole or not at all, never through a link —
/// [`crate::rewrite::replace`] with [`crate::rewrite::Mode::Private`] (#434).
///
/// The bytes go to a temp file beside `path`, created 0600 with `O_NOFOLLOW` through the
/// containment walk from `plane`, and are renamed over it. So a file an older charter left at
/// a looser mode is replaced by one that was never readable by anyone else, a link planted at
/// `path` after the walk answered is replaced rather than written through, and a crash leaves
/// the previous state whole instead of truncated.
///
/// The chmod is best effort, as `charter/config.py:_private_fd`'s is: filesystems with fixed
/// permissions (exFAT, many network mounts) cannot hold a mode, and refusing to write state to
/// protect a mode the filesystem was never going to keep helps nobody.
pub fn write_private(plane: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::rewrite::replace(plane, path, bytes, crate::rewrite::Mode::Private)
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
        let at = bare_worktree_of(main, name, at);
        fs::write(at.join(MANIFEST), "").unwrap();
        at
    }

    /// The same, with NO `charter.toml` checked out into it — the common shape, not the
    /// exotic one: `charter init` writes the marker and never stages it, so a worktree cut
    /// from `main` does not carry one.
    fn bare_worktree_of(main: &Path, name: &str, at: &Path) -> PathBuf {
        fs::create_dir_all(main.join(".git/worktrees").join(name)).unwrap();
        fs::create_dir_all(at).unwrap();
        fs::write(
            at.join(".git"),
            format!("gitdir: {}/.git/worktrees/{name}\n", main.display()),
        )
        .unwrap();
        at.to_path_buf()
    }

    #[test]
    fn a_clone_key_is_sixteen_lowercase_hex_characters_and_nothing_else() {
        let key = CloneKey::of_canonical(Path::new("/some/clone"));
        assert_eq!(CloneKey::parse(key.as_str()), Some(key.clone()));
        for word in [
            "abc",
            "0123456789abcdef0",
            "0123456789abcdeg",
            "0123456789ABCDEF",
            "",
        ] {
            assert_eq!(CloneKey::parse(word), None, "{word:?}");
        }
    }

    // ---- the rename window (RN-1, V93e): `purlis.toml` or `charter.toml` ------------- //

    #[test]
    fn a_project_marked_only_by_the_old_name_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join("charter.toml"), "").unwrap();
        let deep = root.join("workspaces/ide/svc");
        fs::create_dir_all(&deep).unwrap();

        assert_eq!(find_root(&deep), Ok(root.clone()));
        assert_eq!(manifest(&root), root.join("charter.toml"));
    }

    #[test]
    fn a_project_marked_only_by_the_purlis_name_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join("purlis.toml"), "").unwrap();
        let deep = root.join("workspaces/ide/svc");
        fs::create_dir_all(&deep).unwrap();

        assert_eq!(find_root(&deep), Ok(root.clone()));
        assert!(is_plane(&root));
        assert_eq!(manifest(&root), root.join("purlis.toml"));
    }

    #[test]
    fn a_project_marked_by_both_names_reads_the_purlis_one() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join("charter.toml"), "").unwrap();
        fs::write(root.join("purlis.toml"), "").unwrap();

        assert_eq!(find_root(&root), Ok(root.clone()));
        assert_eq!(manifest(&root), root.join("purlis.toml"));
    }

    #[test]
    fn a_directory_marked_by_neither_name_is_no_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        // A FOLDER of the new name is not a marker, as a folder of the old name never was.
        fs::create_dir(root.join("purlis.toml")).unwrap();

        assert!(!is_plane(&root));
        assert!(find_root(&root).is_err());
    }

    #[test]
    fn a_project_inside_another_projects_workspaces_hops_out_whichever_name_each_has() {
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().canonicalize().unwrap();
        fs::write(outer.join("purlis.toml"), "").unwrap();
        let inner = outer.join("workspaces/ide/clone");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("charter.toml"), "").unwrap();

        assert_eq!(find_root(&inner), Ok(outer));
    }

    #[test]
    fn the_state_folder_stays_the_old_one_whatever_else_is_there() {
        // D-RN1-11: the guards that keep `.charter/` out of reads, walks and commits do not
        // know `.purlis/` yet, so the state folder moves only when they do (RN-2a).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert_eq!(state_dir(&root), root.join(".charter"));

        fs::create_dir(root.join(".purlis")).unwrap();
        assert_eq!(state_dir(&root), root.join(".charter"));

        fs::create_dir(root.join(".charter")).unwrap();
        assert_eq!(state_dir(&root), root.join(".charter"));
    }

    #[test]
    fn the_other_questions_of_is_this_a_plane_accept_either_name() {
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().canonicalize().unwrap();
        fs::write(outer.join("purlis.toml"), "").unwrap();
        let inner = outer.join("workspaces/ide/clone");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("purlis.toml"), "").unwrap();

        assert_eq!(crate::machine::still_a_plane(&outer), Ok(()));
        assert_eq!(
            crate::planegit::nested_plane_in(&outer, &inner),
            Some(inner.clone())
        );
    }

    #[test]
    fn the_manifest_a_writer_keeps_is_the_one_that_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert_eq!(manifest_to_keep(&root), "charter.toml");
        fs::write(root.join("purlis.toml"), "").unwrap();
        assert_eq!(manifest_to_keep(&root), "purlis.toml");
        fs::write(root.join("charter.toml"), "").unwrap();
        assert_eq!(manifest_to_keep(&root), "purlis.toml");
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

    // M2.23: the second walk. Nothing above the caller is marked at all, because the marker
    // was never committed — so `plane_of` has no marker to redirect and the first walk is
    // simply empty. What charter does here is ask whether this is a linked worktree whose
    // plane is in the repo it was cut from.

    #[test]
    fn a_worktree_whose_plane_was_never_committed_resolves_to_the_plane_it_was_cut_from() {
        // The failure this closes: `charter init` writes `charter.toml` and never stages it,
        // so a worktree cut from `main` carries no marker. A session that followed charter's
        // own `enter:` line landed in this directory with no personas, no vault, and its
        // memory written where `git worktree remove --force` deletes it — while `doctor`
        // reported green, because everything it asked resolved to this same directory.
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let main = top.join("plane");
        fs::create_dir_all(&main).unwrap();
        fs::write(main.join(MANIFEST), "").unwrap();
        let tree = bare_worktree_of(&main, "feature", &top.join("feature"));

        assert!(
            !tree.join(MANIFEST).exists(),
            "the worktree must carry no marker, or this tests the FIRST walk"
        );
        assert_eq!(find_root(&tree), Ok(main.clone()));
        assert_eq!(find_root(&tree.join("crates/src")), Ok(main.clone()));
        // And `init`, which is `find_root_or_cwd` and takes the same walk in Python. Without
        // it, `charter init` in this directory scaffolds a SECOND plane into the tree
        // `git worktree remove` deletes, beside the one it was cut from.
        assert_eq!(
            place(&tree),
            Place {
                root: main,
                is_plane: true
            }
        );
    }

    #[test]
    fn a_worktree_of_a_clone_finds_the_plane_above_the_clone_and_not_the_clone() {
        // Why the MAIN TREE'S PARENTS are walked and not just the main tree: a worktree cut
        // from a fleet clone has its main tree at `workspaces/<ws>/<repo>`, so the plane is
        // above the clone. Stopping at the main tree would answer `None` here and land the
        // session in the plane-less directory this whole walk exists to avoid.
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let plane = top.join("plane");
        let clone = plane.join("workspaces/ide/acme");
        fs::create_dir_all(&clone).unwrap();
        fs::write(plane.join(MANIFEST), "").unwrap();
        // OUTSIDE the plane: a worktree under it would be answered by the first walk, and
        // this test would then say nothing about the second.
        let tree = bare_worktree_of(&clone, "feature", &top.join("feature"));

        assert_eq!(find_root(&tree), Ok(plane));
    }

    #[test]
    fn a_worktree_whose_main_tree_has_no_plane_above_it_is_still_not_a_plane() {
        // The walk answers with a plane or with nothing. A main tree that is not in a plane
        // must not make one up out of the working directory.
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let main = top.join("repo");
        fs::create_dir_all(&main).unwrap();
        let tree = bare_worktree_of(&main, "feature", &top.join("feature"));

        assert_eq!(
            find_root(&tree),
            Err(PlaneError::NotFound(tree.clone())),
            "a worktree of a plane-less repo is not in a plane"
        );
        assert_eq!(
            place(&tree),
            Place {
                root: tree,
                is_plane: false
            }
        );
    }

    #[test]
    fn the_first_worktree_above_the_caller_decides_and_the_walk_stops_there() {
        // Python's `break`. `outer` is a worktree of the plane; `inner`, inside it, is a
        // worktree of an unrelated plane-less repo. Standing in `inner`, the answer is "no
        // plane" — walking on would hand `inner` the plane behind a tree it is only
        // physically inside of, which is an answer neither implementation has ever given.
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let plane = top.join("plane");
        fs::create_dir_all(&plane).unwrap();
        fs::write(plane.join(MANIFEST), "").unwrap();
        let other = top.join("other");
        fs::create_dir_all(&other).unwrap();

        let outer = bare_worktree_of(&plane, "outer", &top.join("outer"));
        let inner = bare_worktree_of(&other, "inner", &outer.join("inner"));

        assert_eq!(find_root(&outer), Ok(plane), "the outer one still answers");
        assert_eq!(find_root(&inner), Err(PlaneError::NotFound(inner)));
    }

    #[test]
    fn a_marker_above_the_caller_still_wins_over_the_worktree_walk() {
        // The second walk is a FALLBACK and only that: Python reaches it only after the
        // first walk found nothing. A worktree that sits inside a plane resolves through the
        // marker above it, which is the walk `plane_of` and `outermost` are attached to.
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let cut_from = top.join("repo");
        fs::create_dir_all(&cut_from).unwrap();
        fs::write(cut_from.join(MANIFEST), "").unwrap();
        let holding = top.join("holding");
        fs::create_dir_all(&holding).unwrap();
        fs::write(holding.join(MANIFEST), "").unwrap();
        let tree = bare_worktree_of(&cut_from, "feature", &holding.join("feature"));

        assert_eq!(find_root(&tree), Ok(holding));
    }

    /// #434: charter's own state is replaced whole, so a crash between the write and the
    /// rename leaves the previous state rather than a truncated file.
    #[test]
    fn private_state_that_dies_before_its_rename_leaves_the_old_state_whole() {
        let plane = tempfile::tempdir().unwrap();
        let state = plane.path().join(".charter/active-persona");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        fs::write(&state, "old\n").unwrap();
        let _killed = crate::rewrite::hook::set(|_, _| Err(std::io::Error::other("killed")));

        write_private(plane.path(), &state, b"new\n").unwrap_err();

        assert_eq!(fs::read_to_string(&state).unwrap(), "old\n");
        assert_eq!(fs::read_dir(state.parent().unwrap()).unwrap().count(), 1);
    }

    /// #434: a link at the state file is refused, and what it points at is untouched.
    #[cfg(unix)]
    #[test]
    fn private_state_at_a_link_is_refused_and_the_links_target_is_untouched() {
        let plane = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("theirs");
        fs::write(&target, "theirs\n").unwrap();
        let state = plane.path().join(".charter/active-persona");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &state).unwrap();

        write_private(plane.path(), &state, b"new\n").unwrap_err();

        assert_eq!(fs::read_to_string(&target).unwrap(), "theirs\n");
        assert!(state.is_symlink());
    }
}
