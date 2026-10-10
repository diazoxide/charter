//! What a branch changed, file by file and rolled up onto its folders (FM-4, #1107; #1103 V86
//! F6): the tree's markers, and what "Changed only" collapses it to.
//!
//! **Against the branch it was cut from**, committed or not. The base is the one fact charter
//! records about a piece, `branch.<branch>.charterBase` in the clone's config (ADR 0027,
//! `worktree::add`), and the changes are counted from where the branch left it (the merge
//! base), so what the base gained since is not the branch's change. A branch with no recorded
//! base — a branch cut by plain git — is marked against its last commit: what is not committed
//! yet. **The repo's own folder records none, so its upstream stands in** (#1130): the branch
//! `branch.<head>.remote` and `.merge` name, as `refs/remotes/<remote>/<branch>`, read from the
//! config before it is cut and held to the same rule as a recorded base. With no upstream that
//! passes, it too is marked against its last commit.
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
    let held = super::compare::hold(plane, &opened.base).map_err(unreadable)?;
    let head = opened.repo.head_commit().ok().map(|commit| commit.id);
    let based = head.and_then(|head| opened.head_based(head));
    let repo = opened.repo;
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
    // The folder is the work tree, never the repository's own git directory: a git directory
    // made at the folder, naming the folder as its `core.worktree`, is not a clone (#1550).
    let git_dir = std::fs::canonicalize(opened.git_dir()).ok();
    if workdir.as_deref() != Some(base.as_path()) || git_dir.as_deref() == Some(base.as_path()) {
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
    let upstream = match piece {
        None => upstream_of_head(&opened),
        Some(_) => None,
    };
    allowed_only(&mut opened).map_err(|why| Refused::Unreadable {
        what: branch.called().to_string(),
        why,
    })?;
    Ok(Opened {
        base,
        repo: opened,
        recorded,
        upstream,
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
    /// The repo's own folder's upstream (#1130), read before the config was cut: the branch
    /// checked out there by name, and the remote-tracking ref it follows, validated by
    /// [`upstream_of_head`]. Always `None` for a piece, which records its base.
    pub(super) upstream: Option<(String, String)>,
}

impl Opened {
    /// The base of the branch checked out in the folder, resolved against `head`: its recorded
    /// base, else, in the repo's own folder, its upstream; `None` when neither resolves to a
    /// commit `head` shares.
    pub(super) fn head_based(&self, head: gix::ObjectId) -> Option<Based> {
        let current = self.repo.head_name().ok()??.shorten().to_string();
        self.based(&current, head)
    }

    /// [`Self::head_based`], for the branch `name` with its tip at `tip`: the upstream stands in
    /// only for the branch it was read for.
    pub(super) fn based(&self, name: &str, tip: gix::ObjectId) -> Option<Based> {
        if let Some(value) = self.recorded.get(name) {
            return recorded_base(&self.repo, value, tip);
        }
        let (of, upstream) = self.upstream.as_ref()?;
        if of != name {
            return None;
        }
        let mut reference = self.repo.find_reference(upstream.as_str()).ok()?;
        let at = reference.peel_to_commit().ok()?.id;
        let named = reference.name().shorten().to_string();
        let fork = self.repo.merge_base(at, tip).ok()?.detach();
        Some(Based { at, fork, named })
    }
}

/// The remote-tracking ref the branch checked out in `repo` follows (`@{upstream}`), as
/// `(branch, refs/remotes/…)`: read from `branch.<branch>.remote` and `branch.<branch>.merge`,
/// each holding exactly one value, and mapped through `remote.<remote>.fetch` as git maps it
/// ([`tracking_ref`], #1130). **Read, never trusted**: anything working in the folder can write
/// its config, so the remote must be one plain name — no `/`, not `.` (a local upstream) — the
/// merged branch must be under `refs/heads/`, and the ref it maps to must be under
/// `refs/remotes/` and a name git would accept (no `..`, no control characters). Anything else
/// is no upstream. Only a remote-tracking ref is ever named, never a pattern or a path.
fn upstream_of_head(repo: &gix::Repository) -> Option<(String, String)> {
    let head = repo.head_name().ok()??;
    let head = head
        .as_ref()
        .category_and_short_name()
        .and_then(|(kind, short)| {
            (kind == gix::refs::Category::LocalBranch).then(|| short.to_string())
        })?;
    name::branch_name_ok(&head).ok()?;
    let config = repo.config_snapshot();
    let mut remotes = Vec::new();
    let mut merges = Vec::new();
    for section in config.plumbing().sections_by_name("branch")? {
        if section.header().subsection_name().map(|n| n.to_string()) != Some(head.clone()) {
            continue;
        }
        remotes.extend(section.values("remote").iter().map(|v| v.to_string()));
        merges.extend(section.values("merge").iter().map(|v| v.to_string()));
    }
    let ([remote], [merge]) = (remotes.as_slice(), merges.as_slice()) else {
        return None;
    };
    let (remote, merge) = (remote.trim(), merge.trim());
    name::branch_name_ok(remote).ok()?;
    if remote.contains('/') || remote == "." {
        return None;
    }
    let merged = merge.strip_prefix("refs/heads/")?;
    name::branch_name_ok(merged).ok()?;
    let mut fetch = Vec::new();
    for section in config.plumbing().sections_by_name("remote")? {
        if section
            .header()
            .subsection_name()
            .map(|n| n.to_string())
            .as_deref()
            != Some(remote)
        {
            continue;
        }
        fetch.extend(section.values("fetch").iter().map(|v| v.to_string()));
    }
    Some((head, tracking_ref(&fetch, merged)?))
}

/// The remote-tracking ref the remote's `fetch` refspecs map `refs/heads/<merged>` to, as git's
/// `@{upstream}` finds it (#1130): the first refspec that maps it somewhere wins (one with no
/// destination is passed over, as git passes it over), a negative refspec that
/// matches it leaves it unmapped, and a refspec that does not parse is passed over. Only a ref
/// under `refs/remotes/` that git would accept is answered: a refspec mapping the branch onto a
/// local branch, a tag or nowhere gives no upstream.
fn tracking_ref(fetch: &[String], merged: &str) -> Option<String> {
    use gix::bstr::ByteSlice as _;
    let specs: Vec<gix::refspec::RefSpec> = fetch
        .iter()
        .filter_map(|spec| {
            gix::refspec::parse(
                spec.trim().as_bytes().as_bstr(),
                gix::refspec::parse::Operation::Fetch,
            )
            .ok()
            .map(|spec| spec.to_owned())
        })
        .collect();
    let full = format!("refs/heads/{merged}");
    let null = gix::ObjectId::null(gix::hash::Kind::Sha1);
    let item = gix::refspec::match_group::Item {
        full_ref_name: full.as_bytes().as_bstr(),
        target: &null,
        object: None,
    };
    let group = gix::refspec::MatchGroup::from_fetch_specs(specs.iter().map(|spec| spec.to_ref()));
    let mapped = group
        .match_lhs(std::iter::once(item))
        .mappings
        .into_iter()
        .filter(|mapping| mapping.rhs.is_some())
        .min_by_key(|mapping| mapping.spec_index)?
        .rhs?
        .to_str()
        .ok()?
        .to_string();
    mapped.strip_prefix("refs/remotes/")?;
    gix::refs::FullName::try_from(mapped.as_str()).ok()?;
    Some(mapped)
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
/// is not its repository's work tree, or is its git directory, so neither a bare repository nor
/// a git directory made at the folder is read here (#1415, #1550).
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
    /// Commits the branch has that its base does not, counted up to one past
    /// [`MOST_COUNTED`]: that number means more than [`MOST_COUNTED`].
    pub ahead: usize,
    /// Commits its base gained that the branch does not have, counted as `ahead` is.
    pub behind: usize,
    /// The base they are counted against, as recorded; `None` when the branch has no base
    /// recorded that resolves, and then both counts are `0` and mean nothing.
    pub base: Option<String>,
}

/// The most commits [`AheadBehind`] counts exactly on either side (#1152). A branch thousands of
/// commits off its base would otherwise walk until the reader's deadline and show a refusal
/// instead of a number. Each side is counted to one past this, so a count of exactly
/// `MOST_COUNTED` is said as it is, and only one past it as "10,000+".
///
/// **What the cap does not bound** (#1152, measured 2026-10-09). gitoxide's walk paints both
/// sides back to where they meet before it yields its first commit, so the work before the
/// count is the distance to the fork point on both sides, whatever the cap: with 200,000
/// commits on each side, 5.7 s before the first commit in a debug build, 0.75 s with a
/// commit-graph, and 0.13 s to count to the cap after that. Finding the merge base costs the
/// same paint again. A base from another history never reaches the count: [`recorded_base`]
/// paints both histories whole, finds no merge base, and the branch reads as having no base.
/// All of it is bounded by the reader's deadline and memory cap, as every read is.
pub const MOST_COUNTED: usize = 10_000;

/// How far the branch is from its recorded base, counted as `git rev-list --left-right
/// --count <base>...HEAD` counts, through gitoxide with the config cut down as [`open`] cuts
/// it. Read here, in this process: only the bounded reader's child calls it (`reader.rs`).
pub(super) fn ahead_behind_here(plane: &Path, branch: Branch<'_>) -> Result<AheadBehind, Refused> {
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("how far {} is from its base", branch.called()),
        why,
    };
    let opened = open(plane, branch)?;
    let none = AheadBehind {
        ahead: 0,
        behind: 0,
        base: None,
    };
    let Some(head) = opened.repo.head_commit().ok().map(|commit| commit.id) else {
        return Ok(none);
    };
    // The same base the markers count from: the recorded one, else the repo's own folder's
    // upstream, so the header and the tree never disagree on what the branch is measured by.
    let Some(Based { at, named, .. }) = opened.head_based(head) else {
        return Ok(none);
    };
    let repo = opened.repo;
    let count = |from: gix::ObjectId, hidden: gix::ObjectId| -> Result<usize, String> {
        let walk = repo
            .rev_walk([from])
            .with_hidden([hidden])
            .all()
            .map_err(|e| e.to_string())?;
        counted(walk)
    };
    Ok(AheadBehind {
        ahead: count(head, at).map_err(unreadable)?,
        behind: count(at, head).map_err(unreadable)?,
        base: Some(named),
    })
}

/// How many commits `walk` yields, counted to one past [`MOST_COUNTED`]: exactly
/// [`MOST_COUNTED`] is a count, and only more than that reads as "that many or more".
fn counted<T, E: std::fmt::Display>(
    walk: impl IntoIterator<Item = Result<T, E>>,
) -> Result<usize, String> {
    counted_up_to(walk, MOST_COUNTED + 1)
}

/// How many commits `walk` yields, stopping at `most`: the walk is not read past it.
fn counted_up_to<T, E: std::fmt::Display>(
    walk: impl IntoIterator<Item = Result<T, E>>,
    most: usize,
) -> Result<usize, String> {
    let mut n = 0;
    for commit in walk.into_iter().take(most) {
        commit.map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
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
///
/// A renamed file is counted in the folders above its new path, and also in those above where it
/// came from, as a file each of them lost (#1130): `src/` shows the file moved out of it, as
/// [`Mark::Deleted`]. A folder above both is counted once, as the rename.
fn rolled_up(changes: &[Change]) -> Vec<Rolled> {
    let mut folders: BTreeMap<String, (BTreeSet<Mark>, usize)> = BTreeMap::new();
    for change in changes {
        let above = folders_above(&change.path);
        for folder in &above {
            let entry = folders.entry((*folder).to_string()).or_default();
            entry.0.insert(change.mark);
            entry.1 += 1;
        }
        let Some(from) = change.from.as_deref() else {
            continue;
        };
        for folder in folders_above(from) {
            if above.contains(&folder) {
                continue;
            }
            let entry = folders.entry(folder.to_string()).or_default();
            entry.0.insert(Mark::Deleted);
            entry.1 += 1;
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

/// The folders above `path`, nearest first, the branch's own (`""`) last.
fn folders_above(path: &str) -> Vec<&str> {
    let mut above = Vec::new();
    let mut at = path;
    while let Some(cut) = at.rfind('/') {
        at = &at[..cut];
        above.push(at);
    }
    above.push("");
    above
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(all: &[&str]) -> Vec<String> {
        all.iter().map(|one| (*one).to_string()).collect()
    }

    /// #1130: the upstream follows the remote's fetch refspec, as git's `@{upstream}` does, so a
    /// remote fetched into `refs/remotes/up/` is followed there.
    #[test]
    fn an_upstream_follows_the_remotes_fetch_refspec() {
        let tracked = tracking_ref(&specs(&["+refs/heads/*:refs/remotes/up/*"]), "main");

        assert_eq!(tracked.as_deref(), Some("refs/remotes/up/main"));
    }

    /// #1130: with git's default refspec the upstream is the ref it always was.
    #[test]
    fn the_default_refspec_gives_the_remotes_own_tracking_ref() {
        let tracked = tracking_ref(
            &specs(&["+refs/heads/*:refs/remotes/origin/*"]),
            "topic/one",
        );

        assert_eq!(tracked.as_deref(), Some("refs/remotes/origin/topic/one"));
    }

    /// #1130: the first refspec that maps the branch wins, as in git; one fetched by name
    /// counts as well as a pattern, and one with no destination maps nothing and is passed over.
    #[test]
    fn the_first_refspec_that_maps_the_branch_wins() {
        let fetch = specs(&[
            "refs/heads/main",
            "+refs/heads/release:refs/remotes/rel/release",
            "refs/heads/main:refs/remotes/mine/main",
            "+refs/heads/*:refs/remotes/origin/*",
        ]);

        assert_eq!(
            tracking_ref(&fetch, "main").as_deref(),
            Some("refs/remotes/mine/main")
        );
        assert_eq!(
            tracking_ref(&fetch, "other").as_deref(),
            Some("refs/remotes/origin/other")
        );
    }

    /// #1130: no refspec that maps the branch, one a negative refspec leaves out, or one that
    /// maps it outside `refs/remotes/` gives no upstream: no base, as before.
    #[test]
    fn a_branch_no_refspec_maps_to_a_remote_tracking_ref_has_no_upstream() {
        assert_eq!(tracking_ref(&[], "main"), None);
        assert_eq!(
            tracking_ref(&specs(&["+refs/heads/dev:refs/remotes/o/dev"]), "main"),
            None
        );
        assert_eq!(
            tracking_ref(
                &specs(&["+refs/heads/*:refs/remotes/o/*", "^refs/heads/main"]),
                "main"
            ),
            None
        );
        for elsewhere in [
            "+refs/heads/*:refs/heads/*",
            "+refs/heads/*:refs/tags/*",
            "refs/heads/main",
            "not a refspec at all",
        ] {
            assert_eq!(
                tracking_ref(&specs(&[elsewhere]), "main"),
                None,
                "{elsewhere}"
            );
        }
    }

    /// #1152: a count stops at its cap, and reads no commit past it.
    #[test]
    fn a_count_stops_at_its_cap_and_walks_no_further() {
        let mut walked = 0;
        let walk = std::iter::repeat_with(|| {
            walked += 1;
            Ok::<(), String>(())
        })
        .take(MOST_COUNTED * 2);

        let counted = counted_up_to(walk, MOST_COUNTED);

        assert_eq!(counted, Ok(MOST_COUNTED));
        assert_eq!(walked, MOST_COUNTED);
    }

    /// #1152 review: a branch exactly [`MOST_COUNTED`] commits away is counted as that many;
    /// only one past it is counted as more, and the walk stops there.
    #[test]
    fn exactly_the_cap_is_a_count_and_only_past_it_is_more() {
        let exactly = (0..MOST_COUNTED).map(Ok::<usize, String>);
        assert_eq!(counted(exactly), Ok(MOST_COUNTED));

        let mut walked = 0;
        let far = std::iter::repeat_with(|| {
            walked += 1;
            Ok::<(), String>(())
        })
        .take(MOST_COUNTED * 2);
        assert_eq!(counted(far), Ok(MOST_COUNTED + 1));
        assert_eq!(walked, MOST_COUNTED + 1);
    }

    /// Under the cap a count is every commit, and a commit that cannot be read is said.
    #[test]
    fn under_its_cap_a_count_is_every_commit_and_a_bad_one_is_said() {
        let three = (0..3).map(Ok::<u32, String>);
        assert_eq!(counted_up_to(three, MOST_COUNTED), Ok(3));

        let broken = [Ok(1), Err("object missing".to_string())];
        assert_eq!(
            counted_up_to(broken, MOST_COUNTED),
            Err("object missing".to_string())
        );
    }

    /// A plane whose `workspaces/alpha/widget` is a git directory made by `make`, opened as
    /// the clone it should be.
    fn opened_where_a_clone_should_be(make: &[&[&str]]) -> Result<Opened, Refused> {
        let plane = tempfile::tempdir().unwrap();
        let clone = plane.path().join("workspaces").join("alpha").join("widget");
        std::fs::create_dir_all(&clone).unwrap();
        for args in make {
            assert!(
                crate::testgit::run_unconfigured(&clone, args).ok(),
                "{args:?}"
            );
        }
        let branch = Branch {
            ws: "alpha",
            repo: "widget",
            piece: None,
        };
        open(plane.path(), branch)
    }

    /// #1550: gitoxide is given no `safe.bareRepository`, and needs none: a bare repository
    /// where a clone should be is refused as no repository, never read in its place.
    #[test]
    fn a_bare_repository_where_a_clone_should_be_is_never_read() {
        let opened = opened_where_a_clone_should_be(&[&["init", "-q", "--bare", "."]]);
        assert!(
            matches!(opened, Err(Refused::Piece(worktree::Refusal::NotARepo(_)))),
            "{:?}",
            opened.err()
        );
    }

    /// #1550: nor is a git directory made at the folder that names the folder itself as its
    /// work tree, which reads as a work tree to gitoxide.
    #[test]
    fn a_git_directory_naming_its_own_folder_as_its_work_tree_is_never_read() {
        let opened = opened_where_a_clone_should_be(&[
            &["init", "-q", "--bare", "."],
            &["config", "core.bare", "false"],
            &["config", "core.worktree", "."],
        ]);
        assert!(
            matches!(opened, Err(Refused::Piece(worktree::Refusal::NotARepo(_)))),
            "{:?}",
            opened.err()
        );
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

    fn change(path: &str, mark: Mark, from: Option<&str>) -> Change {
        Change {
            path: path.to_string(),
            mark,
            from: from.map(str::to_string),
            uncommitted: true,
        }
    }

    fn rolled(folder: &str, mark: Mark, count: usize) -> Rolled {
        Rolled {
            folder: folder.to_string(),
            mark,
            count,
        }
    }

    /// #1130: a file moved out of `src/` marks `src/` as having lost it; a folder above both
    /// ends counts the rename once.
    #[test]
    fn a_renames_source_folders_are_rolled_up_as_losing_the_file() {
        let changes = [change("lib/deep/a.rs", Mark::Renamed, Some("src/old/a.rs"))];

        assert_eq!(
            rolled_up(&changes),
            [
                rolled("", Mark::Renamed, 1),
                rolled("lib", Mark::Renamed, 1),
                rolled("lib/deep", Mark::Renamed, 1),
                rolled("src", Mark::Deleted, 1),
                rolled("src/old", Mark::Deleted, 1),
            ]
        );
    }

    /// #1130: within one folder a rename is one change there; where the source folder holds
    /// other changes too, it is marked as changed, counting each.
    #[test]
    fn a_renames_source_folder_with_other_changes_is_marked_changed() {
        let changes = [
            change("src/b.rs", Mark::Renamed, Some("src/a.rs")),
            change("src/kept/c.rs", Mark::Added, None),
            change("top.rs", Mark::Renamed, Some("src/kept/d.rs")),
        ];

        assert_eq!(
            rolled_up(&changes),
            [
                rolled("", Mark::Changed, 3),
                rolled("src", Mark::Changed, 3),
                rolled("src/kept", Mark::Changed, 2),
            ]
        );
    }
}
