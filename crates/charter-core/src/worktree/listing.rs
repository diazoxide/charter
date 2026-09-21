//! Which working trees share one repository — asked of git, once per repository per block.
//!
//! A port of `charter/workspace.py`'s `worktree_answers`, `_worktree_list`, `_live_trees` and
//! `_listed_trees`. Two readers want the same listing and a launch reaches both of them:
//!
//! - **which trees read one `info/exclude`** ([`live_trees`]), so charter's block in a clone's
//!   exclude holds what every linked worktree of it needs and drops no line a live sibling
//!   still has a file for ([`crate::guest`]);
//! - **which PIECES a workspace holds** ([`crate::wslayer::guest_trees`]), at
//!   `.worktrees/<repo>/<piece>` — the directory `charter wt add` tells a worker to start a
//!   session in, which got no layer, no repair and no row until this listing existed.
//!
//! # Git is the registry
//!
//! `git worktree list --porcelain`, as it is for `worktree.py` and ADR 0011: a worktree made
//! by hand at the layout's path counts and one removed by hand does not. Never a walk of the
//! directory, which would take a copy somebody made for a live worktree.
//!
//! # Trusted only when it can be backed up
//!
//! charter's ruling G (its #942 review round 3), ported whole. The listing is trusted only
//! when git answered in time, every entry but a bare repository's names a directory holding a
//! `.git`, and none is marked prunable. Each way it can fail names **what clears it**, because
//! `reinit` clears none of them and a reader sent to the wrong repair is a reader who stops
//! reading.
//!
//! Two of those arms are incidents rather than theory. Git 2.50.1 lists a
//! `--separate-git-dir` clone's GIT DIRECTORY as its main worktree, so that clone never
//! entered the union; and a `workspace rename` left git listing the old path as prunable, so
//! every launch of another workspace unhid the moved worktree's machine-local file.
//!
//! # Git is not asked at all when the common directory has no `worktrees/`
//!
//! Git keeps every linked worktree's administrative directory there, so without one the only
//! tree is the one asking — and a launch wires every checkout in its workspace, at ~7 ms a
//! spawn. The directory is READ rather than `lstat`ed, and that is load bearing: measured on
//! git 2.50.1, with `worktrees/` unreadable, or one worktree's directory missing from it, or
//! only that worktree's `gitdir` missing, `git worktree list` lists the rest and **exits 0**.
//! An `lstat` that passed all three let charter take git's short list for the whole one and
//! drop the line a live sibling still needed.
//!
//! # The block
//!
//! [`answers`] opens one. Inside it each repository is asked at most once, and so is each
//! "does git track this" and "is this file of yours untracked" question that goes with it —
//! charter's own scope, for the same reason: a launch wires every checkout twice over, and
//! the deadline is per call, so a copy per reader was a second budget on a hung git.
//!
//! Per thread and never across processes: a later launch asks again, because by then the
//! answer may have changed. A block opened inside another shares the outer one's answers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::worktree::{git, porcelain};

/// What one `git worktree list` answered, or why it could not be asked.
type Listed = Result<git::Run, String>;

/// The answers this block has already paid for.
#[derive(Default)]
struct Scope {
    /// `git worktree list --porcelain`, by the repository's common git directory.
    listed: HashMap<PathBuf, Listed>,
    /// [`live_trees`]' verdict, by the same key.
    answers: HashMap<PathBuf, (Option<Vec<PathBuf>>, String)>,
    /// Whether git tracks a checkout's `.charter-generated`, by checkout.
    tracked: HashMap<PathBuf, bool>,
    /// Whether a path in a checkout holds an untracked file of the operator's.
    untracked: HashMap<(PathBuf, String), bool>,
}

