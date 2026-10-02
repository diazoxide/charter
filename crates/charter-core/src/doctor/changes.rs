//! `changes`: cross-repo change records charter cannot read, and a member's branch sitting in
//! a clone that is a member of no change — in EVERY workspace (`doctor.check_changes` at
//! `cli-final`; ADR 0060).
//!
//! **Every workspace, not just the active one**: a change is a per-workspace store, and the
//! one that needs attention is rarely the one you are standing in.
//!
//! **Read from what is on this disk; nothing is asked of a remote.** This runs from the
//! SessionStart hook. The git it runs is local ref listings and reads, every argument a
//! literal or the default branch charter already knows for the repo, so no value out of a
//! record reaches an argv: branch names are compared here.
//!
//! **An unreadable record is FAIL, and it is kept apart from a divergence**: "charter cannot
//! read this file" and "git disagrees with this file" send the reader to two different places.
//!
//! **Three divergences need charter's landing records** (#472), and all are FAIL, since charter
//! can see them (ADR 0013 rule 2):
//!
//! - **landed out of order:** the log declares a member landed while a member it needs has no
//!   landing. Charter refuses that landing and cannot stop a person merging in the browser;
//!   this is the half that says so.
//! - **merged outside charter:** the member's pushed branch (`refs/remotes/origin/<branch>`)
//!   is already in the clone's default branch, and the log has no landing for it, so there is
//!   no landing commit for a revert to undo. When charter's pending landing says it started
//!   that landing, the advice is to record it with `land`, not to revert by hand.
//! - **left to merge later:** GitLab set a request to merge later and charter could not undo
//!   it, so whatever head the branch has when a pipeline passes will merge. A branch never pushed is
//!   not read as merged: a branch cut and not yet worked on is in the default branch too.
//!   A squash merge leaves no trace on this disk, so this can under-report, never invent.

use std::collections::{BTreeMap, BTreeSet};

use super::fsx::{self, Unread};
use super::git::git_in;
use super::{Doctor, Row};
use crate::change::pending::{self, At, Stage};
use crate::change::{Record, land, landing, store};
use crate::shown;

const NAME: &str = "changes";

/// How many findings the detail names before it counts the rest.
const SHOWN: usize = 3;

pub(super) fn changes(d: &Doctor) -> Row {
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    let root = d.root.as_path();
    let (workspaces, mut unseen): (Vec<String>, Vec<Unread>) = match fsx::read_workspaces(root) {
        Ok(found) => found,
        Err(e) => return Row::not_checked(NAME, fsx::py_os_error(&e, &root.join("workspaces"))),
    };
    let mut total = 0usize;
    let mut unreadable: Vec<String> = Vec::new();
    let mut found: Vec<String> = Vec::new();
    for ws in &workspaces {
        let listing = store::read_all(root, ws);
        if let Some(unread) = listing.unread {
            unseen.push((unread.path, unread.errno));
            continue;
        }
        total += listing.records.len();
        for (slug, complaint) in &listing.refused {
            unreadable.push(format!("{ws}/{}: {complaint}", shown::short(slug)));
        }
        match stray_branches(root, ws, &listing.records) {
            Ok(lines) => found.extend(lines.into_iter().map(|line| format!("{ws}: {line}"))),
            Err(why) => return Row::not_checked(NAME, why),
        }
        match landing_divergences(root, ws, &listing.records) {
            Ok(lines) => found.extend(lines.into_iter().map(|line| format!("{ws}: {line}"))),
            Err(why) => return Row::not_checked(NAME, why),
        }
    }

    let row = if !unreadable.is_empty() {
        Row::fail(
            NAME,
            format!("unreadable record(s): {}", first_few(&unreadable)),
            "Fix the file the message names — the key set is closed at both ends, so an unknown \
             or missing key is named rather than ignored. `charter change list` prints the same \
             complaint.",
        )
    } else if !found.is_empty() {
        Row::fail(
            NAME,
            first_few(&found),
            "Read from what is already on this disk, so it can under-report and never invent. \
             `charter change show <slug>` is the whole picture.",
        )
    } else if total == 0 {
        Row::ok(NAME, "none")
    } else {
        Row::ok(NAME, format!("{total} change(s), none divergent"))
    };
    fsx::beside_unread(root, row, &unseen)
}

/// `a; b; c (+N more)`.
fn first_few(lines: &[String]) -> String {
    let mut shown = lines
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if lines.len() > SHOWN {
        shown.push_str(&format!(" (+{} more)", lines.len() - SHOWN));
    }
    shown
}

