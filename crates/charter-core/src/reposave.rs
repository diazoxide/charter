//! Saving a workspace repo by its `[repos.<name>]` mode (charter-app#299, ADR 0051).
//!
//! The plane's save ([`crate::planegit::save_as`]) is the model, and this reuses its parts
//! where they apply to a clone: the hardened git runner, the claim that keeps two saves of one
//! tree apart, the signing `-c`, the forge CLI's credential helper for the push, and the save
//! journal, where a repo's line carries the target `repo:<workspace>/<name>`.
//!
//! # What a repo save does
//!
//! It commits on the branch the clone is on — `git add -A`, with the message it was given or
//! one listing the files — and then goes as far as the mode:
//!
//! - `off`: nothing at all.
//! - `commit`: it stops at the commit.
//! - `push`: it pushes that branch.
//! - `pr` and `pr-merge`: it pushes that branch and opens or updates a pull request from it
//!   into `branch`, which defaults to the repo's `default_branch` in `inventory/repos.json`
//!   (else the clone's `origin/HEAD`). `pr-merge` then asks the forge to merge it once its
//!   checks pass, pinned to the pushed commit.
//!
//! **A PR mode never pushes to the default branch or to the PR's base.** A clone standing on
//! either gets a branch of its own first, `charter/<workspace>/<short-sha>`, created at HEAD
//! and pushed instead. The clone stays where it is: a save commits on the branch the repo is
//! on, and nothing moves an agent's checkout under it. The next save from there reuses that
//! branch while it is an ancestor of HEAD, so one pull request collects the saves.
//!
//! # What it does not do
//!
//! - **No rebase.** The plane's push rebases onto a remote that moved; a repo's push does not.
//!   A clone's history is its developers', and nothing rewrites anyone's history (ADR 0051): a
//!   push the remote refuses leaves the repo blocked, in git's words.
//! - **Not the plane's secret rule.** The plane's guard reads memory and ref files with a
//!   credential-assignment rule that refuses ordinary code (`password: String`). A repo save
//!   refuses by a narrower rule instead, [`secret_files`]: a file named like a credential, or a
//!   private key or a forge token in anything staged (ADR 0051).
//! - **Nobody else's pull request.** An open PR from the same branch that charter did not open
//!   — its head is not under `charter/` and its body has no [`pr::MARKER`] — keeps its title,
//!   its description and its merging; the save names it and leaves it.
//!
//! # A session mid-turn
//!
//! A repo is where a workspace's chats work, so a save waits while any of them is mid-turn
//! (ADR 0051). The caller says which are, by hook state ([`Request::mid_turn`]). A save the
//! operator asked for is refused with a sentence naming them — it does not queue itself, so
//! nothing runs later that nobody is watching — and an auto-save skips that cycle.

use std::path::Path;

use crate::forge::{self, Forge, pr};
use crate::planegit::{self, Claim, Stage, Trigger};
use crate::planesave::{self, Mode};
use crate::repocmd::{Say, Sink};
use crate::repos::{self, Head};
use crate::shown;
use crate::worktree::git;

/// What a repo save was asked for.
pub struct Request<'a> {
    /// The plane: its `charter.toml` says the mode, and its journal records the save.
    pub plane: &'a Path,
    pub workspace: &'a str,
    /// The repo's name — its `[repos.<name>]` table and its clone's directory.
    pub name: &'a str,
    /// The clone, as [`repos::clones`] checked it.
    pub clone: &'a Path,
    /// The commit message, or `None` for one listing the files.
    pub message: Option<&'a str>,
    /// Commit only, whatever the mode: the commit a quit makes before its bounded push.
    pub no_push: bool,
    /// The chats that are mid-turn where they could be writing this clone, by the names the
    /// window shows. Any at all, and the save waits. **Asked, not handed over**: it is asked
    /// once before the save starts and again right before `git add -A`, because a turn can
    /// start while a save waits for its claim or its first git call.
    pub mid_turn: &'a dyn Fn() -> Vec<String>,
}

