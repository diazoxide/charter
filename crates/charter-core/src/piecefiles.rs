//! A piece's files, as the light editor reads them (RC-5, ADR 0081 §1, ADR 0084 §3).
//!
//! **The window names a piece, never a directory.** It asks by workspace, repo and piece, the
//! names [`worktree::list`] answers with, and this module finds the folder: so whatever can
//! reach the command cannot point it at a directory of its choosing. A file is named by its
//! path relative to that folder, and it opens only while it stays inside it.
//!
//! **Nothing the repo's configuration names runs.** Every piece is a folder an agent can write
//! (ADR 0084 §2), and the one git call here, the file list, goes through `worktree::git`'s
//! hardened runner, which turns off the fsmonitor and hooks on the command line.
//!
//! **Read only.** RC-5 is the viewer; saving an edit, with its stale check and its human-edit
//! record, is RC-10's (ADR 0084 §8). Nothing here writes.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::worktree::{self, git};

/// The largest file the light editor draws, in bytes: 5 MiB.
///
/// A file past it is answered with its size, and the window offers your editor instead (ADR
/// 0081 §3). The light editor is for reading and small edits; a log or a generated bundle that
/// size is neither, and holding it would cost the web content process (ADR 0086 row M2) for a
/// file nobody reads line by line.
pub const LARGEST: u64 = 5 * 1024 * 1024;

/// How much of a file is looked at to call it binary: git's own heuristic, a NUL byte in the
/// first 8,000 bytes (`buffer_is_binary` in git's `xdiff-interface.c`).
const SNIFF: usize = 8000;

/// What opening a file of a piece found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// Text, to draw. Bytes that are not UTF-8 are drawn as U+FFFD: the light editor only reads
    /// here, so nothing is written back through the replacement.
    Text { text: String },
    /// A file git would call binary, by its size. Drawn as a sentence, never as text.
    Binary { bytes: u64 },
    /// A file past [`LARGEST`], by its size.
    TooLarge { bytes: u64 },
}

/// Why a piece or one of its files did not open, as the sentence the window shows.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error(transparent)]
    Piece(#[from] worktree::Refusal),
    #[error("there is no worktree '{piece}' of {repo} in workspace '{ws}'")]
    NoSuchPiece {
        ws: String,
        repo: String,
        piece: String,
    },
    #[error("'{0}' is not a path inside the worktree")]
    NotInPiece(String),
    #[error("'{0}' is not in the worktree any more")]
    NotThere(String),
    #[error(
        "'{0}' is not one of the worktree's files: the light editor opens what git tracks there \
         and what it does not ignore"
    )]
    NotOffered(String),
    #[error("'{0}' is not a file")]
    NotAFile(String),
    #[error("charter could not read '{what}': {why}")]
    Unreadable { what: String, why: String },
}

/// The piece's files: every file git tracks, and every one it does not track and does not
/// ignore, as paths relative to the piece, sorted.
///
/// What `git ls-files` answers, which is what a review of the branch can show: an ignored
/// build output is not offered.
pub fn list(plane: &Path, ws: &str, repo: &str, piece: &str) -> Result<Vec<String>, Refused> {
    files_in(&folder_of(plane, ws, repo, piece)?, piece)
}

