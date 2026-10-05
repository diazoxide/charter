//! `charter change push`: every member's branch pushed, its request opened or found, and the
//! change's cross-link block spliced into each request's description (ADR 0060 §4; the
//! Python spec's §8.2 "EXTEND" answer, `charter/commands_change.py` at `cli-final`).
//!
//! **Membership is committed; destination is local.** The record names repos and branches and
//! never a place. Where each member goes, its forge, its repository path and its HTTPS push
//! URL, is read from the member's own clone's `origin`, which the operator put there by hand.
//! Every destination is printed before anything is pushed, so an agent's run is readable.
//!
//! **What it never does.** It commits nothing, and it never force-pushes: each push is the one
//! refspec `refs/heads/<branch>:refs/heads/<branch>`, with no `+` and no flag, so it can only
//! fast-forward the remote's branch or create it. It ignores a repo's `[repos.<name>] mode`,
//! `off` included: `off` governs saves, and this verb is run by hand over repos somebody named
//! (ADR 0060 D4, ADR 0051 amended).
//!
//! **The destination printed is the one git pushes to.** A repo whose git config would send
//! the push elsewhere is refused, naming the setting, and the push sets on its own command
//! line that no tag, push option or submodule goes with the branch.
//!
//! **Fire and report.** One member's failure is said and the rest still run: a third member
//! that is not a repo here is no reason to leave the first two unpushed. The exit is 1 when
//! any member was not pushed, opened or written, as the Python charter answered, and 2 only
//! when the whole command is refused. A member charter reached and could not open a request
//! for is a `—` row of the block, never a missing one, because a table without it says the
//! change has fewer members than it has.
//!
//! **A reference only where it resolves.** A row names a request on the description's own
//! host by reference (`acme/widget#7`, `acme/plat/widget!7`), and one on any other host by its
//! URL, so each member's description gets a block written for its own host.
//!
//! **The block is charter's; the rest of the description is not.** Charter writes only
//! between [`BLOCK_BEGIN`] and [`BLOCK_END`]. A request whose description lacks exactly one
//! pair of them, outside any code fence, is left exactly as it is and named.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::cmd::{REFUSED, load, named, workspace_ok};
use super::record::{Record, TEXT_LIMIT};
use crate::forge::pr::{Pr, Repo, State};
use crate::forge::{self, Caller, ForgeBackend};
use crate::names::{CHANGE_BLOCK_BEGIN, CHANGE_BLOCK_END};
use crate::repocmd::Say;
use crate::shown;
use crate::tui::{self, Align};
use crate::worktree::git;

/// The line that opens charter's block in a request's description. A request opened before
/// the rename carries the charter spelling — Python's `BLOCK_BEGIN`, kept to the byte — which
/// [`splice`] still finds, and rewrites to this one (V93j, [`crate::names::CHANGE_BLOCK_BEGIN`]).
pub const BLOCK_BEGIN: &str = crate::names::CHANGE_BLOCK_BEGIN.write;
/// The line that closes it.
pub const BLOCK_END: &str = crate::names::CHANGE_BLOCK_END.write;

/// A markdown fence, either spelling. A marker inside one is prose about the block, not the
/// block, and splicing there would rewrite an example somebody wrote out.
const FENCES: [&str; 2] = ["```", "~~~"];

/// One table cell: contained to one line first, so a `why` cannot become a second row; then
/// markdown's column separator escaped, so it cannot become a second column; then an HTML
/// comment's opening neutralised, so a `why` that spells one of charter's markers cannot put
/// a second marker in the block and lock it against every later splice.
fn cell(value: &str) -> String {
    shown::one_line(value, TEXT_LIMIT)
        .replace('|', "\\|")
        .replace("<!--", "&lt;!--")
}

/// The git config keys that send a push somewhere other than the URL it was given. A repo that
/// sets one is refused, so the destination printed is always the one git pushes to.
const PUSH_REWRITES: &str = r"^(url\..*\.pushinsteadof|remote\..*\.pushurl)$";