/// A [`Request::mid_turn`] that says no chat is working: nothing in this process can be.
pub fn nobody_working() -> Vec<String> {
    Vec::new()
}

/// The journal's `target` for a repo.
pub fn target(workspace: &str, name: &str) -> String {
    format!("repo:{workspace}/{name}")
}

/// The sentence a save held back by working sessions says.
pub fn waiting_for(workspace: &str, mid_turn: &[String]) -> String {
    let who = match mid_turn {
        [one] => format!("{one} is"),
        [first @ .., last] => format!("{} and {last} are", first.join(", ")),
        [] => "a session is".to_owned(),
    };
    format!(
        "Not saved: {who} mid-turn in {workspace}. A repo is saved between turns — save again \
         when the turn ends."
    )
}

/// The sentence a save refused for its clone's [`Claim`] says.
pub fn already_saving(workspace: &str, name: &str) -> String {
    format!("A save of {workspace}/{name} is already running — wait for it to finish.")
}

/// Save a repo, as `trigger` asked. Returns the exit status; every save that ran leaves one
/// line in the plane's save journal.
///
/// A save held back by a session mid-turn before it starts journals nothing: nothing was
/// attempted. The window's button hears why (exit 1); an auto-save hears nothing and tries next
/// cycle. A save at quit is journalled as skipped, since there is no next cycle to try in.
pub fn save_as(request: &Request, trigger: Trigger, say: Sink) -> u8 {
    let busy = (request.mid_turn)();
    if !busy.is_empty() && trigger != Trigger::Quit {
        if matches!(trigger, Trigger::Manual | Trigger::Cli) {
            say(Say::Fail(waiting_for(request.workspace, &busy)));
            return 1;
        }
        return 0;
    }
    let Some(claim) = Claim::of(request.clone) else {
        say(Say::Fail(already_saving(request.workspace, request.name)));
        return 1;
    };
    save_claimed(request, trigger, &claim, say)
}

/// [`save_as`], for a caller that already holds the clone's [`Claim`].
pub fn save_claimed(request: &Request, trigger: Trigger, _claim: &Claim, say: Sink) -> u8 {
    let started = std::time::Instant::now();
    let repo = planesave::Settings::read(request.plane).repo(request.name);
    let mode = if request.no_push && repo.mode.value != Mode::Off {
        Mode::Commit
    } else {
        repo.mode.value
    };
    let mut attempt = Attempt::default();
    let mut refused: Option<String> = None;
    let mut heard = |line: Say| {
        if let Say::Fail(text) = &line
            && refused.is_none()
        {
            refused = Some(text.clone());
        }
        say(line);
    };
    let say: Sink = &mut heard;
    let code = if mode == Mode::Off {
        say(Say::Info(format!(
            "[repos.{}] mode is off ({}), so charter commits nothing in {}/{}.",
            request.name,
            repo.mode.source.file().unwrap_or("default"),
            request.workspace,
            request.name
        )));
        attempt.outcome = "skipped";
        0
    } else {
        commit_push(request, trigger, &repo, mode, &mut attempt, say)
    };
    if attempt.detail.is_empty()
        && let Some(refused) = refused
    {
        attempt.detail = refused;
    }
    planegit::journal_append(
        request.plane,
        &serde_json::json!({
            "at": planegit::now(),
            "target": target(request.workspace, request.name),
            "trigger": trigger.word(),
            "mode": mode.as_str(),
            "files": attempt.files,
            "commit": attempt.commit,
            "branch": attempt.branch,
            "pr": attempt.pr,
            "ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "outcome": attempt.outcome,
            "detail": attempt.detail,
        }),
    );
    code
}

/// What one repo save came to — the journal's line.
#[derive(Debug)]
struct Attempt {
    outcome: &'static str,
    files: usize,
    /// HEAD once the save was done with it: the commit it made or pushed.
    commit: Option<String>,
    /// The remote branch it pushed.
    branch: Option<String>,
    pr: Option<String>,
    detail: String,
}