thread_local! {
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

/// One block of answers, held open for as long as the value lives.
///
/// A block opened inside another is a no-op that shares the outer one's answers, so a caller
/// never has to know whether it is the outermost — which is what makes it safe to open one in
/// every entry point.
#[must_use = "the block ends the moment this value is dropped"]
pub struct Answers {
    outermost: bool,
}

impl Drop for Answers {
    fn drop(&mut self) {
        if self.outermost {
            SCOPE.with(|scope| *scope.borrow_mut() = None);
        }
    }
}

/// Open a block — charter's `worktree_answers`.
pub fn answers() -> Answers {
    let outermost = SCOPE.with(|scope| {
        let mut scope = scope.borrow_mut();
        if scope.is_some() {
            return false;
        }
        *scope = Some(Scope::default());
        true
    });
    Answers { outermost }
}

/// Read one memoised answer, or compute and keep it. Outside a block nothing is kept and
/// `compute` simply runs.
///
/// `read` and `keep` are two closures rather than one that hands back a `&mut` into the
/// scope, because the borrow must not be held across `compute` — the computation runs git,
/// and a git that wires another checkout would open the same `RefCell`.
fn cached<K, V>(
    key: K,
    read: impl Fn(&Scope, &K) -> Option<V>,
    keep: impl Fn(&mut Scope, K, V),
    compute: impl FnOnce() -> V,
) -> V
where
    V: Clone,
{
    let held = SCOPE.with(|scope| scope.borrow().as_ref().and_then(|scope| read(scope, &key)));
    if let Some(value) = held {
        return value;
    }
    let value = compute();
    SCOPE.with(|scope| {
        if let Some(scope) = scope.borrow_mut().as_mut() {
            keep(scope, key, value.clone());
        }
    });
    value
}

/// `git worktree list --porcelain` for the repository whose common git directory is `common`,
/// asked from `tree` — charter's `_worktree_list`.
///
/// **One spawn for two readers** (charter#951). [`live_trees`] asks which trees share an
/// exclude and [`crate::wslayer`] asks which pieces a workspace holds, and a launch asks both
/// of every repository with a worktree — so a copy of this per reader was a second deadline on
/// a hung git.
pub fn worktree_list(tree: &Path, common: &Path) -> Listed {
    cached(
        real(common),
        |scope, key| scope.listed.get(key).cloned(),
        |scope, key, value| {
            scope.listed.insert(key, value);
        },
        || {
            git::run(tree, &["worktree", "list", "--porcelain"], git::READ)
                .map_err(|unavailable| unavailable.to_string())
        },
    )
}

/// Whether git TRACKS `rel` in the checkout at `tree`, asked at most once per checkout per
/// block — [`crate::guest::tracked`]'s memo.
pub fn tracked(tree: &Path, ask: impl FnOnce() -> bool) -> bool {
    cached(
        real(tree),
        |scope, key| scope.tracked.get(key).copied(),
        |scope, key, value| {
            scope.tracked.insert(key, value);
        },
        ask,
    )
}

/// Whether `tree` holds an untracked file of the operator's at `rel`, asked at most once per
/// (checkout, path) per block — [`crate::guest`]'s memo for charter#1072.
pub fn untracked(tree: &Path, rel: &str, ask: impl FnOnce() -> bool) -> bool {
    cached(
        (real(tree), rel.to_owned()),
        |scope, key| scope.untracked.get(key).copied(),
        |scope, key, value| {
            scope.untracked.insert(key, value);
        },
        ask,
    )
}

/// A path as the cache keys it: where it lands, or the path itself when charter cannot say.
///
/// Two checkouts spelled differently are one repository, and a key that kept both spellings
/// would ask git twice for one answer — which is the whole cost this block exists to avoid.
fn real(path: &Path) -> PathBuf {
    crate::contain::resolved(path).unwrap_or_else(|| path.to_path_buf())
}

/// Whether a path is there — `Some(true)`, `Some(false)`, or `None` when the filesystem will
/// not say. charter's `_exists`, and its ruling G(e).
///
/// **Only "not there" and "a component is not a directory" prove a path gone.** Every other
/// errno — EACCES, EIO, ESTALE — is doubt, and reading doubt as "gone" is how one refused
/// `lstat` during a launch forgot a marker entry for good.
pub fn exists(path: &Path) -> Option<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Some(true),
        Err(e) if gone(&e) => Some(false),
        Err(_) => None,
    }
}

/// Whether an `io::Error` proves the path is not there — charter's `FileNotFoundError` and
/// `NotADirectoryError`, and nothing else.
///
/// `ErrorKind::NotADirectory` is still unstable, so the errno is compared: `ENOTDIR` is 20 on
/// macOS and on Linux, and this crate takes no `libc` dependency for one number.
pub fn gone(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(ENOTDIR)
}

