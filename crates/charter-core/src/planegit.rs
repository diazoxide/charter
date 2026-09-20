//! `charter save`: git for the control plane itself — staging, the secret guard, committing,
//! and pushing over the plane's OWN forge's HTTPS token. A port of `charter/planegit.py` and
//! `charter/commands.py`'s `cmd_save`.
//!
//! # `save` commits whatever is pending, and says so before it does
//!
//! `charter save` runs `git add -A` in the plane. That is charter's documented behaviour and
//! it is a foot-gun: measured on the operator's own plane on 2026-09-12, a save reported "169
//! file(s)" — which read as a large but ordinary batch of persona memories — and carried a
//! `uv.lock` that had never existed in that repository, created by some agent's local `uv run`
//! and never gitignored. A project file reached `main` with no pull request and nobody
//! choosing it, and the only visible evidence was a count.
//!
//! **A count is not a description**, so this port prints the directory breakdown of what is
//! staged BEFORE the commit is made. It is the one thing the incident's own write-up says is
//! the whole check, and it is the only block this command has that Python does not. Being
//! charter's alone is not a reason for nothing to check it: the differential harness takes it
//! out of the Rust stderr (`rust_only_lines`) and then compares the block it took out, byte
//! for byte, against what the scenario declares (`rust_only_block`) — the headline's counts,
//! every row's count and directory, the order they are in, and the tail that says how many
//! directories were not listed. The behaviour itself is ported faithfully:
//! `save` still stages everything, because narrowing it would silently stop saving things
//! planes rely on it saving.
//!
//! # Where the credential is, and where it is not
//!
//! The token itself never enters this process. Every push goes through
//! [`crate::worktree::git::run_network`], which hands git ONE credential helper — the forge
//! CLI's, by absolute path — and resets every other one, refuses SSH, `ext::`, `git://` and
//! plain `http://`, and passes the forge CLI's own credential environment to the child
//! without ever reading it here. So:
//!
//! - **argv** carries a helper COMMAND, never a secret;
//! - **an error** is git's stderr, and the URL charter pushes to is one it BUILT from the
//!   forge's own HTTPS base rather than one it was handed. A remote carrying userinfo
//!   (`https://x:tok@host/…`) is not that URL, so [`origin_https`] answers `None` and the
//!   warning that follows names no URL at all: a token an operator put in their remote is
//!   neither pushed with nor printed;
//! - **a commit** is guarded by [`crate::secretshape`]: a staged memory or ref file that looks
//!   like it holds a credential stops the save.
//!
//! # Where this is stricter than Python, on purpose
//!
//! - **The plane's own git hooks do not run for charter's commit.** Every call goes through
//!   the hardened runner, which sets `core.hooksPath=/dev/null`. That is the same policy that
//!   turns signing off: a `pre-commit` that prompts, builds or waits hangs an autonomous agent
//!   exactly the way a signer prompt does, and a hook charter triggers is not one the operator
//!   asked for. Python runs them.
//! - **A memory or ref file that leads out of the plane is refused rather than read through.**
//!   Python reads it and scans whatever is on the other end; charter #442 is what a read
//!   through a committed link costs, and a memory file charter cannot read is not one it will
//!   commit unexamined.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::forge::{self, Forge};
use crate::gitstate;
use crate::repocmd::{Say, Sink};
use crate::secretshape;
use crate::shown;
use crate::worktree::git;

/// What a push of the plane root's HEAD did, in one word. Python's module constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It landed on the branch HEAD is on.
    Pushed,
    /// The branch requires a pull request → it landed on `charter/<sha>`.
    Branched,
    /// The branch requires a pull request and THAT push failed too.
    Stranded,
    /// An ordinary push failure, reported rather than diagnosed.
    Failed,
    /// The remote moved and the rebase onto it conflicted.
    Conflict,
    /// No origin on a forge charter knows — nothing to push to.
    Unreachable,
}

impl Outcome {
    /// The word the record carries, which `doctor` reads back. Python's literals.
    pub fn word(self) -> &'static str {
        match self {
            Outcome::Pushed => "pushed",
            Outcome::Branched => "branched",
            Outcome::Stranded => "stranded",
            Outcome::Failed => "failed",
            Outcome::Conflict => "conflict",
            Outcome::Unreachable => "unreachable",
        }
    }
}

/// What [`push_head`] did, in a shape both a human and `doctor` can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushResult {
    pub outcome: Outcome,
    /// The branch charter tried to advance (the plane root's HEAD).
    pub branch: String,
    /// The remote branch the commit actually reached, when that is a different one. "It is on
    /// the remote under another name" and "it is on this laptop only" are one exit code apart
    /// and worlds apart in consequence.
    pub landed: Option<String>,
    pub url: Option<String>,
    pub detail: String,
}

impl PushResult {
    fn of(outcome: Outcome, branch: &str) -> PushResult {
        PushResult {
            outcome,
            branch: branch.to_string(),
            landed: None,
            url: None,
            detail: String::new(),
        }
    }
}

// --------------------------------------------------------------------------------------- //
// the plane's origin                                                                        //
// --------------------------------------------------------------------------------------- //

