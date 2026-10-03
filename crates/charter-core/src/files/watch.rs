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
//! **The app holds the resolved folder; the window never names one.** A [`Root`] is found from a
//! branch named the window's way, with [`super::tree`]'s checks.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use super::{Branch, Refused, base_of, files_in, ignored_of};

/// The most folders [`Root::folders`] answers with, the shallowest first: a branch of tens of
/// thousands of folders is listened to near its top.
pub const KNOWN: usize = 4096;

/// A branch's folder, resolved, with the names it was found by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    base: PathBuf,
    ws: String,
    repo: String,
    piece: Option<String>,
}

/// The branch's folder, resolved as [`super::tree`] resolves it.
pub fn root(plane: &Path, branch: Branch<'_>) -> Result<Root, Refused> {
    Ok(Root {
        base: base_of(plane, branch)?,
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
    /// ignore, the shallowest first, at most [`KNOWN`].
    pub fn folders(&self) -> Result<Vec<PathBuf>, Refused> {
        let mut folders: BTreeSet<(usize, String)> = BTreeSet::new();
        folders.insert((0, String::new()));
        for file in files_in(&self.base, self.branch())? {
            let mut above = Path::new(&file).parent();
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
            .map(|(_, folder)| self.base.join(folder))
            .collect())
    }

    /// Whether any of `moved` is something the branch's status could show: inside the
    /// branch's folder, not git's own, and not ignored by git. When git cannot say, it is.
    pub fn matters(&self, moved: &[PathBuf]) -> bool {
        let mut asked: Vec<&Path> = Vec::new();
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
                asked.push(relative);
            }
        }
        if asked.is_empty() {
            return false;
        }
        match ignored_of(&self.base, &asked, self.branch()) {
            Ok(ignored) => ignored.iter().any(|one| !one),
            Err(_) => true,
        }
    }
}