/// [`list`], for a folder already found.
fn files_in(folder: &Path, piece: &str) -> Result<Vec<String>, Refused> {
    let listed = git::run(
        folder,
        &[
            "--no-optional-locks",
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
        git::READ,
    )
    .map_err(worktree::Refusal::from)?;
    if !listed.ok() {
        return Err(Refused::Unreadable {
            what: format!("the files of {piece}"),
            why: listed.err.trim().to_string(),
        });
    }
    let mut files: Vec<String> = listed
        .out
        .split('\0')
        .filter(|one| !one.is_empty())
        .map(str::to_owned)
        .collect();
    // `--cached` lists a file once per stage while a merge is unresolved.
    files.sort();
    files.dedup();
    Ok(files)
}

/// One file of the piece, by its path relative to the piece.
///
/// Only a path [`list`] offers opens, and through a link only a file it offers too. Refused as
/// well when the path is empty, absolute, walks up, has a `.git` component, or resolves
/// (through a link) to somewhere outside the piece. A link to another offered file of the same
/// piece opens, as `CLAUDE.md` linked to `AGENTS.md` does in many repos.
pub fn open(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    path: &str,
) -> Result<Opened, Refused> {
    let folder = folder_of(plane, ws, repo, piece)?;
    let relative = inside(path)?;
    // **Only what the list offers opens** (ADR 0084 §2, ADR 0052): a file git tracks, or one
    // it does not track and does not ignore. An ignored `.env`, or a secret materialised into
    // the worktree, is not something a review shows, and `piece_file` hands the window no
    // value the vault keeps out of it. One more git call per open.
    let offered = files_in(&folder, piece)?;
    let offers = |relative: &Path| offered.binary_search(&slashed(relative)).is_ok();
    if !offers(relative) {
        return Err(Refused::NotOffered(path.to_string()));
    }
    let base = std::fs::canonicalize(&folder).map_err(|e| Refused::Unreadable {
        what: piece.to_string(),
        why: e.to_string(),
    })?;
    let resolved = match std::fs::canonicalize(base.join(relative)) {
        Ok(resolved) => resolved,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refused::NotThere(path.to_string()));
        }
        Err(e) => {
            return Err(Refused::Unreadable {
                what: path.to_string(),
                why: e.to_string(),
            });
        }
    };
    if resolved == base || !resolved.starts_with(&base) || names_git(&resolved, &base) {
        return Err(Refused::NotInPiece(path.to_string()));
    }
    // A link opens only a file the list offers too: one to an ignored file is refused.
    if !resolved.strip_prefix(&base).is_ok_and(offers) {
        return Err(Refused::NotOffered(path.to_string()));
    }
    // The resolved path has no link on it, so the open refuses one planted since.
    let mut file = crate::contain::open_no_link(&base, &resolved).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Refused::NotThere(path.to_string())
        } else {
            Refused::Unreadable {
                what: path.to_string(),
                why: e.to_string(),
            }
        }
    })?;
    let unreadable = |e: std::io::Error| Refused::Unreadable {
        what: path.to_string(),
        why: e.to_string(),
    };
    let meta = file.metadata().map_err(unreadable)?;
    if !meta.is_file() {
        return Err(Refused::NotAFile(path.to_string()));
    }
    let bytes = meta.len();
    if bytes > LARGEST {
        return Ok(Opened::TooLarge { bytes });
    }
    let mut read = Vec::with_capacity(bytes as usize);
    // Bounded, so a file that grew since the size was read is not read past the limit.
    file.by_ref()
        .take(LARGEST + 1)
        .read_to_end(&mut read)
        .map_err(unreadable)?;
    if read.len() as u64 > LARGEST {
        return Ok(Opened::TooLarge {
            bytes: read.len() as u64,
        });
    }
    if read[..read.len().min(SNIFF)].contains(&0) {
        return Ok(Opened::Binary {
            bytes: read.len() as u64,
        });
    }
    Ok(Opened::Text {
        text: String::from_utf8_lossy(&read).into_owned(),
    })
}

/// The piece's folder, found the way the Explorer finds it: among the pieces git has.
fn folder_of(plane: &Path, ws: &str, repo: &str, piece: &str) -> Result<PathBuf, Refused> {
    // The names are checked before anything is joined or run.
    worktree::path_for(plane, ws, repo, piece)?;
    worktree::list(plane, ws, repo)?
        .into_iter()
        .find(|one| one.piece == piece && one.prunable.is_none())
        .map(|one| one.path)
        .ok_or_else(|| Refused::NoSuchPiece {
            ws: ws.to_string(),
            repo: repo.to_string(),
            piece: piece.to_string(),
        })
}

/// A path relative to the piece, with nothing in it that could leave: no root, no `..`, not
/// empty.
fn inside(path: &str) -> Result<&Path, Refused> {
    let relative = Path::new(path);
    let plain = !path.is_empty()
        && relative
            .components()
            .all(|step| matches!(step, Component::Normal(_) | Component::CurDir));
    if plain {
        Ok(relative)
    } else {
        Err(Refused::NotInPiece(path.to_string()))
    }
}

/// Whether a resolved path is, or is inside, a `.git` entry at any depth: the worktree's own
/// (in a worktree `.git` is a file naming the clone's git directory) or a nested repo's. Either
/// is git's, not the branch's.
fn names_git(resolved: &Path, base: &Path) -> bool {
    resolved
        .strip_prefix(base)
        .is_ok_and(|below| below.components().any(|step| step.as_os_str() == ".git"))
}

/// A relative path as git's file list spells it: its plain components joined by `/`.
fn slashed(relative: &Path) -> String {
    relative
        .components()
        .filter_map(|step| match step {
            Component::Normal(name) => Some(name.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
