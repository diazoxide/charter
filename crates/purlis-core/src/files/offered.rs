//! The two git reads the file commands make, read by gitoxide in the bounded child (#1189,
//! FM-11): the branch's **offered list** — what `git ls-files --cached --others
//! --exclude-standard` answers — and, for one folder level of the tree, **which paths git
//! ignores** and which folders are submodules — what `git check-ignore` and `git ls-files
//! --stage` answer.
//!
//! The app's file commands ask these of the reader ([`super::Ask::Offered`],
//! [`super::Ask::Ignored`]), so the app process starts no git to read a branch an agent can
//! write. The command line and the tests keep the free functions in `files.rs`, which run the
//! hardened git: the answers here are held to be theirs.
//!
//! **An index gitoxide cannot read** is answered as such ([`super::Answer::Unindexed`]), not
//! refused: the file command then reads that branch with the hardened git in the app, as it did
//! before #1189, so a branch tracking a path gitoxide's index decoder trips on (#1130) still
//! lists its files. Only that answer, from a child that answered within its bounds, does so.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::status::open;
use super::{Answer, Branch, Refused};

/// What the child answers [`super::Ask::Offered`] with: the offered list, sorted and once each,
/// cut at the most asked for, and how long it was before the cut.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Offered {
    pub files: Vec<String>,
    pub total: usize,
}

/// What the child answers [`super::Ask::Ignored`] with: whether git ignores each path asked, by
/// position, and which of the folders asked the index records as a submodule.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Ignored {
    pub ignored: Vec<bool>,
    pub gitlinks: Vec<PathBuf>,
}

/// The branch's offered list, here, in this process: the bounded child's half. Every path the
/// index records, once whatever its stages, and every file the walk finds that git does not
/// track and does not ignore; an untracked repository nested in the branch is one entry, its
/// path and a `/`, as git lists it. At most `most`, after sorting. An index gitoxide cannot
/// read is answered [`Answer::Unindexed`].
pub(super) fn offered_here(
    plane: &Path,
    branch: Branch<'_>,
    most: Option<usize>,
) -> Result<Answer, Refused> {
    use gix::dir::entry::{Kind, Status};
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("the files of {}", branch.called()),
        why,
    };
    let repo = open(plane, branch)?.repo;
    let index = match repo.index_or_empty() {
        Ok(index) => index,
        Err(e) => return Ok(Answer::Unindexed(e.to_string())),
    };
    let mut files: BTreeSet<String> = index
        .entries()
        .iter()
        .map(|entry| entry.path(&index).to_string())
        .collect();
    // git walks into no submodule, checked out or not: a file in one is the submodule's.
    let submodules: BTreeSet<String> = index
        .entries()
        .iter()
        .filter(|entry| entry.mode == gix::index::entry::Mode::COMMIT)
        .map(|entry| entry.path(&index).to_string())
        .collect();
    let in_submodule = |path: &str| {
        path.match_indices('/')
            .any(|(at, _)| submodules.contains(&path[..at]))
    };
    let options = repo
        .dirwalk_options()
        .map_err(|e| unreadable(e.to_string()))?
        .emit_tracked(false)
        .emit_untracked(gix::dir::walk::EmissionMode::Matching)
        .emit_ignored(None)
        .emit_empty_directories(false)
        .recurse_repositories(false);
    let walk = repo
        .dirwalk_iter(
            index,
            None::<gix::bstr::BString>,
            Default::default(),
            options,
        )
        .map_err(|e| unreadable(e.to_string()))?;
    for item in walk {
        let item = item.map_err(|e| unreadable(e.to_string()))?;
        let entry = item.entry;
        if entry.status != Status::Untracked {
            continue;
        }
        let path = entry.rela_path.to_string();
        if in_submodule(&path) {
            continue;
        }
        match entry.disk_kind {
            Some(Kind::File | Kind::Symlink) => {
                files.insert(path);
            }
            Some(Kind::Repository) => {
                files.insert(format!("{path}/"));
            }
            // git lists no FIFO, socket or device, and no folder but a repository.
            _ => {}
        }
    }
    let total = files.len();
    Ok(Answer::Offered(Offered {
        files: files.into_iter().take(most.unwrap_or(usize::MAX)).collect(),
        total,
    }))
}

/// Whether git ignores each of `paths`, and which of `folders` are submodules, here, in this
/// process: the bounded child's half of [`super::Ask::Ignored`].
///
/// **As `git check-ignore` answers**: a path the index records is not ignored whatever the
/// rules say, and one inside an ignored folder is ignored. A path is matched as what it is on
/// disk, a folder or not, without following a link. A name git would compose differently is
/// asked both ways, and ignored when either spelling is, as the in-process read asks it.
///
/// **As `git ls-files --stage` answers**: a folder is a submodule when the index records it, by
/// either spelling, as a gitlink.
///
/// An index gitoxide cannot read is answered [`Answer::Unindexed`].
pub(super) fn ignored_here(
    plane: &Path,
    branch: Branch<'_>,
    paths: &[PathBuf],
    folders: &[PathBuf],
) -> Result<Answer, Refused> {
    use unicode_normalization::UnicodeNormalization as _;
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("the files of {}", branch.called()),
        why,
    };
    let opened = open(plane, branch)?;
    let base = opened.base;
    let repo = opened.repo;
    let index = match repo.index_or_empty() {
        Ok(index) => index,
        Err(e) => return Ok(Answer::Unindexed(e.to_string())),
    };
    let spellings = |path: &Path| -> Vec<String> {
        let Some(spelled) = path.to_str().map(|one| one.replace('\\', "/")) else {
            return Vec::new();
        };
        let composed: String = spelled.nfc().collect();
        if composed == spelled {
            vec![spelled]
        } else {
            vec![composed, spelled]
        }
    };
    let entry = |spelled: &str| index.entry_by_path(spelled.into());

    let gitlinks = folders
        .iter()
        .filter(|folder| {
            spellings(folder).iter().any(|spelled| {
                entry(spelled).is_some_and(|one| one.mode == gix::index::entry::Mode::COMMIT)
            })
        })
        .cloned()
        .collect();

    let mut excludes = repo
        .excludes(
            &index,
            None,
            gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
        )
        .map_err(|e| unreadable(e.to_string()))?;
    let mut ignored = Vec::with_capacity(paths.len());
    for path in paths {
        let mut any = false;
        for spelled in spellings(path) {
            if entry(&spelled).is_some() {
                continue;
            }
            // What it is on disk, as git asks it: a folder matches a folder's rule.
            let mode = match std::fs::symlink_metadata(base.join(&spelled)) {
                Ok(meta) if meta.is_dir() => Some(gix::index::entry::Mode::DIR),
                Ok(_) => Some(gix::index::entry::Mode::FILE),
                Err(_) => None,
            };
            let platform = excludes
                .at_path(Path::new(&spelled), mode)
                .map_err(|e| unreadable(e.to_string()))?;
            if platform.is_excluded() {
                any = true;
            }
        }
        ignored.push(any);
    }
    Ok(Answer::Ignored(Ignored { ignored, gitlinks }))
}
