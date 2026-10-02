//! `charter change land <slug> --repo <name>`: one member merged, at the head commit its checks
//! passed on, after the members it needs have landed (ADR 0060 §2–§4, D3; ruling Q16; the
//! Python charter's `_land` at `cli-final`).
//!
//! **One member per run, and no `--all`.** A loop over members would have to decide what to do
//! when member 3 of 5 is refused, and every answer is wrong in a case the others handle. A
//! shell loop over this command is N gated landings, since the gates are in here.
//!
//! **The gates, in order, each refusal its own sentence (exit 2):**
//!
//! 1. one member named, and not by rebase;
//! 2. it is a member, with a clone here and a request that is open;
//! 3. every member it `needs` has landed: the forge reports its request merged, and where
//!    charter logged the landing, this clone's default branch still holds the logged commit;
//! 4. the checks at the request's head are `PASSED`, read at that exact commit.
//!
//! **The merge is pinned to that head.** The forge's own guard (`sha` on GitHub's and GitLab's
//! merge, `expectedHeadOid` on GitHub's queue, `sha` on GitLab's train) refuses a head that
//! moved after the checks were read, and charter then names the move. Nothing here asks a forge
//! to merge *later*: auto-merge would merge whatever head the branch has when a pipeline
//! passes, not the head charter read.
//!
//! **Through the queue where there is one (Q16).** When the target branch has a merge queue
//! (GitHub) or a merge train (GitLab), the request is put in it at the verified head and the
//! forge merges it once the queue's own checks pass. Nothing is logged then, because nothing
//! has merged. Run again after it merged, `land` records that landing, after the same gates.
//! Where there is no queue, charter merges it now, which is what "auto-merge when green" comes
//! to when green has just been read.
//!
//! **Logged only once confirmed.** After a merge charter reads the request again; only a
//! request the forge reports merged, at the head charter verified, gets a landing line.
//!
//! **Attended only.** `floorguard::PUBLISH_FORGE` refuses `charter change land` from a run
//! nobody is watching, exactly as it refuses `gh pr merge`.

use std::path::Path;

use chrono::{DateTime, Utc};

use super::cmd::{REFUSED, load, named, workspace_ok};
use super::landing::{self, Landing};
use super::record::{Member, Record};
use crate::forge::checks::{Checks, Ci};
use crate::forge::pr::{MergeAs, Pr, Repo, Request, State};
use crate::forge::{Caller, ForgeBackend};
use crate::repocmd::Say;
use crate::shown;
use crate::worktree::git;

/// The trailer charter writes on the landing commit it authors.
pub const TRAILER: &str = "Charter-Change";

/// How the landing commit is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// A merge commit: the default.
    Merge,
    /// One squashed commit.
    Squash,
    /// Refused, with the reason: a rebase leaves no commit of charter's to carry the trailer
    /// and no single commit to revert.
    Rebase,
}

/// The first twelve characters of a commit, contained.
fn short(sha: &str) -> String {
    shown::line(sha).chars().take(12).collect()
}

/// `1 check`, `3 checks`, or nothing when charter could not count them.
fn counted(checks: &Checks) -> String {
    match checks.total {
        Some(1) => " (1 check)".to_string(),
        Some(n) => format!(" ({n} checks)"),
        None => String::new(),
    }
}

/// What the check gate says for a value other than `PASSED`, each its own sentence.
fn not_passed(checks: &Checks, head: &str) -> String {
    let at = short(head);
    let n = counted(checks);
    match checks.ci {
        Ci::Passed => format!("checks PASSED at {at}{n}."),
        Ci::Failed => format!("checks FAILED at {at}{n}."),
        Ci::Running => format!("checks RUNNING at {at}{n} — not a verdict yet."),
        Ci::NotRun => format!(
            "checks NOT RUN at {at} — this head has no check run and no commit status. Nothing \
             ran, or nothing has run yet."
        ),
        Ci::Unknown => format!(
            "checks UNKNOWN at {at} — charter could not read them, or could not read all of \
             them{}. It will not treat 'I did not look' as 'nothing to see'.",
            checks
                .why
                .as_deref()
                .map(|why| format!(" ({})", shown::line(why)))
                .unwrap_or_default()
        ),
    }
}

