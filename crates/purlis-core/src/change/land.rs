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
//! 3. every member it `needs` has landed: the forge reports its request merged, and charter
//!    landed it ([`evidence`]): the landing log has it and this clone's default branch still
//!    holds the logged commit, or a pending landing of charter's is found merged at its head;
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
//! forge merges it once the queue's own checks pass. Where there is none, charter merges it
//! now, which is what "auto-merge when green" comes to when green has just been read. Where
//! GitLab cannot say whether there is a train, nothing is merged.
//!
//! **Recorded only on evidence that charter started it (D-472a).** Before the forge is asked,
//! a pending landing goes into [`super::pending`]. A direct merge the read-back confirms is
//! logged at once. A queued one, or one whose read-back failed, is logged by a later `land`
//! that finds the request merged at the pending head. A request merged with no pending landing
//! (a person in the browser, an admin, a merge from before the queue existed) is never
//! recorded as charter's.
//!
//! **Attended only.** `floorguard::PUBLISH_FORGE` refuses `charter change land` from a run
//! nobody is watching, exactly as it refuses `gh pr merge`.

use std::path::Path;

use chrono::{DateTime, Utc};

use super::cmd::{REFUSED, load, named, workspace_ok};
use super::landing::{self, Landing, Landings};
use super::pending::{self, At, Pending, Pendings, Stage, Via};
use super::record::{Member, Record};
use crate::forge::checks::{Checks, Ci};
use crate::forge::pr::{MergeAs, MergedAt, Pr, Repo, Request, State};
use crate::forge::{Caller, ForgeBackend};
use crate::repocmd::Say;
use crate::shown;
use crate::worktree::git;

/// The trailer charter writes on the landing commit it authors. Landings made before the rename
/// carry `Charter-Change`, which `change revert` still finds ([`crate::names::TRAILER_CHANGE`]).
pub const TRAILER: &str = crate::names::TRAILER_CHANGE.write;

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

/// A forge's or git's own words, contained to one line. Not clipped to a row's width: the
/// reason a merge did not happen is the one thing the reader needs whole.
fn forge_said(words: &str) -> String {
    shown::one_line(words, super::record::TEXT_LIMIT)
}

/// The first twelve characters of a commit, contained.
pub(super) fn short(sha: &str) -> String {
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
            "checks UNKNOWN at {at} — purlis could not read them, or could not read all of \
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
        &crate::dispatch::this_log_name(),
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
            "{}: not a repo in workspace '{ws}'. Clone it first: purlis clone {} -w {ws}",
            named(repo),
            named(repo)
        ));
    };
    let on = Repo::of_clone(plane, &clone.path)
        .map_err(|why| format!("{}: {}", named(repo), forge_said(&why)))?;
    Ok(Reached {
        backend: backend_of(&on),
        clone: clone.path,
        on,
    })
}

/// What charter's own records say about a member's landing: the one definition the land gate
/// and doctor share (D-472a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence<'a> {
    /// The landing log has it.
    Logged(&'a Landing),
    /// Charter started a landing and has not seen it merge.
    Started(&'a Pending),
    /// Charter has no record of landing it.
    None,
}

/// What `log` and `pending` say about `repo`'s landing `at`. A landing in the log wins; a
/// pending line counts only when it still stands and is about that landing
/// ([`Pending::started`]), so `land` and doctor never disagree about which merge is charter's.
pub fn evidence<'a>(log: &'a Landings, pending: &'a Pendings, repo: &str, at: At) -> Evidence<'a> {
    if let Some(line) = log.get(repo) {
        return Evidence::Logged(line);
    }
    match pending.get(repo) {
        Some(p) if p.started(at) => Evidence::Started(p),
        _ => Evidence::None,
    }
}

/// What is said when the pending landing could not be moved on from `asked`: it would stand as
/// evidence for a merge charter did not make, so the reader is told which line to remove.
fn unnoted(plane: &Path, ws: &str, repo: &str) -> String {
    format!(
        "{}: purlis could not note that in its pending landing, which still says it asked. \
         Remove this member's last line from {} so a later merge of this head is not taken for \
         purlis's.",
        named(repo),
        shown::line(&pending::pending_dir(plane, ws).display().to_string())
    )
}

/// The records a landing reads and writes, for one change in one workspace.
struct Books<'p> {
    plane: &'p Path,
    ws: &'p str,
    slug: &'p str,
    host: &'p str,
    now: DateTime<Utc>,
    log: Landings,
    pending: Pendings,
}

