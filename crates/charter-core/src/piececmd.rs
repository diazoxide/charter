//! `charter worktree` (alias `wt`): cut a piece, declare it done or abandoned, list what exists,
//! read what happened, and remove one — `charter/commands_worktree.py` at `cli-final`
//! (charter#368).
//!
//! **Git decides and the log remembers.** Every mutation is git's: [`crate::worktree::add`]
//! and [`crate::worktree::remove`], through the hardened runner, never a folder deleted by
//! hand. What git cannot know — that charter cut the piece, and what its worker said about it —
//! is appended to the piece log ([`crate::pieces::record`]), and nothing git could answer is
//! written there (ADR 0011).
//!
//! Each verb speaks through a [`Say`] sink and returns its exit code, as the repo commands do,
//! so the binary prints each line as it is known and a test reads the same lines as values.
//!
//! # Where this differs from Python, on purpose
//!
//! - **`--branch` names the NEW branch.** Python's checked out an existing branch.
//!   [`crate::worktree::add`] always cuts a fresh branch off the clone's HEAD, because the
//!   branch a piece was cut from is recorded at cut time and an existing branch has no such
//!   record.
//! - **A removal names what it would discard.** Python counted unique commits and said
//!   "uncommitted changes". The refusal here lists the changed paths and the commits, because
//!   `--force` is an informed choice only when the operator can see what it takes.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::pieces::{self, Declaration, Event, Who};
use crate::repocmd::Say;
use crate::tui::{self, Align};
use crate::worktree::{self, Dirt, Refusal};

/// The exit code for "this piece is already claimed", distinct from the generic 1. A worker
/// that loses a race takes the next name from its plan, and must not have to parse English to
/// know that is what happened — nor mistake an invalid name for a lost race.
pub const CLAIM_TAKEN: u8 = 2;

/// The gap between two columns of a listing.
const GAP: usize = 2;

/// Who already holds this piece, or `None` when it is free: its directory exists, or its
/// branch is checked out in another live worktree — git refuses both, and both are the same
/// collision under two names. A branch that merely EXISTS is not a holder: nobody is working
/// in it.
fn holder(plane: &Path, ws: &str, repo: &str, piece: &str, branch: &str) -> Option<String> {
    let path = worktree::path_for(plane, ws, repo, piece).ok()?;
    if path.symlink_metadata().is_ok() {
        return Some(format!("a worktree at {}", shown(plane, &path)));
    }
    worktree::list(plane, ws, repo)
        .ok()?
        .into_iter()
        .find(|p| p.prunable.is_none() && p.branch.as_deref() == Some(branch))
        .map(|p| format!("piece '{}'", p.piece))
}

