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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
/// The differential therefore measured what `origin` resolved to before it ran the command,
/// and failed any scenario whose plane had a stand-in forge beside it and an origin that came
/// back `file://` without the scenario saying it meant that. Its scenarios are recorded now
/// (ADR 0046) and their setups cannot move; a NEW forge scenario has to make the same check by
/// hand: key `url.<local>.insteadOf` on the HTTPS base and leave `origin` in the SSH form.
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
/// A plain HTTPS link, deliberately: it closes the pull-request-gated workflow with no API call
/// and no extra token scope. [`forge::pr`] can now open the PR itself, into an explicit base;
/// the save's PR modes replace this link with it (ADR 0051, #298). Which form to build is decided by RESOLVING the forge, never by
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
    //
    // The state directory is what the containment walk is ROOTED at, for that same reason:
    // it is the deepest thing here charter already trusts, `private_dir` has just refused it
    // as a link, and the walk then covers the temp file the bytes actually land on
    // (charter-app#113).
    if let Some(dir) = path.parent()
        && crate::profiletrust::private_dir(dir).is_ok()
    {
        let _ = crate::profiletrust::write_private(
            dir,
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

// --------------------------------------------------------------------------------------- //
// where unsaved work sits                                                                   //
// --------------------------------------------------------------------------------------- //

/// Where a plane's unsaved work sits, furthest back first (ADR 0051). The Saving view and the
/// title bar show the furthest-back one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// A save cannot go further without a person: the last push conflicted or failed, and
    /// its commit has not landed since.
    Blocked,
    /// Files the next save would commit.
    Changed,
    /// Commits the remote does not have yet.
    Committed,
    /// Pushed, and waiting on a pull request.
    PrOpen,
    /// On the target branch.
    Saved,
}

impl Stage {
    pub fn word(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Changed => "changed",
            Self::Committed => "committed",
            Self::PrOpen => "pr-open",
            Self::Saved => "saved",
        }
    }
}

/// A plane's save standing, read from git and the push record — never from the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub stage: Stage,
    /// What the next save would commit: tracked changes and new files git does not ignore,
    /// as `git add -A` would take them, sorted.
    pub changed: Vec<String>,
    /// Commits on HEAD that the remote-tracking branch does not have. `None` when there is no
    /// remote-tracking branch to count against.
    pub ahead: Option<u32>,
    /// The pull request a pushed commit is waiting on.
    pub pr: Option<String>,
    /// Why the save is blocked, in git's own words.
    pub blocked: Option<String>,
    /// The target branch commits are counted against: `[plane] branch`, or the one HEAD is on.
    pub branch: String,
    /// Whether a save would push: a mode that goes past the commit, and an origin on a forge
    /// charter knows. When it would not, a commit is as far as a save goes, and a plane with
    /// nothing left to commit is saved.
    pub pushes: bool,
    /// Commits the remote-tracking branch has that HEAD does not: what the last fetch brought
    /// in and has not been fast-forwarded to. `None` when there is nothing to count against.
    pub behind: Option<u32>,
    /// Why the last push did not land, when it failed rather than conflicted — offline, a
    /// token, the forge down. Not blocked: auto-save tries again later.
    pub push_failed: Option<String>,
}

