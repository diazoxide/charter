//! What a branch changed, file by file and rolled up onto its folders (FM-4, #1107; #1103 V86
//! F6): the tree's markers, and what "Changed only" collapses it to.
//!
//! **Against the branch it was cut from**, committed or not. The base is the one fact charter
//! records about a piece, `branch.<branch>.charterBase` in the clone's config (ADR 0027,
//! `worktree::add`), and the changes are counted from where the branch left it (the merge
//! base), so what the base gained since is not the branch's change. A branch with no recorded
//! base — the repo's own folder, a branch cut by plain git — is marked against its last
//! commit: what is not committed yet.
//!
//! **Read by gitoxide in a short-lived, bounded child of charter's own binary, starting no
//! program** (D-88f, D-88h). The app reads this on its own after every write an agent makes,
//! and a branch's git config is something that agent can write: any git process charter
//! started there would read it, and git config can name programs for git to run. So nothing
//! here starts git. The repository is read with gitoxide (`gix`), in a child (`reader.rs`) that
//! is killed past a deadline and past a memory cap, so a branch built to hang or swell the read
//! (an ignore file linked to an endless device, a FIFO) costs that child and not the app:
//! - its config is read once, when the branch is opened, and then cut down to the keys a read
//!   needs ([`ALLOWED`]), so no filter driver, process filter, external diff, text conversion,
//!   or memory or size setting is seen, whatever the files on disk say by then;
//! - gitoxide has no fsmonitor, runs no hook, holds no credential helper and never fetches,
//!   so an object the branch lacks fails the read instead of being fetched;
//! - the status is taken HEAD → index → working tree, and the committed changes as the
//!   difference between the fork point's tree and HEAD's, with renames found in both. The
//!   working tree is compared on every core, and walked for untracked files at the same time
//!   (#1153): it is read after every write, so it must stay near `git status`'s own speed.
//! - A file whose time moved and whose size did not is compared as it is on disk, with no
//!   filter: such a file can be marked changed when git would not mark it. Wrong marks are
//!   acceptable; running a program is not.
//!
//! **The recorded base is read, never trusted.** Anything working in the branch can write the
//! clone's config, so the value must be a branch name charter would hand git, or a commit
//! named by its sha, and must resolve to a commit; a base that does not is treated as none.
//!
//! **Every path answered is a plain path inside the branch**: relative, no `..`, never git's
//! own. A renamed file's source is a name, shown and never followed. A repository nested in the
//! branch, and a submodule, are left out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{Branch, Refused, inside};
use crate::worktree::{self, confine, name};

/// The most changes one status answers with, sorted by path: a branch that generated a
/// hundred thousand files is marked as its first and a count of the rest. Folders roll up all
/// of them.
pub const MARKED: usize = 10_000;

/// What a branch did to one path.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Mark {
    /// Its content or its kind changed.
    Changed,
    /// It is new: git does not have it at the base.
    Added,
    /// It is gone.
    Deleted,
    /// It moved here from another path.
    Renamed,
}

/// One path the branch changed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Change {
    /// Its path relative to the branch's folder.
    pub path: String,
    pub mark: Mark,
    /// Where a renamed file came from: a name, never followed.
    pub from: Option<String>,
    /// Whether the change is not committed yet; `false` is one the branch committed.
    pub uncommitted: bool,
}

/// One folder holding changes: the mark its changes share, or [`Mark::Changed`] when they are
/// of more than one kind, and how many it holds at any depth.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rolled {
    /// Its path relative to the branch's folder; `""` is the branch's own.
    pub folder: String,
    pub mark: Mark,
    pub count: usize,
}

/// What a branch changed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Status {
    /// Sorted by path, the first [`MARKED`].
    pub changes: Vec<Change>,
    /// Every folder holding a change, sorted by path.
    pub folders: Vec<Rolled>,
    /// How many changes past [`MARKED`] are not in `changes`.
    pub more: usize,
    /// The branch the changes are counted against, or `None` when they are counted against the
    /// last commit.
    pub base: Option<String>,
}