/// `charter change land`, through each member's forge's own backend, logged as this machine.
pub fn land(
    plane: &Path,
    ws: &str,
    slug: &str,
    repos: &[String],
    how: How,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    land_with(
        plane,
        ws,
        slug,
        repos,
        how,
        &|repo: &Repo| repo.backend(),
        &crate::dispatch::host(),
        now,
        say,
    )
}

/// One member's forge, reached from its clone.
struct Reached {
    clone: std::path::PathBuf,
    on: Repo,
    backend: Box<dyn ForgeBackend>,
}

/// The member's clone and forge, or the refusal said.
fn reach(
    plane: &Path,
    ws: &str,
    repo: &str,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
) -> Result<Reached, String> {
    let Some(clone) = crate::repos::clone_at(plane, ws, repo) else {
        return Err(format!(
            "{}: not a repo in workspace '{ws}'. Clone it first: charter clone {} -w {ws}",
            named(repo),
            named(repo)
        ));
    };
    let on = Repo::of_clone(plane, &clone.path)
        .map_err(|why| format!("{}: {}", named(repo), shown::line(&why)))?;
    Ok(Reached {
        backend: backend_of(&on),
        clone: clone.path,
        on,
    })
}

/// [`land`], asking the backend `backend_of` builds for each member's repo, and logging as
/// `host`.
#[allow(clippy::too_many_arguments)]
pub fn land_with(
    plane: &Path,
    ws: &str,
    slug: &str,
    repos: &[String],
    how: How,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    host: &str,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    let refuse = |say: &mut dyn FnMut(Say), why: String| {
        say(Say::Fail(why));
        REFUSED
    };
    // The refusals of the request come before anything is read.
    let repo = match repos {
        [one] => one.as_str(),
        [] => {
            return refuse(
                say,
                "name the one member to land: --repo <name>. There is no --all.".into(),
            );
        }
        many => {
            let names: Vec<String> = many.iter().map(|r| named(r)).collect();
            return refuse(
                say,
                format!(
                    "charter lands one member per landing, and {} were named: {}. Land each \
                     in its own run, blockers first; there is no --all.",
                    many.len(),
                    names.join(", ")
                ),
            );
        }
    };
    if how == How::Rebase {
        return refuse(
            say,
            format!(
                "charter does not land by rebase. A rebase merge replays the author's own \
                 commits and charter authors none of them, so there is no commit to carry \
                 `{TRAILER}: {}` and no single commit for `charter change revert` to run \
                 against. Use the default merge or --squash; a repo that permits only rebase is \
                 one a person lands by hand.",
                named(slug)
            ),
        );
    }
    if !workspace_ok(plane, ws, say) {
        return 1;
    }
    let record = match load(plane, ws, slug, say) {
        Ok(record) => record,
        Err(code) => return code,
    };
    let Some(member) = record.member(repo) else {
        if let Some(out) = record.exclusion(repo) {
            return refuse(
                say,
                format!(
                    "{} was excluded from {}: {}",
                    named(repo),
                    named(slug),
                    shown::line(&out.why)
                ),
            );
        }
        return refuse(
            say,
            format!("{} is not a member of {}.", named(repo), named(slug)),
        );
    };
    let me = match reach(plane, ws, repo, backend_of) {
        Ok(me) => me,
        Err(why) => return refuse(say, why),
    };
    let caller = Caller::command();
    let sigil = me.on.forge.kind.change_sigil();
    let path = me.on.path.as_str();

    // Gate: the member has a request, and it is open.
    let req = match me.backend.by_head(&caller, path, &member.branch) {
        Ok(Some(req)) => req,
        Ok(None) => {
            return refuse(
                say,
                format!(
                    "{}: no request from branch {}. Push it first: charter change push {}",
                    named(repo),
                    named(&member.branch),
                    named(slug)
                ),
            );
        }
        Err(why) => {
            say(Say::Fail(format!(
                "{}: charter could not ask for its request: {}",
                named(repo),
                shown::line(&why.to_string())
            )));
            return 1;
        }
    };
    let pr = Pr {
        number: req.number,
        url: req.url.clone(),
    };
    let log = landing::landings(plane, ws, slug);
    match &req.state {
        State::Open => {}
        State::Closed => {
            return refuse(
                say,
                format!(
                    "{}: {sigil}{} is REJECTED — closed unmerged. Narrow the change instead: \
                     charter change drop {} {} --why \"…\"",
                    named(repo),
                    req.number,
                    named(slug),
                    named(repo)
                ),
            );
        }
        State::Merged { commit } => {
            return merged_already(
                plane, ws, &record, member, &me, &req, commit, &log, backend_of, host, now, say,
            );
        }
    }

    // Gate: every blocker has landed.
    if let Err(code) = blockers_landed(plane, ws, &record, member, &log, backend_of, say) {
        return code;
    }

    // Gate: the checks, at this head and no other.
    let checks = me.backend.checks_at(&caller, path, &req.head, req.number);
    if checks.ci != Ci::Passed {
        return refuse(
            say,
            format!("{}: {}", named(repo), not_passed(&checks, &req.head)),
        );
    }
    say(Say::Info(format!(
        "{}: checks PASSED at {}{}",
        named(repo),
        short(&req.head),
        counted(&checks)
    )));

    let queued = match me.backend.lands_through_queue(&caller, path, &pr) {
        Ok(queued) => queued,
        Err(why) => {
            say(Say::Fail(format!(
                "{}: charter could not read how {sigil}{} lands: {}. Nothing was merged.",
                named(repo),
                req.number,
                shown::line(&why.to_string())
            )));
            return 1;
        }
    };
    let trailer = format!("{TRAILER}: {}", shown::line(slug));
    let merge_as = MergeAs {
        squash: how == How::Squash,
        title: format!(
            "{}: {} ({sigil}{})",
            shown::line(slug),
            shown::line(repo),
            req.number
        ),
        message: format!("{}\n\n{trailer}", shown::line(&record.why)),
    };
    let asked = if queued {
        me.backend
            .enqueue_at(&caller, path, &pr, &req.head, &merge_as)
    } else {
        me.backend
            .merge_at(&caller, path, &pr, &req.head, &merge_as)
    };
    if let Err(why) = asked {
        // The forge's refusal is the evidence. Whether the head moved is asked again, so the
        // refusal names it in charter's words whichever words the forge used.
        if let Ok(Some(now_at)) = me.backend.by_head(&caller, path, &member.branch)
            && now_at.head != req.head
        {
            return refuse(
                say,
                format!(
                    "{}: the head moved since the check: checks PASSED at {}, and the branch \
                     is now at {}. Nothing was merged. Land it again once the checks at the new \
                     head have passed.",
                    named(repo),
                    short(&req.head),
                    short(&now_at.head)
                ),
            );
        }
        say(Say::Fail(format!(
            "{}: {}. Nothing was merged.",
            named(repo),
            shown::line(&why.to_string())
        )));
        return 1;
    }
    if queued {
        let noun = me.on.forge.kind.queue_noun();
        say(Say::Done(format!(
            "queued {sigil}{} in its {noun} at {}: the forge merges it once the {noun}'s own \
             checks pass.",
            req.number,
            short(&req.head)
        )));
        if how == How::Squash && me.on.forge.kind == crate::forge::Kind::GitHub {
            say(Say::Warn(
                "GitHub's merge queue lands by the method its own rule sets, so --squash was \
                 not GitHub's to take."
                    .into(),
            ));
        }
        say(Say::Info(format!(
            "Nothing is recorded until it has merged. Then run: charter change land {} --repo {}",
            named(slug),
            named(repo)
        )));
        return 0;
    }

    // The read-back: only what the forge confirms is logged.
    let confirmed = match me.backend.by_head(&caller, path, &member.branch) {
        Ok(Some(Request {
            state: State::Merged { commit: Some(c) },
            head,
            ..
        })) if head == req.head => c,
        Ok(_) => {
            say(Say::Fail(format!(
                "{}: the forge did not confirm the merge on a read-back, so nothing was recorded.",
                named(repo)
            )));
            return 1;
        }
        Err(why) => {
            say(Say::Fail(format!(
                "{}: merged, but the read-back failed ({}), so nothing was recorded.",
                named(repo),
                shown::line(&why.to_string())
            )));
            return 1;
        }
    };
    let line = Landing::new(slug, repo, req.number, &req.head, &confirmed, now);
    if landing::append(plane, ws, host, &line).is_none() {
        say(Say::Fail(format!(
            "{}: merged {sigil}{} as {}, but the landing log could not be written.",
            named(repo),
            req.number,
            short(&confirmed)
        )));
        return 1;
    }
    let squash = if how == How::Squash { " (squash)" } else { "" };
    say(Say::Done(format!(
        "merged {sigil}{} as {}{squash}, trailer {trailer}",
        req.number,
        short(&confirmed)
    )));
    0
}