/// What this push sets for itself on git's command line, which beats every config file: no
/// tag goes with the branch, no push option a repo's config names is sent, and no submodule is
/// pushed or checked.
const PUSH_ONLY_THE_BRANCH: [&str; 6] = [
    "-c",
    "push.followTags=false",
    "-c",
    "push.pushOption=",
    "-c",
    "push.recurseSubmodules=no",
];

/// A member charter reached: its request's number, and where that request lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reached {
    pub number: u64,
    /// The repository's path on its forge: `owner/name`, or a GitLab namespace.
    pub path: String,
    /// `#` on GitHub, `!` on GitLab: per member, since one workspace can hold both.
    pub sigil: &'static str,
    /// The forge's host: `github.com`, `gitlab.com`, or a self-managed one.
    pub host: String,
    /// The request's web page, as its forge gave it.
    pub url: String,
}

impl Reached {
    /// How a description on `host` names this request. A reference (`acme/widget#7`,
    /// `acme/plat/widget!7`) is resolved by the forge that renders it, against its own repos,
    /// so it is written only into a description on the request's own host. On any
    /// other host it would name somebody else's issue or nothing at all, and the request's
    /// URL is written instead.
    fn named_on(&self, host: &str) -> String {
        if self.host.eq_ignore_ascii_case(host) {
            format!("{}{}{}", cell(&self.path), self.sigil, self.number)
        } else {
            cell(&self.url)
        }
    }
}

/// The block charter writes into a request on `host`: the change, why, and one row per member
/// with its request, or `—` for a member `reached` has no request for.
pub fn block<'r>(
    record: &Record,
    reached: &dyn Fn(&str) -> Option<&'r Reached>,
    host: &str,
) -> String {
    let mut lines = vec![
        BLOCK_BEGIN.to_string(),
        format!(
            "**Cross-repo change: `{}`** — {}",
            cell(&record.change),
            cell(&record.why)
        ),
        String::new(),
        "| repo | request | needs |".to_string(),
        "|---|---|---|".to_string(),
    ];
    for m in &record.members {
        let at = reached(&m.repo).map_or_else(|| "—".to_string(), |r| r.named_on(host));
        let needs = if m.needs.is_empty() {
            "—".to_string()
        } else {
            m.needs
                .iter()
                .map(|n| cell(n))
                .collect::<Vec<_>>()
                .join(", ")
        };
        lines.push(format!("| {} | {at} | {needs} |", cell(&m.repo)));
    }
    lines.push(BLOCK_END.to_string());
    lines.join("\n")
}

/// `body` with the region between charter's markers — under any spelling they have had —
/// replaced by `block`, or `None` — a
/// refusal, not a fallback — when the markers are absent, when either appears more than
/// once, when they are out of order, or when either sits inside a code fence.
///
/// Everything outside the block goes back byte for byte: its line endings, and whether the
/// description ends in a newline. The block's own lines take the ending of the line its first
/// marker was on, so a description written with `\r\n` stays `\r\n` throughout.
pub fn splice(body: &str, block: &str) -> Option<String> {
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let (mut begins, mut ends) = (Vec::new(), Vec::new());
    let (mut fence, mut fenced): (Option<&str>, bool) = (None, false);
    for (i, line) in lines.iter().enumerate() {
        // A fence is closed only by a fence of its own kind: a `~~~` inside a ``` fence is
        // the fence's text.
        if let Some(kind) = FENCES.iter().find(|f| line.trim().starts_with(**f)) {
            match fence {
                None => fence = Some(kind),
                Some(open) if open == *kind => fence = None,
                Some(_) => {}
            }
        }
        if CHANGE_BLOCK_BEGIN.spellings().any(|m| line.contains(m)) {
            begins.push(i);
            fenced |= fence.is_some();
        }
        if CHANGE_BLOCK_END.spellings().any(|m| line.contains(m)) {
            ends.push(i);
            fenced |= fence.is_some();
        }
    }
    let ([begin], [end]) = (begins.as_slice(), ends.as_slice()) else {
        return None;
    };
    if fenced || begin > end {
        return None;
    }
    let ending = if lines[*begin].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let last = lines[*end];
    let kept_ending = &last[last.trim_end_matches(['\n', '\r']).len()..];
    let mut out = lines[..*begin].concat();
    out.push_str(&block.lines().collect::<Vec<_>>().join(ending));
    out.push_str(kept_ending);
    out.push_str(&lines[end + 1..].concat());
    Some(out)
}

