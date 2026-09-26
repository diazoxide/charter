//! The plane's `pr` and `pr-merge` modes (charter-app#298, ADR 0051): one rolling save branch
//! per machine, and one pull request from it into the target branch.
//!
//! # What a save does
//!
//! The commit is made on the target branch, as every mode makes it. Then:
//!
//! 1. **The last PR is settled** ([`settle`]): if it has merged since, the local target branch
//!    is moved onto the remote's, and if it was closed without merging, the plane is blocked.
//! 2. **HEAD is pushed to the save branch** (`[plane] save_branch`, default
//!    `charter/save/<host>-<this clone>`), with `--force-with-lease` against the commit this
//!    clone last pushed there, which `save-branch.json` keeps. A force is needed because a
//!    squash or rebase merge leaves the save branch's old tip out of the target's history, and
//!    the lease is what keeps it from overwriting anything somebody else pushed there — a
//!    second machine with the same name included. With nothing kept, the branch is leased as
//!    absent, and one that exists is pushed over only when its tip is already in HEAD's
//!    history. **Only the save branch is ever forced**: the target branch is never pushed to.
//! 3. **The PR is opened or updated** into the target branch, named explicitly
//!    ([`crate::forge::pr::open_or_update`]), and in `pr-merge` set to merge at the commit
//!    just pushed ([`crate::forge::pr::request_auto_merge`]).
//! 4. **The outcome is recorded** in `plane-push.json` as `pr-open`, with the PR's link and
//!    number, so [`super::standing`] can say *pushed, PR open* without asking the network.
//!    The PR itself — number, link, and the commit it was last pushed at — is kept apart, in
//!    `save-branch.json` ([`Kept`]), which no failure overwrites: a merged PR is settled even
//!    when the saves after it failed, and never proposed a second time.
//!
//! # After the PR merges
//!
//! Merged is the forge's word, asked by [`settle`] on every save and every fetch while a PR is
//! known. A save whose question the forge cannot answer stops before it pushes. The target
//! branch is fetched, and it holds this machine's work when the pushed commit is in its
//! history (a merge commit), or when the commit the forge names as the merge — a squash, or a
//! rebase's last commit — is in its history and matches the pushed tree on every path the PR
//! touched. It is that commit and not the target's tip that is compared, so an edit somebody
//! made to one of those paths after the merge does not count against it. Then the local branch
//! is moved onto the remote's: `reset --keep` when nothing was committed since the push, which
//! keeps uncommitted work and refuses rather than overwrite it — the next look tries again —
//! or a rebase of the newer commits onto it. A merge commit that does not hold the work blocks
//! the plane, and nothing is moved.

use super::*;
use crate::forge::pr::{self, AutoMerge, Pr, Repo, State};
use crate::planesave::{Mode, Plane};

/// Whether a push record is one a PR mode wrote: its present tense is asked of the target
/// branch and HEAD ([`still_holds`]) rather than of the branch's upstream.
pub(super) fn is_pr_record(rec: &serde_json::Value) -> bool {
    matches!(
        rec.get("outcome").and_then(serde_json::Value::as_str),
        Some("pr-open" | "blocked")
    )
}

/// Whether a PR mode's record is still true: the commit it is about has not reached the
/// remote's target branch, and the plane is still on it or on a commit made after it.
///
/// The second half is what lets a person clear a block by hand: a plane moved off the
/// recorded commit — reset onto the remote's branch, say — is no longer the plane the record
/// is about. So is one whose save_branch was changed away from the branch the record names.
pub(super) fn still_holds(root: &Path, rec: &serde_json::Value, head: &str) -> bool {
    let Some(branch) = rec
        .get("branch")
        .and_then(serde_json::Value::as_str)
        .filter(|b| crate::planesave::branch_ok(b))
    else {
        return false;
    };
    // A block about a save branch the plane no longer pushes to — its save_branch changed, by
    // hand, to get past another clone's commits — is not about this plane any more.
    let landed = rec.get("landed").and_then(serde_json::Value::as_str);
    let plane = crate::planesave::Settings::read(root).plane;
    if landed.is_some_and(|landed| landed != plane.save_branch_or_default(root)) {
        return false;
    }
    is_object_name(head)
        && !is_ancestor(root, head, &format!("refs/remotes/origin/{branch}"))
        && is_ancestor(root, head, "HEAD")
}

