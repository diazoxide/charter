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
//! commit, resolved once to its full id. "Landed" is that line and the default branch still
//! holding the commit; no forge is read. A member with no line (merged in a browser, or not
//! landed) is named as a person's to find, never guessed from a branch name, and so is a
//! logged member since dropped from the change. A line whose commit is not a commit id is
//! refused by name before it reaches git, since the log is a local file anybody can edit. A
//! logged commit the default branch no longer holds (rewritten, or not fetched since) is
//! refused by name, and so is one the default branch already carries a revert of.
//!
//! **Run again, it fills in.** When `revert-<slug>` exists, a member already seeded (its
//! branch is there) is left exactly as it is, a member excluded from it is left out, and a
//! member that was refused is seeded now. So "fix it, then revert again" is always the advice.
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
//!
//! **Exit 2 is a named refusal** of a member or of the whole command, and 1 is something that
//! went wrong (git failed, a revert conflicted, the record could not be written), as `land`
//! answers. When members differ, 1 wins over 2.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::cmd::{REFUSED, load, named, now_iso, save, workspace_ok};
use super::land::{holds, sha_ok, short};
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
/// rebase is stopped part-way), `switch` to make and leave the branch, and `revert` (with its
/// `--abort`). No `push`, `branch`, `reset`, `update-ref` or `clean`.
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

/// One git call in `clone` that succeeded, its output trimmed; `None` for anything else.
fn ask(clone: &Path, args: &[&str]) -> Option<String> {
    git::run(clone, args, git::READ)
        .ok()
        .filter(git::Run::ok)
        .map(|run| run.out.trim().to_string())
}

/// What git said when a call failed: its first line, or the error running it.
fn git_said(done: Result<git::Run, git::GitUnavailable>, otherwise: &str) -> String {
    let said = done.map_or_else(|e| e.to_string(), |run| crate::gitstate::said(&run));
    shown::line(if said.is_empty() { otherwise } else { &said })
}

/// A member not reverted, why, and the exit code that says so: 2 refused, 1 went wrong.
struct Refused {
    repo: String,
    why: String,
    code: u8,
}

impl Refused {
    fn named(repo: &str, why: String) -> Refused {
        Refused {
            repo: repo.to_string(),
            why,
            code: REFUSED,
        }
    }

    fn failed(repo: &str, why: String) -> Refused {
        Refused {
            repo: repo.to_string(),
            why,
            code: 1,
        }
    }
}

/// A member whose revert branch was made, and the commit it reverts.
struct Seeded {
    repo: String,
    commit: String,
}