/// The description charter writes when it opens a request on `host`: the change's `why`, and
/// its block with no request in it yet, so the splice that follows has its markers to find.
fn new_body(record: &Record, host: &str) -> String {
    format!(
        "{}\n\n{}",
        cell(&record.why),
        block(record, &|_| None, host)
    )
}

/// Where one member goes, resolved from its repo before anything is pushed.
struct Plan {
    repo: String,
    branch: String,
    /// The commit the branch is at here.
    head: String,
    clone: PathBuf,
    on: Repo,
    /// The destination printed: the HTTPS URL the clone's `origin` names.
    https: String,
    /// The URL git is handed: `https` itself, except under a test's route.
    to: String,
    base: Option<String>,
}

/// One member's plan, or `Err` with the reason it is not pushed already said.
fn plan(
    plane: &Path,
    ws: &str,
    slug: &str,
    repo: &str,
    branch: &str,
    route: &dyn Fn(&str) -> String,
    say: &mut dyn FnMut(Say),
) -> Result<Plan, ()> {
    let Some(clone) = crate::repos::clone_at(plane, ws, repo) else {
        say(Say::Fail(format!(
            "{}: not a repo in workspace '{ws}', so it is not pushed.",
            named(repo)
        )));
        say(Say::Info(format!(
            "Clone it first: charter clone {} -w {ws}",
            named(repo)
        )));
        return Err(());
    };
    let fail = |say: &mut dyn FnMut(Say), why: String| -> Result<Plan, ()> {
        say(Say::Fail(format!("{}: {}", named(repo), shown::line(&why))));
        Err(())
    };
    let on = match Repo::of_clone(plane, &clone.path) {
        Ok(on) => on,
        Err(why) => return fail(say, why),
    };
    let Some(https) = crate::planegit::origin_https_of(plane, &clone.path) else {
        return fail(
            say,
            "its origin is not a GitHub or GitLab URL charter can push to over HTTPS".into(),
        );
    };
    let local = git::run(
        &clone.path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
        git::READ,
    );
    let head = match local {
        Ok(run) if run.ok() => run.out.trim().to_string(),
        _ => String::new(),
    };
    if head.is_empty() {
        return fail(
            say,
            format!(
                "it has no branch {} here. Create it in the repo, or set the member's branch \
                 in workspaces/{ws}/changes/{}.json",
                named(branch),
                named(slug)
            ),
        );
    }
    match git::run(
        &clone.path,
        &["config", "--get-regexp", PUSH_REWRITES],
        git::READ,
    ) {
        // `git config --get-regexp` exits 1 when no key matches.
        Ok(run) if run.code == Some(1) => {}
        Ok(run) if run.ok() => {
            let keys: Vec<String> = run
                .out
                .lines()
                .filter_map(|l| l.split_whitespace().next())
                .map(shown::line)
                .collect();
            return fail(
                say,
                format!(
                    "its git config sends pushes somewhere other than the URL printed here \
                     ({}), so it is not pushed. Remove that setting to push it.",
                    keys.join(", ")
                ),
            );
        }
        _ => {
            return fail(
                say,
                "charter could not read its git config, so it cannot say where a push would \
                 go"
                .into(),
            );
        }
    }
    let to = route(&https);
    if let Err(why) = git_pushes_to(&clone.path, &to) {
        return fail(say, why);
    }
    let base = crate::reposave::default_branch(plane, repo, &clone.path);
    Ok(Plan {
        repo: repo.to_string(),
        branch: branch.to_string(),
        head,
        clone: clone.path,
        on,
        https,
        to,
        base,
    })
}

