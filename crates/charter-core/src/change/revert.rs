//! `charter change revert <slug>`: a new change, `revert-<slug>`, that reverts every member
//! charter landed (ADR 0060 §8, D5; the Python charter's `cmd_change_revert` at `cli-final`).
//!
//! **A revert is a new change.** Force-pushing default branches back past the merges would
//! leave a world where the change happened, was undone, and no repository's history says
//! either. `api-2` and `revert-api-2`, both named in every repo they touched, read as a
//! decision later. So this only seeds: in each landed member's clone it creates the new
//! change's branch off the default branch, carrying `git revert` of the logged commit, and
//! writes the new record. From there it is an ordinary change: `charter change push` pushes
//! it and `charter change land` lands it one member at a time, through the same gates and the
//! same attended-only floor. Nothing here asks a forge anything.
//!
//! **Only what charter landed, at the commit it logged.** The members are the ones the
//! landing log ([`super::landing`]) records, and each is reverted at that line's `merge`
//! commit. A member with no line (merged in a browser, or not landed) is named as a person's
//! to find, never guessed from a branch name. A line whose commit is not a commit id is
//! refused by name before it reaches git, since the log is a local file anybody can edit. A
//! logged commit the default branch no longer holds is refused by name: reverted already,
//! rewritten, or not fetched since.
//!
//! **`-m 1` is asked of git.** A merge landing has two parents and `git revert` needs a
//! mainline for it; a squash landing has one, and is reverted without `-m`.
//!
//! **What it never does.** It never pushes, force-pushes, deletes a branch, resets anything,
//! or touches a request: those argv are never built. A test records every git argv a revert
//! makes and holds them to [`VERBS`]. A revert that conflicts is aborted and named, its branch
//! left for a person to finish, since charter has no business picking a side of a conflict.
//! The checkout is put back on the branch it was on.
//!
//! **Undoing goes the other way round.** A member's blockers in the revert are the members
//! that needed it in the original and are being reverted too: a dependent built on a new API
//! has to stop using it before the API is taken away.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, Utc};

use super::cmd::{REFUSED, load, named, now_iso, save, workspace_ok};
use super::landing::{self, Landing};
use super::pending::{self, At};
use super::record::{Member, Record, TEXT_LIMIT, default_branch};
use super::store;
use crate::repocmd::Say;
use crate::shown;
use crate::worktree::git;

/// The prefix of a revert's slug. `name_ok` of a slug implies `name_ok` of the prefixed one,
/// since the rule has no length bound and the prefix starts with a letter.
pub const REVERT_PREFIX: &str = "revert-";

/// Every git verb a revert runs, and no other: reads (`diff-files` asks whether a merge or
/// rebase is stopped part-way), `switch` to make and leave the branch,
/// and `revert` (with its `--abort`). No `push`, `branch`, `reset`, `update-ref` or `clean`.
pub const VERBS: [&str; 8] = [
    "status",
    "diff-files",
    "rev-parse",
    "rev-list",
    "merge-base",
    "symbolic-ref",
    "switch",
    "revert",
];

/// The first twelve characters of a commit, contained.
fn short(sha: &str) -> String {
    shown::line(sha).chars().take(12).collect()
}

/// One git call in `clone` that succeeded, its output trimmed; `None` for anything else.
fn ask(clone: &Path, args: &[&str]) -> Option<String> {
    git::run(clone, args, git::READ)
        .ok()
        .filter(git::Run::ok)
        .map(|run| run.out.trim().to_string())
}

/// How many parents `sha` has, asked of git; `None` when git does not know the commit.
fn parents(clone: &Path, sha: &str) -> Option<usize> {
    let line = ask(clone, &["rev-list", "--parents", "-n", "1", sha])?;
    let parts = line.split_whitespace().count();
    (parts > 0).then(|| parts - 1)
}

/// The ref a member's revert branches from: the default branch as the remote has it, which
/// is where the landing is, or the local one when there is no tracking ref.
fn base(clone: &Path, default: &str) -> Option<String> {
    [
        format!("refs/remotes/origin/{default}"),
        format!("refs/heads/{default}"),
    ]
    .into_iter()
    .find(|r| ask(clone, &["rev-parse", "--verify", "--quiet", r]).is_some())
}

/// One landed member, checked and ready to seed: its clone, the logged commit, how many
/// parents it has, and the ref to branch from.
struct Seed {
    clone: std::path::PathBuf,
    merge: String,
    parents: usize,
    base: String,
}