/// A landed member checked and ready to revert: its clone, the logged commit as git's full
/// id, how many parents it has, and the ref the revert branches from.
struct Target {
    checkout: PathBuf,
    commit: String,
    parents: usize,
    base: String,
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

/// The regex-safe spelling of a change name (letters, digits, `.`, `_`, `-`).
fn literal(slug: &str) -> String {
    slug.replace('.', "\\.")
}

/// What stops `line`'s member being reverted as `new_slug` before anything is created, or the
/// target. Every refusal here is a named one.
fn checked(plane: &Path, ws: &str, line: &Landing, new_slug: &str) -> Result<Target, Refused> {
    let repo = line.repo.as_str();
    let refuse = |why: String| Refused::named(repo, why);
    let Some(clone) = crate::repos::clone_at(plane, ws, repo) else {
        return Err(refuse(format!(
            "not a repo in workspace '{ws}'. Clone it first: purlis clone {} -w {ws}",
            named(repo)
        )));
    };
    let clone = clone.path;
    // The log is never committed, which makes it local rather than trustworthy: a hand edit
    // reaches here, and then a git argv.
    if !sha_ok(&line.merge) {
        return Err(refuse(format!(
            "the landing log records {} as the merge, which is not a commit id, and purlis \
             will not hand it to git",
            shown::short(&line.merge)
        )));
    }
    let Some(default) = crate::reposave::default_branch(plane, repo, &clone) else {
        return Err(refuse(
            "purlis cannot tell this clone's default branch, so it will not guess what to \
             branch the revert from"
                .into(),
        ));
    };
    let Some(base) = base(&clone, &default) else {
        return Err(refuse(format!(
            "this clone has no {} branch to branch the revert from",
            shown::line(&default)
        )));
    };
    // Resolved once, to the full id every later call is handed. A ref whose name is the
    // logged short id would resolve to its own commit, which does not start with it.
    let peeled = format!("{}^{{commit}}", line.merge);
    let commit = match ask(&clone, &["rev-parse", "--verify", "--quiet", &peeled]) {
        Some(oid) if oid.starts_with(&line.merge) => oid,
        Some(_) => {
            return Err(refuse(format!(
                "{} names something else in this clone too (a branch or tag of that name), so \
                 purlis will not guess which commit the landing log meant",
                short(&line.merge)
            )));
        }
        None => {
            return Err(refuse(format!(
                "git does not know the commit {} the landing log names. `git fetch` in it, \
                 then revert again",
                short(&line.merge)
            )));
        }
    };
    let Some(parents) = ask(&clone, &["rev-list", "--parents", "-n", "1", &commit])
        .map(|line| line.split_whitespace().count().saturating_sub(1))
    else {
        return Err(refuse(format!(
            "git could not read the parents of {}",
            short(&commit)
        )));
    };
    if !holds(&clone, &commit, &base) {
        return Err(refuse(format!(
            "{} no longer holds {}, the commit purlis landed: the branch was rewritten, or it \
             is not fetched since. `git fetch` in it, then revert again",
            shown::line(&base),
            short(&commit)
        )));
    }
    // A revert already landed, as a merge (git's own line) or a squash (charter's trailer, under
    // every key it has had: a landing made before the rename says `Charter-Change`, V93j).
    let mut argv = vec![
        "rev-list".to_string(),
        "-n".into(),
        "1".into(),
        "--extended-regexp".into(),
        format!("--grep=^This reverts commit {commit}"),
    ];
    argv.extend(
        crate::names::TRAILER_CHANGE
            .spellings()
            .map(|key| format!("--grep=^{key}: {}$", literal(new_slug))),
    );
    argv.push(format!("{commit}..{base}"));
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let reverted = ask(&clone, &argv).filter(|found| !found.is_empty());
    if let Some(by) = reverted {
        return Err(refuse(format!(
            "{} already carries a revert of {}: {}. Nothing to revert there",
            shown::line(&base),
            short(&commit),
            short(&by)
        )));
    }
    Ok(Target {
        checkout: clone,
        commit,
        parents,
        base,
    })
}

/// Create `branch` off the target's base carrying a revert of its commit, signed as `sign`
/// says, and put the checkout back where it was.
fn branch_and_revert(repo: &str, target: &Target, branch: &str, sign: bool) -> Result<(), Refused> {
    let clone = target.checkout.as_path();
    // `git revert` commits into the checkout, so work in progress would be folded into it.
    let Some(dirty) = ask(clone, &["status", "--porcelain"]) else {
        return Err(Refused::failed(
            repo,
            "git could not read the working tree".into(),
        ));
    };
    if !dirty.is_empty() {
        return Err(Refused::named(
            repo,
            "the working tree has uncommitted changes, and `git revert` commits into the \
             checkout. Commit or stash them, then revert again"
                .into(),
        ));
    }
    if let Some(stopped) = crate::gitstate::stopped(clone) {
        return Err(Refused::named(repo, stopped.why()));
    }
    let wanted = format!("refs/heads/{branch}");
    if ask(clone, &["rev-parse", "--verify", "--quiet", &wanted]).is_some() {
        return Err(Refused::named(
            repo,
            format!(
                "branch {} already exists here, and purlis neither reuses nor replaces it",
                named(branch)
            ),
        ));
    }
    // Where to put the checkout back: its branch, or the commit it is detached at.
    let back = match ask(clone, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Some(on) => vec!["switch".to_string(), on],
        None => match ask(clone, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
            Some(at) => vec!["switch".to_string(), "--detach".to_string(), at],
            None => {
                return Err(Refused::failed(
                    repo,
                    "purlis cannot tell what this clone has checked out".into(),
                ));
            }
        },
    };
    // `--no-track`: the base is a tracking ref, and the revert's branch is not `main`'s.
    let made = git::run(
        clone,
        &["switch", "--no-track", "-c", branch, &target.base],
        crate::planegit::WRITE,
    );
    if !made.as_ref().is_ok_and(git::Run::ok) {
        return Err(Refused::failed(
            repo,
            git_said(made, "git could not create the branch"),
        ));
    }
    let mut argv: Vec<&str> = vec!["-c", crate::planegit::gpgsign(sign), "revert", "--no-edit"];
    if target.parents > 1 {
        argv.extend(["-m", "1"]);
    }
    argv.push(&target.commit);
    let done = git::run(clone, &argv, crate::planegit::WRITE);
    let reverted = match done {
        Ok(run) if run.ok() => Ok(()),
        done => {
            let _ = git::run(clone, &["revert", "--abort"], crate::planegit::WRITE);
            Err(Refused::failed(
                repo,
                format!(
                    "{}: the branch is here and the revert was aborted; finish it by hand",
                    git_said(done, "the revert did not apply")
                ),
            ))
        }
    };
    let back: Vec<&str> = back.iter().map(String::as_str).collect();
    let returned = git::run(clone, &back, crate::planegit::WRITE).is_ok_and(|run| run.ok());
    match (reverted, returned) {
        (Err(why), _) => Err(why),
        (Ok(()), true) => Ok(()),
        (Ok(()), false) => Err(Refused::failed(
            repo,
            format!(
                "reverted on {}, and git could not switch the checkout back to {}",
                named(branch),
                shown::line(back.last().copied().unwrap_or_default())
            ),
        )),
    }
}

/// Whether `repo`'s clone has the local branch `branch`.
fn branch_made(plane: &Path, ws: &str, repo: &str, branch: &str) -> bool {
    crate::repos::clone_at(plane, ws, repo).is_some_and(|clone| {
        let wanted = format!("refs/heads/{branch}");
        ask(&clone.path, &["rev-parse", "--verify", "--quiet", &wanted]).is_some()
    })
}

/// `charter change revert <slug>`: seed `revert-<slug>`, or fill in the members it is missing,
/// recorded as created by `by` at `now`.
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
    let existing = if store::exists(plane, ws, &new_slug) {
        match store::read(plane, ws, &new_slug) {
            Ok(found) => Some(found),
            Err(e) => {
                say(Say::Fail(e.to_string()));
                return 1;
            }
        }
    } else {
        None
    };
    let log = landing::landings(plane, ws, slug);
    let started = pending::pendings(plane, ws, slug);
    // Named before anything is created, so the half charter cannot revert is read first.
    for m in record.members.iter().filter(|m| !log.contains_key(&m.repo)) {
        let repo = named(&m.repo);
        if started.get(&m.repo).is_some_and(|p| p.started(At::Any)) {
            say(Say::Warn(format!(
                "{repo}: a landing purlis started is not recorded yet, so there is no commit \
                 to revert. Record it first: purlis change land {} --repo {repo}",
                named(slug)
            )));
        } else {
            say(Say::Warn(format!(
                "{repo}: no landing record, so no commit to revert — purlis cannot revert \
                 this member. If it was merged in a browser, finding the commit is a person's \
                 job."
            )));
        }
    }
    for repo in log.keys().filter(|r| record.member(r).is_none()) {
        say(Say::Warn(format!(
            "{}: landed by purlis, and later dropped from {}; not reverted.",
            named(repo),
            named(slug)
        )));
    }
    let landed: Vec<(&Member, &Landing)> = record
        .members
        .iter()
        .filter_map(|m| Some((m, log.get(&m.repo)?)))
        .collect();
    if landed.is_empty() {
        say(Say::Fail(format!(
            "{}: purlis has landed no member of this change, so there is nothing to revert.",
            named(slug)
        )));
        return REFUSED;
    }