/// What the branch changed against the branch it was cut from, committed or not, with each
/// folder rolling up what it holds. What git ignores is never marked. Read here, in this
/// process: only the bounded reader's child calls it (`reader.rs`).
pub(super) fn status_here(plane: &Path, branch: Branch<'_>) -> Result<Status, Refused> {
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("the changes of {}", branch.called()),
        why,
    };
    let opened = open(plane, branch)?;
    let charter_base = opened.head_base();
    let held = super::compare::hold(plane, &opened.base).map_err(unreadable)?;
    let repo = opened.repo;
    let head = repo.head_commit().ok().map(|commit| commit.id);
    let based =
        charter_base.and_then(|value| head.and_then(|head| recorded_base(&repo, &value, head)));
    let (since, named) = match based {
        Some(Based { fork, named, .. }) => (Some(fork), Some(named)),
        None => (head, None),
    };
    // The one diff engine's file list, without its line counts (RC-2).
    let all = super::compare::changed(&repo, since, super::compare::At::WorkingTree, Some(&held))
        .map_err(unreadable)?
        .changes;
    let folders = rolled_up(&all);
    let more = all.len().saturating_sub(MARKED);
    let mut changes = all;
    changes.truncate(MARKED);
    Ok(Status {
        changes,
        folders,
        more,
        base: named,
    })
}

/// The branch's folder, resolved, and its repository, opened in-process: found by name the way
/// [`super::tree`] finds it, with no git process started.
///
/// A piece must be a worktree git registered for the workspace's clone of `repo`, at the place
/// charter cuts it; the repo's own folder must be a repository's top. The repository's config
/// is read once, here, and then cut down to [`ALLOWED`]: every other key is dropped from what
/// was read, so no filter, diff driver, memory or size setting the branch's config names is
/// seen by anything after.
pub(super) fn open(plane: &Path, branch: Branch<'_>) -> Result<Opened, Refused> {
    let Branch { ws, repo, piece } = branch;
    worktree::relocation_refusal(plane)?;
    if !crate::contain::workspace_name_ok(ws) {
        return Err(worktree::Refusal::BadWorkspace(ws.to_string()).into());
    }
    if !crate::contain::repo_name_ok(repo) {
        return Err(worktree::Refusal::BadRepo(repo.to_string()).into());
    }
    let clone = plane.join("workspaces").join(ws).join(repo);
    let clone = confine::within_workspace(plane, ws, &clone).map_err(worktree::Refusal::from)?;
    let folder = match piece {
        Some(piece) => {
            let at = worktree::path_for(plane, ws, repo, piece)?;
            confine::within_workspace(plane, ws, &at).map_err(worktree::Refusal::from)?
        }
        None => clone.clone(),
    };
    let missing = || -> Refused {
        match piece {
            Some(piece) => Refused::NoSuchPiece {
                ws: ws.to_string(),
                repo: repo.to_string(),
                piece: piece.to_string(),
            },
            None => worktree::Refusal::NotARepo(repo.to_string()).into(),
        }
    };
    let base = std::fs::canonicalize(&folder).map_err(|_| missing())?;
    let mut opened = gix::open_opts(&base, options()).map_err(|_| missing())?;
    let workdir = opened
        .workdir()
        .and_then(|dir| std::fs::canonicalize(dir).ok());
    if workdir.as_deref() != Some(base.as_path()) {
        return Err(missing());
    }
    if piece.is_some() {
        let common = std::fs::canonicalize(opened.common_dir()).ok();
        let clones = std::fs::canonicalize(clone.join(".git")).ok();
        let linked = matches!(opened.kind(), gix::repository::Kind::LinkedWorkTree);
        if !linked || common.is_none() || common != clones {
            return Err(missing());
        }
    }
    let recorded = recorded_values(&opened);
    allowed_only(&mut opened).map_err(|why| Refused::Unreadable {
        what: branch.called().to_string(),
        why,
    })?;
    Ok(Opened {
        base,
        repo: opened,
        recorded,
    })
}

/// A branch, opened in-process.
pub(super) struct Opened {
    /// The branch's folder, resolved.
    pub(super) base: PathBuf,
    pub(super) repo: gix::Repository,
    /// The base each branch was cut from, as recorded (`branch.<name>.charterBase`), read before
    /// the config was cut: by branch name, for the branches that record exactly one value.
    pub(super) recorded: BTreeMap<String, String>,
}

