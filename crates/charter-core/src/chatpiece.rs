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

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::pieces::{self, Event, Who};
use crate::worktree::{self, Refusal, name};

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
    /// What the cut found to say: a dirty clone whose changes stay behind, or a layer that did
    /// not land. The core's sentences, unchanged.
    pub warnings: Vec<String>,
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

/// Cut a branch in `repo`'s clone, named as `naming` says.
pub fn cut(plane: &Path, ws: &str, repo: &str, naming: &Naming) -> Result<Cut, Refusal> {
    let label = match naming {
        Naming::Exactly(piece) => return cut_as(plane, ws, repo, piece),
        Naming::After(label) => label.as_deref().and_then(name::slug),
    };
    let mut last = None;
    for n in 1..=TRIES {
        // A chat called `fix login` gets `fix-login`, then `fix-login-2`. One with no name
        // gets `chat-1`, `chat-2`: a bare `chat` would say nothing about which.
        let piece = match &label {
            Some(slug) if n == 1 => slug.clone(),
            Some(slug) => format!("{slug}-{n}"),
            None => format!("chat-{n}"),
        };
        let taken = worktree::path_for(plane, ws, repo, &piece)?
            .symlink_metadata()
            .is_ok();
        if taken {
            continue;
        }
        match cut_as(plane, ws, repo, &piece) {
            Err(Refusal::BranchTaken { .. }) => last = Some(piece),
            done => return done,
        }
    }
    Err(Refusal::BranchTaken {
        repo: repo.to_string(),
        branch: last.unwrap_or_else(|| "chat".to_string()),
    })
}

fn cut_as(plane: &Path, ws: &str, repo: &str, piece: &str) -> Result<Cut, Refusal> {
    let added = worktree::add(plane, ws, repo, piece, None)?;
    Ok(Cut {
        workspace: ws.to_string(),
        repo: repo.to_string(),
        piece: piece.to_string(),
        path: added.path,
        branch: added.branch,
        warnings: added.warnings,
    })
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

/// Take back a cut whose chat did not start: its folder and its branch, which nothing has
/// written to yet. git's own safe removal, never a forced one: if anything did land on it in
/// the meantime, the refusal says what and the branch stays.
pub fn undo(plane: &Path, cut: &Cut) -> Result<(), Refusal> {
    worktree::remove(plane, &cut.workspace, &cut.repo, &cut.piece, false, true).map(|_| ())
}