/// `Ok` when git, in `clone`, pushes to `to` itself; else why not, naming the setting.
///
/// A `url.<base>.insteadOf` rewrites the URL of a push as well as a fetch's, and git applies
/// it to a URL given on the command line too, so the rewrite is read the way git makes it:
/// `ls-remote --get-url`, under the same settings the push runs with, which rewrites and asks
/// no network. (`pushInsteadOf` and `pushurl`, which only a push reads, are refused before this
/// by [`PUSH_REWRITES`].)
fn git_pushes_to(clone: &Path, to: &str) -> Result<(), String> {
    let mut args: Vec<&str> = PUSH_ONLY_THE_BRANCH.to_vec();
    args.extend(["ls-remote", "--get-url", to]);
    let unread =
        || "charter could not ask git where a push would go, so it is not pushed".to_string();
    let run = git::run(clone, &args, git::READ).map_err(|_| unread())?;
    if !run.ok() {
        return Err(unread());
    }
    if run.out.trim() == to {
        return Ok(());
    }
    // The settings that rewrite it: each `insteadOf` whose value begins the URL.
    let keys: Vec<String> = git::run(
        clone,
        &["config", "--get-regexp", r"^url\..*\.insteadof$"],
        git::READ,
    )
    .map(|run| {
        run.out
            .lines()
            .filter_map(|l| l.split_once(' '))
            .filter(|(_, value)| !value.is_empty() && to.starts_with(value))
            .map(|(key, _)| shown::line(key))
            .collect()
    })
    .unwrap_or_default();
    let named = if keys.is_empty() {
        String::new()
    } else {
        format!(" ({})", keys.join(", "))
    };
    Err(format!(
        "its git config sends pushes somewhere other than the URL printed here{named}, so it \
         is not pushed. Remove that setting to push it."
    ))
}

/// `charter change push <slug>`, through each member's forge's own backend.
pub fn push(plane: &Path, ws: &str, slug: &str, say: &mut dyn FnMut(Say)) -> u8 {
    push_with(
        plane,
        ws,
        slug,
        &|repo: &Repo| repo.backend(),
        &|https: &str| https.to_string(),
        say,
    )
}

/// Where one member would be pushed, as the window names it before its Push asks the operator
/// (#474), and as the operator's yes hands it back ([`push_confirmed`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub repo: String,
    pub branch: String,
    /// The commit the branch is at in the member's clone: what the push would send.
    pub head: String,
    /// The HTTPS URL the clone's `origin` names: where the branch goes.
    pub to: String,
    /// The branch a request opened for it would go into, when charter can tell.
    pub base: Option<String>,
    /// Its forge, which names its request in that forge's own words.
    pub kind: crate::forge::Kind,
}

impl From<&Plan> for Destination {
    fn from(p: &Plan) -> Self {
        Destination {
            repo: p.repo.clone(),
            branch: p.branch.clone(),
            head: p.head.clone(),
            to: p.https.clone(),
            base: p.base.clone(),
            kind: p.on.forge.kind,
        }
    }
}

/// Where each member of `slug` would be pushed, read from its clone with no network and
/// nothing pushed: what the window's "asks first" names (#474). A member that would not be
/// pushed is left out and said, in the words `charter change push` says it in. `Err` is the
/// refusal of the whole command (no such change, no members), already said.
pub fn destinations(
    plane: &Path,
    ws: &str,
    slug: &str,
    say: &mut dyn FnMut(Say),
) -> Result<Vec<Destination>, u8> {
    destinations_with(plane, ws, slug, &|https: &str| https.to_string(), say)
}

/// [`destinations`], under `route` ([`push_with`]'s).
pub fn destinations_with(
    plane: &Path,
    ws: &str,
    slug: &str,
    route: &dyn Fn(&str) -> String,
    say: &mut dyn FnMut(Say),
) -> Result<Vec<Destination>, u8> {
    let (_, plans, _) = planned(plane, ws, slug, route, say)?;
    Ok(plans.iter().map(Destination::from).collect())
}