impl Books<'_> {
    /// Log `req`'s merge as `commit`, for `repo`: what a confirmed merge, or a pending landing
    /// found merged, comes to. `false` when the log could not be written.
    fn log(&mut self, repo: &str, req: &Request, commit: &str) -> bool {
        let line = Landing::new(self.slug, repo, req.number, &req.head, commit, self.now);
        if landing::append(self.plane, self.ws, self.host, &line).is_none() {
            return false;
        }
        self.log.insert(repo.to_string(), line);
        true
    }

    /// Append `line` to the pending file. `false` when it could not be written.
    fn pend(&mut self, line: Pending) -> bool {
        if pending::append(self.plane, self.ws, self.host, &line).is_none() {
            return false;
        }
        self.pending.insert(line.repo.clone(), line);
        true
    }
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
    gated(
        plane,
        ws,
        slug,
        repos,
        how,
        backend_of,
        host,
        now,
        Asked::Land(None),
        &mut None,
        say,
    )
}

/// How a verified landing goes, as the operator is shown it before saying yes (#474).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Through {
    /// Charter merges it now, at the verified head.
    Merge,
    /// Charter puts it in its branch's merge queue or merge train at the verified head.
    Queue,
    /// It has merged at a head charter started landing, and the landing is only recorded:
    /// the forge is asked for nothing.
    Record,
}

/// What [`verify`] found a landing would do: what the window's "asks first" names, and what
/// its Land hands back so that nothing else is landed ([`land_verified`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub repo: String,
    /// The request's number on its forge.
    pub number: u64,
    /// The request's web page, as its forge gave it.
    pub url: String,
    /// The head commit its checks passed at, in full.
    pub head: String,
    pub through: Through,
    /// Its forge, which names the request and the queue in that forge's own words.
    pub kind: crate::forge::Kind,
}

impl Verified {
    /// How it lands, in one sentence naming the head and the forge's own words for its queue:
    /// what the window's question says before the operator's yes.
    pub fn how(&self) -> String {
        let at = short(&self.head);
        let forge = self.kind.display();
        let noun = self.kind.queue_noun();
        match self.through {
            Through::Merge => format!("purlis merges it now, at {at} and no other."),
            Through::Queue => format!(
                "purlis puts it in its {noun} at {at}, and {forge} merges it once the {noun}'s \
                 own checks pass."
            ),
            Through::Record => format!(
                "It has merged at {at}, which purlis started landing. Land records the landing \
                 and asks {forge} for nothing."
            ),
        }
    }

    /// Whether squashing is charter's to ask for: a direct merge, or a queue that takes it
    /// (`Kind::queue_takes_squash`). GitHub's merge queue merges by its rule's method, and a
    /// landing only recorded merges nothing.
    pub fn squash_is_charters(&self) -> bool {
        match self.through {
            Through::Merge => true,
            Through::Queue => self.kind.queue_takes_squash(),
            Through::Record => false,
        }
    }

    /// The head as the core's lines show it: its first twelve characters.
    pub fn head_short(&self) -> String {
        short(&self.head)
    }
}

/// What [`gated`] is asked to do once every gate has passed.
enum Asked<'v> {
    /// Land it; when the operator confirmed a [`Verified`], only that.
    Land(Option<&'v Verified>),
    /// Say what it would do, and do none of it.
    Verify,
}

