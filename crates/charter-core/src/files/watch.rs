//! Where to listen for what an agent does to a branch, so its change markers follow (FM-4,
//! #1107): the branch's folder, the folders in it that git knows, and whether what moved is
//! anything git would mark.
//!
//! **A first write anywhere in the branch has to be heard**, in a folder nobody opened and that
//! held no change yet. Where the platform watches a whole tree as one (FSEvents on macOS,
//! `ReadDirectoryChangesW` on Windows), the app watches the branch's folder recursively and asks
//! [`Root::matters`] whether what moved is worth reading the status again for: what git ignores
//! (a build's output) and git's own folder are not. Where a recursive watch is one per folder
//! (inotify), the app watches [`Root::folders`] instead: the folders holding a file git tracks
//! or does not ignore, so `node_modules/` and `target/` cost nothing.
//!
//! **Read as the status is** (D-88f, D-88h): by gitoxide, in the bounded child
//! ([`super::Reader`]), starting no program. The app asks these by itself as an agent writes.
//!
//! **The app holds the resolved folder; the window never names one.** A [`Root`] is found from a
//! branch named the window's way, with the status read's checks.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use super::reader::{Answer, Ask, Reader};
use super::status::open;
use super::{Branch, Refused};

/// The most folders [`Root::folders`] answers with, the shallowest first: a branch of tens of
/// thousands of folders is listened to near its top.
pub const KNOWN: usize = 4096;

/// The most moved paths [`Root::matters`] looks at in one batch. Past it, the batch matters:
/// a burst that large is read again rather than sorted.
pub const ASKED: usize = 256;

/// A branch's folder, resolved, with the names it was found by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    base: PathBuf,
    plane: PathBuf,
    ws: String,
    repo: String,
    piece: Option<String>,
}

/// The branch's folder, resolved as the status read resolves it, by `reader`.
pub fn root(reader: &Reader, plane: &Path, branch: Branch<'_>) -> Result<Root, Refused> {
    let base = match reader.ask(plane, branch, Ask::Root)? {
        Answer::Root(base) => base,
        _ => return Err(Refused::Read("the reader answered something else".into())),
    };
    Ok(Root {
        base,
        plane: plane.to_path_buf(),
        ws: branch.ws.to_string(),
        repo: branch.repo.to_string(),
        piece: branch.piece.map(str::to_string),
    })
}

impl Root {
    /// The branch's folder.
    pub fn path(&self) -> &Path {
        &self.base
    }

    fn branch(&self) -> Branch<'_> {
        Branch {
            ws: &self.ws,
            repo: &self.repo,
            piece: self.piece.as_deref(),
        }
    }

    /// The branch's own folder and every folder in it holding a file git tracks or does not
    /// ignore, the shallowest first, at most [`KNOWN`], read by `reader`.
    pub fn folders(&self, reader: &Reader) -> Result<Vec<PathBuf>, Refused> {
        match reader.ask(&self.plane, self.branch(), Ask::Folders)? {
            Answer::Folders(folders) => Ok(folders
                .into_iter()
                .filter(|folder| folder.starts_with(&self.base))
                .collect()),
            _ => Err(Refused::Read("the reader answered something else".into())),
        }
    }

    /// Whether any of `moved` is something the branch's status could show: inside the
    /// branch's folder, not git's own, and not ignored by git. Past [`ASKED`] paths, or when
    /// `reader` does not answer in time, it is.
    pub fn matters(&self, reader: &Reader, moved: &[PathBuf]) -> bool {
        let mut asked: Vec<PathBuf> = Vec::new();
        for path in moved {
            let Ok(relative) = path.strip_prefix(&self.base) else {
                continue;
            };
            if relative.components().any(|step| step.as_os_str() == ".git") {
                continue;
            }
            if relative.as_os_str().is_empty() {
                // The branch's folder itself: something in it changed.
                return true;
            }
            if relative
                .components()
                .all(|step| matches!(step, Component::Normal(_)))
            {
                asked.push(relative.to_path_buf());
            }
        }
        if asked.is_empty() {
            return false;
        }
        if asked.len() > ASKED {
            return true;
        }
        match reader.ask(&self.plane, self.branch(), Ask::Matters(asked)) {
            Ok(Answer::Matters(matters)) => matters,
            _ => true,
        }
    }
}

/// [`root`], here, in this process: the bounded child's half.
pub(super) fn root_here(plane: &Path, branch: Branch<'_>) -> Result<PathBuf, Refused> {
    open(plane, branch).map(|opened| opened.base)
}

/// [`Root::folders`], here, in this process: the bounded child's half.
pub(super) fn folders_here(plane: &Path, branch: Branch<'_>) -> Result<Vec<PathBuf>, Refused> {
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("the folders of {}", branch.called()),
        why,
    };
    let opened = open(plane, branch)?;
    let repo = opened.repo;
    let index = repo
        .index_or_empty()
        .map_err(|e| unreadable(e.to_string()))?;
    let options = repo
        .dirwalk_options()
        .map_err(|e| unreadable(e.to_string()))?
        .emit_tracked(true)
        .emit_untracked(gix::dir::walk::EmissionMode::Matching)
        .emit_ignored(None)
        .recurse_repositories(false);
    let walk = repo
        .dirwalk_iter(
            index,
            None::<gix::bstr::BString>,
            Default::default(),
            options,
        )
        .map_err(|e| unreadable(e.to_string()))?;
    let mut folders: BTreeSet<(usize, String)> = BTreeSet::new();
    folders.insert((0, String::new()));
    for item in walk {
        let item = item.map_err(|e| unreadable(e.to_string()))?;
        let Ok(file) = std::str::from_utf8(&item.entry.rela_path) else {
            continue;
        };
        let mut above = Path::new(file).parent();
        while let Some(folder) = above {
            let spelled = folder.to_string_lossy().into_owned();
            if spelled.is_empty() {
                break;
            }
            let depth = folder.components().count();
            if !folders.insert((depth, spelled)) {
                break;
            }
            above = folder.parent();
        }
    }
    Ok(folders
        .into_iter()
        .take(KNOWN)
        .map(|(_, folder)| opened.base.join(folder))
        .collect())
}

/// [`Root::matters`] for paths relative to the branch's folder, here, in this process: the
/// bounded child's half.
pub(super) fn matters_here(
    plane: &Path,
    branch: Branch<'_>,
    relative: &[PathBuf],
) -> Result<bool, Refused> {
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("the files of {}", branch.called()),
        why,
    };
    let repo = open(plane, branch)?.repo;
    let index = repo
        .index_or_empty()
        .map_err(|e| unreadable(e.to_string()))?;
    let mut excludes = repo
        .excludes(
            &index,
            None,
            gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
        )
        .map_err(|e| unreadable(e.to_string()))?;
    Ok(relative.iter().take(ASKED).any(|path| {
        let plain = path
            .components()
            .all(|step| matches!(step, Component::Normal(_)));
        !plain
            || excludes
                .at_path(path, None)
                .map_or(true, |platform| !platform.is_excluded())
    }))
}