/// What stops `line`'s member being reverted before anything is created, or the seed.
fn checked(plane: &Path, ws: &str, line: &Landing) -> Result<Seed, String> {
    let repo = line.repo.as_str();
    let Some(clone) = crate::repos::clone_at(plane, ws, repo) else {
        return Err(format!(
            "not a repo in workspace '{ws}'. Clone it first: charter clone {} -w {ws}",
            named(repo)
        ));
    };
    let clone = clone.path;
    // The log is never committed, which makes it local rather than trustworthy: a hand edit
    // reaches here, and then a git argv.
    if !super::land::sha_ok(&line.merge) {
        return Err(format!(
            "the landing log records {} as the merge, which is not a commit id, and charter \
             will not hand it to git",
            shown::short(&line.merge)
        ));
    }
    let Some(default) = crate::reposave::default_branch(plane, repo, &clone) else {
        return Err(
            "charter cannot tell this clone's default branch, so it will not guess what to \
             branch the revert from"
                .into(),
        );
    };
    let Some(base) = base(&clone, &default) else {
        return Err(format!(
            "this clone has no {} branch to branch the revert from",
            shown::line(&default)
        ));
    };
    let Some(parents) = parents(&clone, &line.merge) else {
        return Err(format!(
            "git does not know the commit {} the landing log names. `git fetch` in it, then \
             revert again",
            short(&line.merge)
        ));
    };
    let holds = git::run(
        &clone,
        &["merge-base", "--is-ancestor", &line.merge, &base],
        git::READ,
    )
    .is_ok_and(|run| run.ok());
    if !holds {
        return Err(format!(
            "{} no longer holds {}, the commit charter landed: it was reverted already, the \
             branch was rewritten, or it is not fetched since. Nothing to revert there",
            shown::line(&base),
            short(&line.merge)
        ));
    }
    Ok(Seed {
        clone,
        merge: line.merge.clone(),
        parents,
        base,
    })
}

/// Create `branch` off the seed's base carrying a revert of its commit, and put the checkout
/// back where it was. `Ok`, or the reason.
fn seed(seed: &Seed, branch: &str, sign: bool) -> Result<(), String> {
    let clone = seed.clone.as_path();
    // `git revert` commits into the checkout, so work in progress would be folded into it.
    let Some(dirty) = ask(clone, &["status", "--porcelain"]) else {
        return Err("git could not read the working tree".into());
    };
    if !dirty.is_empty() {
        return Err(
            "the working tree has uncommitted changes, and `git revert` commits into the \
             checkout. Commit or stash them first"
                .into(),
        );
    }
    if let Some(stopped) = crate::gitstate::stopped(clone) {
        return Err(stopped.why());
    }
    let wanted = format!("refs/heads/{branch}");
    if ask(clone, &["rev-parse", "--verify", "--quiet", &wanted]).is_some() {
        return Err(format!(
            "branch {} already exists here, and charter neither reuses nor replaces it",
            named(branch)
        ));
    }
    // Where to put the checkout back: its branch, or the commit it is detached at.
    let back = match ask(clone, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Some(on) => vec!["switch".to_string(), on],
        None => match ask(clone, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
            Some(at) => vec!["switch".to_string(), "--detach".to_string(), at],
            None => return Err("charter cannot tell what this clone has checked out".into()),
        },
    };
    // `--no-track`: the base is a tracking ref, and the revert's branch is not `main`'s.
    let made = git::run(
        clone,
        &["switch", "--no-track", "-c", branch, &seed.base],
        crate::planegit::WRITE,
    );
    if !made.as_ref().is_ok_and(git::Run::ok) {
        let said = made.map_or_else(|e| e.to_string(), |run| run.err);
        return Err(shown::line(said.trim()));
    }
    let mut argv: Vec<&str> = vec!["-c", crate::planegit::gpgsign(sign), "revert", "--no-edit"];
    if seed.parents > 1 {
        argv.extend(["-m", "1"]);
    }
    argv.push(&seed.merge);
    let done = git::run(clone, &argv, crate::planegit::WRITE);
    let reverted = match done {
        Ok(run) if run.ok() => Ok(()),
        done => {
            let _ = git::run(clone, &["revert", "--abort"], crate::planegit::WRITE);
            let said = done.map_or_else(|e| e.to_string(), |run| run.err);
            let said = said.trim();
            Err(format!(
                "{}: the branch is here and the revert was aborted; finish it by hand",
                shown::line(if said.is_empty() {
                    "the revert did not apply"
                } else {
                    said
                })
            ))
        }
    };
    let back: Vec<&str> = back.iter().map(String::as_str).collect();
    let returned = git::run(clone, &back, crate::planegit::WRITE).is_ok_and(|run| run.ok());
    match (reverted, returned) {
        (Err(why), _) => Err(why),
        (Ok(()), true) => Ok(()),
        (Ok(()), false) => Err(format!(
            "reverted on {}, and git could not switch the checkout back to {}",
            named(branch),
            shown::line(back.last().copied().unwrap_or_default())
        )),
    }
}