impl Opened {
    /// The base recorded for the branch checked out in the folder, if it records one.
    pub(super) fn head_base(&self) -> Option<String> {
        let current = self.repo.head_name().ok()??;
        self.recorded.get(&current.shorten().to_string()).cloned()
    }
}

/// The config keys a read uses, `section.key`, lower-cased: how the working tree is read and
/// compared, and what git ignores. Everything else the config holds is dropped after it is
/// read — filter and diff drivers, and every memory, cache and size setting a branch's config
/// could raise (`core.bigFileThreshold`, `core.deltaBaseCacheLimit`, `gitoxide.*`).
pub(super) const ALLOWED: [&str; 15] = [
    "core.bare",
    "core.worktree",
    "core.repositoryformatversion",
    "core.ignorecase",
    "core.precomposeunicode",
    "core.symlinks",
    "core.filemode",
    "core.trustctime",
    "core.checkstat",
    "core.autocrlf",
    "core.eol",
    "core.excludesfile",
    "extensions.objectformat",
    "extensions.worktreeconfig",
    "extensions.refstorage",
];

/// Every `branch.<name>.charterBase` that holds exactly one value, by branch name: the names
/// charter would hand git only.
fn recorded_values(repo: &gix::Repository) -> BTreeMap<String, String> {
    let config = repo.config_snapshot();
    let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Some(sections) = config.plumbing().sections_by_name("branch") else {
        return BTreeMap::new();
    };
    for section in sections {
        let Some(name) = section.header().subsection_name() else {
            continue;
        };
        let name = name.to_string();
        if name::branch_name_ok(&name).is_err() {
            continue;
        }
        for value in section.values("charterBase") {
            let value = value.to_string().trim().to_string();
            if !value.is_empty() {
                values.entry(name.clone()).or_default().push(value);
            }
        }
    }
    values
        .into_iter()
        .filter_map(|(name, mut all)| (all.len() == 1).then(|| (name, all.remove(0))))
        .collect()
}

/// Cuts what was read of the config down to [`ALLOWED`], in memory.
fn allowed_only(repo: &mut gix::Repository) -> Result<(), String> {
    let mut config = repo.config_snapshot_mut();
    let kept: Vec<(&str, &str, gix::bstr::BString)> = ALLOWED
        .iter()
        .filter_map(|key| {
            let (section, name) = key.split_once('.')?;
            let value = config.raw_value(*key).ok()?;
            Some((section, name, value))
        })
        .collect();
    let ids: Vec<_> = config.sections_and_ids().map(|(_, id)| id).collect();
    for id in ids {
        config.remove_section_by_id(id);
    }
    for (section, name, value) in kept {
        config
            .set_raw_value_by(section, None, name, value.as_slice())
            .map_err(|e| e.to_string())?;
    }
    config.commit().map(|_| ()).map_err(|e| e.to_string())
}

/// How a branch is opened: the repository's own config and the operator's (`~/.gitconfig`,
/// `~/.config/git`), with their includes; never the system's, never git's own binary asked
/// where its config is, never the environment's `GIT_*`.
///
/// Opened at the folder named, never found by climbing: `safe.bareRepository`, which every git
/// process purlis starts is given, does not reach gitoxide, and [`open`] refuses a folder that
/// is not its repository's work tree, so a bare repository is never read here (#1415, #1550).
fn options() -> gix::open::Options {
    let mut permissions = gix::open::Permissions::isolated();
    permissions.config.user = true;
    permissions.config.git = true;
    permissions.config.includes = true;
    permissions.attributes.git = true;
    permissions.env.home = gix::sec::Permission::Allow;
    permissions.env.xdg_config_home = gix::sec::Permission::Allow;
    gix::open::Options::isolated().permissions(permissions)
}

/// How far a branch is from the branch it was cut from (FM-5, #1108): the branch cockpit's
/// header.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AheadBehind {
    /// Commits the branch has that its base does not.
    pub ahead: usize,
    /// Commits its base gained that the branch does not have.
    pub behind: usize,
    /// The base they are counted against, as recorded; `None` when the branch has no base
    /// recorded that resolves, and then both counts are `0` and mean nothing.
    pub base: Option<String>,
}