/// The control plane's own `origin` as an HTTPS URL, rewriting an SSH one through ITS forge's
/// own rewrite rule — or `None` when there is no origin, when its host is not a forge this
/// charter knows, or when it carries userinfo.
///
/// `None` rather than a guess: the caller then warns and skips the push instead of silently
/// trying — and failing — against the wrong forge. Python's `_origin_https`.
///
/// **A remote carrying a credential is refused here, by construction.** A CI machine's
/// `https://x-access-token:<token>@github.com/acme/plane.git` does not start with the forge's
/// HTTPS base and is not one of its SSH forms, so it falls out of the rewrite below with
/// `None` — the token is neither pushed with nor named in the warning the caller then prints.
/// That is a property worth a test rather than a second check: the danger is not the `@`, it is
/// handing git a URL charter did not build.
///
/// # Testing this against a local stand-in forge, and the trap in doing so
///
/// **`git remote get-url` APPLIES `url.<base>.insteadOf`.** A test that stands a bare
/// repository up beside the plane and rewrites the forge's URL onto it — which is the only way
/// to exercise a push without reaching a real forge — decides, by which URL it keys the
/// rewrite, whether this function sees a forge at all:
///
/// - keyed on the HTTPS base (`url.file:///…/forge/acme/.insteadOf =
///   https://github.com/acme/`) with `origin` in the SSH form, `get-url` returns the SSH URL
///   untouched, [`forge::resolve_host`] places it, the rewrite below runs, and git maps the
///   HTTPS URL charter built onto the bare repository. This is the arrangement that tests
///   anything.
/// - keyed on the SSH form, or with `origin` already HTTPS, `get-url` hands back
///   `file:///…`. [`forge::resolve_host`] answers `None`, this function answers `None`, and
///   the caller warns and skips the push. **Both implementations agree perfectly and nothing
///   is pushed** — a green differential scenario over a code path neither side entered.
///
/// `env_clear()` in [`crate::worktree::git`] does not close this: `insteadOf` arrives through
/// CONFIG, not the environment, and `HOME` is the one variable that runner deliberately keeps
/// (its module docs say why), so `~/.gitconfig`'s rewrite reaches every git call charter
/// makes. It cannot be measured by looking at argv either — the rewrite happens inside git.
///
/// `tests/differential/run.py` therefore measures what `origin` resolves to before it runs
/// the command, and fails any scenario whose plane has a stand-in forge beside it and an
/// origin that came back `file://` without the scenario saying it meant that
/// (`local_origin_why`). Read that check before writing a new forge scenario.
pub fn origin_https(root: &Path) -> Option<String> {
    let url = git::run(root, &["remote", "get-url", "origin"], git::READ)
        .ok()
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    if url.is_empty() {
        return None;
    }
    let forge = forge::resolve_host(&url, root)?;
    let (https_base, ssh_forms) = forge.insteadof();
    if url.starts_with(&https_base) {
        return Some(url);
    }
    for prefix in &ssh_forms {
        if let Some(rest) = url.strip_prefix(prefix.as_str()) {
            return Some(format!("{https_base}{rest}"));
        }
    }
    None
}

/// A one-click "open a pull request for this branch" URL, or `None`. Python's `_compare_url`.
///
/// A plain HTTPS link, deliberately: charter has no PR-creation capability in any forge
/// adapter, so this closes the pull-request-gated workflow without adding one — no API call,
/// no extra token scope. Which form to build is decided by RESOLVING the forge, never by
/// looking for a hostname inside the URL string: a self-hosted GitLab with a
/// `mirrors/github.com/…` namespace was handed GitHub's compare URL by the substring check
/// this replaces.
pub fn compare_url(https: &str, branch: &str, root: &Path) -> Option<String> {
    let base = https.strip_suffix(".git").unwrap_or(https);
    if !base.starts_with("https://") {
        return None;
    }
    let forge = forge::resolve_host(base, root)?;
    match forge.kind {
        forge::Kind::GitHub => Some(format!("{base}/compare/{branch}?expand=1")),
        // GitLab, and self-hosted GitLab, use the same new-MR form.
        forge::Kind::GitLab => Some(format!(
            "{base}/-/merge_requests/new?merge_request%5Bsource_branch%5D={branch}"
        )),
    }
}

// --------------------------------------------------------------------------------------- //
// the push record                                                                           //
// --------------------------------------------------------------------------------------- //

/// Where the push record lives. Public because `doctor` names it, which is what gives the
/// recorded `detail` — git's own words about a push nobody heard — a reader.
///
/// Under [`crate::plane::state_dir`] and not `<root>/.charter`, because `$CHARTER_HOME` moves
/// that directory verbatim: hardcoding the default wrote the record where `doctor` — which
/// already resolves it — would never look, and a record nobody reads is the same as no record.
pub fn push_record_path(root: &Path) -> PathBuf {
    crate::plane::state_dir(root).join("plane-push.json")
}

/// Write down what a push of the plane root's HEAD did, and return `res` unchanged.
///
/// A record and not a printed line, because the reactive-memory push runs detached with
/// `/dev/null` for a voice and has no caller to tell. What is written is the PAST tense — *a
/// push of commit `head` at time `at` came out this way* — which is the carve-out ADR 0011
/// makes: the present tense is reconstructed at read time by joining `head` against git
/// ([`is_spent`]), so a stale file cannot contradict a reality nobody re-derived.
///
/// A clean push DELETES the file rather than writing "pushed": the record exists to carry a
/// condition that outlives the process, and there is no condition left to carry.
///
/// Never fails loudly. Every caller is a push, and a push that cannot write a note must still
/// have pushed.
pub fn record_push(root: &Path, res: PushResult, head: &str) -> PushResult {
    let path = push_record_path(root);
    if res.outcome == Outcome::Pushed {
        let _ = std::fs::remove_file(&path);
        return res;
    }
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default();
    let doc = serde_json::json!({
        "outcome": res.outcome.word(),
        "branch": res.branch,
        "landed": res.landed,
        "url": res.url,
        "detail": res.detail,
        "head": head,
        "at": at,
    });
    // NOT gated on `within_plane`: the state directory is machine-local and `$CHARTER_HOME`
    // may legitimately put it outside the plane, so that question is the wrong one to ask
    // about this file. `private_dir` refuses a state directory that is a symlink, and
    // `write_private` writes beside the record and renames over it — so a link planted AT the
    // record is replaced rather than written through.
    if let Some(dir) = path.parent()
        && crate::profiletrust::private_dir(dir).is_ok()
    {
        let _ = crate::profiletrust::write_private(
            &path,
            crate::pyjson::dumps_indent2(&doc).as_bytes(),
        );
    }
    res
}