/// Push `slug` as the operator confirmed it, and only so: when what [`destinations`] finds now
/// is not `confirmed`, the push is refused, naming what changed, before anything is pushed.
pub fn push_confirmed(
    plane: &Path,
    ws: &str,
    slug: &str,
    confirmed: &[Destination],
    say: &mut dyn FnMut(Say),
) -> u8 {
    push_confirmed_with(
        plane,
        ws,
        slug,
        confirmed,
        &|repo: &Repo| repo.backend(),
        &|https: &str| https.to_string(),
        say,
    )
}

/// [`push_confirmed`], through `backend_of` and under `route` ([`push_with`]'s).
pub fn push_confirmed_with(
    plane: &Path,
    ws: &str,
    slug: &str,
    confirmed: &[Destination],
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    route: &dyn Fn(&str) -> String,
    say: &mut dyn FnMut(Say),
) -> u8 {
    pushing(plane, ws, slug, backend_of, route, Some(confirmed), say)
}

/// The first twelve characters of a commit, contained, as `land` shows one.
pub fn short(sha: &str) -> String {
    shown::line(sha).chars().take(12).collect()
}

/// What differs between what the operator `confirmed` and what charter would push `now`, one
/// phrase per member, or nothing when they are the same.
fn changed_since(confirmed: &[Destination], now: &[Destination]) -> Vec<String> {
    let mut out = Vec::new();
    for d in now {
        match confirmed.iter().find(|c| c.repo == d.repo) {
            None => out.push(format!("{} is pushed too", named(&d.repo))),
            Some(c) if c.to != d.to => out.push(format!(
                "{} now goes to {}",
                named(&d.repo),
                shown::line(&d.to)
            )),
            Some(c) if c.branch != d.branch => out.push(format!(
                "{} now pushes branch {}",
                named(&d.repo),
                named(&d.branch)
            )),
            Some(c) if c.head != d.head => out.push(format!(
                "{}'s branch {} is now at {}",
                named(&d.repo),
                named(&d.branch),
                short(&d.head)
            )),
            Some(c) if c.kind != d.kind => {
                out.push(format!("{} is now on {}", named(&d.repo), d.kind.display()))
            }
            Some(c) if c.base != d.base => out.push(format!(
                "{}'s request now goes into {}",
                named(&d.repo),
                d.base
                    .as_deref()
                    .map_or_else(|| "a branch charter cannot tell".into(), shown::line)
            )),
            Some(_) => {}
        }
    }
    for c in confirmed {
        if !now.iter().any(|d| d.repo == c.repo) {
            out.push(format!("{} would not be pushed", named(&c.repo)));
        }
    }
    out
}

/// The change's record and each member's plan, with how many members have none (each said);
/// `Err` when the whole command is refused.
fn planned(
    plane: &Path,
    ws: &str,
    slug: &str,
    route: &dyn Fn(&str) -> String,
    say: &mut dyn FnMut(Say),
) -> Result<(Record, Vec<Plan>, usize), u8> {
    if !workspace_ok(plane, ws, say) {
        return Err(1);
    }
    let record = load(plane, ws, slug, say)?;
    if record.members.is_empty() {
        say(Say::Fail(format!("change {} has no members.", named(slug))));
        say(Say::Info(format!(
            "Add one: charter change add {} <repo>",
            named(slug)
        )));
        return Err(REFUSED);
    }
    let mut failed = 0usize;
    let mut plans = Vec::new();
    for m in &record.members {
        match plan(plane, ws, slug, &m.repo, &m.branch, route, say) {
            Ok(p) => plans.push(p),
            Err(()) => failed += 1,
        }
    }
    Ok((record, plans, failed))
}

/// [`push`], asking the backend `backend_of` builds for each member's repo, and handing git
/// the URL `route` makes of each printed destination. [`push`]'s route is the identity, so
/// the URL printed is the URL git is handed. A test routes it to a local bare remote, since a
/// clone's own config may not rewrite where a push goes.
pub fn push_with(
    plane: &Path,
    ws: &str,
    slug: &str,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    route: &dyn Fn(&str) -> String,
    say: &mut dyn FnMut(Say),
) -> u8 {
    pushing(plane, ws, slug, backend_of, route, None, say)
}

