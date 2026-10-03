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
//!   difference between the fork point's tree and HEAD's, with renames found in both.
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
    let Opened {
        repo, charter_base, ..
    } = open(plane, branch)?;
    let head = repo.head_commit().ok().map(|commit| commit.id);
    let recorded = charter_base
        .as_deref()
        .and_then(|value| head.and_then(|head| recorded_base(&repo, value, head)));
    let (since, named) = match recorded {
        Some((fork, named)) => (Some(fork), Some(named)),
        None => (head, None),
    };

    // Committed: the fork point's tree against HEAD's.
    let mut changes: BTreeMap<String, Change> = BTreeMap::new();
    let mut renamed: Vec<(String, String)> = Vec::new();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    if let (Some(since), Some(head)) = (since, head)
        && since != head
    {
        committed(&repo, since, head, &mut touched, &mut renamed).map_err(unreadable)?;
    }

    // Not committed: HEAD → index → working tree, and what git does not track.
    let mut loose: BTreeSet<String> = BTreeSet::new();
    let mut untracked: BTreeSet<String> = BTreeSet::new();
    let mut gone: BTreeSet<String> = BTreeSet::new();
    uncommitted(&repo, &mut loose, &mut untracked, &mut gone, &mut renamed).map_err(unreadable)?;
    touched.extend(loose.iter().cloned());
    touched.extend(untracked.iter().cloned());

    let index = repo
        .index_or_empty()
        .map_err(|e| unreadable(e.to_string()))?;
    let fork_tree = match since {
        Some(id) => Some(
            repo.find_commit(id)
                .and_then(|commit| commit.tree())
                .map_err(|e| unreadable(e.to_string()))?,
        ),
        None => None,
    };
    let at_fork = |path: &str| -> bool {
        fork_tree.as_ref().is_some_and(|tree| {
            tree.lookup_entry_by_path(path)
                .ok()
                .flatten()
                .is_some_and(|entry| !entry.mode().is_tree())
        })
    };
    for path in &touched {
        let Some(plain) = plain(path) else { continue };
        let now = untracked.contains(path)
            || (index
                .entry_by_path(path.as_str().into())
                .is_some_and(|entry| !entry.mode.is_submodule())
                && !gone.contains(path));
        let then = at_fork(path);
        let mark = match (then, now) {
            (true, true) => Mark::Changed,
            (false, true) => Mark::Added,
            (true, false) => Mark::Deleted,
            (false, false) => continue,
        };
        changes.insert(
            plain.clone(),
            Change {
                path: plain,
                mark,
                from: None,
                uncommitted: loose.contains(path) || untracked.contains(path),
            },
        );
    }
    // A file added where one the fork point had is now gone is that file, moved.
    for (from, to) in renamed {
        let (Some(from), Some(to)) = (plain(&from), plain(&to)) else {
            continue;
        };
        let moved = changes.get(&to).is_some_and(|one| one.mark == Mark::Added)
            && changes
                .get(&from)
                .is_some_and(|one| one.mark == Mark::Deleted);
        if moved {
            changes.remove(&from);
            if let Some(one) = changes.get_mut(&to) {
                one.mark = Mark::Renamed;
                one.from = Some(from);
            }
        }
    }

    let all: Vec<Change> = changes.into_values().collect();
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
    let charter_base = recorded_value(&opened);
    allowed_only(&mut opened).map_err(|why| Refused::Unreadable {
        what: branch.called().to_string(),
        why,
    })?;
    Ok(Opened {
        base,
        repo: opened,
        charter_base,
    })
}