/// `git merge-base --is-ancestor`: only a clean yes counts.
fn is_ancestor(root: &Path, commit: &str, of: &str) -> bool {
    git::run(
        root,
        &["merge-base", "--is-ancestor", commit, of],
        git::READ,
    )
    .is_ok_and(|r| r.ok())
}

/// The branch HEAD is on, or an empty string.
fn here(root: &Path) -> String {
    git::run(root, &["rev-parse", "--abbrev-ref", "HEAD"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default()
}

/// The object name `rev` resolves to, or `None`.
fn resolve(root: &Path, rev: &str) -> Option<String> {
    git::run(root, &["rev-parse", "--verify", "-q", rev], git::READ)
        .ok()
        .filter(git::Run::ok)
        .map(|r| r.line().trim().to_string())
        .filter(|sha| !sha.is_empty())
}

/// What a PR mode needs before it can push anything: the forge repo the plane's origin names,
/// the save branch, and the target branch. `Err` is a config error, in words, and the plane is
/// blocked on it (ADR 0051).
pub(super) fn config(root: &Path, plane: &Plane) -> Result<(Repo, String, String), String> {
    let repo = Repo::of_plane(root)?;
    let save = plane.save_branch_or_default(root);
    let target = plane.branch.value.clone().unwrap_or_else(|| here(root));
    if save == target {
        return Err(format!(
            "[plane] save_branch is {save}, the target branch itself, and a PR mode never \
             pushes to the branch its pull request goes into. Name another save_branch, or \
             remove it for charter/save/<host>-<clone>"
        ));
    }
    Ok((repo, save, target))
}

/// What this clone keeps about its save branch: the commit it last pushed there, which the
/// next push is leased against, and the PR it last opened from there. `save-branch.json`, in
/// the plane's state directory, beside `plane-push.json` — and apart from it, because that
/// record is rewritten by every push, failures included.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Kept {
    /// The save branch the rest is about.
    pub branch: Option<String>,
    /// The commit this clone last pushed to it.
    pub pushed: Option<String>,
    pub pr: Option<KeptPr>,
}

/// The PR a save last opened or updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct KeptPr {
    pub number: u64,
    pub url: String,
    /// The commit it was last pushed at.
    pub head: String,
    /// The branch it goes into.
    pub target: String,
}

/// Where [`Kept`] lives.
pub fn kept_path(root: &Path) -> PathBuf {
    crate::plane::state_dir(root).join("save-branch.json")
}