/// How far the branch is from its recorded base, counted as `git rev-list --left-right
/// --count <base>...HEAD` counts, through gitoxide with the config cut down as [`open`] cuts
/// it. Read here, in this process: only the bounded reader's child calls it (`reader.rs`).
pub(super) fn ahead_behind_here(plane: &Path, branch: Branch<'_>) -> Result<AheadBehind, Refused> {
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("how far {} is from its base", branch.called()),
        why,
    };
    let opened = open(plane, branch)?;
    let charter_base = opened.head_base();
    let repo = opened.repo;
    let none = AheadBehind {
        ahead: 0,
        behind: 0,
        base: None,
    };
    let Some(head) = repo.head_commit().ok().map(|commit| commit.id) else {
        return Ok(none);
    };
    let Some(Based { at, named, .. }) = charter_base
        .as_deref()
        .and_then(|value| recorded_base(&repo, value, head))
    else {
        return Ok(none);
    };
    let count = |from: gix::ObjectId, hidden: gix::ObjectId| -> Result<usize, String> {
        let walk = repo
            .rev_walk([from])
            .with_hidden([hidden])
            .all()
            .map_err(|e| e.to_string())?;
        let mut n = 0;
        for commit in walk {
            commit.map_err(|e| e.to_string())?;
            n += 1;
        }
        Ok(n)
    };
    Ok(AheadBehind {
        ahead: count(head, at).map_err(unreadable)?,
        behind: count(at, head).map_err(unreadable)?,
        base: Some(named),
    })
}

/// A recorded base, resolved: the commit it names, where the branch left it, and its name.
pub(super) struct Based {
    pub(super) at: gix::ObjectId,
    pub(super) fork: gix::ObjectId,
    pub(super) named: String,
}

/// Where the branch left its recorded base, and the base's name: `None` when the record does
/// not resolve to a commit HEAD shares.
pub(super) fn recorded_base(
    repo: &gix::Repository,
    value: &str,
    head: gix::ObjectId,
) -> Option<Based> {
    let (at, named) = match value.strip_prefix(worktree::DETACHED_PREFIX) {
        Some(sha) => {
            let sha = sha.trim();
            let full = sha.len() == 40 || sha.len() == 64;
            if !full || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            (
                gix::ObjectId::from_hex(sha.as_bytes()).ok()?,
                sha.to_string(),
            )
        }
        None => {
            name::branch_name_ok(value).ok()?;
            let mut reference = repo.find_reference(name::as_ref(value).as_str()).ok()?;
            (reference.peel_to_commit().ok()?.id, value.to_string())
        }
    };
    repo.find_commit(at).ok()?;
    let fork = repo.merge_base(at, head).ok()?.detach();
    Some(Based { at, fork, named })
}

/// The most threads a status compares the working tree on: the machine's cores, at most
/// [`THREADS`].
pub(super) fn threads() -> usize {
    std::thread::available_parallelism().map_or(1, |cores| cores.get().min(THREADS))
}

/// The cap on [`threads`]. A status runs after every write an agent makes, on a machine the
/// agents' builds share; past eight threads the compare of 100,000 files gains little (#1153).
/// Each thread holds its own buffers under the reader's [`MEMORY`](super::MEMORY) watchdog,
/// and on Linux each counts toward the user's `RLIMIT_NPROC`, which a busy machine's builds
/// already spend.
const THREADS: usize = 8;

/// `path` when it is a plain path inside the branch: relative, no `..`, and never git's own.
/// gitoxide's paths are bytes, read here lossily: a name that was not UTF-8 holds U+FFFD and is
/// dropped, as the tree refuses it.
pub(super) fn plain(path: &str) -> Option<String> {
    let relative = inside(path).ok()?;
    // git's own folder in any ASCII case, as `files::open` refuses it: on a case-insensitive
    // folder `.GIT` is `.git`.
    let git_s = super::gits(relative);
    (!git_s && !path.contains('\\') && !path.contains('\u{FFFD}')).then(|| path.to_string())
}