/// The push, of what the operator `confirmed` when there is a confirmation.
fn pushing(
    plane: &Path,
    ws: &str,
    slug: &str,
    backend_of: &dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    route: &dyn Fn(&str) -> String,
    confirmed: Option<&[Destination]>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    let (record, plans, mut failed) = match planned(plane, ws, slug, route, say) {
        Ok(planned) => planned,
        Err(code) => return code,
    };
    if let Some(confirmed) = confirmed {
        let now: Vec<Destination> = plans.iter().map(Destination::from).collect();
        let changed = changed_since(confirmed, &now);
        if !changed.is_empty() {
            say(Say::Fail(format!(
                "what charter would push is not what you confirmed: {}. Nothing was pushed. \
                 Look at it again and confirm what it says now.",
                changed.join("; ")
            )));
            return REFUSED;
        }
        if now.is_empty() {
            say(Say::Fail(
                "no member of this change can be pushed, so nothing was pushed.".into(),
            ));
            return 1;
        }
    }
    let names: Vec<String> = plans.iter().map(|p| shown::line(&p.repo)).collect();
    let w = tui::column("", names.iter().map(String::as_str), 0, None);
    if !plans.is_empty() {
        say(Say::Info(format!(
            "Pushing {} member(s) of {}. Nothing is committed and nothing is forced:",
            plans.len(),
            named(slug)
        )));
    }
    for (p, name) in plans.iter().zip(&names) {
        let into = p.base.as_deref().map_or_else(String::new, |b| {
            format!(", its request into {}", shown::line(b))
        });
        say(Say::Out(format!(
            "  {}  {} → {}{into}",
            tui::pad(name, w, Align::Left),
            shown::line(&p.branch),
            shown::line(&p.https)
        )));
    }

    let caller = Caller::command();
    // Every member charter reached: where its request is, the request, and its forge.
    let mut reached: BTreeMap<String, (Reached, Pr, Box<dyn ForgeBackend>)> = BTreeMap::new();
    for (p, name) in plans.iter().zip(&names) {
        let named_row = tui::pad(name, w, Align::Left);
        if !pushed(p, say) {
            failed += 1;
            continue;
        }
        let backend = backend_of(&p.on);
        let found = match backend.by_head(&caller, &p.on.path, &p.branch) {
            Ok(found) => found,
            Err(why) => {
                say(Say::Fail(format!(
                    "{}: pushed, but charter could not ask for its request: {}",
                    named(&p.repo),
                    shown::line(&why.to_string())
                )));
                failed += 1;
                continue;
            }
        };
        let sigil = p.on.forge.kind.change_sigil();
        let (pr, verb, standing) = match found {
            Some(req) => (
                Pr {
                    number: req.number,
                    url: req.url,
                },
                "pushed",
                (req.state != State::Open)
                    .then(|| format!(" ({})", super::observe::standing(&req.state))),
            ),
            None => {
                let Some(base) = p.base.as_deref() else {
                    say(Say::Fail(format!(
                        "{}: pushed, but charter does not know its default branch, so it \
                         cannot say where a request goes. Run: git -C {} remote set-head \
                         origin --auto",
                        named(&p.repo),
                        shown::line(&p.clone.display().to_string())
                    )));
                    failed += 1;
                    continue;
                };
                let title = format!("{}: {}", cell(&record.change), cell(&p.repo));
                match backend.open_or_update(
                    &caller,
                    &p.on.path,
                    &p.branch,
                    base,
                    &title,
                    &new_body(&record, &p.on.forge.host),
                ) {
                    Ok(opened) => (opened.pr, "opened", None),
                    Err(why) => {
                        say(Say::Fail(format!(
                            "{}: pushed, but its request into {} was not opened: {}",
                            named(&p.repo),
                            shown::line(base),
                            shown::line(&why.to_string())
                        )));
                        failed += 1;
                        continue;
                    }
                }
            }
        };
        say(Say::Done(format!(
            "{named_row}  {verb} → {sigil}{}{}  {}",
            pr.number,
            standing.unwrap_or_default(),
            shown::line(&pr.url)
        )));
        let at = Reached {
            number: pr.number,
            path: p.on.path.clone(),
            sigil,
            host: p.on.forge.host.clone(),
            url: pr.url.clone(),
        };
        reached.insert(p.repo.clone(), (at, pr, backend));
    }

    let reached_at = |repo: &str| reached.get(repo).map(|(at, _, _)| at);
    let (mut written, mut current) = (0usize, 0usize);
    for (repo, (at, pr, backend)) in &reached {
        let (path, sigil) = (&at.path, at.sigil);
        // Per request: a member on another host is named by its URL there (`Reached::named_on`).
        let block = block(&record, &reached_at, &at.host);
        let body = match backend.body(&caller, path, pr) {
            Ok(body) => body,
            Err(why) => {
                say(Say::Fail(format!(
                    "{}: {sigil}{}'s description could not be read: {}",
                    named(repo),
                    pr.number,
                    shown::line(&why.to_string())
                )));
                failed += 1;
                continue;
            }
        };
        let Some(spliced) = splice(&body, &block) else {
            say(Say::Fail(format!(
                "{}: {sigil}{}'s description has no single charter block outside a code \
                 fence, so charter left it as it is. Charter writes only between its own \
                 markers.",
                named(repo),
                pr.number
            )));
            failed += 1;
            continue;
        };
        if spliced == body {
            current += 1;
            continue;
        }
        match backend.set_body(&caller, path, pr, &spliced) {
            Ok(()) => written += 1,
            Err(why) => {
                say(Say::Fail(format!(
                    "{}: {sigil}{}'s cross-link block was not written: {}",
                    named(repo),
                    pr.number,
                    shown::line(&why.to_string())
                )));
                failed += 1;
            }
        }
    }
    if written > 0 {
        say(Say::Done(format!(
            "cross-link block written into {written} request description(s)"
        )));
    }
    if current > 0 {
        say(Say::Info(format!(
            "cross-link block already current in {current} request description(s)"
        )));
    }
    // 1 when any member was not pushed, opened or written, as the Python charter answered:
    // a script reads "not everything it was asked to do happened" from one code. Exit 2 stays
    // the refusal of the whole command (no such change, no members).
    u8::from(failed > 0)
}