/// `charter change land`'s gates for `repo`, run and asked of no forge's merge: what the window
/// names before its Land asks the operator (#474). `Err` carries the exit the gate refused
/// with, its refusal said in the same words the CLI says.
///
/// It may record a blocker's landing that charter started and finds merged, exactly as
/// [`land`] would; it never writes a pending landing and never asks a forge to merge.
pub fn verify(
    plane: &Path,
    ws: &str,
    slug: &str,
    repo: &str,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> Result<Verified, u8> {
    verify_with(
        plane,
        ws,
        slug,
        repo,
        &|repo: &Repo| repo.backend(),
        &crate::dispatch::this_log_name(),
        now,
        say,
    )
}

/// [`verify`], through `backend_of` and logging as `host`.
#[allow(clippy::too_many_arguments)]
pub fn verify_with(
    plane: &Path,
    ws: &str,
    slug: &str,
    repo: &str,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    host: &str,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> Result<Verified, u8> {
    let mut found = None;
    let code = gated(
        plane,
        ws,
        slug,
        &[repo.to_string()],
        How::Merge,
        backend_of,
        host,
        now,
        Asked::Verify,
        &mut found,
        say,
    );
    found.ok_or(if code == 0 { 1 } else { code })
}

/// Land the landing the operator confirmed, and nothing else: every gate is taken again, and
/// when the request, its head or how it lands is not what `confirmed` says, it is refused
/// before any forge is asked to merge (#474).
pub fn land_verified(
    plane: &Path,
    ws: &str,
    slug: &str,
    confirmed: &Verified,
    how: How,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    land_verified_with(
        plane,
        ws,
        slug,
        confirmed,
        how,
        &|repo: &Repo| repo.backend(),
        &crate::dispatch::this_log_name(),
        now,
        say,
    )
}

/// [`land_verified`], through `backend_of` and logging as `host`.
#[allow(clippy::too_many_arguments)]
pub fn land_verified_with(
    plane: &Path,
    ws: &str,
    slug: &str,
    confirmed: &Verified,
    how: How,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    host: &str,
    now: DateTime<Utc>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    gated(
        plane,
        ws,
        slug,
        std::slice::from_ref(&confirmed.repo),
        how,
        backend_of,
        host,
        now,
        Asked::Land(Some(confirmed)),
        &mut None,
        say,
    )
}

/// How a landing is said in the refusal of one that is not what was confirmed.
fn as_said(sigil: &str, number: u64, head: &str, through: Through, noun: &str) -> String {
    let how = match through {
        Through::Merge => "merged now".to_string(),
        Through::Queue => format!("into its {noun}"),
        Through::Record => "already merged, to be recorded".to_string(),
    };
    format!("{sigil}{number} at {}, {how}", short(head))
}

/// `Err` with the refusal when what the gates found is not the landing `confirmed` names.
fn as_confirmed(
    confirmed: &Verified,
    found: &Verified,
    sigil: &str,
    noun: &str,
) -> Result<(), String> {
    if confirmed == found {
        return Ok(());
    }
    Err(format!(
        "{}: this is not the landing you confirmed: you confirmed {}, and it is now {}. Nothing \
         was merged. Look at it again and confirm what it says now.",
        named(&found.repo),
        as_said(
            sigil,
            confirmed.number,
            &confirmed.head,
            confirmed.through,
            noun
        ),
        as_said(sigil, found.number, &found.head, found.through, noun)
    ))
}

/// The gates, then what `asked` says to do. Asked to verify, a landing every gate let through
/// is put in `found`, and nothing is written or asked to merge.
#[allow(clippy::too_many_arguments)]
fn gated(
    plane: &Path,
    ws: &str,
    slug: &str,
    repos: &[String],
    how: How,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    host: &str,
    now: DateTime<Utc>,
    asked: Asked,
    found: &mut Option<Verified>,
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
                    "purlis lands one member per landing, and {} were named: {}. Land each \
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
                "purlis does not land by rebase. A rebase merge replays the author's own \
                 commits and purlis authors none of them, so there is no commit to carry \
                 `{TRAILER}: {}` and no single commit a revert could undo. Use the default \
                 merge or --squash; a repo that permits only rebase is one a person lands by \
                 hand.",
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
    let mut books = Books {
        plane,
        ws,
        slug,
        host,
        now,
        log: landing::landings(plane, ws, slug),
        pending: pending::pendings(plane, ws, slug),
    };

    // Gate: the member has a request, and it is open.
    let req = match me.backend.by_head(&caller, path, &member.branch) {
        Ok(Some(req)) => req,
        Ok(None) => {
            return refuse(
                say,
                format!(
                    "{}: no request from branch {}. Push it first: purlis change push {}",
                    named(repo),
                    named(&member.branch),
                    named(slug)
                ),
            );
        }
        Err(why) => {
            say(Say::Fail(format!(
                "{}: purlis could not ask for its request: {}",
                named(repo),
                forge_said(&why.to_string())
            )));
            return 1;
        }
    };
    let pr = Pr {
        number: req.number,
        url: req.url.clone(),
    };
    match &req.state {
        State::Open => {}
        State::Closed => {
            return refuse(
                say,
                format!(
                    "{}: {sigil}{} is REJECTED — closed unmerged. Narrow the change instead: \
                     purlis change drop {} {} --why \"…\"",
                    named(repo),
                    req.number,
                    named(slug),
                    named(repo)
                ),
            );
        }
        State::Merged { commit } => {
            let started = matches!(
                evidence(
                    &books.log,
                    &books.pending,
                    repo,
                    At::Request(req.number, &req.head)
                ),
                Evidence::Started(_)
            );
            let this = Verified {
                repo: repo.to_string(),
                number: req.number,
                url: req.url.clone(),
                head: req.head.clone(),
                through: Through::Record,
                kind: me.on.forge.kind,
            };
            match asked {
                // Only a landing charter started is one to record; the others are refused
                // below, in the words `land` says them in.
                Asked::Verify if started => {
                    *found = Some(this);
                    return 0;
                }
                Asked::Land(Some(confirmed)) => {
                    if let Err(why) =
                        as_confirmed(confirmed, &this, sigil, me.on.forge.kind.queue_noun())
                    {
                        return refuse(say, why);
                    }
                }
                _ => {}
            }
            return merged_already(&mut books, member, &me, &req, commit.as_deref(), say);
        }
    }

    // Gate: every blocker has landed.
    if let Err(code) = blockers_landed(&mut books, &record, member, backend_of, say) {
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
                "{}: purlis could not tell how {sigil}{} lands: {}. Nothing was merged.",
                named(repo),
                req.number,
                forge_said(&why.to_string())
            )));
            return 1;
        }
    };
    let this = Verified {
        repo: repo.to_string(),
        number: req.number,
        url: req.url.clone(),
        head: req.head.clone(),
        through: if queued {
            Through::Queue
        } else {
            Through::Merge
        },
        kind: me.on.forge.kind,
    };
    match asked {
        Asked::Verify => {
            *found = Some(this);
            return 0;
        }
        Asked::Land(Some(confirmed)) => {
            if let Err(why) = as_confirmed(confirmed, &this, sigil, me.on.forge.kind.queue_noun()) {
                return refuse(say, why);
            }
        }
        Asked::Land(None) => {}
    }
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
    // The evidence that charter started this landing, written before the forge is asked: it
    // is what lets a later `land` record a merge it did not see happen, and nothing else may.
    let via = if queued { Via::Queue } else { Via::Direct };
    let started = Pending::new(slug, repo, req.number, &req.head, via, Stage::Asked, now);
    if !books.pend(started.clone()) {
        say(Say::Fail(format!(
            "{}: purlis could not write its pending landing, so it did not ask the forge. \
             Nothing was merged.",
            named(repo)
        )));
        return 1;
    }
    let asked = if queued {
        me.backend
            .enqueue_at(&caller, path, &pr, &req.head, &merge_as)
            .map(|()| MergedAt::Now)
    } else {
        me.backend
            .merge_at(&caller, path, &pr, &req.head, &merge_as)
    };
    let asked = match asked {
        Ok(asked) => asked,
        Err(why) => {
            let noted = books.pend(started.at(Stage::Refused, now));
            // The forge's refusal is the evidence. Whether the head moved is asked again, so
            // the refusal names it in charter's words whichever words the forge used.
            let (said, code) = match me.backend.by_head(&caller, path, &member.branch) {
                Ok(Some(now_at)) if now_at.head != req.head => (
                    format!(
                        "{}: the head moved since the check: checks PASSED at {}, and the \
                         branch is now at {}. Nothing was merged. Land it again once the checks \
                         at the new head have passed.",
                        named(repo),
                        short(&req.head),
                        short(&now_at.head)
                    ),
                    REFUSED,
                ),
                _ => (
                    format!(
                        "{}: {}. Nothing was merged.",
                        named(repo),
                        forge_said(&why.to_string())
                    ),
                    1,
                ),
            };
            say(Say::Fail(said));
            if !noted {
                say(Say::Fail(unnoted(plane, ws, repo)));
                return 1;
            }
            return code;
        }
    };
    if let MergedAt::Later(why) = asked {
        let noted = books.pend(started.at(Stage::MergeLater, now));
        say(Say::Fail(format!(
            "{}: {}. It will merge whatever head the branch has when a pipeline passes, not \
             the one purlis checked. Cancel its auto-merge on GitLab.",
            named(repo),
            forge_said(&why)
        )));
        if !noted {
            say(Say::Fail(unnoted(plane, ws, repo)));
        }
        return 1;
    }
    if queued {
        let kind = me.on.forge.kind;
        let noun = kind.queue_noun();
        say(Say::Done(format!(
            "queued {sigil}{} in its {noun} at {}: the forge merges it once the {noun}'s own \
             checks pass.",
            req.number,
            short(&req.head)
        )));
        if how == How::Squash && !kind.queue_takes_squash() {
            say(Say::Warn(format!(
                "this {noun} lands by the method its own rule sets, so --squash was not \
                 purlis's to choose."
            )));
        }
        say(Say::Info(format!(
            "Nothing is recorded until it has merged. Then run: purlis change land {} --repo {}",
            named(slug),
            named(repo)
        )));
        return 0;
    }

    // The read-back: only what the forge confirms is logged.
    let again = format!(
        "Run purlis change land {} --repo {} again to record it once the forge shows it merged.",
        named(slug),
        named(repo)
    );
    let confirmed = match me.backend.by_head(&caller, path, &member.branch) {
        Ok(Some(Request {
            state: State::Merged { commit: Some(c) },
            head,
            ..
        })) if head == req.head => c,
        Ok(_) => {
            say(Say::Fail(format!(
                "{}: the forge did not confirm the merge on a read-back, so nothing was \
                 recorded yet. {again}",
                named(repo)
            )));
            return 1;
        }
        Err(why) => {
            say(Say::Fail(format!(
                "{}: the read-back failed ({}), so nothing was recorded yet. {again}",
                named(repo),
                forge_said(&why.to_string())
            )));
            return 1;
        }
    };
    if !books.log(repo, &req, &confirmed) {
        say(Say::Fail(format!(
            "{}: merged {sigil}{} as {}, but the landing log could not be written. {again}",
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

/// A request already merged: in the landing log, started by charter and now recorded, or
/// merged by somebody else, which is never recorded as charter's.
fn merged_already(
    books: &mut Books,
    member: &Member,
    me: &Reached,
    req: &Request,
    commit: Option<&str>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    let repo = member.repo.as_str();
    let sigil = me.on.forge.kind.change_sigil();
    match evidence(
        &books.log,
        &books.pending,
        repo,
        At::Request(req.number, &req.head),
    ) {
        Evidence::Logged(line) => {
            say(Say::Fail(format!(
                "{}: {sigil}{} is already landed, and the landing log has it as {}. Nothing to \
                 land.",
                named(repo),
                req.number,
                short(&line.merge)
            )));
            REFUSED
        }
        Evidence::Started(p) => {
            let Some(commit) = commit else {
                say(Say::Fail(format!(
                    "{}: {sigil}{} is merged, and the forge names no commit it merged as, so \
                     nothing was recorded.",
                    named(repo),
                    req.number
                )));
                return 1;
            };
            let via = p.via;
            if !books.log(repo, req, commit) {
                say(Say::Fail(format!(
                    "{}: the landing log could not be written, so nothing was recorded.",
                    named(repo)
                )));
                return 1;
            }
            let how = match via {
                Via::Queue => format!("its {}", me.on.forge.kind.queue_noun()),
                Via::Direct => "charter".to_string(),
            };
            say(Say::Done(format!(
                "recorded the landing {how} made: {sigil}{} merged as {} at {}",
                req.number,
                short(commit),
                short(&req.head)
            )));
            0
        }
        Evidence::None => {
            say(Say::Fail(format!(
                "{}: {sigil}{} is already merged, and not by purlis: purlis has no record of \
                 starting that landing, so it records none. Nothing to land.",
                named(repo),
                req.number
            )));
            REFUSED
        }
    }
}

/// `Ok` when every member `member` needs has landed; else the refusal said, and its code.
fn blockers_landed(
    books: &mut Books,
    record: &Record,
    member: &Member,
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
        let reached = match reach(books.plane, books.ws, need, backend_of) {
            Ok(reached) => reached,
            Err(why) => {
                say(Say::Fail(format!("{}: blocker {why}", named(repo))));
                return Err(REFUSED);
            }
        };
        match member_landed(books, blocker, &reached) {
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
                    "{}: purlis could not read blocker {}: {}",
                    named(repo),
                    named(need),
                    forge_said(&why)
                )));
                return Err(1);
            }
        }
    }
    Ok(())
}