    let mut new = existing.clone().unwrap_or_else(|| {
        let why: String = format!("reverts change '{slug}': {}", record.why)
            .chars()
            .take(TEXT_LIMIT)
            .collect();
        Record::new(&new_slug, &why, by, &now_iso(now))
    });
    let settings = crate::planesave::Settings::read(plane);
    let mut refused: Vec<Refused> = Vec::new();
    let mut seeded: Vec<Seeded> = Vec::new();
    let mut joined: Vec<String> = Vec::new();
    for (m, line) in landed {
        let repo = m.repo.as_str();
        if new.exclusion(repo).is_some() {
            say(Say::Info(format!(
                "{}: excluded from {new_slug}; left out.",
                named(repo)
            )));
            continue;
        }
        let branch = new
            .member(repo)
            .map_or_else(|| default_branch(&new_slug), |x| x.branch.clone());
        // Seeded before: its branch is a person's from here, and is not touched.
        if new.member(repo).is_some() && branch_made(plane, ws, repo, &branch) {
            say(Say::Info(format!(
                "{}: already seeded on {}; left as it is.",
                named(repo),
                named(&branch)
            )));
            continue;
        }
        let target = match checked(plane, ws, line, &new_slug) {
            Ok(target) => target,
            Err(why) => {
                refused.push(why);
                continue;
            }
        };
        // The record holds every member that is to be reverted, seeded or not: a branch that
        // could not be made is a thing to fix and revert again, not a member to drop.
        if new.member(repo).is_none() {
            new.members.push(Member {
                repo: repo.to_string(),
                branch: branch.clone(),
                needs: Vec::new(),
            });
            joined.push(repo.to_string());
        }
        match branch_and_revert(repo, &target, &branch, settings.repo(repo).sign.value) {
            Ok(()) => seeded.push(Seeded {
                repo: repo.to_string(),
                commit: target.commit,
            }),
            Err(why) => refused.push(why),
        }
    }
    let code = refused.iter().map(|r| r.code).fold(0, |worst, code| {
        if worst == 1 || code == 1 {
            1
        } else {
            code.max(worst)
        }
    });
    let said_refused = |say: &mut dyn FnMut(Say)| {
        for r in &refused {
            say(Say::Fail(format!("{}: {}", named(&r.repo), r.why)));
        }
    };
    if new.members.is_empty() {
        said_refused(say);
        say(Say::Fail(format!(
            "no member of {} could be reverted, so no change was created.",
            named(slug)
        )));
        return code;
    }
    if joined.is_empty() && seeded.is_empty() && existing.is_some() {
        said_refused(say);
        if refused.is_empty() {
            say(Say::Done(format!(
                "{new_slug}: every member purlis landed is seeded; nothing to do."
            )));
        }
        return code;
    }
    // Undoing goes the other way round, and only the order a joining member brings is
    // written: a member that joined now waits for the members that needed it, and a member
    // it needed waits for it. Order written before is left as it is.
    let members: BTreeSet<String> = new.members.iter().map(|m| m.repo.clone()).collect();
    for repo in &joined {
        let waits: Vec<String> = record
            .dependents(repo)
            .into_iter()
            .filter(|d| members.contains(d))
            .collect();
        for m in &mut new.members {
            if &m.repo == repo {
                for d in &waits {
                    if !m.needs.contains(d) {
                        m.needs.push(d.clone());
                    }
                }
            } else if record
                .member(repo)
                .is_some_and(|r| r.needs.contains(&m.repo))
                && !m.needs.contains(repo)
            {
                m.needs.push(repo.clone());
            }
        }
    }
    let saved = save(plane, ws, &new, say);
    if saved != 0 {
        return saved;
    }
    say(Say::Done(if existing.is_some() {
        format!(
            "change '{new_slug}' updated — {} member(s) to revert",
            new.members.len()
        )
    } else {
        format!(
            "change '{new_slug}' created — {} member(s) to revert",
            new.members.len()
        )
    }));
    for s in &seeded {
        let branch = new
            .member(&s.repo)
            .map(|m| m.branch.as_str())
            .unwrap_or_default();
        say(Say::Out(format!(
            "  {}  branch {}  reverts {}",
            shown::line(&s.repo),
            shown::line(branch),
            short(&s.commit)
        )));
    }
    said_refused(say);
    say(Say::Info(format!(
        "It is an ordinary change from here: purlis change push {new_slug}, then purlis \
         change land {new_slug} --repo <name> for each member, blockers first."
    )));
    code
}

#[cfg(test)]
mod tests;