/// Push one member's branch to its own name on its origin, and nothing else. `false` with
/// git's own words said when the remote refused it.
fn pushed(p: &Plan, say: &mut dyn FnMut(Say)) -> bool {
    let refspec = format!("refs/heads/{0}:refs/heads/{0}", p.branch);
    let helper = forge::helper_for(&p.on.forge);
    let mut args: Vec<&str> = PUSH_ONLY_THE_BRANCH.to_vec();
    args.extend(["push", p.to.as_str(), refspec.as_str()]);
    let run = match git::run_network(&p.clone, Some(&helper), &args) {
        Ok(run) => run,
        Err(unavailable) => {
            say(Say::Fail(format!(
                "{}: not pushed: {unavailable}",
                named(&p.repo)
            )));
            return false;
        }
    };
    if !run.ok() {
        let all = if run.err.trim().is_empty() {
            &run.out
        } else {
            &run.err
        };
        let last = all
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("hint:"))
            .unwrap_or("git said nothing");
        say(Say::Fail(format!(
            "{}: not pushed: {}",
            named(&p.repo),
            shown::line(last)
        )));
        return false;
    }
    // Kept in step, so the repo's own view of its remote matches what the remote now holds.
    let tracking = format!("refs/remotes/origin/{}", p.branch);
    let moved = git::run(
        &p.clone,
        &["update-ref", &tracking, &format!("refs/heads/{}", p.branch)],
        git::READ,
    );
    if !moved.is_ok_and(|run| run.ok()) {
        say(Say::Warn(format!(
            "{}: pushed, but {} was not moved to it. `git fetch` in the repo corrects it.",
            named(&p.repo),
            shown::line(&tracking)
        )));
    }
    true
}

#[cfg(test)]
mod tests;