/// A request already merged: landed and logged, landed by its queue and now recorded, or
/// merged by somebody else.
#[allow(clippy::too_many_arguments)]
fn merged_already(
    plane: &Path,
    ws: &str,
    record: &Record,
    member: &Member,
    me: &Reached,
    req: &Request,
    commit: &Option<String>,
    log: &std::collections::BTreeMap<String, Landing>,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    host: &str,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    let repo = member.repo.as_str();
    let sigil = me.on.forge.kind.change_sigil();
    let caller = Caller::command();
    let path = me.on.path.as_str();
    let pr = Pr {
        number: req.number,
        url: req.url.clone(),
    };
    if let Some(line) = log.get(repo) {
        say(Say::Fail(format!(
            "{}: {sigil}{} is already landed: charter recorded it as {}. Nothing to land.",
            named(repo),
            req.number,
            short(&line.merge)
        )));
        return REFUSED;
    }
    let queued = match me.backend.lands_through_queue(&caller, path, &pr) {
        Ok(queued) => queued,
        Err(why) => {
            say(Say::Fail(format!(
                "{}: charter could not read how {sigil}{} lands: {}",
                named(repo),
                req.number,
                shown::line(&why.to_string())
            )));
            return 1;
        }
    };
    let Some(commit) = commit.as_deref().filter(|_| queued) else {
        say(Say::Fail(format!(
            "{}: {sigil}{} is already merged, and not by charter, so there is no landing of \
             charter's to record. Nothing to land.",
            named(repo),
            req.number
        )));
        return REFUSED;
    };
    // The same gates as a landing: the queue merged what charter put in it only if they hold.
    if let Err(code) = blockers_landed(plane, ws, record, member, log, backend_of, say) {
        return code;
    }
    let checks = me.backend.checks_at(&caller, path, &req.head, req.number);
    if checks.ci != Ci::Passed {
        say(Say::Fail(format!(
            "{}: merged by its {}, and {} Nothing was recorded.",
            named(repo),
            me.on.forge.kind.queue_noun(),
            not_passed(&checks, &req.head)
        )));
        return REFUSED;
    }
    let line = Landing::new(&record.change, repo, req.number, &req.head, commit, now);
    if landing::append(plane, ws, host, &line).is_none() {
        say(Say::Fail(format!(
            "{}: the landing log could not be written, so nothing was recorded.",
            named(repo)
        )));
        return 1;
    }
    say(Say::Done(format!(
        "recorded the landing its {} made: {sigil}{} merged as {} at {}",
        me.on.forge.kind.queue_noun(),
        req.number,
        short(commit),
        short(&req.head)
    )));
    0
}