/// `charter change revert <slug>`: seed `revert-<slug>`, recorded as created by `by` at `now`.
pub fn revert(
    plane: &Path,
    ws: &str,
    slug: &str,
    by: &str,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    if !workspace_ok(plane, ws, say) {
        return 1;
    }
    let record = match load(plane, ws, slug, say) {
        Ok(record) => record,
        Err(code) => return code,
    };
    let new_slug = format!("{REVERT_PREFIX}{slug}");
    if store::exists(plane, ws, &new_slug) {
        say(Say::Fail(format!(
            "change '{new_slug}' already exists in workspace '{ws}'."
        )));
        say(Say::Info(format!(
            "Show it: charter change show {new_slug}  ·  or forget it first: charter change \
             forget {new_slug}"
        )));
        return REFUSED;
    }
    let log = landing::landings(plane, ws, slug);
    let started = pending::pendings(plane, ws, slug);
    // Named before anything is created, so the half charter cannot revert is read first.
    for m in record.members.iter().filter(|m| !log.contains_key(&m.repo)) {
        let repo = named(&m.repo);
        if started.get(&m.repo).is_some_and(|p| p.started(At::Any)) {
            say(Say::Warn(format!(
                "{repo}: a landing charter started is not recorded yet, so there is no commit \
                 to revert. Record it first: charter change land {} --repo {repo}",
                named(slug)
            )));
        } else {
            say(Say::Warn(format!(
                "{repo}: no landing record, so no commit to revert — charter cannot revert \
                 this member. If it was merged in a browser, finding the commit is a person's \
                 job."
            )));
        }
    }
    if log.is_empty() {
        say(Say::Fail(format!(
            "{}: charter has landed no member of this change, so there is nothing to revert.",
            named(slug)
        )));
        return REFUSED;
    }

    let branch = default_branch(&new_slug);
    let mut refused: Vec<(String, String)> = Vec::new();
    let mut seeds: Vec<(&Member, Seed)> = Vec::new();
    for m in &record.members {
        let Some(line) = log.get(&m.repo) else {
            continue;
        };
        match checked(plane, ws, line) {
            Ok(seed) => seeds.push((m, seed)),
            Err(why) => refused.push((m.repo.clone(), why)),
        }
    }
    if seeds.is_empty() {
        for (repo, why) in &refused {
            say(Say::Fail(format!("{}: {why}", named(repo))));
        }
        say(Say::Fail(format!(
            "no member of {} could be reverted, so no change was created.",
            named(slug)
        )));
        return 1;
    }
    // The record holds every member that is to be reverted, seeded or not: a branch that
    // could not be made is a thing to fix, not a member to drop without a word.
    let joined: BTreeSet<&str> = seeds.iter().map(|(m, _)| m.repo.as_str()).collect();
    let mut seeded: Vec<(String, String)> = Vec::new();
    let mut members = Vec::new();
    let settings = crate::planesave::Settings::read(plane);
    for (m, s) in &seeds {
        members.push(Member {
            repo: m.repo.clone(),
            branch: branch.clone(),
            needs: record
                .dependents(&m.repo)
                .into_iter()
                .filter(|d| joined.contains(d.as_str()))
                .collect(),
        });
        match seed(s, &branch, settings.repo(&m.repo).sign.value) {
            Ok(()) => seeded.push((m.repo.clone(), s.merge.clone())),
            Err(why) => refused.push((m.repo.clone(), why)),
        }
    }
    let why: String = format!("reverts change '{slug}': {}", record.why)
        .chars()
        .take(TEXT_LIMIT)
        .collect();
    let mut new = Record::new(&new_slug, &why, by, &now_iso(now));
    new.members = members;
    let code = save(plane, ws, &new, say);
    if code != 0 {
        return code;
    }
    say(Say::Done(format!(
        "change '{new_slug}' created — {} member(s) to revert",
        new.members.len()
    )));
    for (repo, merge) in &seeded {
        say(Say::Out(format!(
            "  {}  branch {}  reverts {}",
            shown::line(repo),
            shown::line(&branch),
            short(merge)
        )));
    }
    for (repo, why) in &refused {
        say(Say::Fail(format!("{}: {why}", named(repo))));
    }
    say(Say::Info(format!(
        "It is an ordinary change from here: charter change push {new_slug}, then charter \
         change land {new_slug} --repo <name> for each member, blockers first."
    )));
    u8::from(!refused.is_empty())
}

#[cfg(test)]
mod tests;
