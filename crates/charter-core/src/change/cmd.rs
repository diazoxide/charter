//! `charter change create|add|drop|list|show|forget`: the record half, with no network
//! (`charter/commands_change.py` at `cli-final`; ADR 0060).
//!
//! Each verb speaks through a [`Say`] sink and returns its exit code, as `piececmd` does.
//!
//! **Exit 2 is a named refusal** — the request was understood and charter will not do it —
//! and 1 is something wrong. Four conditions share 2 and none shares a message: a repo with
//! no clone, a change that does not exist, a member added twice, and an ordering that cannot
//! be true. A caller must not have to parse English to tell "refused" from "broken".
//!
//! **Membership is enumerated by hand.** There is no glob, no pattern and no `--all`: every
//! member was typed by somebody and must resolve to a clone already in this workspace, so a
//! change reaches no further than what is on the disk in front of you.

use std::path::Path;

use chrono::{DateTime, Utc};

use super::record::{
    Exclusion, Member, Record, TEXT_LIMIT, branch_refusal, default_branch, name_ok,
};
use super::store::{self, WriteError};
use crate::repocmd::Say;
use crate::shown;
use crate::tui::{self, Align};

/// The exit code for a named refusal, distinct from the generic 1.
pub const REFUSED: u8 = 2;

/// A value from a record or the command line, as one row shows it.
fn cell(value: &str) -> String {
    shown::line(value)
}

/// A value a refusal names, readable back off the line.
fn named(value: &str) -> String {
    shown::short(value)
}