impl Default for Attempt {
    fn default() -> Self {
        Self {
            outcome: "failed",
            files: 0,
            commit: None,
            branch: None,
            pr: None,
            detail: String::new(),
        }
    }
}

/// `git rev-parse <what>`, or `None`.
fn rev(clone: &Path, what: &str) -> Option<String> {
    git::run(clone, &["rev-parse", what], git::READ)
        .ok()
        .filter(git::Run::ok)
        .map(|r| r.line().trim().to_string())
        .filter(|sha| !sha.is_empty())
}

fn commit_push(
    request: &Request,
    trigger: Trigger,
    repo: &planesave::Repo,
    mode: Mode,
    attempt: &mut Attempt,
    say: Sink,
) -> u8 {
    let clone = request.clone;
    let at = format!("{}/{}", request.workspace, request.name);
    let state = match repos::state_of(clone) {
        Ok(state) => state,
        Err(unreadable) => {
            say(Say::Fail(format!("Not saved: {unreadable}.")));
            attempt.outcome = "blocked";
            return 1;
        }
    };
    let branch = match &state.head {
        Head::Branch(branch) | Head::Unborn(branch) => branch.clone(),
        Head::Detached(sha) => {
            attempt.commit = rev(clone, "HEAD");
            let why = format!(
                "{at} is not on a branch (detached at {sha}), so a save has no branch to commit on"
            );
            say(Say::Fail(format!(
                "Not saved: {why}. Check out a branch, then save again."
            )));
            attempt.outcome = "blocked";
            attempt.detail = why;
            return 1;
        }
    };

    // Asked again at the last moment before anything is staged: a turn that started while
    // this save waited must not have its half-written files committed.
    let busy = (request.mid_turn)();
    if !busy.is_empty() {
        let why = waiting_for(request.workspace, &busy);
        attempt.outcome = "skipped";
        attempt.detail = why.clone();
        if matches!(trigger, Trigger::Manual | Trigger::Cli) {
            say(Say::Fail(why));
            return 1;
        }
        return 0;
    }

    // As the plane's save: the add's exit status decides, never the probe after it.
    // Untimed, as the plane's: an `add` killed at a deadline leaves `index.lock` behind.
    let added = git::run_untimed(clone, &["add", "-A"]);
    if !added.as_ref().is_ok_and(git::Run::ok) {
        let said = added.map_or_else(|e| e.to_string(), |run| planegit::tail(&run));
        say(Say::Fail(format!(
            "Not saved: git could not stage the changes in {at} — {said}"
        )));
        return 1;
    }
    let probe = git::run(clone, &["diff", "--cached", "--quiet"], git::READ)
        .ok()
        .and_then(|run| run.code);
    let committed = match probe {
        Some(0) => false,
        Some(1) => {
            let staged: Vec<String> =
                git::run(clone, &["diff", "--cached", "--name-only", "-z"], git::READ)
                    .map(|r| {
                        r.out
                            .split('\0')
                            .filter(|l| !l.trim().is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
            attempt.files = staged.len();
            // What a save must never carry to a remote: a secret-shaped file, or a private key
            // or a live token in anything it stages (ADR 0051, for a repo). Asked of the index,
            // as the plane's guard asks it, and refused before the commit exists; what was
            // staged stays staged, as the plane's refusal leaves it.
            let flagged = secret_files(clone, &staged);
            if !flagged.is_empty() {
                let named: Vec<String> = flagged
                    .iter()
                    .map(|(path, kind)| {
                        format!("{} ({kind})", shown::readable(path, shown::DISPLAY_LIMIT))
                    })
                    .collect();
                let why = format!(
                    "a secret-shaped file would be committed in {at}: {}",
                    named.join(", ")
                );
                say(Say::Fail(format!("Not saved: {why}.")));
                say(Say::Info(
                    "  Take it out of the tree or add it to .gitignore, then save again. Secrets \
                     belong in a vault."
                        .into(),
                ));
                attempt.outcome = "blocked";
                attempt.detail = why;
                attempt.commit = rev(clone, "HEAD");
                return 1;
            }
            let msg = match request.message {
                Some(text) if !text.trim().is_empty() => text.trim().to_string(),
                _ => summary(&staged),
            };
            let sign = repo.sign.value;
            let mut commit: Vec<&str> = vec!["-c", planegit::gpgsign(sign)];
            commit.extend(["commit", "-q", "-m", msg.as_str()]);
            let made = git::run(clone, &commit, planegit::WRITE);
            let still_staged =
                git::run(clone, &["diff", "--cached", "--quiet"], git::READ).is_ok_and(|r| !r.ok());
            if still_staged {
                let why = match (sign, made) {
                    (true, Ok(run)) => format!(
                        "charter could not sign the commit: {}",
                        planegit::signer_said(&run.err)
                    ),
                    (_, Ok(run)) => format!("git commit failed: {}", planegit::tail(&run)),
                    (_, Err(e)) => format!("git commit failed: {e}"),
                };
                say(Say::Fail(format!(
                    "Not saved: {why} — {} file(s) in {at} are staged but not committed.",
                    staged.len()
                )));
                attempt.detail = why;
                return 1;
            }
            let short = git::run(clone, &["rev-parse", "--short", "HEAD"], git::READ)
                .map(|r| r.line().trim().to_string())
                .unwrap_or_default();
            say(Say::Done(format!(
                "Committed {short} on {branch} in {at}: {msg}  ({} file(s))",
                staged.len()
            )));
            true
        }
        _ => {
            say(Say::Fail(format!(
                "Not saved: charter could not tell whether {at} has anything staged."
            )));
            return 1;
        }
    };
    attempt.commit = rev(clone, "HEAD");
    attempt.outcome = if committed { "committed" } else { "skipped" };

    if mode == Mode::Commit {
        if !committed {
            say(Say::Info(format!("Nothing to save in {at}.")));
        } else if !request.no_push {
            say(Say::Info(format!(
                "Not pushed: [repos.{}] mode is commit ({}).",
                request.name,
                repo.mode.source.file().unwrap_or("default")
            )));
        }
        return 0;
    }
    let Some(head_sha) = attempt.commit.clone() else {
        say(Say::Info(format!(
            "Nothing to save in {at}: {branch} has no commit yet."
        )));
        return 0;
    };
    let Some(https) = planegit::origin_https_of(request.plane, clone) else {
        let why = format!(
            "{at}'s origin is not on a GitHub or GitLab host this plane knows, so there is \
             nowhere to push"
        );
        say(Say::Warn(format!("{why}. Committed locally.")));
        if mode.opens_a_pr() {
            attempt.outcome = "blocked";
            attempt.detail = why;
            return 1;
        }
        return 0;
    };
    let forge = crate::gitpolicy::forge_for(clone, request.plane)
        .unwrap_or_else(|| Forge::default_of(forge::DEFAULT_KIND));
    let helper = forge::helper_for(&forge);

    // Where the commit goes: the branch it is on — or, in a PR mode on the branch a PR would
    // go into, a branch of charter's own.
    let (base, remote_branch) = if mode.opens_a_pr() {
        let default = default_branch(request.plane, request.name, clone);
        let Some(base) = repo.branch.value.clone().or_else(|| default.clone()) else {
            let why = format!(
                "charter does not know {}'s default branch, so it cannot say where a pull \
                 request goes. Set [repos.{}] branch",
                request.name, request.name
            );
            say(Say::Fail(format!("Not pushed: {why}.")));
            attempt.outcome = "blocked";
            attempt.detail = why;
            return 1;
        };
        let protected = branch == base || default.as_deref() == Some(branch.as_str());
        let remote = if protected {
            match save_branch(request, &branch, &head_sha) {
                Ok(name) => name,
                Err(why) => {
                    say(Say::Fail(format!("Not pushed: {why}.")));
                    attempt.outcome = "blocked";
                    attempt.detail = why;
                    return 1;
                }
            }
        } else {
            branch.clone()
        };
        (Some(base), remote)
    } else {
        (None, branch.clone())
    };

    let pushed = git::run_network(
        clone,
        Some(helper.as_str()),
        &["push", &https, &format!("HEAD:refs/heads/{remote_branch}")],
    );
    let run = match pushed {
        Ok(run) => run,
        Err(unavailable) => {
            say(Say::Warn(format!(
                "Committed, but the push failed: {unavailable}"
            )));
            attempt.detail = unavailable.to_string();
            attempt.outcome = "failed";
            return 1;
        }
    };
    if !run.ok() {
        // All of what git said is asked, and what is repeated leaves its hints out: the
        // rejection's own line comes before four lines of advice.
        let all = if run.err.is_empty() {
            &run.out
        } else {
            &run.err
        };
        let said = all
            .lines()
            .filter(|line| !line.trim_start().starts_with("hint:") && !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let why = if planegit::is_protected_rejection(all) {
            let fix = if mode.opens_a_pr() {
                // Already a PR mode, on a branch that is neither the default nor the base: the
                // branch itself is protected, so saves go through a PR into it instead.
                format!(
                    "Set [repos.{}] branch = \"{remote_branch}\" to save it through a pull \
                     request into it, or work on another branch",
                    request.name
                )
            } else {
                format!(
                    "Set [repos.{}] mode = \"pr\" to save it through a pull request",
                    request.name
                )
            };
            format!("{remote_branch} is protected and takes no direct push. {fix}")
        } else if ["fetch first", "non-fast-forward"]
            .iter()
            .any(|s| all.contains(s))
        {
            format!(
                "the remote's {remote_branch} has commits {at} does not. Bring them in, then \
                 save again"
            )
        } else {
            attempt.outcome = "failed";
            said.clone()
        };
        if attempt.outcome != "failed" {
            attempt.outcome = "blocked";
        }
        say(Say::Fail(format!("Committed, but not pushed: {why}")));
        for line in said.lines() {
            say(Say::Info(format!("  {line}")));
        }
        attempt.detail = why;
        return 1;
    }
    // Kept in step, so the next look counts against what the remote now has.
    let _ = git::run(
        clone,
        &[
            "update-ref",
            &format!("refs/remotes/origin/{remote_branch}"),
            "HEAD",
        ],
        git::READ,
    );
    attempt.branch = Some(remote_branch.clone());
    say(Say::Done(format!(
        "Pushed {remote_branch} of {at} via {}.",
        forge.kind.cli()
    )));
    let Some(base) = base else {
        attempt.outcome = "saved";
        return 0;
    };
    attempt.outcome = "pushed";

    let repo_on_forge = match pr::Repo::of_clone(request.plane, clone) {
        Ok(found) => found,
        Err(why) => {
            say(Say::Fail(format!("Pushed, but no pull request: {why}.")));
            attempt.outcome = "blocked";
            attempt.detail = why;
            return 1;
        }
    };
    let title = git::run(clone, &["log", "-1", "--format=%s"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    let body = format!(
        "Saved by charter from the {} workspace ([repos.{}] mode = {}).\n\n{}",
        request.workspace,
        request.name,
        mode.as_str(),
        pr::MARKER
    );
    let opened = match pr::open_or_update(&repo_on_forge, &remote_branch, &base, &title, &body) {
        Ok(opened) => opened,
        Err(why) => {
            say(Say::Fail(format!(
                "Pushed {remote_branch}, but the pull request into {base} was not opened: {why}"
            )));
            attempt.detail = why;
            return 1;
        }
    };
    attempt.outcome = "pr-open";
    attempt.pr = Some(opened.pr.url.clone());
    if !opened.ours {
        // Somebody's own PR from this branch: its title, its body and whether it merges are
        // theirs. The push has already reached it, which is what a save of their branch is.
        let said = format!(
            "{remote_branch} already has a pull request charter did not open, #{}: {} — \
             charter left its title, description and merging alone",
            opened.pr.number, opened.pr.url
        );
        say(Say::Info(said.clone()));
        attempt.detail = said;
        return 0;
    }
    let opened = opened.pr;
    say(Say::Done(format!(
        "Pull request #{} from {remote_branch} into {base}: {}",
        opened.number, opened.url
    )));
    if mode == Mode::PrMerge {
        match pr::request_auto_merge(&repo_on_forge, &opened, &head_sha) {
            Ok(pr::AutoMerge::Queued) => say(Say::Done(format!(
                "#{} is set to merge once its checks pass.",
                opened.number
            ))),
            Ok(pr::AutoMerge::NotQueued(why)) => {
                let said = format!(
                    "#{} is not set to auto-merge — {why}. It stays open for a person to merge.",
                    opened.number
                );
                say(Say::Warn(said.clone()));
                attempt.detail = said;
            }
            Err(why) => {
                let said = format!("#{} is not set to auto-merge: {why}", opened.number);
                say(Say::Warn(said.clone()));
                attempt.detail = said;
            }
        }
    }
    0
}

/// The branch a PR-mode save pushes when the clone stands on its PR's base or its default
/// branch: one an earlier save of this repo pushed, while HEAD still descends from it, else
/// a fresh `charter/<workspace>/<short-sha>` at HEAD. Created or moved forward locally too, so
/// the operator can see it; never moved backwards or sideways.
fn save_branch(request: &Request, on: &str, head_sha: &str) -> Result<String, String> {
    let clone = request.clone;
    let prefix = format!("charter/{}/", request.workspace);
    // The newest journal line that NAMES one — a quit's commit-only line or a failed push
    // names none, and must not break the chain — then every local branch charter made here,
    // newest first. The first that HEAD still descends from carries on.
    let wanted = target(request.workspace, request.name);
    let journalled = planegit::journal(request.plane)
        .into_iter()
        .rev()
        .filter(|line| line.get("target").and_then(serde_json::Value::as_str) == Some(&wanted))
        .find_map(|line| line.get("branch")?.as_str().map(str::to_owned))
        .filter(|name| name.starts_with(&prefix));
    let local = git::run(
        clone,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            &format!("refs/heads/{prefix}"),
        ],
        git::READ,
    )
    .ok()
    .filter(git::Run::ok)
    .map(|run| run.out.lines().map(str::to_owned).collect::<Vec<_>>())
    .unwrap_or_default();
    let reuse = journalled
        .into_iter()
        .chain(local)
        .filter(|name| name.starts_with(&prefix) && planesave::branch_ok(name))
        .find(|name| {
            git::run(
                clone,
                &[
                    "merge-base",
                    "--is-ancestor",
                    &format!("refs/heads/{name}"),
                    "HEAD",
                ],
                git::READ,
            )
            .is_ok_and(|r| r.ok())
        });
    let name = match reuse {
        Some(name) => name,
        None => {
            let short = &head_sha[..head_sha.len().min(7)];
            let name = format!("{prefix}{short}");
            if !planesave::branch_ok(&name) {
                return Err(format!(
                    "{name} is not a branch name git would accept, so charter cannot move \
                     {on}'s commits off {on}"
                ));
            }
            name
        }
    };
    let moved = git::run(
        clone,
        &["update-ref", &format!("refs/heads/{name}"), "HEAD"],
        git::READ,
    );
    if !moved.as_ref().is_ok_and(git::Run::ok) {
        let said = moved.map_or_else(|e| e.to_string(), |run| planegit::tail(&run));
        return Err(format!("charter could not create {name}: {said}"));
    }
    Ok(name)
}

/// The names a file of credentials goes by, whatever is in it.
fn secret_name(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    let example = [".example", ".sample", ".template", ".dist"]
        .iter()
        .any(|suffix| lower.ends_with(suffix));
    if lower == ".env" || (lower.starts_with(".env.") && !example) {
        return Some("an environment file");
    }
    if ["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"].contains(&lower.as_str()) {
        return Some("an SSH private key");
    }
    if [".pem", ".p12", ".pfx", ".key"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return Some("a key or certificate store");
    }
    if lower == "credentials.json" {
        return Some("a credentials file");
    }
    None
}

/// The staged files of `staged` a repo save refuses, each with what it looks like — **the
/// narrower rule a repo takes** (ADR 0051). The plane's own guard reads its memory and ref
/// files with a credential-assignment rule that ordinary code trips (`password: String`), so a
/// clone is asked two things instead:
///
/// - a file whose NAME is a credential's: `.env` and `.env.*` (not `.example`, `.sample`,
///   `.template`, `.dist`), an SSH private key, `*.pem`, `*.p12`, `*.pfx`, `*.key`,
///   `credentials.json`, and a `.npmrc` or `.pypirc` that holds a token or a password;
/// - a private key block or a live token by its forge's own prefix in any file's staged text
///   ([`crate::secretshape::token_kind`]).
///
/// Asked of the staged blob (`git show :<path>`), as the plane's guard asks it. What was found
/// is named by path and kind, never by value.
fn secret_files(clone: &Path, staged: &[String]) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for path in staged {
        let name = path.rsplit('/').next().unwrap_or(path);
        if let Some(kind) = secret_name(name) {
            // A deletion is staged too, and taking a secret out is not what this refuses.
            if git::run(clone, &["cat-file", "-e", &format!(":{path}")], git::READ)
                .is_ok_and(|r| r.ok())
            {
                out.push((path.clone(), kind));
            }
            continue;
        }
        let Ok(blob) = git::run(clone, &["show", &format!(":{path}")], git::READ) else {
            continue;
        };
        if !blob.ok() {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if (lower == ".npmrc" || lower == ".pypirc")
            && blob.out.lines().any(|line| {
                let line = line.to_ascii_lowercase();
                (line.contains("_authtoken") || line.contains("password")) && line.contains('=')
            })
        {
            out.push((path.clone(), "a package registry token"));
            continue;
        }
        if let Some(kind) = crate::secretshape::token_kind(&blob.out) {
            out.push((path.clone(), kind));
        }
    }
    out
}

/// The repo's default branch: its `default_branch` in `inventory/repos.json`, else what the
/// clone's `origin/HEAD` names.
pub fn default_branch(plane: &Path, name: &str, clone: &Path) -> Option<String> {
    let listed = crate::inventory::load(plane, "")
        .ok()
        .map(|doc| crate::inventory::listed(&doc))
        .unwrap_or_default();
    let recorded = crate::inventory::find(&listed, name)
        .and_then(|record| record.get("default_branch")?.as_str())
        .filter(|b| planesave::branch_ok(b))
        .map(str::to_owned);
    recorded.or_else(|| {
        git::run(
            clone,
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
            git::READ,
        )
        .ok()
        .filter(git::Run::ok)
        .map(|r| r.line().trim().to_string())
        .and_then(|head| head.strip_prefix("origin/").map(str::to_owned))
        .filter(|b| planesave::branch_ok(b))
    })
}

/// How many files a generated message names before it says how many more there are.
const SUMMARY_FILES: usize = 5;

/// The message a repo save writes when it is given none: how many files, and which —
/// `charter save: 2 files (src/a.rs, README.md)`.
fn summary(staged: &[String]) -> String {
    let mut named: Vec<String> = staged.iter().take(SUMMARY_FILES).cloned().collect();
    if staged.len() > SUMMARY_FILES {
        named.push(format!("+{} more", staged.len() - SUMMARY_FILES));
    }
    let files = if staged.len() == 1 { "file" } else { "files" };
    format!(
        "charter save: {} {files} ({})",
        staged.len(),
        named.join(", ")
    )
}

/// The newest journal line about this repo.
fn last_entry(plane: &Path, workspace: &str, name: &str) -> Option<serde_json::Value> {
    let wanted = target(workspace, name);
    planegit::journal(plane)
        .into_iter()
        .rev()
        .find(|line| line.get("target").and_then(serde_json::Value::as_str) == Some(&wanted))
}

/// Where a repo's unsaved work sits, read from git and the journal — never the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub name: String,
    pub mode: Mode,
    /// The file that decided the mode, or `default`.
    pub mode_from: &'static str,
    pub autosave: bool,
    pub stage: Stage,
    /// The branch the clone is on; `None` when it is on none.
    pub branch: Option<String>,
    /// Files a save would take.
    pub changed: u32,
    /// Commits the remote's copy of the branch lacks; `None` when there is nothing to count
    /// against — a branch never pushed.
    pub ahead: Option<u32>,
    /// The pull request HEAD is waiting on.
    pub pr: Option<String>,
    pub blocked: Option<String>,
    /// HEAD's commit, when there is one.
    pub head: Option<String>,
    /// Whether a save would push: a mode past the commit, and an origin on a known forge.
    pub pushes: bool,
}

impl Standing {
    /// Whether a save of a repo standing like this would do anything.
    pub fn worth_saving(&self) -> bool {
        self.mode != Mode::Off
            && (self.changed > 0 || self.stage == Stage::Blocked || self.stage == Stage::Committed)
    }

    /// What is unsaved, as one string that changes whenever it does — auto-save's quiet
    /// period is measured from its last change.
    pub fn fingerprint(&self) -> String {
        format!("{:?}\0{}\0{:?}", self.head, self.changed, self.ahead)
    }
}

/// Where the clone `repo` of `workspace` stands.
pub fn standing(plane: &Path, workspace: &str, repo: &repos::Repo) -> Standing {
    let settings = planesave::Settings::read(plane).repo(&repo.name);
    let mode = settings.mode.value;
    let mut out = Standing {
        name: repo.name.clone(),
        mode,
        mode_from: settings.mode.source.file().unwrap_or("default"),
        autosave: settings.autosave.value,
        stage: Stage::Saved,
        branch: None,
        changed: 0,
        ahead: None,
        pr: None,
        blocked: None,
        head: None,
        pushes: false,
    };
    let state = match repos::state_of(&repo.path) {
        Ok(state) => state,
        Err(unreadable) => {
            out.stage = Stage::Blocked;
            out.blocked = Some(unreadable.to_string());
            return out;
        }
    };
    out.changed = state.tracked.saturating_add(state.untracked);
    out.head = rev(&repo.path, "HEAD");
    out.pushes = !matches!(mode, Mode::Off | Mode::Commit)
        && planegit::origin_https_of(plane, &repo.path).is_some();
    if let Head::Branch(branch) = &state.head {
        out.ahead = planegit::count(&repo.path, &format!("refs/remotes/origin/{branch}..HEAD"));
    }
    out.branch = match &state.head {
        Head::Branch(b) | Head::Unborn(b) => Some(b.clone()),
        Head::Detached(_) => None,
    };
    let last = last_entry(plane, workspace, &repo.name);
    let said = |key: &str| {
        last.as_ref()
            .and_then(|l| l.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    // A journal line speaks for the clone only while HEAD is still the commit it names.
    let current = said("commit").is_none_or(|sha| Some(sha) == out.head);
    out.stage = if current && said("outcome").as_deref() == Some("blocked") {
        out.blocked = said("detail").or_else(|| Some("the last save could not finish".into()));
        Stage::Blocked
    } else if out.changed > 0 {
        Stage::Changed
    } else if !out.pushes {
        Stage::Saved
    } else if current && said("commit").is_some() && said("outcome").as_deref() == Some("pr-open") {
        out.pr = said("pr");
        Stage::PrOpen
    } else if out.head.is_some() && out.ahead != Some(0) {
        Stage::Committed
    } else {
        Stage::Saved
    };
    out
}

#[cfg(test)]
mod tests;