/// A path as the operator reads it: relative to the plane when it is inside it.
fn shown(plane: &Path, path: &Path) -> String {
    path.strip_prefix(plane)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn taken(piece: &str, held: &str, say: &mut dyn FnMut(Say)) -> u8 {
    say(Say::Fail(format!(
        "'{piece}' is already claimed — held by {held}."
    )));
    say(Say::Info(
        "Take the next unclaimed piece from the plan, or work in the existing worktree.".into(),
    ));
    CLAIM_TAKEN
}

/// `charter worktree add <repo> <piece>`: cut a piece off the clone's HEAD, wire the layer,
/// and log `claimed`.
#[allow(clippy::too_many_arguments)]
pub fn add(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    branch: Option<&str>,
    who: &Who,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    say(Say::Info(format!("workspace: {ws}")));
    let branch_name = branch.unwrap_or(piece);
    if let Some(held) = holder(plane, ws, repo, piece, branch_name) {
        return taken(piece, &held, say);
    }
    let added = match worktree::add(plane, ws, repo, piece, branch) {
        Ok(added) => added,
        Err(refusal) => {
            // The check above is not the mutex — git is. A worker that won between that check
            // and git's own lock leaves a failure that reads like a broken repo unless charter
            // looks again, so it looks: the cause is read off reality, not off git's English.
            if let Some(held) = holder(plane, ws, repo, piece, branch_name) {
                return taken(piece, &held, say);
            }
            say(Say::Fail(refusal.to_string()));
            return 1;
        }
    };
    // The worktree is the claim; this records who took it. Best-effort: a claim git granted
    // is not undone by a log that could not be written — but it is said.
    if pieces::record(plane, ws, Event::Claimed, repo, piece, None, who, now).is_none() {
        say(Say::Warn(format!(
            "the piece log could not be written, so this claim is not recorded: {}",
            shown(plane, &pieces::log_path(plane, ws, &who.host))
        )));
    }
    for warning in &added.warnings {
        say(Say::Warn(warning.clone()));
    }
    say(Say::Done(format!(
        "{repo} · {piece} → {}",
        shown(plane, &added.path)
    )));
    let base = match &added.base {
        worktree::Base::Branch(b) => b.clone(),
        worktree::Base::Detached(sha) => format!("{sha} (detached)"),
    };
    say(Say::Info(format!("  base:   {base}")));
    say(Say::Info(format!("  branch: {}", added.branch)));
    say(Say::Info(format!(
        "  enter:  cd {} && claude    (or hand this path to EnterWorktree)",
        shown(plane, &added.path)
    )));
    say(Say::Info(
        "  When the work is finished: charter worktree done — or charter worktree abandon \
         \"<why>\" if it cannot be."
            .into(),
    ));
    0
}

/// `charter worktree done` / `abandon <reason>`: declare the piece `cwd` stands in.
///
/// Neither verb takes a piece argument, and that absence is the design: a worker naming its
/// own piece is a worker that can name someone else's. The piece is read off the directory.
pub fn declare(
    plane: &Path,
    cwd: &Path,
    what: Declaration<'_>,
    who: &Who,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    let Some(here) = worktree::locate(plane, cwd) else {
        say(Say::Fail(
            "Not inside a worktree — `done` and `abandon` declare the piece you are standing \
             in, so there is nothing here to declare."
                .into(),
        ));
        say(Say::Info(
            "cd into the piece first; `charter worktree list` shows where they are.".into(),
        ));
        return 1;
    };
    let (ws, repo, piece) = (&here.workspace, &here.repo, &here.piece);
    match pieces::declare(plane, ws, repo, piece, what, who, now) {
        Ok(declared) => {
            if let Some(over) = &declared.over {
                say(Say::Warn(format!(
                    "'{piece}' was already declared {} — recording {} over it (the earlier \
                     one stays in the log).",
                    // Read from the log, which any host's worker writes: one line, always.
                    tui::sanitize(over),
                    declared.event.word()
                )));
            }
            let reason = match what {
                Declaration::Abandoned { reason } => {
                    format!(": {}", tui::sanitize(reason.trim()))
                }
                Declaration::Done => String::new(),
            };
            say(Say::Done(format!(
                "{repo} · {piece} — {}{reason}",
                declared.event.word()
            )));
            0
        }
        Err(why) => {
            say(Say::Fail(why.to_string()));
            1
        }
    }
}

/// `charter worktree list [<repo>]`: every piece git has, joined to what the log says of it.
///
/// Rows come from git and only from git; the log only puts a name and a word on what git
/// found. `state` has three answers and a fourth: `clean`, `dirty`, `unknown` when git could
/// not say — never `clean` for a tree charter could not read — and `missing` for a
/// registration whose directory is gone.
pub fn list(
    plane: &Path,
    ws: &str,
    repo: Option<&str>,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    say(Say::Info(format!("workspace: {ws}")));
    let repos: Vec<String> = match repo {
        Some(repo) => vec![repo.to_string()],
        None => match crate::repos::clones(plane, ws) {
            Ok(found) => found.repos.into_iter().map(|r| r.name).collect(),
            Err(why) => {
                say(Say::Fail(why.to_string()));
                return 1;
            }
        },
    };
    let claims = pieces::claims(plane, ws);
    let mut code = 0;
    let mut total = 0;
    for repo in repos {
        let found = match worktree::list(plane, ws, &repo) {
            Ok(found) => found,
            Err(why) => {
                say(Say::Fail(format!("{repo}: {why}")));
                code = 1;
                continue;
            }
        };
        if found.is_empty() {
            continue;
        }
        say(Say::Info(repo.clone()));
        let rows: Vec<[String; 5]> = found
            .iter()
            .map(|p| {
                let state = if p.prunable.is_some() {
                    "missing"
                } else {
                    match worktree::dirt_of(&p.path) {
                        Dirt::Clean => "clean",
                        Dirt::Dirty => "dirty",
                        Dirt::Unknown => "unknown",
                    }
                };
                let branch = p
                    .branch
                    .clone()
                    .unwrap_or_else(|| format!("detached {}", p.path.display()));
                let who = pieces::claimant(claims.get(&(repo.clone(), p.piece.clone())));
                let said = pieces::said(plane, ws, &repo, &p.piece, now);
                [p.piece.clone(), branch, state.to_string(), who, said]
            })
            .collect();
        table(&rows, say);
        total += rows.len();
    }
    if total == 0 && code == 0 {
        say(Say::Info(format!(
            "No worktrees. Create one: charter worktree add <repo> <piece> -w {ws}"
        )));
    }
    code
}

/// `charter worktree history [<repo> [<piece>]]`: what happened to this workspace's pieces,
/// read from the log alone — a piece whose worktree is long gone is the main reason to ask.
pub fn history(
    plane: &Path,
    ws: &str,
    repo: Option<&str>,
    piece: Option<&str>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    say(Say::Info(format!("workspace: {ws}")));
    // The log is read from `workspaces/<ws>/pieces`, so the name is checked — and the
    // workspace's directory asked for, a link there refused — before anything is joined.
    if let Err(why) = worktree::confine::workspace_dir(plane, ws) {
        say(Say::Fail(why.to_string()));
        return 1;
    }
    let field = |e: &Value, k: &str| {
        e.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let rows: Vec<[String; 5]> = pieces::events(plane, ws)
        .into_iter()
        .filter(|e| repo.is_none_or(|r| field(e, "repo") == r))
        .filter(|e| piece.is_none_or(|p| field(e, "piece") == p))
        .map(|e| {
            let reason = field(&e, "reason");
            let who = pieces::claimant(Some(&e));
            let tail = if reason.is_empty() {
                who
            } else {
                format!("{who} — {reason}")
            };
            [
                field(&e, "ts"),
                field(&e, "repo"),
                field(&e, "piece"),
                field(&e, "event"),
                tail,
            ]
        })
        .collect();
    if rows.is_empty() {
        let scope = match (repo, piece) {
            (Some(r), Some(p)) => format!(" for {r}/{p}"),
            (Some(r), None) => format!(" for {r}"),
            _ => String::new(),
        };
        say(Say::Info(format!(
            "No piece history recorded{scope}. Claims are recorded by `charter worktree add`."
        )));
        return 0;
    }
    table(&rows, say);
    0
}

/// `charter worktree remove <repo> <piece>`: `git worktree remove`, refused while the piece
/// holds uncommitted changes or commits no other ref reaches — each named — unless `force`.
pub fn remove(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    force: bool,
    delete_branch: bool,
    say: &mut dyn FnMut(Say),
) -> u8 {
    say(Say::Info(format!("workspace: {ws}")));
    match worktree::remove(plane, ws, repo, piece, force, delete_branch) {
        Ok(removed) => {
            if removed.was_stale {
                say(Say::Done(format!(
                    "Cleared stale registration for {repo} · {piece} (its worktree directory \
                     was already gone)."
                )));
            } else {
                say(Say::Done(format!("Removed {repo} · {piece}")));
            }
            if delete_branch {
                match &removed.branch {
                    Some(b) if removed.branch_deleted => {
                        say(Say::Done(format!("Deleted branch {b}")));
                    }
                    Some(b) => say(Say::Warn(format!(
                        "Worktree removed, but branch {b} was kept: git would not delete it."
                    ))),
                    None => say(Say::Warn(
                        "Worktree was on a detached HEAD — no branch to delete.".into(),
                    )),
                }
            }
            0
        }
        Err(refusal) => {
            say(Say::Fail(refusal.to_string()));
            if matches!(refusal, Refusal::NoSuchPiece { .. }) {
                say(Say::Info(format!(
                    "See what exists: charter worktree list {repo} -w {ws}"
                )));
            }
            1
        }
    }
}

/// Rows of five cells, the first four sized to their widest value in terminal cells, the last
/// left unpadded — it has nothing to its right. Each cell is sanitised: these values come out
/// of a log and out of directory names, and a newline in one would shear every column below.
fn table(rows: &[[String; 5]], say: &mut dyn FnMut(Say)) {
    let clean: Vec<Vec<String>> = rows
        .iter()
        .map(|r| r.iter().map(|c| tui::sanitize(c).into_owned()).collect())
        .collect();
    let widths: Vec<usize> = (0..4)
        .map(|i| tui::column("", clean.iter().map(|r| r[i].as_str()), GAP, None))
        .collect();
    for row in &clean {
        let cells: String = row
            .iter()
            .zip(&widths)
            .map(|(c, w)| tui::pad(c, *w, Align::Left))
            .collect();
        say(Say::Out(
            format!("    {cells}{}", row[4]).trim_end().to_string(),
        ));
    }
}