/// `ENOTDIR`, the same value on macOS and on Linux.
const ENOTDIR: i32 = 20;

/// Every working tree whose git reads `exclude`, as git lists them — or `(None, why)` when
/// that list cannot be trusted. charter's `_live_trees`.
pub fn live_trees(tree: &Path, exclude: &Path) -> (Option<Vec<PathBuf>>, String) {
    let Some(common) = exclude.parent().and_then(Path::parent) else {
        return (Some(vec![tree.to_path_buf()]), String::new());
    };
    let admin = common.join("worktrees");
    // READ, never only `lstat`: see the module docs for the three shapes git answers 0 to.
    let names = match std::fs::read_dir(&admin) {
        Ok(entries) => entries,
        Err(e) if gone(&e) => {
            return (Some(vec![tree.to_path_buf()]), String::new());
        }
        Err(_) => {
            return (
                None,
                format!(
                    "{} cannot be checked — restoring read access clears this",
                    admin.display()
                ),
            );
        }
    };
    for entry in names.flatten() {
        let gitdir = entry.path().join("gitdir");
        match std::fs::read(&gitdir) {
            Ok(_) => {}
            Err(e) if e.raw_os_error() == Some(ENOTDIR) => {
                // A stray file in `worktrees/`, which git passes over too (measured).
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Git leaves that worktree out of its list as well, and says nothing
                // (measured), so charter cannot account for the lines it would have needed.
                return (
                    None,
                    format!(
                        "{} is missing, so git leaves that worktree out of its list — `git \
                         worktree repair` from that worktree clears this",
                        gitdir.display()
                    ),
                );
            }
            Err(_) => {
                return (
                    None,
                    format!(
                        "{} cannot be checked — restoring read access clears this",
                        gitdir.display()
                    ),
                );
            }
        }
    }
    cached(
        real(common),
        |scope, key| scope.answers.get(key).cloned(),
        |scope, key, value| {
            scope.answers.insert(key, value);
        },
        || listed_trees(tree, exclude, common),
    )
}

