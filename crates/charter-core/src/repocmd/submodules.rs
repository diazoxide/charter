//! Submodules: reported, never fetched. Python's `submodule_drift` and
//! `report_submodule_drift` (charter #817).
//!
//! A submodule URL comes out of the cloned repo's own `.gitmodules` and can name anything,
//! and a submodule fetch is a nested clone that does not read the policy charter wrote into
//! the clone — so charter says what is missing and names the command, and never runs it.

use std::path::Path;

use super::{Say, Sink};
use crate::worktree::git;

/// The one remedy charter names.
const REMEDY: &str = "submodule update --init --recursive";

/// `(nothing checked out, not at the recorded commit)` — submodule paths in the tree at `d`,
/// in git's order. Empty for a tree with no `.gitmodules` and for a `git` that failed: this
/// feeds reports, and a report is not the place to refuse.
pub fn drift(d: &Path) -> (Vec<String>, Vec<String>) {
    if std::fs::symlink_metadata(d.join(".gitmodules")).is_err() {
        return (Vec::new(), Vec::new());
    }
    let Ok(seen) = git::run(d, &["submodule", "status"], git::READ) else {
        return (Vec::new(), Vec::new());
    };
    // Not satisfied by an empty stdout: `git submodule status` prints the submodules it
    // mapped and THEN fails on one it did not, and lines out of a run git disowned are not
    // findings.
    if !seen.ok() {
        return (Vec::new(), Vec::new());
    }
    let mut absent = Vec::new();
    let mut moved = Vec::new();
    for line in seen.out.lines() {
        let mut chars = line.chars();
        let mark = chars.next();
        let rest = chars.as_str();
        if mark != Some('-') && mark != Some('+') {
            continue;
        }
        // `<mark><sha> <path>`, and for a checked-out one a trailing ` (<describe>)`, taken
        // off by the MARK and from the right: a path may hold spaces and parentheses.
        let path = rest.split_once(' ').map(|(_, p)| p).unwrap_or_default();
        if mark == Some('+') {
            moved.push(
                path.rsplit_once(" (")
                    .map(|(p, _)| p)
                    .unwrap_or_default()
                    .to_string(),
            );
        } else {
            absent.push(path.to_string());
        }
    }
    (absent, moved)
}

/// Say what `d`'s submodules are not, and the command that fixes it. Returns whether
/// anything was said, so a caller can drop its own tick rather than print one beside this.
pub fn report(
    root: &Path,
    d: &Path,
    label: &str,
    branch: Option<&str>,
    lead: Option<&str>,
    say: Sink,
) -> bool {
    let (absent, moved) = drift(d);
    if absent.is_empty() && moved.is_empty() {
        return false;
    }
    let mut bits = Vec::new();
    if !absent.is_empty() {
        bits.push(format!(
            "{} submodule(s) recorded but not initialised ({}) — nothing is checked out \
             there, so anything that runs from them fails with 'no such file or directory'",
            absent.len(),
            absent.join(", ")
        ));
    }
    if !moved.is_empty() {
        bits.push(format!(
            "{} submodule(s) not at the commit {} records ({})",
            moved.len(),
            branch.unwrap_or("this branch"),
            moved.join(", ")
        ));
    }
    let facts = bits.join("; ");
    say(Say::Warn(match lead {
        None => format!("{label}: {facts}."),
        Some(lead) => format!("{label}: {lead} — {facts}."),
    }));
    let at = d.strip_prefix(root).unwrap_or(d);
    say(Say::Info(format!(
        "  charter does not fetch them: a submodule URL comes out of the cloned repo's own \
         .gitmodules and can point anywhere, and charter's token-only policy does not reach a \
         submodule fetch. Yours to run: git -C {} {REMEDY}",
        at.display()
    )));
    true
}
