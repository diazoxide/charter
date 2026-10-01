//! A branch of its own for a chat that writes to a repo (GL-1, ADR 0072 §4).
//!
//! **A writing chat is one that starts in a repo's clone**: `workspaces/<ws>/<repo>`, the
//! directory "New tab in <repo>" and "Start new chats in <repo>" point at. By default it does
//! not start there. charter cuts a piece off the clone's HEAD, and the chat starts in that
//! instead, so two chats in one repo stop sharing a working tree. A chat that starts in the
//! workspace's own directory, at the project root, or in a piece that already exists gets
//! nothing new: it is not writing to one repo, or it already has a branch.
//!
//! On screen a piece is a **branch** (V23d), so the words a person reads here say branch. The
//! code says piece, because the difference between the directory and the ref is the point.
//!
//! Every mutation is [`worktree`]'s, so the same confinement, naming and wiring rules hold as
//! for `charter worktree add`. This module adds three things the CLI does not need: which
//! directory is a clone, a name for a chat that was given none, and the undo for a start that
//! did not happen.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use chrono::{DateTime, Utc};

use crate::pieces::{self, Event, Who};
use crate::worktree::{self, Base, Note, Refusal, name};

/// The name a branch is cut under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Naming {
    /// Exactly this name, which the operator typed for the branch. Taken is a refusal.
    Exactly(String),
    /// After the chat's name, when it has one, else `chat-<n>`. A name already taken moves to
    /// the next free one, because nobody typed it as a branch name.
    After(Option<String>),
}

/// A branch charter cut for a chat: where it is, and what git calls it.
#[derive(Debug, Clone)]
pub struct Cut {
    pub workspace: String,
    pub repo: String,
    pub piece: String,
    pub path: PathBuf,
    pub branch: String,
    /// The branch it was cut from, or the commit when the clone's HEAD was detached.
    pub base: Base,
    /// What the cut found to say: a dirty clone whose changes stay behind, or a layer that did
    /// not land. Each has the CLI's sentence and the window's.
    pub notes: Vec<Note>,
}

/// The workspace and repo whose clone `cwd` is, or `None` when it is not one.
///
/// Path arithmetic and one existence check: `cwd` is exactly `workspaces/<ws>/<repo>` and holds
/// a `.git`. A directory inside a clone is not the clone, so a chat started in `api/src` is not
/// cut a branch it did not ask for; nor is `.worktrees` a repo.
pub fn clone_at(plane: &Path, cwd: &Path) -> Option<(String, String)> {
    let here = std::fs::canonicalize(cwd).ok()?;
    let workspaces = std::fs::canonicalize(plane.join("workspaces")).ok()?;
    let rest = here.strip_prefix(&workspaces).ok()?;
    let parts: Vec<&str> = rest
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<_>>()?;
    let [ws, repo] = parts.as_slice() else {
        return None;
    };
    if !crate::contain::workspace_name_ok(ws)
        || !crate::contain::repo_name_ok(repo)
        || *repo == worktree::DIR_NAME
    {
        return None;
    }
    here.join(".git")
        .symlink_metadata()
        .is_ok()
        .then(|| ((*ws).to_string(), (*repo).to_string()))
}

/// The most names [`Naming::After`] tries before it says so.
const TRIES: usize = 100;

/// One lock per clone, held across the choice of a name and the cut under it.
///
/// Two starts in one repo used to be ordered by the window's main thread. Off it (GL-1 review
/// S3), two chats started together would both see `chat-1` free and race git for it, and the
/// loser's `worktree add` fails with git's own words rather than moving on to `chat-2`. Only
/// this process is ordered; a `charter worktree add` in a terminal still meets git's refusal.
fn clone_lock(plane: &Path, ws: &str, repo: &str) -> Arc<Mutex<()>> {
    static LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let key = plane.join("workspaces").join(ws).join(repo);
    let mut held = LOCKS.lock().unwrap_or_else(PoisonError::into_inner);
    Arc::clone(held.entry(key).or_default())
}

/// Whether `refusal` is about the NAME, which a chat that did not choose it should never pay
/// for: the next `chat-<n>` is tried instead (GL-1 review B1).
fn about_the_name(refusal: &Refusal) -> bool {
    matches!(
        refusal,
        Refusal::BadPiece(_)
            | Refusal::PieceGoesElsewhere { .. }
            | Refusal::BadBranch(_)
            | Refusal::BadBranchName(_)
    )
}