fn now_iso(now: DateTime<Utc>) -> String {
    now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

/// Who is recorded as creating a change: `git config user.name`, first line, contained; else
/// `$USER`; else `unknown`.
pub fn author(plane: &Path) -> String {
    let asked = crate::worktree::git::run(
        plane,
        &["config", "user.name"],
        std::time::Duration::from_secs(20),
    )
    .ok()
    .filter(|run| run.ok())
    .map(|run| run.out)
    .unwrap_or_default();
    let name = contained(asked.lines().next().unwrap_or_default());
    if !name.is_empty() {
        return name;
    }
    std::env::var("USER")
        .ok()
        .map(|u| contained(&u))
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// `value` trimmed, escaped to one line and cut to [`TEXT_LIMIT`] characters, with no `…`
/// added: exactly a string the record's own one-line rule accepts back.
fn contained(value: &str) -> String {
    shown::one_line(value.trim(), shown::NO_CLIP)
        .chars()
        .take(TEXT_LIMIT)
        .collect()
}

/// Whether `ws` is a workspace this plane has, reached without a link. Asked first by every
/// verb, so a mistyped `-w` is refused rather than creating `workspaces/<typo>/changes/`.
fn workspace_ok(plane: &Path, ws: &str, say: &mut dyn FnMut(Say)) -> bool {
    match crate::worktree::confine::workspace_dir(plane, ws) {
        Ok(_) => {
            // `ws` passed `workspace_name_ok` above, so it prints as itself from here on.
            say(Say::Info(format!("workspace: {ws}")));
            true
        }
        Err(why) => {
            say(Say::Fail(shown::line(&why.to_string())));
            false
        }
    }
}

/// The `--why` given, trimmed, or `None` with the refusal said. A `why` that cannot be one
/// line is prose, and prose belongs in `workspace.md`.
fn why_given(why: Option<&str>, tail: &str, say: &mut dyn FnMut(Say)) -> Option<String> {
    let why = why.unwrap_or_default().trim();
    if why.is_empty() {
        say(Say::Fail(format!(
            "--why is required: one line saying {tail}."
        )));
        return None;
    }
    if shown::one_line(why, TEXT_LIMIT) != why {
        say(Say::Fail(
            "--why must be one plain line — it is repeated back on a report row and written \
             into a pull request body. Longer reasoning belongs in workspace.md, which is where \
             this plane keeps prose."
                .into(),
        ));
        return None;
    }
    Some(why.to_string())
}

fn unknown(ws: &str, slug: &str, say: &mut dyn FnMut(Say)) -> u8 {
    say(Say::Fail(format!(
        "no change {} in workspace '{ws}'.",
        named(slug)
    )));
    say(Say::Info(format!(
        "List them: charter change list  ·  create it: charter change create {} --why \"…\"",
        named(slug)
    )));
    REFUSED
}

/// The record, or the exit code with the refusal said. No such change is a refusal (2); a
/// record that exists and does not read is a defect in a file (1).
fn load(plane: &Path, ws: &str, slug: &str, say: &mut dyn FnMut(Say)) -> Result<Record, u8> {
    if !store::exists(plane, ws, slug) {
        return Err(unknown(ws, slug, say));
    }
    store::read(plane, ws, slug).map_err(|e| {
        say(Say::Fail(e.to_string()));
        1
    })
}

/// Write it back: an ordering that cannot be true refuses the request (2), a path charter
/// must not write is an error about the plane (1).
fn save(plane: &Path, ws: &str, record: &Record, say: &mut dyn FnMut(Say)) -> u8 {
    match store::write(plane, ws, record) {
        Ok(_) => 0,
        Err(WriteError::Record(e)) => {
            say(Say::Fail(e.to_string()));
            REFUSED
        }
        Err(WriteError::Plane(e)) => {
            say(Say::Fail(e));
            1
        }
    }
}

/// One member's row. Every field is contained before any width is measured.
fn member_line(record: &Record, m: &Member) -> String {
    let names: Vec<String> = record.members.iter().map(|x| cell(&x.repo)).collect();
    let w = tui::column("", names.iter().map(String::as_str), 0, None);
    let mut row = format!(
        "  {}  branch {}",
        tui::pad(&cell(&m.repo), w, Align::Left),
        cell(&m.branch)
    );
    if !m.needs.is_empty() {
        row.push_str("   needs: ");
        row.push_str(
            &m.needs
                .iter()
                .map(|n| cell(n))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    row
}

/// `charter change create <slug> --why …`: a name, a reason, and no members yet.
pub fn create(
    plane: &Path,
    ws: &str,
    slug: &str,
    why: Option<&str>,
    by: &str,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    if !workspace_ok(plane, ws, say) {
        return 1;
    }
    if !name_ok(slug) {
        say(Say::Fail(format!(
            "{} is not a change name (letters, digits, '.', '_', '-'; must not start with a dot \
             or a dash).",
            named(slug)
        )));
        say(Say::Info(
            "The slug names a file in this plane, a branch in every member, and the \
             `Charter-Change:` trailer on each landing commit — so it is refused rather than \
             rewritten."
                .into(),
        ));
        return 1;
    }
    let Some(why) = why_given(why, "what this work is for", say) else {
        return 1;
    };
    if store::exists(plane, ws, slug) {
        say(Say::Fail(format!(
            "change '{slug}' already exists in workspace '{ws}'."
        )));
        say(Say::Info(format!("Show it: charter change show {slug}")));
        return REFUSED;
    }
    let record = Record::new(slug, &why, by, &now_iso(now));
    let code = save(plane, ws, &record, say);
    if code != 0 {
        return code;
    }
    say(Say::Done(format!("change '{slug}' created")));
    say(Say::Info(format!(
        "Add its repos: charter change add {slug} <repo>"
    )));
    0
}

/// `charter change add <slug> <repo> [--branch B] [--needs R]…`: one member, by literal name.
pub fn add(
    plane: &Path,
    ws: &str,
    slug: &str,
    repo: &str,
    branch: Option<&str>,
    needs: &[String],
    say: &mut dyn FnMut(Say),
) -> u8 {
    if !workspace_ok(plane, ws, say) {
        return 1;
    }
    let mut record = match load(plane, ws, slug, say) {
        Ok(record) => record,
        Err(code) => return code,
    };
    if crate::repos::clone_at(plane, ws, repo).is_none() {
        say(Say::Fail(format!(
            "{}: no clone in workspace '{ws}'.",
            named(repo)
        )));
        say(Say::Info(format!(
            "Clone it first: charter clone {} -w {ws}",
            named(repo)
        )));
        return REFUSED;
    }
    if record.member(repo).is_some() {
        say(Say::Fail(format!(
            "'{}' is already a member of '{slug}'.",
            named(repo)
        )));
        say(Say::Info(format!(
            "Change its branch or blockers by editing workspaces/{ws}/changes/{slug}.json, or \
             drop it first: charter change drop {slug} {} --why \"…\"",
            named(repo)
        )));
        return REFUSED;
    }
    // An empty `--branch` is no branch named, as Python's `args.branch or default` reads it.
    let branch = branch
        .filter(|b| !b.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_branch(slug));
    if let Some(complaint) = branch_refusal(&branch) {
        say(Say::Fail(complaint));
        return 1;
    }
    let member = Member {
        repo: repo.to_string(),
        branch,
        needs: needs.to_vec(),
    };
    record.members.push(member.clone());
    // A repo cannot be both in and out, so re-adding one lifts its exclusion — loudly, since
    // the exclusion carried a reason somebody wrote down.
    let lifted: Option<Exclusion> = record.exclusion(repo).cloned();
    record.excluded.retain(|e| e.repo != repo);
    let code = save(plane, ws, &record, say);
    if code != 0 {
        return code;
    }
    if let Some(lifted) = lifted {
        say(Say::Warn(format!(
            "the exclusion recorded on {} is lifted: {}",
            cell(&lifted.at),
            cell(&lifted.why)
        )));
    }
    say(Say::Out(member_line(&record, &member)));
    say(Say::Done(format!(
        "'{}' is a member of '{slug}'",
        named(repo)
    )));
    0
}

/// `charter change drop <slug> <repo> --why …`: out of the members (or never one) and into
/// `excluded`, with the reason and the time.
pub fn drop(
    plane: &Path,
    ws: &str,
    slug: &str,
    repo: &str,
    why: Option<&str>,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    if !workspace_ok(plane, ws, say) {
        return 1;
    }
    let mut record = match load(plane, ws, slug, say) {
        Ok(record) => record,
        Err(code) => return code,
    };
    let Some(why) = why_given(why, "why this repo is out", say) else {
        return 1;
    };
    if record.exclusion(repo).is_some() {
        say(Say::Fail(format!(
            "'{}' is already excluded from '{slug}'.",
            named(repo)
        )));
        return REFUSED;
    }
    let dependents = record.dependents(repo);
    if !dependents.is_empty() {
        let names: Vec<String> = dependents
            .iter()
            .map(|d| format!("'{}'", named(d)))
            .collect();
        let verb = if dependents.len() > 1 {
            "need"
        } else {
            "needs"
        };
        say(Say::Fail(format!(
            "'{}' cannot be dropped from '{slug}': {} still {verb} it to land first.",
            named(repo),
            names.join(", ")
        )));
        say(Say::Info(
            "Drop those members first, or edit their `needs` — a blocker nothing will land is a \
             member that never becomes ready."
                .into(),
        ));
        return REFUSED;
    }
    let was_member = record.member(repo).is_some();
    record.members.retain(|m| m.repo != repo);
    record.excluded.push(Exclusion {
        repo: repo.to_string(),
        why,
        at: now_iso(now),
    });
    let code = save(plane, ws, &record, say);
    if code != 0 {
        return code;
    }
    say(Say::Done(if was_member {
        format!(
            "'{}' excluded — {} member(s) left",
            named(repo),
            record.members.len()
        )
    } else {
        format!("'{}' excluded (never a member)", named(repo))
    }));
    0
}

/// `charter change forget <slug>`: delete the record and nothing else.
pub fn forget(plane: &Path, ws: &str, slug: &str, say: &mut dyn FnMut(Say)) -> u8 {
    if !workspace_ok(plane, ws, say) {
        return 1;
    }
    if !store::exists(plane, ws, slug) {
        say(Say::Fail(format!(
            "no change {} in workspace '{ws}'.",
            named(slug)
        )));
        return REFUSED;
    }
    match store::forget(plane, ws, slug) {
        Ok(Some(_)) => {
            say(Say::Done(format!(
                "change '{slug}' forgotten — the record is gone; branches, requests and the \
                 landing log are untouched."
            )));
            0
        }
        Ok(None) => unknown(ws, slug, say),
        Err(e) => {
            say(Say::Fail(e));
            1
        }
    }
}

/// `charter change list`: one row per change. A record it could not read is named and the
/// exit is 1; a `changes/` it could not list is never "No changes".
pub fn list(plane: &Path, ws: &str, say: &mut dyn FnMut(Say)) -> u8 {
    if !workspace_ok(plane, ws, say) {
        return 1;
    }
    let listing = store::read_all(plane, ws);
    if let Some(unread) = listing.unread {
        say(Say::Fail(unread.why));
        return 1;
    }
    if listing.records.is_empty() && listing.refused.is_empty() {
        say(Say::Info(format!(
            "No changes in workspace '{ws}'. Create one: charter change create <slug> --why \"…\""
        )));
        return 0;
    }
    let names: Vec<String> = listing.records.iter().map(|r| cell(&r.change)).collect();
    let w = tui::column("", names.iter().map(String::as_str), 0, None);
    for (record, name) in listing.records.iter().zip(&names) {
        let mut counts = format!("{} member(s)", record.members.len());
        if !record.excluded.is_empty() {
            counts.push_str(&format!(", {} excluded", record.excluded.len()));
        }
        say(Say::Out(format!(
            "{}  {counts}  ·  {}",
            tui::pad(name, w, Align::Left),
            cell(&record.why)
        )));
    }
    for (slug, complaint) in &listing.refused {
        say(Say::Fail(format!(
            "{}: {}",
            named(slug),
            shown::line(complaint)
        )));
    }
    u8::from(!listing.refused.is_empty())
}

/// `charter change show <slug>`: the record whole — why, members, branches, blockers,
/// exclusions. Nothing here asks a forge; it is true without asking anybody.
pub fn show(plane: &Path, ws: &str, slug: &str, say: &mut dyn FnMut(Say)) -> u8 {
    if !workspace_ok(plane, ws, say) {
        return 1;
    }
    let record = match load(plane, ws, slug, say) {
        Ok(record) => record,
        Err(code) => return code,
    };
    let mut head = format!(
        "{} · {} member(s)",
        cell(&record.change),
        record.members.len()
    );
    if !record.excluded.is_empty() {
        head.push_str(&format!(" · {} excluded", record.excluded.len()));
    }
    say(Say::Out(head));
    say(Say::Out(format!("  why: {}", cell(&record.why))));
    say(Say::Out(format!(
        "  created {} by {}",
        cell(&record.created),
        cell(&record.by)
    )));
    if !record.members.is_empty() {
        say(Say::Out(String::new()));
        for m in &record.members {
            say(Say::Out(member_line(&record, m)));
        }
    }
    if !record.excluded.is_empty() {
        say(Say::Out(String::new()));
        say(Say::Out("  excluded:".into()));
        let names: Vec<String> = record.excluded.iter().map(|e| cell(&e.repo)).collect();
        let w = tui::column("", names.iter().map(String::as_str), 0, None);
        for (e, name) in record.excluded.iter().zip(&names) {
            say(Say::Out(format!(
                "  {}  {}  ({})",
                tui::pad(name, w, Align::Left),
                cell(&e.why),
                cell(&e.at)
            )));
        }
    }
    0
}