/// Where the plane at `root`'s unsaved work sits.
pub fn standing(root: &Path) -> Standing {
    let plane = crate::planesave::Settings::read(root).plane;
    let here = git::run(root, &["rev-parse", "--abbrev-ref", "HEAD"], git::READ)
        .ok()
        .filter(git::Run::ok)
        .map(|r| r.line().trim().to_string());
    let Some(here) = here else {
        return Standing {
            stage: Stage::Blocked,
            changed: Vec::new(),
            ahead: None,
            pr: None,
            blocked: Some(
                "this plane is not a git repository, so there is nothing to commit to".into(),
            ),
            branch: String::new(),
            pushes: false,
            behind: None,
            push_failed: None,
        };
    };
    let branch = plane.branch.value.clone().unwrap_or(here);
    let pushes = matches!(plane.mode.value, None | Some(crate::planesave::Mode::Push))
        && origin_https(root).is_some();
    let changed = changed_paths(root);
    let ahead = count(root, &format!("refs/remotes/origin/{branch}..HEAD"));
    let behind = count(root, &format!("HEAD..refs/remotes/origin/{branch}"));
    let record = unlanded(root);
    let said = |key: &str| {
        record
            .as_ref()
            .and_then(|r| r.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let outcome = said("outcome");
    let pr = (outcome.as_deref() == Some(Outcome::Branched.word()))
        .then(|| said("url"))
        .flatten();
    let push_failed = (outcome.as_deref() == Some(Outcome::Failed.word()))
        .then(|| said("detail").unwrap_or_else(|| "the push failed".into()));
    let blocked = matches!(outcome.as_deref(), Some("conflict" | "stranded"))
        .then(|| said("detail").unwrap_or_else(|| outcome.clone().unwrap_or_default()));
    let stage = if blocked.is_some() {
        Stage::Blocked
    } else if !changed.is_empty() {
        Stage::Changed
    } else if pr.is_some() {
        Stage::PrOpen
    } else if pushes && ahead != Some(0) {
        // Nothing to count against is not nothing unpushed: say committed, never saved.
        Stage::Committed
    } else {
        Stage::Saved
    };
    Standing {
        stage,
        changed,
        ahead,
        pr,
        blocked,
        branch,
        pushes,
        behind,
        push_failed,
    }
}

/// `git rev-list --count <range>`, or `None` when git cannot count it — a ref that is not there.
fn count(root: &Path, range: &str) -> Option<u32> {
    git::run(root, &["rev-list", "--count", range], git::READ)
        .ok()
        .filter(git::Run::ok)
        .and_then(|r| r.line().trim().parse().ok())
}

/// What is unsaved in the plane, as one string that changes whenever it does: HEAD, the
/// commits the remote lacks, and each changed file with its size and modification time — so a
/// file written again is a change even when git's one-letter status for it is not.
///
/// Auto-save's quiet period is measured from the last time this changed
/// ([`crate::autosave::Quiet`]).
pub fn fingerprint(root: &Path) -> String {
    fingerprint_of(root, &standing(root))
}

/// [`fingerprint`], from a [`Standing`] already read — so a look at the plane asks git for its
/// status once, not twice.
pub fn fingerprint_of(root: &Path, standing: &Standing) -> String {
    let head = git::run(root, &["rev-parse", "HEAD"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    let mut out = format!("{head}\0{:?}", standing.ahead);
    for path in &standing.changed {
        let seen = std::fs::symlink_metadata(root.join(path)).ok().map(|meta| {
            let at = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            (meta.len(), at)
        });
        out.push('\0');
        out.push_str(path);
        out.push_str(&format!("{seen:?}"));
    }
    out
}

/// What a fetch of the plane's target branch found, and whether the plane was moved onto it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    /// Commits the fetch found on the remote that the plane did not have.
    pub behind: u32,
    /// Whether the plane was fast-forwarded onto them.
    pub moved: bool,
}

/// Fetch the plane's target branch, and — when `fast_forward` (auto-save is on) — move onto it
/// when that cannot touch anyone's work (charter-app#296): a clean tree, on the target branch, with nothing of its own the
/// remote lacks, and no merge or rebase in progress. Otherwise the plane is left exactly as it
/// is, and the next save's rebase brings the commits in.
///
/// `Err` when there is nothing to fetch from — no origin on a forge charter knows — or the
/// fetch failed, in git's words.
pub fn fetch(root: &Path, fast_forward: bool) -> Result<Incoming, String> {
    // A fetch moves refs and may move the tree: never while a save of the plane runs.
    let _claim = Claim::of(root).ok_or(ALREADY_SAVING)?;
    let https = origin_https(root).ok_or("origin is not on a forge charter knows")?;
    let plane = crate::planesave::Settings::read(root).plane;
    let here = git::run(root, &["rev-parse", "--abbrev-ref", "HEAD"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    let branch = plane.branch.value.clone().unwrap_or_else(|| here.clone());
    let forge = crate::gitpolicy::forge_for(root, root)
        .unwrap_or_else(|| Forge::default_of(forge::DEFAULT_KIND));
    let helper = forge::helper_for(&forge);
    let fetched = git::run_network(
        root,
        Some(helper.as_str()),
        &[
            "fetch",
            "--no-recurse-submodules",
            // A save's rebase reads FETCH_HEAD; a fetch of charter's own leaves it alone.
            "--no-write-fetch-head",
            &https,
            &format!("+refs/heads/{branch}:refs/remotes/origin/{branch}"),
        ],
    )
    .map_err(|unavailable| unavailable.to_string())?;
    if !fetched.ok() {
        return Err(tail(&fetched));
    }
    let behind = count(root, &format!("HEAD..refs/remotes/origin/{branch}")).unwrap_or(0);
    let mine = count(root, &format!("refs/remotes/origin/{branch}..HEAD")).unwrap_or(0);
    // The repository's own directory, which is a file's target for a worktree plane.
    let git_dir = git::run(root, &["rev-parse", "--absolute-git-dir"], git::READ)
        .ok()
        .filter(git::Run::ok)
        .map_or_else(|| root.join(".git"), |r| PathBuf::from(r.line().trim()));
    let can_move = fast_forward
        && behind > 0
        && here == branch
        && mine == 0
        && changed_paths(root).is_empty()
        && crate::gitstate::find(&git_dir).is_none()
        && ![
            "MERGE_HEAD",
            "REBASE_HEAD",
            "CHERRY_PICK_HEAD",
            "rebase-merge",
            "rebase-apply",
        ]
        .iter()
        .any(|marker| git_dir.join(marker).exists());
    let moved = can_move
        // Untimed: a merge checks out a tree, and a killed one leaves it half-written.
        && git::run_untimed(
            root,
            &[
                "merge",
                "--ff-only",
                "--no-overwrite-ignore",
                &format!("refs/remotes/origin/{branch}"),
            ],
        )
        .is_ok_and(|run| run.ok());
    Ok(Incoming { behind, moved })
}

/// `git status --porcelain=v1 -z`'s paths: what `git add -A` would take. `-z` for the same
/// reason the save's secret guard uses it — no quoting, NUL the only separator.
fn changed_paths(root: &Path) -> Vec<String> {
    let Ok(run) = git::run(
        root,
        // `--no-optional-locks`, as `profiles.rs` and `guest.rs` ask: a plain `status`
        // refreshes the index and takes `index.lock`, and the title bar asks this every ten
        // seconds — a save's `git add -A` landing inside that window failed on the lock.
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
        git::READ,
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut entries = run.out.split('\0');
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (status, path) = entry.split_at(3);
        out.push(path.to_string());
        // A rename or copy names where it came from next, as an entry of its own.
        if status.contains(['R', 'C']) {
            entries.next();
        }
    }
    out.sort();
    out
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

/// The deadline for every git call `save` makes that writes a commit — the commit, its unsigned
/// retry, the rebase a moved remote sends it through — and for the rebase's abort.
///
/// Not [`git::READ`]: a rebase checks out a tree, and a killed one leaves a rebase in progress
/// for the abort to clear. But not untimed either, which each of them was until
/// charter-app#242: a git that waits — on a signer, on a lock, on anything — held
/// `charter save`, and the hook that ran it, for ever. A plane's save commits a handful of
/// files and replays the few commits the remote does not have yet, so two minutes is ample
/// and still ends.
const WRITE: Duration = Duration::from_secs(120);

/// How the rebase onto a remote that moved ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Rebased {
    /// The plane's commits were replayed onto the remote's.
    Replayed,
    /// git stopped on its own — the trees conflict, it refused, or it died.
    Conflict,
    /// Asked to sign (`--sign`), git could not write a replayed commit: the signer refused.
    /// Carries what the signer said (charter-app#267).
    Unsigned(String),
    /// Still running at its deadline, and killed.
    OutOfTime,
}

/// Rebase the plane root onto `FETCH_HEAD`, giving up at `deadline`.
///
/// **Signed exactly when the commit was** (charter-app#242). A rebase REPLAYS commits and a
/// replay is a commit, so it reads `commit.gpgsign` as the commit did. Unless the operator
/// asked for `--sign`, `-c commit.gpgsign=false` keeps it from the signer the commit was kept
/// from — the commit `save` made unsigned came back out of the rebase asking for a 1Password
/// prompt that blocks, or a signer that fails and turns a remote that merely moved into a
/// reported conflict. With `--sign` the `-c` is left off, as it is for the commit, so the
/// replay is signed the way the commit was; putting it on would hand the remote an unsigned
/// copy of a commit the operator asked to sign.
///
/// **A signer that refuses is not a conflict** (charter-app#267). Under `--sign` a replay that
/// cannot be signed stops the rebase exactly where a conflict would, with no conflict in it:
/// git says `failed to write commit object` (in the C locale the runner pins) after the
/// signer's own words, and a conflict never says that. What the signer said is kept for the
/// operator; which signer, and why, only it knows. A commit object git failed to write for
/// another reason — a full disk — would be put down to the signer too, but git's own words
/// follow the claim, so the operator reads the real cause either way.
///
/// A run with no exit code is out of time only once the deadline has passed: git killed by a
/// signal of somebody else's has no code either, and that is not charter stopping it.
fn rebase_onto_fetched(root: &Path, sign: bool, deadline: Duration) -> Rebased {
    let started = std::time::Instant::now();
    let mut rebase: Vec<&str> = vec!["-c", gpgsign(sign)];
    rebase.extend(["rebase", "FETCH_HEAD"]);
    match git::run(root, &rebase, deadline) {
        Ok(run) if run.ok() => Rebased::Replayed,
        Ok(run) if run.code.is_none() && started.elapsed() >= deadline => Rebased::OutOfTime,
        Ok(run) if sign && run.err.contains(UNWRITTEN) => Rebased::Unsigned(signer_said(&run.err)),
        _ => Rebased::Conflict,
    }
}

/// The `-c` that decides signing for one git command, beating the operator's own
/// `commit.gpgsign` either way: a save that is not asked to sign never runs a signer, and one
/// that is — `--sign`, or `[plane] sign = true` (ADR 0051) — is signed whatever the machine's
/// default says.
fn gpgsign(sign: bool) -> &'static str {
    if sign {
        "commit.gpgsign=true"
    } else {
        "commit.gpgsign=false"
    }
}

/// What git's sequencer says when it cannot write a replayed commit — under `--sign`, because
/// the signer refused. `sequencer.c`'s own message, read in the C locale.
const UNWRITTEN: &str = "failed to write commit object";

/// The signer's words out of a rebase that stopped on it: everything git printed before
/// [`UNWRITTEN`], without its progress, its hints or its `error: ` prefixes.
///
/// git prints `Rebasing (1/1)` with a carriage return and no newline, so the line the signer's
/// error lands on starts with it; a line is read from its last `\r`.
fn signer_said(err: &str) -> String {
    let before = err.split(UNWRITTEN).next().unwrap_or_default();
    let said: Vec<&str> = before
        .lines()
        .map(|line| line.rsplit('\r').next().unwrap_or_default().trim())
        .map(|line| line.strip_prefix("error: ").unwrap_or(line))
        .filter(|line| !line.is_empty() && !line.starts_with("hint:") && *line != "error:")
        .collect();
    if said.is_empty() {
        "the signer gave no reason".to_string()
    } else {
        said.join(" ")
    }
}

/// A rebase that stopped for a reason other than a conflict, undone: aborted, the operator
/// told `detail` and what to do `next`, and recorded as `failed` with that detail — the
/// commit is on this laptop only.
fn rebase_undone(
    root: &Path,
    branch: &str,
    head: &str,
    detail: String,
    next: &str,
    say: Sink,
) -> PushResult {
    let _ = git::run(root, &["rebase", "--abort"], WRITE);
    say(Say::Warn(format!("Committed locally, but {detail}.")));
    say(Say::Info(format!("  {next}")));
    record_push(
        root,
        PushResult {
            detail,
            ..PushResult::of(Outcome::Failed, branch)
        },
        head,
    )
}

/// Push the plane root's HEAD to its own branch on origin — **the one pusher**.
///
/// **The branch is never predicted.** Nothing here asks the forge whether the branch is
/// protected before committing: charter cannot know that without a network call it has no
/// business making from a hook, and guessing it from the branch name is the unearned diagnosis
/// ADR 0009 forbids. The rejection IS the evidence, and it arrives only after the commit
/// exists — so the commit is made, and the OUTCOME is what gets reported honestly.
///
/// `sign` is the `--sign` the commit was made under: a remote that moved sends HEAD through a
/// rebase, and the commit it replays is signed exactly when the commit was.
///
/// `target` is the branch to advance: `[plane] branch`, or `None` for the one HEAD is on.
pub fn push_head(root: &Path, target: Option<&str>, sign: bool, say: Sink) -> PushResult {
    push_head_within(root, target, sign, WRITE, say)
}

/// [`push_head`], with the rebase given `deadline` rather than [`WRITE`] — so a test can drive
/// a rebase that runs out of time without waiting out the real one.
fn push_head_within(
    root: &Path,
    target: Option<&str>,
    sign: bool,
    deadline: Duration,
    say: Sink,
) -> PushResult {
    let branch = match target {
        Some(target) => target.to_string(),
        None => git::run(root, &["rev-parse", "--abbrev-ref", "HEAD"], git::READ)
            .map(|r| r.line().trim().to_string())
            .unwrap_or_default(),
    };
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
        } else {
            match rebase_onto_fetched(root, sign, deadline) {
                Rebased::Replayed => {}
                Rebased::Conflict => {
                    let _ = git::run(root, &["rebase", "--abort"], WRITE);
                    say(Say::Warn(
                        "Committed locally, but rebase hit a conflict — resolve manually, then \
                         `charter save`."
                            .into(),
                    ));
                    return record_push(root, PushResult::of(Outcome::Conflict, &branch), &head);
                }
                Rebased::Unsigned(signer) => {
                    // Stopped, not retried unsigned as the commit is: the commit stays on this
                    // laptop, where re-signing it is the operator's to do, while a push is
                    // public and final. The remote never gets an unsigned copy of a commit the
                    // operator asked to sign.
                    return rebase_undone(
                        root,
                        &branch,
                        &head,
                        format!("the rebase could not sign the replayed commit: {signer}"),
                        "Nothing was pushed, so nothing unsigned reached the remote. Fix the \
                         signer, then rebase onto the remote and push by hand.",
                        say,
                    );
                }
                Rebased::OutOfTime => {
                    // Not a conflict, and not called one: nothing says the trees disagree,
                    // only that git did not finish. The push that started this is what failed
                    // to land.
                    return rebase_undone(
                        root,
                        &branch,
                        &head,
                        format!(
                            "the rebase onto the remote did not finish within {} seconds, so \
                             charter stopped it",
                            deadline.as_secs()
                        ),
                        "Rebase by hand, then `charter save`.",
                        say,
                    );
                }
            }
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

/// `charter save`: [`save_as`] started from the command line.
pub fn save(request: &Request, say: Sink) -> u8 {
    save_as(request, Trigger::Cli, say)
}

/// The planes a save — or a fetch — is running in, in this process, so a second one waits its
/// turn in words rather than racing the first for git's index lock (charter-app#294, #296).
static RUNNING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

/// A save or a fetch running in one plane, for as long as it is held.
#[derive(Debug)]
pub struct Claim(PathBuf);

impl Claim {
    /// The plane at `root`, unless something already holds it.
    pub fn of(root: &Path) -> Option<Self> {
        let mut running = RUNNING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        running
            .insert(root.to_path_buf())
            .then(|| Self(root.to_path_buf()))
    }

    /// The plane at `root`, waiting up to `bound` for whatever holds it to let go.
    pub fn within(root: &Path, bound: Duration) -> Option<Self> {
        let until = std::time::Instant::now() + bound;
        loop {
            if let Some(claim) = Self::of(root) {
                return Some(claim);
            }
            if std::time::Instant::now() >= until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        RUNNING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.0);
    }
}

/// The sentence a save refused for [`Claim`] says.
pub const ALREADY_SAVING: &str = "A save of this plane is already running — wait for it to finish.";

/// What started a save — the journal's `trigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The window's save button.
    Manual,
    /// Auto-save, after the quiet period.
    Quiet,
    /// Auto-save, when a session ended.
    SessionEnd,
    /// Auto-save, as the app quit.
    Quit,
    /// A launch pushing what the last run left unpushed.
    Launch,
    /// `charter save`.
    Cli,
    /// A workspace going LIVE or LOCAL.
    Live,
}

impl Trigger {
    pub fn word(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Quiet => "quiet",
            Self::SessionEnd => "session-end",
            Self::Quit => "quit",
            Self::Launch => "launch",
            Self::Cli => "cli",
            Self::Live => "live",
        }
    }
}

/// Every save of the plane: the one save function (ADR 0051). Returns the exit status, and
/// leaves one line in the save journal ([`journal`]) saying how it ended.
///
/// How far it goes is `[plane] mode` ([`crate::planesave`]). A plane that names no mode is
/// saved as `charter save` always saved it — committed and pushed — because a bare command is
/// the operator asking for exactly that; only the app asks such a plane which mode it wants.
pub fn save_as(request: &Request, trigger: Trigger, say: Sink) -> u8 {
    let Some(claim) = Claim::of(request.root) else {
        say(Say::Fail(ALREADY_SAVING.into()));
        return 1;
    };
    save_claimed(request, trigger, &claim, say)
}

/// [`save_as`], for a caller that already holds the plane's [`Claim`].
pub fn save_claimed(request: &Request, trigger: Trigger, _claim: &Claim, say: Sink) -> u8 {
    let started = std::time::Instant::now();
    let plane = crate::planesave::Settings::read(request.root).plane;
    let mut attempt = Attempt::default();
    // The first thing the save refused with, in its own words, for a journal line that would
    // otherwise say only "failed".
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
    let code = if plane.mode.value == Some(crate::planesave::Mode::Off) {
        say(Say::Info(format!(
            "[plane] mode is off ({}), so charter commits nothing here — commit with git \
             yourself, or set another mode.",
            plane.mode.source.file().unwrap_or("default")
        )));
        attempt.outcome = "skipped";
        0
    } else {
        commit_push(request, &plane, &["add", "-A"], &mut attempt, say)
    };
    if attempt.detail.is_empty()
        && let Some(refused) = refused
    {
        attempt.detail = refused;
    }
    let mode = match (request.no_push, plane.mode.value) {
        (true, Some(crate::planesave::Mode::Off)) | (false, _) => plane.mode.value,
        (true, _) => Some(crate::planesave::Mode::Commit),
    };
    journal_append(
        request.root,
        &serde_json::json!({
            "at": now(),
            "target": "plane",
            "trigger": trigger.word(),
            "mode": mode.unwrap_or(crate::planesave::Mode::Push).as_str(),
            "files": attempt.files,
            "commit": attempt.commit,
            "pr": attempt.pr,
            "ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "outcome": attempt.outcome,
            "detail": attempt.detail,
        }),
    );
    code
}

/// What one save attempt came to, filled in as it goes — the journal's line.
#[derive(Debug)]
struct Attempt {
    /// `saved`, `committed`, `pr-open`, `blocked`, `skipped` or `failed`.
    outcome: &'static str,
    files: usize,
    commit: Option<String>,
    pr: Option<String>,
    detail: String,
}

impl Default for Attempt {
    fn default() -> Self {
        Self {
            // Every path that is not a failure says what it was, so one that says nothing is.
            outcome: "failed",
            files: 0,
            commit: None,
            pr: None,
            detail: String::new(),
        }
    }
}

/// The most lines the journal keeps. The Saving view shows the newest fifty.
const JOURNAL_LINES: usize = 500;

/// Where the save journal lives: beside the push record, in the plane's state directory.
pub fn journal_path(root: &Path) -> PathBuf {
    crate::plane::state_dir(root).join("save-journal.jsonl")
}

/// The save journal, oldest first. A line that is not JSON is left out.
pub fn journal(root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(journal_path(root))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Add `entry` to the journal, keeping the newest [`JOURNAL_LINES`]. Never fails loudly: a save
/// that cannot write its note must still have saved.
///
/// Read, then renamed over, with no lock: two saves finishing at the same instant — the app's
/// auto-save and a `charter save` — can lose one line between them. The file is never torn,
/// because the rename is atomic, and a lost line costs one row of the Saving view's history.
fn journal_append(root: &Path, entry: &serde_json::Value) {
    let path = journal_path(root);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let entry = entry.to_string();
    lines.push(&entry);
    let keep = &lines[lines.len().saturating_sub(JOURNAL_LINES)..];
    let mut out = keep.join("\n");
    out.push('\n');
    // As the push record is written: into the state directory `private_dir` has refused as a
    // link, beside the file and renamed over it.
    if let Some(dir) = path.parent()
        && crate::profiletrust::private_dir(dir).is_ok()
    {
        let _ = crate::profiletrust::write_private(dir, &path, out.as_bytes());
    }
}

/// Seconds since the epoch, as the push record writes them.
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
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

/// How many groups a generated message names before it says how many more there are.
const SUMMARY_GROUPS: usize = 4;

/// The message a save writes when it is given none: how many files, and what they are in the
/// plane's own words — `charter save: 3 files (steward memory 2, ide todos 1)` — biggest group
/// first, ties by name.
fn summary(staged: &[String]) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for path in staged {
        *counts.entry(group_of(path)).or_default() += 1;
    }
    let mut groups: Vec<(String, usize)> = counts.into_iter().collect();
    groups.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut named: Vec<String> = groups
        .iter()
        .take(SUMMARY_GROUPS)
        .map(|(group, n)| format!("{group} {n}"))
        .collect();
    if groups.len() > SUMMARY_GROUPS {
        named.push(format!("+{} more", groups.len() - SUMMARY_GROUPS));
    }
    let files = if staged.len() == 1 { "file" } else { "files" };
    format!(
        "charter save: {} {files} ({})",
        staged.len(),
        named.join(", ")
    )
}

/// What a plane path is part of, as a person would name it: a persona's memory, a
/// workspace's todos, the dispatch log — or, for anything else, its top directory.
fn group_of(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["personas", "_dispatch", ..] => "dispatch".into(),
        ["personas", "_skills", ..] => "skills".into(),
        ["personas", who, kind @ ("memory" | "refs"), _, ..] => format!("{who} {kind}"),
        ["personas", who, _, ..] => format!("{who} persona"),
        [
            "workspaces",
            ws,
            kind @ ("memory" | "todos" | "changes" | "pieces" | "refs"),
            _,
            ..,
        ] => {
            format!("{ws} {kind}")
        }
        ["workspaces", ws, _, ..] => format!("{ws} workspace"),
        [only] => (*only).to_string(),
        [top, ..] => (*top).to_string(),
        [] => String::new(),
    }
}

fn commit_push(
    request: &Request,
    plane: &crate::planesave::Plane,
    add_cmd: &[&str],
    attempt: &mut Attempt,
    say: Sink,
) -> u8 {
    let root = request.root;
    // `--sign` asks for it this once; `[plane] sign` asks for it every time.
    let sign = request.sign || plane.sign.value;
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
            attempt.outcome = "skipped";
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
    // **`-z`, and the reason is the secret guard below** (M3). Without it git applies
    // `core.quotePath`, which is on by default: a staged `personas/café/memory/note.md`
    // comes back as `"personas/caf\303\251/memory/note.md"` — surrounding quotes and octal
    // escapes included. The guard still SELECTS that row — the substring test sees
    // `/memory/` — and then looks for a file under a name that does not exist, finds
    // nothing, and lets the row through. A path with a `"`, a backslash, a newline or any
    // byte over 0x7f walks past the guard that way. `-z` turns the quoting off and NUL is
    // then the only separator, which is also the only one a filename cannot hold.
    let staged: Vec<String> = git::run(root, &["diff", "--cached", "--name-only", "-z"], git::READ)
        .map(|r| {
            r.out
                .split('\0')
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    attempt.files = staged.len();
    show_what_is_staged(root, &staged, say);

    // The secret guard: refuse if a staged memory/ref file looks like it holds a secret. A
    // memory is pushed to a shared repository, so a credential in one is disclosed the moment
    // the save lands.
    //
    // **It reads the STAGED BLOB, which is the thing about to be committed** (M3). Reading
    // the working-tree file instead asked about bytes that need not be the bytes of the
    // commit, and the gap is not theoretical — measured with git 2.50.1:
    //
    //     git add <memory file with the secret in it>
    //     git update-index --skip-worktree <that file>     # `add -A` now skips it
    //     printf 'clean\n' > <that file>
    //
    // leaves the secret in the index, `clean` on disk, the row still listed by
    // `diff --cached --name-only`, and the guard reading `clean`. `git show :<path>` asks
    // the index instead, so there is no second copy of the file to disagree with. It also
    // answers for a row with no file on disk at all, which the working-tree read could only
    // skip.
    // A path the save DELETES carries no content to disclose, and has no staged blob to read:
    // asked for one, the guard below would refuse every save that removes a memory — which is
    // what going LOCAL does to a workspace's (charter-app#301).
    let deleted: std::collections::HashSet<String> = git::run(
        root,
        &["diff", "--cached", "--name-only", "-z", "--diff-filter=D"],
        git::READ,
    )
    .map(|r| {
        r.out
            .split('\0')
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default();
    let mut flagged: Vec<(String, &'static str)> = Vec::new();
    for path in &staged {
        if !(path.contains("/memory/") || path.contains("/refs/")) || deleted.contains(path) {
            continue;
        }
        let file = root.join(path);
        // charter #442's shape, kept and asked first so its own sentence is the one a
        // committed `memory/x -> /etc/passwd` gets: charter will not read through a link
        // that leaves the plane, and a memory file it cannot read is not one it commits.
        if !crate::contain::within_plane(root, &file) {
            flagged.push((
                path.clone(),
                "it leads out of the plane, so charter will not read it",
            ));
            continue;
        }
        // A link that lands back INSIDE the plane is refused too, and the blob is why: what
        // a save commits for a link is the link — a blob holding the target's path — so
        // scanning that blob would answer "no secret" about a file charter never read.
        // Following it instead would be a guard whose answer is about bytes the commit does
        // not carry, which is the whole defect this loop was changed to close.
        if std::fs::symlink_metadata(&file).is_ok_and(|found| found.file_type().is_symlink()) {
            flagged.push((
                path.clone(),
                "it is a link, and what a save commits for one is the link rather than the \
                 text charter would have read",
            ));
            continue;
        }
        // `:<path>` is the index's own blob for that path, resolved from the top of the
        // tree — which is what `--name-only` printed, and what `-C root` puts git in.
        let staged_blob = format!(":{path}");
        match git::run(root, &["show", &staged_blob], git::READ) {
            Ok(run) if run.ok() => {
                if let Some(kind) = secretshape::secret_kind(&run.out) {
                    flagged.push((path.clone(), kind));
                }
            }
            // A row charter could not read what is staged for is not one it commits
            // unexamined — the same rule as the link above, one source along. Before this,
            // an unreadable row fell through to no flag at all, which is the one direction
            // a guard may not fail in.
            //
            // **No test reaches this arm, and that is measured rather than assumed.** The
            // obvious way to make one — `update-index --cacheinfo` with a sha that is not
            // in the object store — does not survive the `add -A` this command runs first:
            // git stages the DELETION of a path with no file on disk, so the row is not
            // listed at all. What is left that reaches here is a `git show` that fails for
            // a reason a fixture cannot manufacture: a corrupt object store, the deadline,
            // git gone from PATH mid-command. Deleting this arm therefore turns nothing
            // red, and the next person to mutate it should know that before they conclude
            // it is dead code.
            _ => flagged.push((
                path.clone(),
                "charter could not read what is staged for it, so it will not commit it \
                 unexamined",
            )),
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
        attempt.outcome = "blocked";
        attempt.detail = "a secret-shaped value in a memory or ref file".into();
        return 1;
    }

    let msg = match request.message {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => summary(&staged),
    };
    // Unsigned by default so a signer never hangs; `--sign` opts in. `-c` on the command line
    // beats the operator's global `commit.gpgsign = true`, which is the whole point: their
    // preference keeps working everywhere else on their machine, and inside the plane the
    // command-line value wins.
    let mut commit: Vec<&str> = vec!["-c", gpgsign(sign)];
    commit.extend(["commit", "-q", "-m", msg.as_str()]);
    let committed = git::run(root, &commit, WRITE);
    let still_staged = |root: &Path| {
        git::run(root, &["diff", "--cached", "--quiet"], git::READ).is_ok_and(|r| !r.ok())
    };
    if sign && still_staged(root) {
        // Asked to sign — `--sign`, or `[plane] sign` — and the signer refused. There is no
        // unsigned retry: an unsigned commit is exactly what the operator asked charter not to
        // make, and the next push would carry it to the remote (ADR 0051).
        let signer = committed
            .map(|run| signer_said(&run.err))
            .unwrap_or_default();
        let why = if signer.is_empty() {
            "charter could not sign the commit".to_string()
        } else {
            format!("charter could not sign the commit: {signer}")
        };
        say(Say::Fail(format!(
            "{why} — {} file(s) are staged but not committed.",
            staged.len()
        )));
        say(Say::Info(
            "  Fix the signer, or set sign = false under [plane], then save again.".into(),
        ));
        attempt.detail = why;
        return 1;
    }
    // Still staged = nothing was committed. Reporting success here is how a
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
    attempt.outcome = "committed";
    attempt.commit = git::run(root, &["rev-parse", "HEAD"], git::READ)
        .ok()
        .map(|r| r.line().trim().to_string())
        .filter(|sha| !sha.is_empty());
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
    let file = plane.mode.source.file().unwrap_or("default");
    let key = if plane.from_share {
        "[memory] share"
    } else {
        "[plane] mode"
    };
    match plane.mode.value {
        Some(crate::planesave::Mode::Commit) => {
            say(Say::Info(format!("Not pushed: {key} is commit ({file}).")));
            return 0;
        }
        Some(mode @ (crate::planesave::Mode::Pr | crate::planesave::Mode::PrMerge)) => {
            // charter-app#298 builds the save branch and the pull request. Until then, the
            // one push a PR mode must never make is to the target branch.
            say(Say::Info(format!(
                "Not pushed: {key} is {} ({file}), and this charter cannot open the pull \
                 request yet.",
                mode.as_str()
            )));
            return 0;
        }
        _ => {}
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
    let target = plane.branch.value.as_deref();
    if let Some(target) = target {
        // Pushing HEAD to a branch it is not on would rebase THIS branch onto that one the
        // moment the remote moved — replaying its history there and rewriting it here. Nothing
        // rewrites anyone's history (ADR 0051): the commit stays, and the operator says which
        // branch they meant.
        let here = git::run(root, &["rev-parse", "--abbrev-ref", "HEAD"], git::READ)
            .map(|r| r.line().trim().to_string())
            .unwrap_or_default();
        if here != target {
            let why = format!("this plane is on {here}, and [plane] branch is {target}");
            say(Say::Warn(format!(
                "Not pushed: {why} ({}).",
                plane.branch.source.file().unwrap_or("default")
            )));
            say(Say::Info(format!(
                "  Check out {target}, or change [plane] branch, then save again."
            )));
            attempt.outcome = "blocked";
            attempt.detail = why;
            return 0;
        }
    }
    let pushed = push_head(root, target, sign, say);
    // The push may have rebased: the commit that landed is HEAD now.
    attempt.commit = git::run(root, &["rev-parse", "HEAD"], git::READ)
        .ok()
        .map(|r| r.line().trim().to_string())
        .filter(|sha| !sha.is_empty());
    attempt.outcome = match pushed.outcome {
        Outcome::Pushed => "saved",
        Outcome::Branched => "pr-open",
        Outcome::Unreachable => "committed",
        Outcome::Conflict => "blocked",
        Outcome::Stranded | Outcome::Failed => "failed",
    };
    attempt.pr = pushed.url.clone();
    attempt.detail = pushed.detail.clone();
    u8::from(pushed.outcome == Outcome::Stranded)
}

#[cfg(test)]
mod tests;