/// Cut a branch in `repo`'s clone, named as `naming` says.
pub fn cut(plane: &Path, ws: &str, repo: &str, naming: &Naming) -> Result<Cut, Refusal> {
    let lock = clone_lock(plane, ws, repo);
    let _one_at_a_time = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut label = match naming {
        Naming::Exactly(piece) => return cut_as(plane, ws, repo, piece),
        Naming::After(label) => label.as_deref().and_then(name::slug),
    };
    let mut last = None;
    let mut n = 1;
    while n <= TRIES {
        // A chat called `fix login` gets `fix-login`, then `fix-login-2`. One with no name
        // gets `chat-1`, `chat-2`: a bare `chat` would say nothing about which.
        let piece = match &label {
            Some(slug) if n == 1 => slug.clone(),
            Some(slug) => format!("{slug}-{n}"),
            None => format!("chat-{n}"),
        };
        n += 1;
        let taken =
            worktree::path_for(plane, ws, repo, &piece).map(|path| path.symlink_metadata().is_ok());
        let tried = match taken {
            Ok(true) => continue,
            Ok(false) => cut_as(plane, ws, repo, &piece),
            Err(refusal) => Err(refusal),
        };
        match tried {
            Err(Refusal::BranchTaken { .. }) => last = Some(piece),
            // The chat's name made a name git or charter will not take. `slug` is meant to
            // make that impossible, and this is what holds if it ever is not: the chat
            // starts on `chat-<n>` rather than being refused for what it was called.
            Err(refusal) if label.is_some() && about_the_name(&refusal) => {
                label = None;
                n = 1;
            }
            done => return done,
        }
    }
    Err(Refusal::BranchTaken {
        repo: repo.to_string(),
        branch: last.unwrap_or_else(|| "chat".to_string()),
    })
}

fn cut_as(plane: &Path, ws: &str, repo: &str, piece: &str) -> Result<Cut, Refusal> {
    match worktree::add(plane, ws, repo, piece, None) {
        Ok(added) => Ok(Cut {
            workspace: ws.to_string(),
            repo: repo.to_string(),
            piece: piece.to_string(),
            path: added.path,
            branch: added.branch,
            base: added.base,
            notes: added.warnings,
        }),
        // git made the folder and the branch, and only the record of where it came from is
        // missing (GL-1 review S1). Nothing has used it, so it goes back rather than being
        // left for the operator to find; the refusal still says what happened.
        Err(refusal @ Refusal::BaseNotRecorded { .. }) => {
            let _ = worktree::remove(plane, ws, repo, piece, false, true);
            Err(refusal)
        }
        Err(refusal) => Err(refusal),
    }
}

/// Log the cut `claimed`, once the chat it was cut for has started. `None` when the log could
/// not be written; the branch is there either way.
pub fn claim(plane: &Path, cut: &Cut, who: &Who, now: DateTime<Utc>) -> Option<PathBuf> {
    pieces::record(
        plane,
        &cut.workspace,
        Event::Claimed,
        &cut.repo,
        &cut.piece,
        None,
        who,
        now,
    )
}

/// What taking a cut back did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Undone {
    /// The folder and the branch are both gone.
    Gone,
    /// The folder is gone and git kept the branch: something landed on it after all.
    BranchKept,
}

/// Take back a cut whose chat did not start: its folder and its branch, which nothing has
/// written to yet. git's own safe removal, never a forced one: if anything did land on it in
/// the meantime, the refusal says what, and a branch git will not delete is reported as kept.
///
/// **Deleting the branch is not the row action ADR 0072 §4 rules out.** That rule is about a
/// branch the operator has had and may have pushed; this is one charter cut a moment ago for a
/// chat that never started, with nothing on it, so taking it back leaves the repo as it was.
pub fn undo(plane: &Path, cut: &Cut) -> Result<Undone, Refusal> {
    let removed = worktree::remove(plane, &cut.workspace, &cut.repo, &cut.piece, false, true)?;
    Ok(if removed.branch_deleted {
        Undone::Gone
    } else {
        Undone::BranchKept
    })
}

/// A cut that is taken back unless it is kept: by [`Held::keep`] once its chat has started, by
/// [`Held::take_back`] when the start is refused, and by `Drop` when neither happens — a panic
/// between the cut and the start (GL-1 review S1).
#[derive(Debug)]
pub struct Held<'a> {
    plane: &'a Path,
    cut: Option<Cut>,
}

impl<'a> Held<'a> {
    pub fn new(plane: &'a Path, cut: Cut) -> Self {
        Self {
            plane,
            cut: Some(cut),
        }
    }

    pub fn cut(&self) -> &Cut {
        self.cut
            .as_ref()
            .expect("a held cut is held until it is kept or taken back")
    }

    /// The chat started: the branch is its.
    pub fn keep(mut self) -> Cut {
        self.cut.take().expect("a held cut is kept once")
    }

    /// The start was refused: take the branch back, and say what that did.
    pub fn take_back(mut self) -> (Cut, Result<Undone, Refusal>) {
        let cut = self.cut.take().expect("a held cut is taken back once");
        let undone = undo(self.plane, &cut);
        (cut, undone)
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        if let Some(cut) = self.cut.take() {
            let _ = undo(self.plane, &cut);
        }
    }
}