/// Whether `at` holds `commit` in `clone`: `commit` is `at` or one of its ancestors. `false`
/// for anything git could not answer. A revert does not take a commit out of a branch's
/// history, so this is never how a revert is seen.
pub(super) fn holds(clone: &Path, commit: &str, at: &str) -> bool {
    git::run(
        clone,
        &["merge-base", "--is-ancestor", commit, at],
        git::READ,
    )
    .is_ok_and(|run| run.ok())
}

/// What may be handed to git as a commit. Doctor holds a landing log's line to it too.
pub(crate) fn sha_ok(sha: &str) -> bool {
    (7..=64).contains(&sha.len())
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether `m` has landed: `Ok(None)` when it has, `Ok(Some(why not))`, or `Err` when the
/// forge could not be asked. The answer is [`verdict`]'s; a pending landing found merged is
/// logged now.
fn member_landed(
    books: &mut Books,
    m: &Member,
    reached: &Reached,
) -> Result<Option<String>, String> {
    let req = reached
        .backend
        .by_head(&Caller::command(), &reached.on.path, &m.branch)
        .map_err(|why| why.to_string())?;
    let Some(req) = req else {
        return Ok(Some("it has no request".into()));
    };
    match verdict(
        books.plane,
        &books.log,
        &books.pending,
        &m.repo,
        &reached.clone,
        &req,
    ) {
        Verdict::Landed => Ok(None),
        Verdict::NotLanded(why) => Ok(Some(why)),
        Verdict::ToRecord(commit) => {
            if !books.log(&m.repo, &req, &commit) {
                return Ok(Some(
                    "merged, and the landing log could not be written".into(),
                ));
            }
            Ok(None)
        }
    }
}

/// Whether a member has landed, by the one definition the land gate and `change show` share
/// (#877): what [`verdict`] says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The forge says merged, the landing log has it, and the clone's default branch holds
    /// the logged commit.
    Landed,
    /// The forge says merged as this commit, and charter started that landing and has not
    /// logged it yet: landed once it is logged, which the land gate does and `show` does not.
    ToRecord(String),
    /// Not landed, and why, in the words the land gate refuses a blocker in.
    NotLanded(String),
}