/// `Ok` when every member `member` needs has landed; else the refusal said, and its code.
fn blockers_landed(
    plane: &Path,
    ws: &str,
    record: &Record,
    member: &Member,
    log: &std::collections::BTreeMap<String, Landing>,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    say: &mut dyn FnMut(Say),
) -> Result<(), u8> {
    let repo = member.repo.as_str();
    for need in &member.needs {
        let Some(blocker) = record.member(need) else {
            say(Say::Fail(format!(
                "{}: blocker {} is not a member of this change.",
                named(repo),
                named(need)
            )));
            return Err(REFUSED);
        };
        let reached = match reach(plane, ws, need, backend_of) {
            Ok(reached) => reached,
            Err(why) => {
                say(Say::Fail(format!("{}: blocker {why}", named(repo))));
                return Err(REFUSED);
            }
        };
        match member_landed(plane, blocker, &reached, log) {
            Ok(None) => {}
            Ok(Some(why)) => {
                say(Say::Fail(format!(
                    "{}: blocker {} has not landed ({why}).",
                    named(repo),
                    named(need)
                )));
                return Err(REFUSED);
            }
            Err(why) => {
                say(Say::Fail(format!(
                    "{}: charter could not read blocker {}: {}",
                    named(repo),
                    named(need),
                    shown::line(&why)
                )));
                return Err(1);
            }
        }
    }
    Ok(())
}