/// Every folder holding a change, with the mark its changes share and how many it holds.
fn rolled_up(changes: &[Change]) -> Vec<Rolled> {
    let mut folders: BTreeMap<String, (BTreeSet<Mark>, usize)> = BTreeMap::new();
    for change in changes {
        let mut above = Some(change.path.as_str());
        while let Some(path) = above {
            let parent = path.rfind('/').map(|at| &path[..at]);
            let folder = parent.unwrap_or("");
            let entry = folders.entry(folder.to_string()).or_default();
            entry.0.insert(change.mark);
            entry.1 += 1;
            above = parent;
        }
    }
    folders
        .into_iter()
        .map(|(folder, (marks, count))| Rolled {
            folder,
            mark: match marks.iter().collect::<Vec<_>>().as_slice() {
                [one] => **one,
                _ => Mark::Changed,
            },
            count,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1550: gitoxide is given no `safe.bareRepository`, and needs none: a bare repository
    /// where a clone should be is refused, never read in its place.
    #[test]
    fn a_bare_repository_where_a_clone_should_be_is_never_read() {
        let plane = tempfile::tempdir().unwrap();
        let clone = plane.path().join("workspaces").join("alpha").join("widget");
        std::fs::create_dir_all(&clone).unwrap();
        assert!(crate::testgit::run_unconfigured(&clone, &["init", "-q", "--bare", "."]).ok());
        let branch = Branch {
            ws: "alpha",
            repo: "widget",
            piece: None,
        };
        assert!(open(plane.path(), branch).is_err());
    }

    #[test]
    fn of_what_the_config_said_only_the_keys_a_read_needs_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let ran = crate::forklock::output(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir.path())
                    .args(args)
                    .env("GIT_CONFIG_GLOBAL", "/dev/null"),
            )
            .unwrap();
            assert!(ran.status.success(), "git {args:?}");
        };
        git(&["init", "-q"]);
        for (key, value) in [
            ("core.bigFileThreshold", "1"),
            ("core.deltaBaseCacheLimit", "1"),
            ("gitoxide.objects.cacheLimit", "1"),
            ("filter.x.clean", "/bin/false"),
            ("diff.d.textconv", "/bin/false"),
            ("core.ignoreCase", "true"),
            ("index.threads", "1000"),
        ] {
            git(&["config", key, value]);
        }
        let mut repo = gix::open_opts(dir.path(), options()).unwrap();

        allowed_only(&mut repo).unwrap();

        let config = repo.config_snapshot();
        for gone in [
            "core.bigFileThreshold",
            "core.deltaBaseCacheLimit",
            "gitoxide.objects.cacheLimit",
            "filter.x.clean",
            "diff.d.textconv",
            "index.threads",
        ] {
            assert_eq!(config.string(gone), None, "{gone} was kept");
        }
        assert_eq!(config.boolean("core.ignoreCase"), Some(true));
    }

    /// #1153: without gitoxide's `parallel` feature, the working tree is compared one file at a
    /// time and the walk for untracked files waits behind it: 1.4 s at 100,000 files against
    /// git's 0.3 s, and 7–24 s cold. With it, both run at once, and the compare on every core.
    #[test]
    fn a_status_compares_the_working_tree_on_every_core() {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        assert_eq!(gix::features::parallel::num_threads(None), cores);
    }

    /// #1153 review: the compare runs on every core, but never on more than eight threads,
    /// whatever the machine or the branch's config says.
    #[test]
    fn a_status_compares_on_at_most_eight_threads() {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        assert_eq!(THREADS, 8);
        assert_eq!(threads(), cores.min(8));
    }

    #[test]
    fn a_path_that_would_leave_the_branch_or_reach_git_is_never_answered() {
        for path in [
            "../out.txt",
            ".git/config",
            "sub/.git/HEAD",
            "/etc/passwd",
            "a\\b",
        ] {
            assert_eq!(plain(path), None, "{path}");
        }
        assert_eq!(plain("in dir/ok.txt"), Some("in dir/ok.txt".to_string()));
    }
}