/// Whether `repo`'s request `req` has landed. The forge must say merged, and charter must
/// have landed it ([`evidence`]): a logged landing whose commit the clone's default branch
/// still holds, or a pending one found merged at its head. A request merged with neither was
/// merged outside charter, and its order cannot be vouched for. Reads only: nothing is written.
pub fn verdict(
    plane: &Path,
    log: &Landings,
    pending: &Pendings,
    repo: &str,
    clone: &Path,
    req: &Request,
) -> Verdict {
    let commit = match &req.state {
        State::Open => return Verdict::NotLanded("its request is open".into()),
        State::Closed => return Verdict::NotLanded("its request is REJECTED".into()),
        State::Merged { commit } => commit.clone(),
    };
    let merge = match evidence(log, pending, repo, At::Request(req.number, &req.head)) {
        Evidence::Logged(line) => line.merge.clone(),
        Evidence::Started(_) => {
            return match commit {
                Some(commit) => Verdict::ToRecord(commit),
                None => {
                    Verdict::NotLanded("merged, and the forge names no commit it merged as".into())
                }
            };
        }
        Evidence::None => {
            return Verdict::NotLanded(format!(
                "merged outside purlis: purlis has no record of landing it, so it cannot \
                 vouch for the order. A person decides whether {} lands without it",
                shown::line(repo)
            ));
        }
    };
    // The log is never committed, which makes it local rather than trustworthy: a hand edit
    // reaches here, and then a git argv.
    if !sha_ok(&merge) {
        return Verdict::NotLanded(format!(
            "merged, but the landing log's commit {} is not a commit id, and purlis will not \
             hand it to git",
            shown::short(&merge)
        ));
    }
    let Some(default) = crate::reposave::default_branch(plane, repo, clone) else {
        return Verdict::NotLanded(
            "merged, but purlis cannot tell this clone's default branch".into(),
        );
    };
    let holds = |at: &str| holds(clone, &merge, at);
    if holds(&format!("refs/remotes/origin/{default}")) || holds(&format!("refs/heads/{default}")) {
        return Verdict::Landed;
    }
    Verdict::NotLanded(format!(
        "merged as {}, which this clone's {} does not contain: rewritten, or not fetched since. \
         `git fetch` in it, then land again",
        short(&merge),
        shown::line(&default)
    ))
}

#[cfg(test)]
mod tests;