/// The last recorded push outcome, or `None` when there is nothing to report.
///
/// `None` for a missing file AND for an unreadable or malformed one, deliberately: the only
/// consumer runs from a hook, where a defect in this file is charter's own to fix and not a
/// reason to take the session down.
///
/// # It is read the way `reopen` reads its record, and for the same reasons
///
/// This runs inside `charter save`, which an operator runs all day, and `read_to_string` on a
/// path is two hazards charter has already paid for once (charter-app #28):
///
/// - **A FIFO is not a link**, so a link check waves it through and reading one never
///   returns. There is no window to close and nothing to cancel — the save simply stops.
/// - **A planted giant** is read whole into memory. git cannot carry a FIFO, but a sparse
///   multi-gigabyte file packs small and arrives full size.
///
/// So the descriptor is opened with `O_NOFOLLOW | O_NONBLOCK` ([`crate::contain::open_no_link`])
/// and both questions are asked of THAT ([`crate::reopen::refuse_unusable`]), not of the name:
/// an `lstat` and a later open are two different objects with a window between them, and the
/// swap through that window is the failure the check exists to stop (ADR 0028).
///
/// **The walk starts at the state directory, not the plane root**, and that is the boundary
/// this file actually has: `$CHARTER_HOME` can put charter's machine-local state outside the
/// plane entirely, and "does any link leave the plane" is not a question about a path that was
/// never inside it. One component is left to walk — the record's own name — and that is
/// exactly the component a link or a FIFO can be.
pub fn push_record(root: &Path) -> Option<serde_json::Value> {
    let path = push_record_path(root);
    let mut open = crate::contain::open_no_link(path.parent()?, &path).ok()?;
    crate::reopen::refuse_unusable(&path, &open.metadata().ok()?).ok()?;
    let text = {
        use std::io::Read;
        let mut text = String::new();
        // Bounded again on the way in: `refuse_unusable` asked how big it was, and a writer
        // that appends between the `fstat` and the read would otherwise still be unbounded.
        open.by_ref()
            .take(crate::reopen::MAX_BYTES)
            .read_to_string(&mut text)
            .ok()?;
        text
    };
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let has_outcome = doc
        .get("outcome")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|w| !w.is_empty());
    (doc.is_object() && has_outcome).then_some(doc)
}

/// An object name as charter writes one: hex, and the length `rev-parse` answers with.
///
/// Both values this module takes out of `plane-push.json` end up in git's ARGV, and the file
/// is ordinary machine-local state that anything running as the operator can rewrite. A `head`
/// of `--help` makes `merge-base` exit 0 — which this module reads as "it landed" and a memory
/// is never pushed again. So each is held to the shape charter itself wrote rather than passed
/// through: git has no `--` that separates a revision from an option here, and a check is
/// cheaper than one that does.
fn is_object_name(text: &str) -> bool {
    (7..=64).contains(&text.len()) && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// Has the commit a record is ABOUT since reached the tracked upstream?
///
/// `--is-ancestor` exits 0 for yes, 1 for no, and non-zero-not-1 for a ref it cannot resolve.
/// **Only a clean 0 counts**: "I could not check" must never read as "it landed", which is the
/// one place where getting that wrong loses a memory.
pub fn is_spent(root: &Path, head: &str) -> bool {
    if !is_object_name(head) {
        return false;
    }
    git::run(
        root,
        &["merge-base", "--is-ancestor", head, "@{upstream}"],
        git::READ,
    )
    .is_ok_and(|r| r.ok())
}

/// The recorded push outcome that is STILL true, or `None`. One decision, several renderings:
/// `doctor`'s row, the status line's one word, and [`land_via_branch`]'s "may I still advance
/// that branch".
pub fn unlanded(root: &Path) -> Option<serde_json::Value> {
    let rec = push_record(root)?;
    let head = rec.get("head").and_then(serde_json::Value::as_str)?;
    (!is_spent(root, head)).then_some(rec)
}

/// The `charter/<sha>` branch an earlier push is STILL waiting on a pull request for.
///
/// Advancing it is a fast-forward — each new HEAD is a descendant of the one before — so one
/// open pull request accumulates the commits instead of leaving one abandoned remote branch
/// per save. Safe because it is only ever a FIRST attempt: the caller pushes without
/// `--force`, so git itself refuses anything that is not a fast-forward.
fn open_pull_request_branch(root: &Path) -> Option<String> {
    let rec = unlanded(root)?;
    if rec.get("outcome").and_then(serde_json::Value::as_str) != Some(Outcome::Branched.word()) {
        return None;
    }
    // Held to the name `land_via_branch` mints, for the reason `is_object_name` gives: this
    // value becomes the remote ref a push writes, and a record naming `main` would make the
    // next save advance the branch the whole pull-request path exists to leave alone.
    rec.get("landed")
        .and_then(serde_json::Value::as_str)
        .and_then(|name| name.strip_prefix("charter/"))
        .filter(|sha| is_object_name(sha))
        .map(|sha| format!("charter/{sha}"))
}

// --------------------------------------------------------------------------------------- //
// pushing                                                                                   //
// --------------------------------------------------------------------------------------- //

/// Signatures a forge uses to say "this branch requires a pull request". Matched, never
/// interpolated, and each one observed rather than imagined (ADR 0009: charter may name a
/// cause it RECOGNISED, never one it inferred). An unmatched rejection falls through to the
/// generic "push failed" warning, which costs precision and can never mislead.
const PROTECTED_SIGNATURES: [&str; 6] = [
    "protected branch",
    "gh006",
    "pre-receive hook declined",
    "required status check",
    "merge_request",
    "not allowed to push",
];

fn is_protected_rejection(stderr: &str) -> bool {
    let blob = stderr.to_lowercase();
    PROTECTED_SIGNATURES.iter().any(|s| blob.contains(s))
}

/// The last four lines of what git said, which is what charter repeats.
fn tail(run: &git::Run) -> String {
    // stderr when there IS any, exactly as Python's `p.stderr or p.stdout or ""` chooses.
    let text = if run.err.is_empty() {
        &run.out
    } else {
        &run.err
    };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(4)..].join("\n")
}

