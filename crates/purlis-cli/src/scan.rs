//! `charter scan [--explain]`: the commit scan of a chat's git hooks (SQ-16, SQ-17), asked of
//! what is staged in the repository here, without committing anything.
//!
//! It prints what a commit would be refused for, masked, and what the allowlist lets through.
//! `--explain` adds, for each finding, the rule that found it and the entry that would let it
//! through — for the operator to review and commit to `.charter-scan-allow.toml`, since a chat's
//! own commit may not change that file.

use std::path::PathBuf;
use std::process::ExitCode;

use purlis_core::diffscan::{self, Finding};
use purlis_core::scanallow::{self, Origin};
use purlis_core::secretshape::{is_personal, leak_rules};
use purlis_core::worktree::git;

/// Runs the scan in the repository this process stands in.
pub fn run(explain: bool) -> ExitCode {
    let Some(repo) = top() else {
        eprintln!("purlis scan: this is not inside a git repository");
        return ExitCode::from(2);
    };
    let scan = match diffscan::checked(&repo) {
        Ok(scan) => scan,
        Err(why) => {
            eprintln!("purlis scan: the staged changes could not be read ({why})");
            return ExitCode::from(2);
        }
    };
    if scan.refused.is_empty() && scan.allowed.is_empty() {
        println!("Nothing staged looks like a secret or personal data.");
    }
    if !scan.refused.is_empty() {
        println!("A commit of what is staged would be refused for:");
        for finding in &scan.refused {
            println!("  {}", diffscan::one_line(finding));
            if explain {
                explained(finding);
            }
        }
    }
    if !scan.allowed.is_empty() {
        println!("Let through by the allowlist:");
        for (finding, entry) in &scan.allowed {
            let by = match entry.origin {
                Origin::Builtin => "purlis's own entry".to_owned(),
                Origin::File(n) => format!("{} entry {n}", scanallow::FILE),
            };
            println!(
                "  {}  — {by}: {}",
                diffscan::one_line(finding),
                entry.reason
            );
        }
    }
    for problem in &scan.problems {
        println!("Note: {problem}");
    }
    if scan.changes_the_allowlist {
        println!(
            "Note: what is staged changes {}, which a chat's own commit may not.",
            scanallow::FILE
        );
    }
    if explain {
        println!("\nThe rules, by the id an entry names:");
        for (id, words) in leak_rules() {
            println!("  {id:<20} {words}");
        }
    }
    if scan.refused.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A finding's rule, and the two entries that would let it through.
fn explained(finding: &Finding) {
    println!("    rule: {} ({})", finding.rule, finding.kind);
    println!(
        "    To let this rule through in this file, the operator commits to {}:",
        scanallow::FILE
    );
    for line in scanallow::suggested(finding.rule, &finding.path).lines() {
        println!("      {line}");
    }
    // Personal data is let through by path only: a hash of it can be reversed by guessing, and
    // the allowlist is committed.
    if !is_personal(finding.rule) {
        println!("    Or, for this one key only:");
        println!("      [[allow]]");
        println!("      fingerprint = \"{}\"", finding.fingerprint);
        println!("      reason = \"<why this is not a leak>\"");
    }
}

/// The top of the work tree this process stands in.
fn top() -> Option<PathBuf> {
    let here = std::env::current_dir().ok()?;
    let run = git::run_in_hook(&here, &["rev-parse", "--show-toplevel"], git::READ).ok()?;
    (run.code == Some(0))
        .then(|| PathBuf::from(String::from_utf8_lossy(&run.out).trim()))
        .filter(|top| !top.as_os_str().is_empty())
}
