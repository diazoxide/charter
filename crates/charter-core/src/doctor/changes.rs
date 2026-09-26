//! `changes`: cross-repo change records charter cannot read, and a member's branch sitting in
//! a clone that is a member of no change — in EVERY workspace (`doctor.check_changes` at
//! `cli-final`; ADR 0060).
//!
//! **Every workspace, not just the active one**: a change is a per-workspace store, and the
//! one that needs attention is rarely the one you are standing in.
//!
//! **Read from what is on this disk; nothing is asked of a remote.** This runs from the
//! SessionStart hook. The only git it runs is one local ref listing per clone, whose one
//! argument is the literal `refs/heads/`, so no value out of a record reaches an argv.
//!
//! **An unreadable record is FAIL, and it is kept apart from a divergence**: "charter cannot
//! read this file" and "git disagrees with this file" send the reader to two different places.
//! The divergences that need the landing log (a member landed out of order, or merged outside
//! charter) arrive with `charter change land` (#472).

use std::collections::{BTreeMap, BTreeSet};

use super::fsx::{self, Unread};
use super::git::git_in;
use super::{Doctor, Row};
use crate::change::{Record, store};
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