impl Kept {
    /// What is kept, with anything not in the shape charter writes left out: each value ends up
    /// in git's argv or a forge's API path.
    pub(super) fn read(root: &Path) -> Kept {
        let path = kept_path(root);
        let Some(doc) = read_json(&path) else {
            return Kept::default();
        };
        let text = |v: &serde_json::Value, key: &str| {
            v.get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let pr = doc.get("pr").and_then(|pr| {
            Some(KeptPr {
                number: pr.get("number").and_then(serde_json::Value::as_u64)?,
                url: text(pr, "url").unwrap_or_default(),
                head: text(pr, "head").filter(|h| is_object_name(h))?,
                target: text(pr, "target").filter(|b| crate::planesave::branch_ok(b))?,
            })
        });
        Kept {
            branch: text(&doc, "branch").filter(|b| crate::planesave::branch_ok(b)),
            pushed: text(&doc, "pushed").filter(|h| is_object_name(h)),
            pr,
        }
    }

    /// Write it down. Never fails loudly, as the push record does not.
    pub(super) fn write(&self, root: &Path) {
        let path = kept_path(root);
        let doc = serde_json::json!({
            "branch": self.branch,
            "pushed": self.pushed,
            "pr": self.pr.as_ref().map(|pr| serde_json::json!({
                "number": pr.number,
                "url": pr.url,
                "head": pr.head,
                "target": pr.target,
            })),
        });
        if let Some(dir) = path.parent()
            && crate::profiletrust::private_dir(dir).is_ok()
        {
            let _ = crate::rewrite::replace(
                dir,
                &path,
                crate::pyjson::dumps_indent2(&doc).as_bytes(),
                crate::rewrite::Mode::Private,
            );
        }
    }
}

/// The link of the PR this clone last opened, while it is known.
pub(super) fn known_pr_url(root: &Path) -> Option<String> {
    Kept::read(root)
        .pr
        .map(|pr| pr.url)
        .filter(|u| !u.is_empty())
}

/// Whether a PR this clone opened is known and not yet settled.
pub(super) fn knows_a_pr(root: &Path) -> bool {
    Kept::read(root).pr.is_some()
}

/// Push the plane's HEAD to the save branch and open or update its pull request — the PR
/// modes' half of a save, once the commit is made.
pub(super) fn push(root: &Path, plane: &Plane, sign: bool, say: Sink) -> PushResult {
    let mode = plane.mode.value.unwrap_or(Mode::Pr);
    let refused = |why: String, say: Sink| {
        say(Say::Warn(format!("Not pushed: {why}.")));
        // Unrecorded: a config error is re-derived by `standing` from the settings, and
        // recording it could only outlive the fix.
        PushResult {
            detail: why,
            ..PushResult::of(Outcome::Blocked, "")
        }
    };
    let Some(https) = origin_https(root) else {
        // Not a block (charter-app#295): a PR mode on a remote no forge adapter serves commits
        // and goes no further, and `standing` says so as a notice.
        say(Say::Warn(
            "Not pushed: this plane's origin is not a GitHub or GitLab forge charter knows, so \
             there is nowhere to open a pull request — committed only."
                .into(),
        ));
        return PushResult::of(Outcome::Unreachable, "");
    };
    let (repo, save, target) = match config(root, plane) {
        Ok(found) => found,
        Err(why) => return refused(why, say),
    };
    let here = here(root);
    if here != target {
        return refused(
            format!("this plane is on {here}, and [plane] branch is {target}"),
            say,
        );
    }
    let helper = forge::helper_for(&repo.forge);
    let cli = repo.forge.kind.cli();

    let head = resolve(root, "HEAD").unwrap_or_default();
    let settled = settle(root, plane, &target, sign, Some((&https, &helper)), say);
    let head = match settled {
        Settled::Blocked(res) => return res,
        Settled::Unknown(why) => {
            say(Say::Warn(format!(
                "Committed, but not pushed: {why}. Nothing is pushed until charter knows \
                 whether the last pull request merged; the next save asks again."
            )));
            return record_push(
                root,
                PushResult {
                    detail: why,
                    ..PushResult::of(Outcome::Failed, &target)
                },
                &head,
            );
        }
        Settled::Waiting(why) => {
            say(Say::Info(format!("Committed, not pushed yet: {why}")));
            // Unrecorded, as nothing went wrong: the next look tries again.
            return PushResult {
                detail: why,
                ..PushResult::of(Outcome::Unreachable, &target)
            };
        }
        // A move rewrote HEAD.
        Settled::Moved => resolve(root, "HEAD").unwrap_or_default(),
        Settled::Nothing | Settled::Open => head,
    };
    if count(root, &format!("refs/remotes/origin/{target}..HEAD")) == Some(0) {
        say(Say::Done(format!(
            "Nothing left to push — {target} on the remote already has all of it."
        )));
        let _ = std::fs::remove_file(push_record_path(root));
        return PushResult::of(Outcome::Pushed, &target);
    }
    let mut kept = Kept::read(root);
    if kept.branch.as_deref() != Some(save.as_str()) {
        // Kept about another save branch: nothing of it applies to this one.
        kept = Kept {
            branch: Some(save.clone()),
            ..Kept::default()
        };
    }
    if settled == Settled::Open
        && let Some(open) = kept.pr.as_ref().filter(|pr| pr.head == head)
        && kept.pushed.as_deref() == Some(head.as_str())
    {
        say(Say::Done(format!(
            "Pull request #{} already carries all of it: {}",
            open.number, open.url
        )));
        return record_push(
            root,
            PushResult {
                landed: Some(save),
                url: Some(open.url.clone()),
                number: Some(open.number),
                ..PushResult::of(Outcome::PrOpen, &target)
            },
            &head,
        );
    }
    if let Err(res) = push_save_branch(root, &https, &helper, &save, &target, &kept, say) {
        return record_push(root, *res, &head);
    }
    kept.pushed = Some(head.clone());
    kept.write(root);
    say(Say::Done(format!(
        "Pushed {save} via {cli} (HTTPS token — no SSH, no 1Password)."
    )));

    let (title, body) = describe(root, &target, &save);
    let pr::Opened { pr: opened, ours } =
        match pr::open_or_update(&repo, &save, &target, &title, &body) {
            Ok(opened) => opened,
            Err(why) => {
                say(Say::Warn(format!(
                    "Pushed {save}, but the pull request into {target} could not be opened: {why}"
                )));
                return record_push(
                    root,
                    PushResult {
                        landed: Some(save),
                        detail: why,
                        ..PushResult::of(Outcome::Failed, &target)
                    },
                    &head,
                );
            }
        };
    kept.pr = Some(KeptPr {
        number: opened.number,
        url: opened.url.clone(),
        head: head.clone(),
        target: target.clone(),
    });
    kept.write(root);
    say(Say::Done(format!(
        "Pull request into {target}: {}",
        opened.url
    )));
    let mut detail = String::new();
    if !ours {
        // A save branch somebody set by hand can carry a PR a person opened: it was left exactly
        // as it was, and nothing more is asked of the forge about it (#299's rule).
        detail = "the open pull request from the save branch is not charter's, so it was left \
                  as it was"
            .into();
        say(Say::Info(format!("  {detail}.")));
    } else if mode == Mode::PrMerge {
        match pr::request_auto_merge(&repo, &opened, &head) {
            Ok(AutoMerge::Queued) => say(Say::Info(
                "  It merges by itself once its checks pass.".into(),
            )),
            Ok(AutoMerge::NotQueued(why)) => {
                say(Say::Info(format!(
                    "  Auto-merge was not queued — {why}. The pull request stays open for a \
                     person to merge."
                )));
                detail = why;
            }
            Err(why) => {
                say(Say::Warn(format!(
                    "  Auto-merge could not be requested: {why}. The pull request stays open."
                )));
                detail = why;
            }
        }
    }
    record_push(
        root,
        PushResult {
            landed: Some(save),
            url: Some(opened.url),
            number: Some(opened.number),
            detail,
            ..PushResult::of(Outcome::PrOpen, &target)
        },
        &head,
    )
}

/// Push HEAD to `save`, leased against the commit this clone last pushed there (`kept`).
///
/// - **Nothing kept** — the first push from this clone — leases the branch as absent. A
///   branch that is there anyway is pushed over only when its tip is already in HEAD's
///   history, so nothing is lost; otherwise it is somebody else's, and the plane is blocked.
/// - **A branch that is gone** — deleted after its PR merged — is leased again as absent.
fn push_save_branch(
    root: &Path,
    https: &str,
    helper: &str,
    save: &str,
    target: &str,
    kept: &Kept,
    say: Sink,
) -> Result<(), Box<PushResult>> {
    let failed = |detail: String, say: Sink| {
        say(Say::Warn(format!("Committed, but pushing {save} failed:")));
        for line in detail.lines() {
            say(Say::Warn(format!("  {line}")));
        }
        Box::new(PushResult {
            landed: Some(save.to_string()),
            detail,
            ..PushResult::of(Outcome::Failed, target)
        })
    };
    let push = |expected: &str| {
        git::run_network(
            root,
            Some(helper),
            &[
                "push",
                &format!("--force-with-lease=refs/heads/{save}:{expected}"),
                https,
                &format!("HEAD:refs/heads/{save}"),
            ],
        )
    };
    let expected = kept.pushed.clone().unwrap_or_default();
    let mut pushed = push(&expected).map_err(|unavailable| failed(unavailable.to_string(), say))?;
    if !pushed.ok() && pushed.err.contains("stale info") {
        // What the remote has instead: nothing, or a tip.
        let listed = git::run_network(
            root,
            Some(helper),
            &["ls-remote", "--heads", https, &format!("refs/heads/{save}")],
        )
        .map_err(|unavailable| failed(unavailable.to_string(), say))?;
        if !listed.ok() {
            return Err(failed(tail(&listed), say));
        }
        let tip = listed
            .out
            .split_whitespace()
            .next()
            .filter(|sha| is_object_name(sha))
            .map(str::to_string);
        let again = match &tip {
            // Gone from the remote: nobody's commits are on it.
            None if !expected.is_empty() => Some(String::new()),
            // Already in HEAD's history — this clone's own earlier push, unrecorded — so
            // nothing of anybody's is lost. The tip has to be here to be asked about.
            Some(tip) if tip_is_in_head(root, https, helper, save, tip) => Some(tip.clone()),
            _ => None,
        };
        if let Some(again) = again {
            pushed = push(&again).map_err(|unavailable| failed(unavailable.to_string(), say))?;
        }
    }
    if pushed.ok() {
        return Ok(());
    }
    if pushed.err.contains("stale info") {
        let why = format!(
            "{save} on the remote has commits this clone did not push there — another clone, or \
             another machine with the same name — so charter will not overwrite them. Name a \
             save_branch of this clone's own under [plane] in charter.local.toml, or delete \
             {save} on the remote if nothing on it is wanted, and save again"
        );
        say(Say::Fail(format!("Not pushed: {why}.")));
        return Err(Box::new(PushResult {
            landed: Some(save.to_string()),
            detail: why,
            ..PushResult::of(Outcome::Blocked, target)
        }));
    }
    Err(failed(tail(&pushed), say))
}

/// Whether the remote save branch's `tip` is in HEAD's history, fetching it first if this
/// clone does not have it.
fn tip_is_in_head(root: &Path, https: &str, helper: &str, save: &str, tip: &str) -> bool {
    if resolve(root, &format!("{tip}^{{commit}}")).is_none() {
        let _ = git::run_network(
            root,
            Some(helper),
            &[
                "fetch",
                "--no-recurse-submodules",
                "--no-write-fetch-head",
                https,
                &format!("+refs/heads/{save}:refs/remotes/origin/{save}"),
            ],
        );
    }
    is_ancestor(root, tip, "HEAD")
}

/// The pull request's title and body: what the save branch carries that the target lacks, in
/// the commits' own words.
fn describe(root: &Path, target: &str, save: &str) -> (String, String) {
    const LISTED: usize = 50;
    let range = format!("refs/remotes/origin/{target}..HEAD");
    let subjects: Vec<String> = git::run(
        root,
        &[
            "log",
            "-n",
            &LISTED.to_string(),
            "--reverse",
            "--format=%s",
            &range,
        ],
        git::READ,
    )
    .map(|r| r.out.lines().map(str::to_string).collect())
    .unwrap_or_default();
    let total = count(root, &range).map_or(subjects.len(), |n| n as usize);
    let host = crate::dispatch::host();
    let title = match subjects.as_slice() {
        [one] if total == 1 => one.clone(),
        _ => format!("{total} saves from {host}"),
    };
    let mut body = format!(
        "Saved by charter on {host}. Every save from this machine pushes to {save} and updates \
         this pull request.\n"
    );
    if total > subjects.len() {
        body.push_str(&format!("\n- … and {} earlier", total - subjects.len()));
    }
    for subject in &subjects {
        body.push_str(&format!("\n- {subject}"));
    }
    (title, body)
}

/// What [`settle`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Settled {
    /// No PR is known.
    Nothing,
    /// The PR is open.
    Open,
    /// The PR merged, and the plane was moved onto the remote's target branch.
    Moved,
    /// The PR merged, and the plane cannot be moved yet: a file is in the way, or newer
    /// commits wait on a clean tree. Not blocked: the next look tries again.
    Waiting(String),
    /// The forge could not say where the PR stands. A save stops before it pushes.
    Unknown(String),
    /// The PR was closed without merging, or merged as a commit that does not hold what was
    /// pushed. Recorded.
    Blocked(PushResult),
}