/// Push the commit that is already on HEAD to a remote branch, and hand back a URL.
///
/// The sanctioned path for a control plane whose own repo requires pull requests. **The plane
/// root's HEAD never moves**: `git push HEAD:refs/heads/<new>` needs no checkout, no branch
/// creation and no worktree — only a different REMOTE ref for a commit that already exists.
fn land_via_branch(
    root: &Path,
    https: &str,
    helper: Option<&str>,
    default_branch: &str,
    say: Sink,
) -> PushResult {
    let sha = git::run(root, &["rev-parse", "--short", "HEAD"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    let fresh = format!("charter/{}", if sha.is_empty() { "change" } else { &sha });
    let reuse = open_pull_request_branch(root).filter(|r| *r != fresh);
    // A reuse that did not fast-forward is not a failure to report: the fresh name has not
    // been tried yet, and naming a branch charter chose on the operator's behalf as the thing
    // that went wrong would send the reader after a problem they do not have.
    let mut attempt: Option<(String, git::Run)> = None;
    for candidate in reuse.into_iter().chain(std::iter::once(fresh.clone())) {
        let pushed = git::run_network(
            root,
            helper,
            &["push", https, &format!("HEAD:refs/heads/{candidate}")],
        );
        if let Ok(run) = pushed {
            let landed = run.ok();
            attempt = Some((candidate, run));
            if landed {
                break;
            }
        }
    }
    if !attempt.as_ref().is_some_and(|(_, run)| run.ok()) {
        let said = attempt
            .as_ref()
            .map(|(_, run)| tail(run))
            .unwrap_or_default();
        say(Say::Fail(format!(
            "'{default_branch}' requires a pull request, and pushing the branch '{fresh}' also \
             failed:"
        )));
        for line in said.lines() {
            say(Say::Fail(format!("  {line}")));
        }
        return PushResult {
            detail: said,
            ..PushResult::of(Outcome::Stranded, default_branch)
        };
    }
    let branch = attempt.map(|(name, _)| name).unwrap_or(fresh);
    let url = compare_url(https, &branch, root);
    say(Say::Done(format!(
        "'{default_branch}' requires a pull request — pushed {branch} instead."
    )));
    if let Some(url) = &url {
        say(Say::Info(format!("  open it: {url}")));
    }
    say(Say::Info(format!(
        "  the commit is also on your local {default_branch}, one ahead of the remote. After \
         the PR merges: git -C {} pull --rebase",
        root.display()
    )));
    PushResult {
        landed: Some(branch),
        url,
        ..PushResult::of(Outcome::Branched, default_branch)
    }
}

/// Push the plane root's HEAD to its own branch on origin — **the one pusher**.
///
/// **The branch is never predicted.** Nothing here asks the forge whether the branch is
/// protected before committing: charter cannot know that without a network call it has no
/// business making from a hook, and guessing it from the branch name is the unearned diagnosis
/// ADR 0009 forbids. The rejection IS the evidence, and it arrives only after the commit
/// exists — so the commit is made, and the OUTCOME is what gets reported honestly.
pub fn push_head(root: &Path, say: Sink) -> PushResult {
    let branch = git::run(root, &["rev-parse", "--abbrev-ref", "HEAD"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    let mut head = git::run(root, &["rev-parse", "HEAD"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    let Some(https) = origin_https(root) else {
        say(Say::Warn(
            "origin isn't on a forge charter knows (gitlab.com/github.com/…) — committed \
             locally; push manually."
                .into(),
        ));
        // Returned UNRECORDED, deliberately. `unreachable` is the one outcome `doctor` never
        // reports — a plane with no origin charter knows has a CONFIGURATION to fix, not a
        // commit to rescue — so writing it could only ever DESTROY a real notice, and it did.
        return PushResult::of(Outcome::Unreachable, &branch);
    };
    // The forge of the plane's OWN origin, and its CLI's helper by absolute path. One
    // credential per forge: a GitHub-hosted plane authenticates with `gh`, never with
    // whichever helper happens to be first in the operator's config.
    // `origin_https` answered, so the host IS one this plane manages and `forge_for` resolves
    // it; the fallback is here so a future divergence between the two cannot panic a push.
    let forge = crate::gitpolicy::forge_for(root, root)
        .unwrap_or_else(|| Forge::default_of(forge::DEFAULT_KIND));
    let helper = forge::helper_for(&forge);
    let cli = forge.kind.cli();
    let push = |root: &Path| {
        git::run_network(
            root,
            Some(helper.as_str()),
            &["push", &https, &format!("HEAD:{branch}")],
        )
    };

    let mut run = match push(root) {
        Ok(run) => run,
        Err(unavailable) => {
            say(Say::Warn(format!("Committed, but the {cli} push failed:")));
            say(Say::Warn(format!("  {unavailable}")));
            say(Say::Info(format!("Check `{cli} auth status`.")));
            return record_push(
                root,
                PushResult {
                    detail: unavailable.to_string(),
                    ..PushResult::of(Outcome::Failed, &branch)
                },
                &head,
            );
        }
    };
    let said = |run: &git::Run| {
        if run.err.is_empty() {
            run.out.clone()
        } else {
            run.err.clone()
        }
    };
    if !run.ok() && is_protected_rejection(&said(&run)) {
        // Asked BEFORE the non-fast-forward retry, and the ordering is load-bearing: git
        // prints its own `! [remote rejected]` line above a server-side hook decline, so the
        // word "rejected" in the retry test below matches a protected branch too. Measured
        // against a real bare remote with a pre-receive hook refusing `refs/heads/main`:
        // taking the retry first fetched an unreachable origin, which REMOVES FETCH_HEAD, so
        // the rebase failed and the outcome was recorded as `conflict` — the pull-request path
        // never reached and charter telling the operator to resolve a conflict that did not
        // exist.
        return record_push(
            root,
            land_via_branch(root, &https, Some(helper.as_str()), &branch, say),
            &head,
        );
    }
    let moved = ["fetch first", "non-fast-forward", "rejected"];
    if !run.ok() && moved.iter().any(|s| run.err.contains(*s)) {
        say(Say::Info(
            "remote moved — fetching + rebasing, then retrying …".into(),
        ));
        let fetched = git::run_network(root, Some(helper.as_str()), &["fetch", &https, &branch]);
        if !fetched.is_ok_and(|r| r.ok()) {
            // A failed fetch leaves NO FETCH_HEAD to rebase onto, so rebasing anyway fails for
            // a reason that has nothing to do with a conflict. Calling that `conflict` would
            // send the reader to resolve one that does not exist while the real answer — the
            // push failure below, in git's own words — is discarded.
            say(Say::Warn(
                "Could not reach origin to fetch, so the retry was skipped.".into(),
            ));
        } else if !git::run_untimed(root, &["rebase", "FETCH_HEAD"]).is_ok_and(|r| r.ok()) {
            let _ = git::run_untimed(root, &["rebase", "--abort"]);
            say(Say::Warn(
                "Committed locally, but rebase hit a conflict — resolve manually, then \
                 `charter save`."
                    .into(),
            ));
            return record_push(root, PushResult::of(Outcome::Conflict, &branch), &head);
        } else {
            // The rebase rewrote it, so the commit the record names is a different one now.
            head = git::run(root, &["rev-parse", "HEAD"], git::READ)
                .map(|r| r.line().trim().to_string())
                .unwrap_or_default();
            match push(root) {
                Ok(again) => run = again,
                Err(_) => {
                    return record_push(root, PushResult::of(Outcome::Failed, &branch), &head);
                }
            }
        }
    }
    if run.ok() {
        // Keep the tracking ref in step, as Python does, so the next status is not one behind.
        let _ = git::run(
            root,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                "HEAD",
            ],
            git::READ,
        );
        say(Say::Done(format!(
            "Pushed {branch} via {cli} (HTTPS token — no SSH, no 1Password)."
        )));
        return record_push(root, PushResult::of(Outcome::Pushed, &branch), &head);
    }
    if is_protected_rejection(&said(&run)) {
        // Not redundant with the identical check above the retry: the FIRST push can fail
        // plain non-fast-forward, and the SECOND — after a rebase that succeeded — is the one
        // the protected branch refuses. Same policy, reached from the other side.
        return record_push(
            root,
            land_via_branch(root, &https, Some(helper.as_str()), &branch, say),
            &head,
        );
    }
    let detail = tail(&run);
    say(Say::Warn(format!("Committed, but the {cli} push failed:")));
    for line in detail.lines() {
        say(Say::Warn(format!("  {line}")));
    }
    say(Say::Info(format!("Check `{cli} auth status`.")));
    record_push(
        root,
        PushResult {
            detail,
            ..PushResult::of(Outcome::Failed, &branch)
        },
        &head,
    )
}

// --------------------------------------------------------------------------------------- //
// staging                                                                                   //
// --------------------------------------------------------------------------------------- //

/// Pathspecs that name the whole repository rather than a path inside it. `git add .` run with
/// `-C <root>` is `git add -A` wearing a different spelling, and `:/` says so outright.
const WHOLE_TREE_PATHSPECS: [&str; 4] = [".", "./", ":/", ":/:"];

/// Flags that, with NO pathspec, make git operate on the whole tree instead of on nothing.
/// `-u` is here with `-A`: it stages only tracked modifications, which is precisely the
/// operator's mid-edit `charter.toml`.
const WHOLE_TREE_FLAGS: [&str; 5] = ["-A", "--all", "--no-ignore-removal", "-u", "--update"];

/// Would `add_cmd` stage EVERYTHING under the root it is run in?
///
/// The property that makes a call dangerous is not which command issued it — it is that it
/// sweeps up files the caller never named and, from another working tree, cannot see. So the
/// worktree refusal is keyed on this rather than on `charter save`, and the next caller to copy
/// that shape inherits the guard.
///
/// **With no pathspec, the flags decide, and a bare `--` bounds nothing.** Measured against git
/// on a repo with one edited tracked file and one untracked file: `git add` and `git add --`
/// stage nothing, `git add -A` and `git add -A --` stage both, `git add -u` stages the tracked
/// edit. The first draft treated `["add", "-A", "--"]` as scoped, and it stages the whole tree.
pub fn stages_the_whole_tree(add_cmd: &[&str]) -> bool {
    let args = &add_cmd[1.min(add_cmd.len())..];
    let (flags, paths): (Vec<&str>, Vec<&str>) = match args.iter().position(|a| *a == "--") {
        // Everything past the separator is a pathspec, even one that starts with a dash: a
        // file really can be called `-A`, and past `--` git stages that file rather than
        // reading the flag.
        Some(i) => (args[..i].to_vec(), args[i + 1..].to_vec()),
        None => (
            args.to_vec(),
            args.iter()
                .copied()
                .filter(|a| !a.starts_with('-'))
                .collect(),
        ),
    };
    if !paths.is_empty() {
        // `any`, not `all`: measured, `git add -- . u.txt` stages the whole tree. One
        // whole-tree spelling anywhere in the list widens the call, and the named paths beside
        // it change nothing.
        return paths.iter().any(|p| WHOLE_TREE_PATHSPECS.contains(p));
    }
    flags.iter().any(|f| WHOLE_TREE_FLAGS.contains(f))
}

/// The linked worktree of `plane`'s repository that `start` stands in, else `None`.
///
/// A plane and a working tree are not the same object. The PLANE is identity and
/// machine-local state; the TREE is committed content on a branch. `save` commits, so an
/// unbounded stage typed in a worktree would commit the plane's clone — the operator's
/// half-finished files under the agent's message — and none of the agent's own work.
pub fn tree_of(plane: &Path, start: &Path) -> Option<PathBuf> {
    let here = start.canonicalize().ok()?;
    let target = plane.canonicalize().ok()?;
    here.ancestors()
        .find(|d| crate::plane::main_worktree_of(d).as_deref() == Some(target.as_path()))
        .map(Path::to_path_buf)
}

/// The plane nested inside `plane`'s `workspaces/` that `start` stands in, else `None`.
///
/// [`tree_of`]'s sibling and deliberately a SECOND route rather than a widening of it: a
/// linked worktree is marked by a `.git` file, and a nested plane is a different repository
/// with a real `.git` directory that happens to carry a tracked `charter.toml` — which every
/// clone of a plane does. `charter clone` puts clones at `workspaces/<ws>/<repo>`, and charter
/// dogfoods on a clone of itself, so this is an everyday layout rather than an exotic one.
///
/// Root-relative, exactly like [`tree_of`], and that is what makes `$CHARTER_ROOT` work: with
/// the plane being committed set to the one the caller stands in, this answers `None` and the
/// operator gets what they explicitly asked for.
pub fn nested_plane_in(plane: &Path, start: &Path) -> Option<PathBuf> {
    let here = start.canonicalize().ok()?;
    let target = plane.canonicalize().ok()?;
    let marked = here
        .ancestors()
        .find(|d| d.join(crate::plane::MANIFEST).is_file())?;
    let inner = crate::plane::plane_of(marked);
    // The chain is WALKED rather than shortcut through `outermost`: in a plane inside a plane
    // inside a plane, under `$CHARTER_ROOT=<the middle one>`, the outermost is not the tree
    // being committed, yet standing in the leaf still commits a tree the caller is not in.
    let mut cur = inner.clone();
    loop {
        let outer = crate::plane::enclosing(&cur)?;
        if outer == target {
            return Some(inner);
        }
        cur = outer;
    }
}

// --------------------------------------------------------------------------------------- //
// the command                                                                               //
// --------------------------------------------------------------------------------------- //

/// What `charter save` was asked for.
pub struct Request<'a> {
    pub root: &'a Path,
    /// The commit message, or `None` for `charter save: N file(s)`.
    pub message: Option<&'a str>,
    /// Sign the commit. Off by default so a signer prompt can never hang an agent.
    pub sign: bool,
    /// Commit only; do not push.
    pub no_push: bool,
    /// Where the operator is standing — what the two refusals below are about.
    pub cwd: &'a Path,
}

/// `charter save`. Returns the exit status.
pub fn save(request: &Request, say: Sink) -> u8 {
    commit_push(request, &["add", "-A"], say)
}

/// Outcome three of three: charter could not determine the tree's state, so it stops.
///
/// **The refusal is the feature.** `charter save` returning 0 is what every caller reads as "it
/// is saved", and charter #917 is entirely made of what happens downstream of believing that.
/// The word "clean" appears nowhere on this path: the whole defect was a sentence containing
/// it.
fn refuse_unreadable(root: &Path, git_dir: &Path, doing: &str, said: &str, say: Sink) -> u8 {
    say(Say::Fail(format!(
        "Refusing to save — charter could not read the state of {}, so it cannot tell an \
         unchanged tree from an unreadable one.",
        root.display()
    )));
    say(Say::Fail(format!(
        "  {doing}{}",
        if said.is_empty() {
            " without saying why".to_string()
        } else {
            format!(" — {said}")
        }
    )));
    match gitstate::find(git_dir) {
        Some(lock) => {
            say(Say::Info(format!("  {}", lock.describe())));
            for line in lock.remedy() {
                say(Say::Info(format!("  {line}")));
            }
        }
        // No lock, so charter has nothing to name and says so rather than inventing a remedy.
        None => say(Say::Info(format!(
            "  Nothing charter can name is holding this index. `git -C {} status` is where the \
             reason will be.",
            root.display()
        ))),
    }
    1
}

/// How many directory rows the breakdown prints before it summarises the rest.
const BREAKDOWN_ROWS: usize = 12;

/// Say what is about to be committed, grouped by directory — the check the 2026-09-12 incident
/// says is the whole check.
fn show_what_is_staged(root: &Path, staged: &[String], say: Sink) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for path in staged {
        let dir = match path.rsplit_once('/') {
            Some((dir, _)) => dir.to_string(),
            // A file at the top of the plane. Named rather than left under a bare `.`, because
            // a stray `uv.lock` at the root is exactly what this breakdown exists to surface.
            None => "(the plane root)".to_string(),
        };
        *counts.entry(dir).or_default() += 1;
    }
    say(Say::Warn(format!(
        "charter save commits everything pending in {} — {} file(s) across {} \
         directories, not only what you meant:",
        root.display(),
        staged.len(),
        counts.len()
    )));
    // Biggest first, ties by name, so the row that explains the count is the first one read.
    let mut rows: Vec<(&String, &usize)> = counts.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    for &(dir, count) in rows.iter().take(BREAKDOWN_ROWS) {
        // Contained: a path comes out of the operator's tree, and a control character in one
        // would otherwise redraw the line it is printed on.
        say(Say::Info(format!(
            "  {count:>5}  {}",
            shown::readable(dir, shown::DISPLAY_LIMIT)
        )));
    }
    if rows.len() > BREAKDOWN_ROWS {
        say(Say::Info(format!(
            "  … and {} more directories. The whole list: git -C {} diff --cached --name-only",
            rows.len() - BREAKDOWN_ROWS,
            root.display()
        )));
    }
}

fn commit_push(request: &Request, add_cmd: &[&str], say: Sink) -> u8 {
    let root = request.root;
    if stages_the_whole_tree(add_cmd) {
        // BEFORE the git-repo check and before any staging: a refusal that has already run
        // `git add -A` has done the damage and merely declined to name it.
        if let Some(tree) = tree_of(root, request.cwd) {
            let tree = tree.display();
            say(Say::Fail(format!(
                "Refusing to stage all of {} — you are standing in {tree}, a linked worktree of \
                 it. Committing every change in a tree you are not in would take that tree's \
                 uncommitted work under your message, and leave your own work here unsaved.",
                root.display()
            )));
            say(Say::Info(format!(
                "  your own work:   git -C {tree} add -A && git -C {tree} commit"
            )));
            say(Say::Info(format!(
                "  the plane's own: run `charter save` from {}",
                root.display()
            )));
            say(Say::Info(format!(
                "  you really do mean this tree: CHARTER_ROOT={tree} charter save"
            )));
            return 1;
        }
        // A SECOND ROUTE, not a widening of the first. A clone has a real `.git` directory, so
        // `tree_of` answers `None` here and must keep doing so. REFUSE rather than commit the
        // inner plane instead: committing the inner one would make `save` disagree with every
        // other command about which plane it acts on, and committing the outer one is the
        // defect. So charter names both trees and lets the caller say which they meant.
        if let Some(nested) = nested_plane_in(root, request.cwd) {
            let nested = nested.display();
            say(Say::Fail(format!(
                "Refusing to stage all of {} — you are standing in {nested}, a control plane of \
                 its own under that plane's workspaces/. charter resolves outward to the plane \
                 holding the vault, so committing every change there would take that tree's \
                 uncommitted work under your message, and leave your own work here unsaved.",
                root.display()
            )));
            say(Say::Info(format!(
                "  your own work:   git -C {nested} add -A && git -C {nested} commit"
            )));
            say(Say::Info(format!(
                "  the plane's own: run `charter save` from {}",
                root.display()
            )));
            say(Say::Info(format!(
                "  you really do mean this plane: CHARTER_ROOT={nested} charter save"
            )));
            return 1;
        }
    }

    // A plane is not always a git repo: `charter init` in a fresh directory does not run `git
    // init`, and that is the README's own 60-second path. Every git call below tolerates a
    // non-zero exit, so without this the add failed silently, the probe answered "no
    // difference", and charter printed `✓ Committed : charter save: 0 file(s)` over a plane
    // with no history at all.
    let found = git::run(root, &["rev-parse", "--git-dir"], git::READ);
    let Some(found) = found.ok().filter(git::Run::ok) else {
        say(Say::Fail(format!(
            "{} is not a git repository, so there is nothing to commit to.",
            root.display()
        )));
        say(Say::Info(
            "  charter init does not create one. Run: git init && git remote add origin <url>  \
             — personas and memory are meant to be committed and shared."
                .into(),
        ));
        return 1;
    };
    // Kept rather than thrown away with the return code it was asked for: it is where the
    // index lock lives, and for a linked worktree that is `<plane>/.git/worktrees/<name>`
    // rather than `<root>/.git` — the tree whose index this call is about.
    let named = found.line().trim();
    let git_dir = root.join(if named.is_empty() { ".git" } else { named });

    // **The add's exit status is the whole fix.** Under a held `.git/index.lock` it exits 128
    // and stages nothing, and the probe below then answers 0 — because there is no difference
    // between HEAD and an index nothing was written to. Discarded, that made "charter could not
    // stage anything" and "there was nothing to stage" the same value.
    let added = match git::run_untimed(root, add_cmd) {
        Ok(run) => run,
        Err(unavailable) => {
            return refuse_unreadable(
                root,
                &git_dir,
                &format!("`git {}` could not be run", add_cmd.join(" ")),
                &unavailable.to_string(),
                say,
            );
        }
    };
    if !added.ok() {
        return refuse_unreadable(
            root,
            &git_dir,
            &format!(
                "`git {}` exited {}",
                add_cmd.join(" "),
                added
                    .code
                    .map_or_else(|| "None".to_string(), |c| c.to_string())
            ),
            &gitstate::said(&added),
            say,
        );
    }

    // Three outcomes, and `diff --quiet` has always answered in three: 0 for no difference, 1
    // for a difference, anything else for a failure. Read as `== 0` / else, the third collapsed
    // into the second and reached `git commit` with a message about zero files.
    let probe = match git::run(root, &["diff", "--cached", "--quiet"], git::READ) {
        Ok(run) => run,
        Err(unavailable) => {
            return refuse_unreadable(
                root,
                &git_dir,
                "`git diff --cached --quiet` could not be run",
                &unavailable.to_string(),
                say,
            );
        }
    };
    match probe.code {
        Some(0) => {
            say(Say::Info(
                "Nothing to save — the control-plane working tree is clean.".into(),
            ));
            return 0;
        }
        Some(1) => {}
        other => {
            return refuse_unreadable(
                root,
                &git_dir,
                &format!(
                    "`git diff --cached --quiet` exited {}",
                    other.map_or_else(|| "None".to_string(), |c| c.to_string())
                ),
                &gitstate::said(&probe),
                say,
            );
        }
    }
    let staged: Vec<String> = git::run(root, &["diff", "--cached", "--name-only"], git::READ)
        .map(|r| {
            r.out
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    show_what_is_staged(root, &staged, say);

    // The secret guard: refuse if a staged memory/ref file looks like it holds a secret. A
    // memory is pushed to a shared repository, so a credential in one is disclosed the moment
    // the save lands.
    let mut flagged: Vec<(String, &'static str)> = Vec::new();
    for path in &staged {
        if !(path.contains("/memory/") || path.contains("/refs/")) {
            continue;
        }
        let file = root.join(path);
        // Stricter than Python, and the reason is charter #442's shape: a committed
        // `memory/x -> /etc/passwd` makes the guard read a file outside the plane. Charter
        // will not read through one, and a memory file it cannot read is not one it will
        // commit unexamined.
        if !crate::contain::within_plane(root, &file) {
            flagged.push((
                path.clone(),
                "it leads out of the plane, so charter will not read it",
            ));
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&file)
            && let Some(kind) = secretshape::secret_kind(&text)
        {
            flagged.push((path.clone(), kind));
        }
    }
    if !flagged.is_empty() {
        say(Say::Fail(
            "Refusing to save — a secret-shaped value in a memory/ref file:".into(),
        ));
        for (path, kind) in &flagged {
            say(Say::Fail(format!(
                "  {}  ({kind})",
                shown::readable(path, shown::DISPLAY_LIMIT)
            )));
        }
        say(Say::Info(
            "Secrets belong in the vault (`charter persona secret set`). Remove it, then retry."
                .into(),
        ));
        return 1;
    }

    let msg = match request.message {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => format!("charter save: {} file(s)", staged.len()),
    };
    // Unsigned by default so a signer never hangs; `--sign` opts in. `-c` on the command line
    // beats the operator's global `commit.gpgsign = true`, which is the whole point: their
    // preference keeps working everywhere else on their machine, and inside the plane the
    // command-line value wins.
    let unsigned = ["-c", "commit.gpgsign=false"];
    let mut commit: Vec<&str> = if request.sign {
        Vec::new()
    } else {
        unsigned.to_vec()
    };
    commit.extend(["commit", "-q", "-m", msg.as_str()]);
    let _ = git::run_untimed(root, &commit);
    let still_staged = |root: &Path| {
        git::run(root, &["diff", "--cached", "--quiet"], git::READ).is_ok_and(|r| !r.ok())
    };
    if still_staged(root) {
        // A signed commit that failed: retry without the signature rather than leaving the
        // work staged and the operator guessing.
        let _ = git::run_untimed(root, &["commit", "--no-gpg-sign", "-q", "-m", msg.as_str()]);
    }
    // Still staged after both attempts = nothing was committed. Reporting success here is how a
    // failed commit became `✓ Committed :` with an empty sha — the sha was empty precisely
    // BECAUSE there was no commit, and that was the only visible symptom.
    if still_staged(root) {
        say(Say::Fail(format!(
            "git commit failed — {} file(s) are staged but not committed.",
            staged.len()
        )));
        say(Say::Info(format!(
            "  Run `git -C {} status` to see why; charter has left them staged.",
            root.display()
        )));
        return 1;
    }
    let short = git::run(root, &["rev-parse", "--short", "HEAD"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    // Names the TREE. charter #806 lived for months because it did not: a commit landing in
    // the plane's clone while the operator stood in a worktree read exactly like a commit
    // landing where they meant.
    say(Say::Done(format!(
        "Committed {short} in {}: {msg}  ({} file(s))",
        root.display(),
        staged.len()
    )));

    if request.no_push {
        say(Say::Info("Skipped push (--no-push).".into()));
        return 0;
    }
    if origin_https(root).is_none() {
        say(Say::Warn(
            "origin isn't on a forge charter knows (gitlab.com/github.com/…) — committed \
             locally; push manually."
                .into(),
        ));
        return 0;
    }
    // rc 1 only when the commit reached NOWHERE — a pull-request branch that also failed. An
    // ordinary push failure has been reported and stays rc 0: `charter save` having committed
    // successfully is not a failed command.
    u8::from(push_head(root, say).outcome == Outcome::Stranded)
}

#[cfg(test)]
mod tests;