/// A member's branch name found in a clone of this workspace that is a member of no change:
/// a member somebody forgot to `charter change add`, or a name that means something else
/// there. A record charter could not read contributes no names.
fn stray_branches(
    root: &std::path::Path,
    ws: &str,
    records: &[Record],
) -> Result<Vec<String>, String> {
    let mut wanted: BTreeMap<&str, &str> = BTreeMap::new();
    let mut members: BTreeSet<&str> = BTreeSet::new();
    for record in records {
        for m in &record.members {
            wanted.entry(&m.branch).or_insert(&record.change);
            members.insert(&m.repo);
        }
    }
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let clones = fsx::read_clones(root, ws)
        .map(|(found, _)| found)
        .unwrap_or_default();
    let mut out = Vec::new();
    for clone in clones {
        let name = clone
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if members.contains(name.as_str()) {
            continue;
        }
        // One listing per clone, not one question per branch: this runs under SessionStart's
        // budget, and the branch names are compared here rather than handed to git.
        let run = git_in(
            &clone,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )?;
        if !run.ok() {
            continue;
        }
        let here: BTreeSet<&str> = run.out.lines().map(str::trim).collect();
        for (branch, slug) in &wanted {
            if here.contains(branch) {
                out.push(format!(
                    "{}: has branch {}, which change '{}' declares — and this repo is a member \
                     of no change. Add it (charter change add), or the branch is a name \
                     collision worth knowing about.",
                    shown::short(&name),
                    shown::short(branch),
                    shown::short(slug)
                ));
            }
        }
    }
    Ok(out)
}

/// Where charter's landing records and git disagree about a change's members: a member landed
/// while a member it needs had not, a request left set to merge later, and a member's pushed
/// branch already in the default branch that charter has not recorded landing.
fn landing_divergences(
    root: &std::path::Path,
    ws: &str,
    records: &[Record],
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for record in records {
        let log = landing::landings(root, ws, &record.change);
        let pending = pending::pendings(root, ws, &record.change);
        let landed: BTreeSet<String> = record
            .members
            .iter()
            .filter(|m| log.contains_key(&m.repo))
            .map(|m| m.repo.clone())
            .collect();
        for (repo, waiting) in record.blocked(&landed) {
            if !landed.contains(&repo) {
                continue;
            }
            let named: Vec<String> = waiting
                .iter()
                .map(|w| format!("'{}'", shown::short(w)))
                .collect();
            out.push(format!(
                "{}: landed while {} had not, by charter's landing log. Charter refuses that \
                 landing and cannot stop a merge in the browser.",
                shown::short(&repo),
                named.join(", ")
            ));
        }
        for m in &record.members {
            let any = land::evidence(&log, &pending, &m.repo, At::Any);
            if matches!(any, land::Evidence::Logged(_)) {
                continue;
            }
            let later = match any {
                land::Evidence::Started(p) if p.stage == Stage::MergeLater => Some(p),
                _ => None,
            };
            let (repo, branch) = (shown::short(&m.repo), shown::short(&m.branch));
            let slug = shown::short(&record.change);
            // The member's pushed branch, when the clone's default branch holds it: merged.
            let merged_at = match crate::repos::clone_at(root, ws, &m.repo) {
                Some(clone) => pushed_and_merged(root, &m.repo, &clone.path, &m.branch)?,
                None => None,
            };
            if let Some((default, head)) = merged_at {
                let default = shown::short(&default);
                let ours = matches!(
                    land::evidence(&log, &pending, &m.repo, At::Head(&head)),
                    land::Evidence::Started(_)
                );
                out.push(if ours {
                    format!(
                        "{repo}: branch {branch} is in {default}, and charter started that \
                         landing and has not recorded it. Record it: charter change land {slug} \
                         --repo {repo}"
                    )
                } else {
                    format!(
                        "{repo}: branch {branch} is in {default} and charter did not land it, \
                         so there is no landing to revert and no {} trailer. A person has to \
                         revert this member by hand.",
                        land::TRAILER
                    )
                });
                continue;
            }
            if let Some(p) = later {
                out.push(format!(
                    "{repo}: request {} was left set to merge later, at whatever head the \
                     branch has when a pipeline passes, not the one charter checked. Cancel its \
                     auto-merge on the forge, then run charter change land {slug} --repo \
                     {repo} again, or drop the member.",
                    p.number
                ));
            }
        }
    }
    Ok(out)
}

/// The default branch and the pushed branch's commit, when `clone`'s pushed `branch`
/// (`refs/remotes/origin/<branch>`) is already in the default branch. A branch never pushed
/// is not read as merged: one cut and not yet worked on is in the default branch too.
fn pushed_and_merged(
    root: &std::path::Path,
    repo: &str,
    clone: &std::path::Path,
    branch: &str,
) -> Result<Option<(String, String)>, String> {
    let Some(default) = crate::reposave::default_branch(root, repo, clone) else {
        return Ok(None);
    };
    let merged = git_in(
        clone,
        &[
            "for-each-ref",
            &format!("--merged=refs/remotes/origin/{default}"),
            "--format=%(refname) %(objectname)",
            "refs/remotes/origin/",
        ],
    )?;
    if !merged.ok() {
        return Ok(None);
    }
    let pushed = format!("refs/remotes/origin/{branch}");
    Ok(merged.out.lines().find_map(|line| {
        let (name, head) = line.trim().split_once(' ')?;
        (name == pushed).then(|| (default.clone(), head.to_string()))
    }))
}