/// What may be handed to git as a commit.
fn sha_ok(sha: &str) -> bool {
    (7..=64).contains(&sha.len())
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether `m` has landed: `Ok(None)` when it has, `Ok(Some(why not))`, or `Err` when the
/// forge could not be asked. Both halves: the forge must say merged, and where charter logged
/// the landing, the clone's default branch must still hold the logged commit. Merged with no
/// log line is landed, outside charter, which doctor names.
fn member_landed(
    plane: &Path,
    m: &Member,
    reached: &Reached,
    log: &std::collections::BTreeMap<String, Landing>,
) -> Result<Option<String>, String> {
    let req = reached
        .backend
        .by_head(&Caller::command(), &reached.on.path, &m.branch)
        .map_err(|why| why.to_string())?;
    let Some(req) = req else {
        return Ok(Some("it has no request".into()));
    };
    match req.state {
        State::Open => return Ok(Some("its request is open".into())),
        State::Closed => return Ok(Some("its request is REJECTED".into())),
        State::Merged { .. } => {}
    }
    let Some(line) = log.get(&m.repo) else {
        return Ok(None);
    };
    // The log is never committed, which makes it local rather than trustworthy: a hand edit
    // reaches here, and then a git argv.
    if !sha_ok(&line.merge) {
        return Ok(Some(format!(
            "merged, but the landing log's commit {} is not a commit id, and charter will not \
             hand it to git",
            shown::short(&line.merge)
        )));
    }
    let Some(default) = crate::reposave::default_branch(plane, &m.repo, &reached.clone) else {
        return Ok(Some(
            "merged, but charter cannot tell this clone's default branch".into(),
        ));
    };
    let holds = |at: &str| {
        git::run(
            &reached.clone,
            &["merge-base", "--is-ancestor", &line.merge, at],
            git::READ,
        )
        .is_ok_and(|run| run.ok())
    };
    if holds(&format!("refs/remotes/origin/{default}")) || holds(&format!("refs/heads/{default}")) {
        return Ok(None);
    }
    Ok(Some(format!(
        "merged as {}, which this clone's {} does not contain: reverted, or not fetched since. \
         `git fetch` in it, then land again",
        short(&line.merge),
        shown::line(&default)
    )))
}

#[cfg(test)]
mod tests;