/// A branch, opened in-process.
pub(super) struct Opened {
    /// The branch's folder, resolved.
    pub(super) base: PathBuf,
    pub(super) repo: gix::Repository,
    /// The base the branch was cut from, as recorded: one value, or `None` for none or several.
    charter_base: Option<String>,
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

/// `branch.<HEAD's branch>.charterBase`, when it holds exactly one value.
fn recorded_value(repo: &gix::Repository) -> Option<String> {
    let current = repo.head_name().ok()??;
    let branch = current.shorten().to_string();
    name::branch_name_ok(&branch).ok()?;
    let values: Vec<String> = repo
        .config_snapshot()
        .plumbing()
        .strings_by(
            "branch",
            Some(gix::bstr::BStr::new(branch.as_bytes())),
            "charterBase",
        )?
        .into_iter()
        .map(|value| value.to_string().trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    match values.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
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

/// Where the branch left its recorded base, and the base's name: `None` when the record does
/// not resolve to a commit HEAD shares.
fn recorded_base(
    repo: &gix::Repository,
    value: &str,
    head: gix::ObjectId,
) -> Option<(gix::ObjectId, String)> {
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
    Some((fork, named))
}

/// What the commits from `since` to `head` changed, each path into `touched`, and each rename
/// found among them into `renamed`.
fn committed(
    repo: &gix::Repository,
    since: gix::ObjectId,
    head: gix::ObjectId,
    touched: &mut BTreeSet<String>,
    renamed: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let tree = |id: gix::ObjectId| {
        repo.find_commit(id)
            .and_then(|commit| commit.tree())
            .map_err(|e| e.to_string())
    };
    let (old, new) = (tree(since)?, tree(head)?);
    let options = gix::diff::Options::default().with_rewrites(Some(gix::diff::Rewrites::default()));
    let changes = repo
        .diff_tree_to_tree(&old, &new, options)
        .map_err(|e| e.to_string())?;
    use gix::object::tree::diff::ChangeDetached;
    for change in changes {
        match change {
            ChangeDetached::Addition {
                location,
                entry_mode,
                ..
            }
            | ChangeDetached::Deletion {
                location,
                entry_mode,
                ..
            }
            | ChangeDetached::Modification {
                location,
                entry_mode,
                ..
            } => {
                if !entry_mode.is_tree() {
                    touched.insert(location.to_string());
                }
            }
            ChangeDetached::Rewrite {
                source_location,
                location,
                entry_mode,
                copy,
                ..
            } => {
                if entry_mode.is_tree() {
                    continue;
                }
                touched.insert(location.to_string());
                if !copy {
                    touched.insert(source_location.to_string());
                    renamed.push((source_location.to_string(), location.to_string()));
                }
            }
        }
    }
    Ok(())
}

/// What is not committed: each path HEAD → index → working tree changed into `loose`, each
/// file git does not track and does not ignore into `untracked`, each tracked file gone from
/// the working tree into `gone`, and each rename staged into `renamed`.
fn uncommitted(
    repo: &gix::Repository,
    loose: &mut BTreeSet<String>,
    untracked: &mut BTreeSet<String>,
    gone: &mut BTreeSet<String>,
    renamed: &mut Vec<(String, String)>,
) -> Result<(), String> {
    use gix::status::{Item, index_worktree};
    let items = repo
        .status(gix::progress::Discard)
        .map_err(|e| e.to_string())?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .index_worktree_submodules(None)
        .tree_index_track_renames(gix::status::tree_index::TrackRenames::Given(
            gix::diff::Rewrites::default(),
        ))
        .into_iter(None)
        .map_err(|e| e.to_string())?;
    for item in items {
        let item = item.map_err(|e| e.to_string())?;
        match item {
            Item::TreeIndex(change) => {
                use gix::diff::index::ChangeRef;
                match change {
                    ChangeRef::Rewrite {
                        source_location,
                        location,
                        copy,
                        ..
                    } => {
                        loose.insert(location.to_string());
                        if !copy {
                            loose.insert(source_location.to_string());
                            renamed.push((source_location.to_string(), location.to_string()));
                        }
                    }
                    other => {
                        loose.insert(other.location().to_string());
                    }
                }
            }
            Item::IndexWorktree(index_worktree::Item::Modification {
                rela_path, status, ..
            }) => {
                use gix::status::plumbing::index_as_worktree::{Change as Worktree, EntryStatus};
                match status {
                    EntryStatus::Change(Worktree::Removed) => {
                        gone.insert(rela_path.to_string());
                        loose.insert(rela_path.to_string());
                    }
                    EntryStatus::Change(Worktree::SubmoduleModification(_)) => {}
                    EntryStatus::NeedsUpdate(_) => {}
                    _ => {
                        loose.insert(rela_path.to_string());
                    }
                }
            }
            Item::IndexWorktree(index_worktree::Item::DirectoryContents { entry, .. }) => {
                use gix::dir::entry::{Kind, Status};
                let file = matches!(entry.disk_kind, Some(Kind::File | Kind::Symlink));
                if entry.status == Status::Untracked && file {
                    untracked.insert(entry.rela_path.to_string());
                }
            }
            Item::IndexWorktree(index_worktree::Item::Rewrite { .. }) => {}
        }
    }
    Ok(())
}

/// `path` when it is a plain path inside the branch: relative, no `..`, and never git's own.
/// gitoxide's paths are bytes, read here lossily: a name that was not UTF-8 holds U+FFFD and is
/// dropped, as the tree refuses it.
fn plain(path: &str) -> Option<String> {
    let relative = inside(path).ok()?;
    let git_s = relative.components().any(|step| step.as_os_str() == ".git");
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
        ] {
            assert_eq!(config.string(gone), None, "{gone} was kept");
        }
        assert_eq!(config.boolean("core.ignoreCase"), Some(true));
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