/// [`live_trees`]' question to git, and its verdict on the answer — charter's `_listed_trees`.
///
/// Each reason names what clears it: `reinit` clears none of them.
fn listed_trees(tree: &Path, exclude: &Path, common: &Path) -> (Option<Vec<PathBuf>>, String) {
    let asked = format!("listing the worktrees that share {}", exclude.display());
    let run = match worktree_list(tree, common) {
        Ok(run) => run,
        Err(why) => {
            return (
                None,
                format!("git could not be run {asked} ({why}) — a git that runs there clears this"),
            );
        }
    };
    let Some(code) = run.code else {
        // The deadline is `git::READ`'s thirty seconds and not charter's five — see that
        // constant's own docs for why — so the sentence says the number charter-app waited.
        return (
            None,
            format!(
                "git timed out after {}s {asked} — a git that answers there in time clears this",
                git::READ.as_secs()
            ),
        );
    };
    if code != 0 {
        return (
            None,
            format!("git exited {code} {asked} — a git that answers there clears this"),
        );
    }
    let mut trees = Vec::new();
    for row in porcelain::parse(&run.out) {
        if row.bare {
            continue;
        }
        if let Some(said) = &row.prunable {
            // Git's own reason, never charter's guess: "moved or deleted" was said of a
            // worktree that was only unreadable.
            if exists(&row.path) == Some(false) {
                return (
                    None,
                    format!(
                        "git lists {} as prunable: {said} — `git worktree repair` from its new \
                         place if it moved, `git worktree prune` if it is gone",
                        row.path.display()
                    ),
                );
            }
            // charter's own lstat decides "gone", never git's word: git calls a worktree it
            // cannot read prunable too, and `git worktree prune` there deletes git's record of
            // a checkout that is still on disk.
            return (
                None,
                format!(
                    "git lists {} as prunable: {said}, but charter does not find it gone — \
                     unreadable: restore access to it",
                    row.path.display()
                ),
            );
        }
        if exists(&row.path.join(".git")) != Some(true) {
            if crate::contain::resolved(&row.path) == crate::contain::resolved(common) {
                // Git 2.50.1 lists a `--separate-git-dir` clone's GIT directory as its main
                // worktree, and no listing will ever say where that checkout is.
                return (
                    None,
                    format!(
                        "git lists {}, which is not a checkout charter can look into — a \
                         separate git dir keeps this line by design; nothing to do",
                        row.path.display()
                    ),
                );
            }
            return (
                None,
                format!(
                    "git lists {}, which is not a checkout charter can look into — restoring \
                     that checkout, or read access to it, clears this",
                    row.path.display()
                ),
            );
        }
        trees.push(row.path);
    }
    (Some(trees), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_with_no_worktrees_directory_is_answered_without_spawning_git() {
        // **The spawn-avoidance property, proved rather than asserted.** There is no git
        // repository anywhere near this directory, so a `git worktree list` here would exit
        // 128 and the answer would be `None` with git's own sentence. It comes back
        // `Some([tree])` instead, which is only reachable by never asking — and that is what
        // keeps a launch wiring a workspace full of ordinary clones at zero git spawns.
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("clone");
        let exclude = tree.join(".git").join("info").join("exclude");
        std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();

        let (trees, why) = live_trees(&tree, &exclude);

        assert_eq!(trees, Some(vec![tree.clone()]), "{why}");
        assert!(why.is_empty(), "{why}");
    }

    #[test]
    fn a_worktrees_directory_that_cannot_be_read_is_doubt_and_never_no_worktrees() {
        // The shape ruling G is about: `git worktree list` exits 0 with a SHORT list there
        // (measured), so a listing charter cannot back up is not evidence of anything.
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("clone");
        let admin = tree.join(".git").join("worktrees");
        std::fs::create_dir_all(&admin).unwrap();
        let exclude = tree.join(".git").join("info").join("exclude");
        std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&admin, std::fs::Permissions::from_mode(0o000)).unwrap();

        let (trees, why) = live_trees(&tree, &exclude);
        std::fs::set_permissions(&admin, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(trees, None);
        assert!(why.contains("cannot be checked"), "{why}");
        assert!(why.contains("restoring read access clears this"), "{why}");
    }

    #[test]
    fn a_worktree_whose_gitdir_is_missing_is_doubt_with_the_command_that_repairs_it() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("clone");
        std::fs::create_dir_all(tree.join(".git").join("worktrees").join("p1")).unwrap();
        let exclude = tree.join(".git").join("info").join("exclude");
        std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();

        let (trees, why) = live_trees(&tree, &exclude);

        assert_eq!(trees, None);
        assert!(why.contains("git worktree repair"), "{why}");
    }

    #[test]
    fn a_path_that_cannot_be_checked_is_neither_there_nor_gone() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(exists(dir.path()), Some(true));
        assert_eq!(exists(&dir.path().join("nope")), Some(false));
        // A component that is a file, not a directory: gone, and not doubt.
        std::fs::write(dir.path().join("file"), "x").unwrap();
        assert_eq!(exists(&dir.path().join("file").join("under")), Some(false));
    }

    #[test]
    fn one_block_asks_a_question_once_and_a_second_block_asks_again() {
        let mut asked = 0;
        {
            let _block = answers();
            let dir = tempfile::tempdir().unwrap();
            for _ in 0..3 {
                untracked(dir.path(), ".claude/settings.json", || {
                    asked += 1;
                    true
                });
            }
            assert_eq!(asked, 1, "one block, one answer");
        }
        let dir = tempfile::tempdir().unwrap();
        let _block = answers();
        untracked(dir.path(), ".claude/settings.json", || {
            asked += 1;
            true
        });
        assert_eq!(asked, 2, "a later block asks again");
    }

    #[test]
    fn a_block_opened_inside_another_shares_its_answers() {
        let mut asked = 0;
        let _outer = answers();
        let dir = tempfile::tempdir().unwrap();
        untracked(dir.path(), "x", || {
            asked += 1;
            true
        });
        {
            let _inner = answers();
            untracked(dir.path(), "x", || {
                asked += 1;
                true
            });
        }
        // The inner block's drop must not close the outer one's.
        untracked(dir.path(), "x", || {
            asked += 1;
            true
        });
        assert_eq!(asked, 1);
    }
}