/// Settle the pull request this clone last opened, if it has merged or closed since.
///
/// `sign` is whether commits a rebase replays are signed, as the save's own commit is.
/// `fetch` is the origin URL and credential helper to fetch the target branch with, for a
/// caller that has not just fetched it; `None` when it has ([`super::fetch`]).
pub(super) fn settle(
    root: &Path,
    plane: &Plane,
    target: &str,
    sign: bool,
    fetch: Option<(&str, &str)>,
    say: Sink,
) -> Settled {
    let mut kept = Kept::read(root);
    let Some(known) = kept.pr.clone() else {
        return Settled::Nothing;
    };
    if known.target != target {
        return Settled::Nothing;
    }
    // A plane moved off the commit the PR carries — by a person clearing a block — is no
    // longer the plane that PR is about.
    if !is_ancestor(root, &known.head, "HEAD") {
        kept.pr = None;
        kept.write(root);
        return Settled::Nothing;
    }
    let Ok((repo, save, _)) = config(root, plane) else {
        return Settled::Nothing;
    };
    let number = known.number;
    let head = known.head.clone();
    let opened = Pr {
        number,
        url: known.url.clone(),
    };
    let blocked = |detail: String, forget: bool, say: Sink| {
        say(Say::Fail(format!("Blocked: {detail}")));
        if forget {
            let mut kept = Kept::read(root);
            kept.pr = None;
            kept.write(root);
        }
        Settled::Blocked(record_push(
            root,
            PushResult {
                landed: Some(save.clone()),
                url: Some(opened.url.clone()),
                number: Some(number),
                detail,
                ..PushResult::of(Outcome::Blocked, target)
            },
            &head,
        ))
    };
    let commit = match pr::state(&repo, &opened) {
        Ok(State::Open) => return Settled::Open,
        Ok(State::Closed) => {
            return blocked(
                format!(
                    "pull request #{number} was closed without merging. This machine's commits \
                     are still here: save again to open a new pull request"
                ),
                true,
                say,
            );
        }
        Ok(State::Merged { commit }) => commit,
        Err(why) => {
            return Settled::Unknown(format!(
                "charter could not ask where pull request #{number} stands: {why}"
            ));
        }
    };
    if let Some((https, helper)) = fetch {
        let fetched = git::run_network(
            root,
            Some(helper),
            &[
                "fetch",
                "--no-recurse-submodules",
                "--no-write-fetch-head",
                https,
                &format!("+refs/heads/{target}:refs/remotes/origin/{target}"),
            ],
        );
        if !fetched.is_ok_and(|run| run.ok()) {
            return Settled::Unknown(format!(
                "pull request #{number} merged, but {target} could not be fetched to move onto it"
            ));
        }
    }
    let theirs = format!("refs/remotes/origin/{target}");
    if !is_ancestor(root, &head, &theirs) {
        // The commit the merge made, which must be on the target; the target's tip only when
        // the forge named none.
        let merged_at = match commit.filter(|c| is_object_name(c)) {
            Some(commit) => {
                if !(resolve(root, &format!("{commit}^{{commit}}")).is_some()
                    && is_ancestor(root, &commit, &theirs))
                {
                    return blocked(
                        format!(
                            "pull request #{number} merged as {commit}, which is not on \
                             {target} on the remote. Nothing was moved"
                        ),
                        false,
                        say,
                    );
                }
                commit
            }
            None => theirs.clone(),
        };
        let differ = differing(root, &head, &merged_at);
        if !differ.is_empty() {
            let shown: Vec<&str> = differ.iter().take(5).map(String::as_str).collect();
            let more = differ.len().saturating_sub(shown.len());
            return blocked(
                format!(
                    "pull request #{number} merged, but what it merged does not match what this \
                     machine pushed in {}{}. Nothing was moved",
                    shown.join(", "),
                    if more > 0 {
                        format!(" and {more} more")
                    } else {
                        String::new()
                    }
                ),
                false,
                say,
            );
        }
    }
    let newer = count(root, &format!("{head}..HEAD")).unwrap_or(0);
    if newer == 0 {
        // `--keep` keeps uncommitted work, and refuses rather than overwrite a file with some:
        // then the plane waits, and the next look tries again.
        match git::run(root, &["reset", "-q", "--keep", &theirs], WRITE) {
            Ok(run) if run.ok() => {}
            Ok(run) => {
                return Settled::Waiting(format!(
                    "pull request #{number} merged, and {target} moves onto the remote's once \
                     nothing here is in the way: {}",
                    tail(&run)
                ));
            }
            Err(unavailable) => {
                return Settled::Waiting(format!(
                    "pull request #{number} merged, and moving {target} did not finish: \
                     {unavailable}"
                ));
            }
        }
    } else {
        if !changed_paths(root).is_empty() {
            // Newer commits are replayed onto the remote's branch, which needs a clean tree:
            // the next save commits first, and settles then.
            return Settled::Waiting(format!(
                "pull request #{number} merged, and {target} moves onto the remote's with the \
                 next save"
            ));
        }
        let run = git::run(
            root,
            &[
                "-c",
                gpgsign(sign),
                "rebase",
                "-q",
                "--onto",
                &theirs,
                &head,
            ],
            WRITE,
        );
        if !run.as_ref().is_ok_and(git::Run::ok) {
            let _ = git::run(root, &["rebase", "--abort"], WRITE);
            let said = match &run {
                Ok(run) => tail(run),
                Err(unavailable) => unavailable.to_string(),
            };
            return blocked(
                format!(
                    "pull request #{number} merged, but the {newer} commit(s) made since would \
                     not replay onto {target}: {said}"
                ),
                false,
                say,
            );
        }
    }
    kept.pr = None;
    kept.write(root);
    let _ = std::fs::remove_file(push_record_path(root));
    say(Say::Done(format!(
        "Pull request #{number} merged — {target} is now the remote's{}.",
        if newer > 0 {
            format!(", with {newer} newer commit(s) on top")
        } else {
            String::new()
        }
    )));
    Settled::Moved
}

/// The paths the PR touched — what `head` changed since it forked from `theirs` — whose
/// content in `theirs` is not what `head` has.
fn differing(root: &Path, head: &str, theirs: &str) -> Vec<String> {
    let Some(base) = git::run(root, &["merge-base", head, theirs], git::READ)
        .ok()
        .filter(git::Run::ok)
        .map(|r| r.line().trim().to_string())
    else {
        return vec!["(no common history)".to_string()];
    };
    let names = |args: &[&str]| -> Option<Vec<String>> {
        let mut argv = vec![
            "--literal-pathspecs",
            "diff",
            "--no-renames",
            "--name-only",
            "-z",
        ];
        argv.extend_from_slice(args);
        git::run(root, &argv, git::READ)
            .ok()
            .filter(git::Run::ok)
            .map(|r| {
                r.out
                    .split('\0')
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect()
            })
    };
    let Some(touched) = names(&[&base, head]) else {
        return vec!["(git could not compare them)".to_string()];
    };
    if touched.is_empty() {
        return Vec::new();
    }
    let mut args: Vec<&str> = vec![head, theirs, "--"];
    args.extend(touched.iter().map(String::as_str));
    names(&args).unwrap_or_else(|| vec!["(git could not compare them)".to_string()])
}
