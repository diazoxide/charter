//! Opening or updating a pull request — a merge request on GitLab — and asking the forge to
//! merge it once it may. The forge adapters' first write that is not a push (ADR 0051).
//!
//! Two calls, the same on both forges:
//!
//! - `open_or_update`: the open PR from `head` into `base`, updated with a new title and body,
//!   or a new one when there is none. Never a second one for the same pair.
//! - `request_auto_merge`: the forge queues it to merge when its checks allow, by the
//!   repo's own method, preferring rebase, then a merge commit, then squash — and only at the
//!   commit this save pushed.
//!
//! # The base is always named
//!
//! A PR is opened into the `base` the caller passes: GitHub's `base`, GitLab's
//! `target_branch`. The compare link this replaces left it out, and GitHub then resolved it
//! against the repo's default branch, so a plane with `[plane] branch = "release"` was offered
//! a PR into `main` (#298).
//!
//! # Credentials
//!
//! The same as the rest of [`crate::forge`]: every question goes through a backend's
//! [`super::transport::Transport`], today the forge's own CLI, so the token stays in that CLI
//! and is never read, passed or printed here. Every value that came from a repo or a person —
//! a branch, a title, a body — is a literal field, which the CLI transport sends with `-f`;
//! `-F` would read a leading `@` as a file to upload (charter #323). A typed field carries
//! only values charter writes itself: a number and `true`/`false`.
//!
//! The operations themselves are [`super::backend::Requests`]' methods, with a body per forge
//! in [`super::github`] and [`super::gitlab`]. This module holds their neutral types and the
//! rules both forges share.
//!
//! # Only the app asks for auto-merge
//!
//! An agent never merges: `floorguard` refuses `gh pr merge` from a session and keeps doing
//! so. This module is called by charter's own save, for a project whose mode asks for it.
//!
//! And the app only ever *requests* it (ADR 0051). Nothing here merges. When the forge will not
//! queue a merge because there is nothing to wait for, the answer is
//! [`AutoMerge::NotQueued`] with the forge's reason, and the PR stays open for a person.

use std::path::Path;

use serde_json::Value;

use super::{Forge, Kind};
use crate::worktree::git;

/// A repository on a forge: which forge, and its path there (`owner/name` on GitHub, the full
/// namespace on GitLab).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub forge: Forge,
    pub path: String,
}

/// An open pull or merge request: the number the forge shows (a GitLab MR's `iid`, not its
/// global `id`) and its web page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pr {
    pub number: u64,
    pub url: String,
}

/// What `request_auto_merge` got the forge to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoMerge {
    /// The forge will merge the PR, at the pushed commit, once its checks pass.
    Queued,
    /// The forge would not queue it because there is nothing to wait for — no checks, or a
    /// pipeline already finished — and nothing was merged. Carries the forge's reason.
    NotQueued(String),
}

impl Repo {
    /// Its forge's backend, over the transport every call takes today.
    pub fn backend(&self) -> Box<dyn super::backend::ForgeBackend> {
        self.forge.backend()
    }

    /// The repo the plane's own `origin` names: [`Repo::of_clone`] of the plane itself.
    pub fn of_plane(plane: &Path) -> Result<Repo, String> {
        Repo::of_clone(plane, plane)
    }

    /// The repo a clone's `origin` names, on a forge `plane`'s `charter.toml` declares or a
    /// kind's default host. Refused for any other host: a PR mode there is a config error
    /// (ADR 0051), and charter does not guess which API an unknown host speaks.
    pub fn of_clone(plane: &Path, clone: &Path) -> Result<Repo, String> {
        let url = git::run(clone, &["remote", "get-url", "origin"], git::READ)
            .map(|run| run.out.trim().to_string())
            .unwrap_or_default();
        if url.is_empty() {
            return Err(format!(
                "{} has no origin, so there is nowhere to open a pull request",
                clone.display()
            ));
        }
        let Some(forge) = super::resolve_host(&url, plane) else {
            return Err(format!(
                "origin {url} is not on a GitHub or GitLab host this plane declares, so charter \
                 cannot open a pull request there. Add it as a [[forge]] in charter.toml, or \
                 choose a mode without a pull request"
            ));
        };
        let Some(path) = super::namespace_of(&url) else {
            return Err(format!("origin {url} names no repository"));
        };
        Ok(Repo { forge, path })
    }
}

/// The request's number and page out of a forge's record of it.
pub(super) fn pr_of(
    record: &Value,
    number_key: &str,
    url_key: &str,
    doing: &str,
) -> Result<Pr, String> {
    let number = record.get(number_key).and_then(Value::as_u64);
    let url = record.get(url_key).and_then(Value::as_str);
    match (number, url) {
        (Some(number), Some(url)) => Ok(Pr {
            number,
            url: url.to_string(),
        }),
        _ => Err(format!(
            "{doing}: the answer named no {number_key} and {url_key}"
        )),
    }
}

/// The first record of a listing, or `None` for an empty one. GitHub's lookup names the head
/// by `owner:branch`, so a fork's PR never matches it.
pub(super) fn first(listing: &Value) -> Option<&Value> {
    listing.as_array().and_then(|items| items.first())
}

/// The first MR in a listing whose source branch is in the project itself. GitLab's lookup
/// matches `source_branch` by name only and lists MRs from forks too, so a stranger's fork
/// MR from a branch of the same name would otherwise be rewritten and set to merge.
pub(super) fn own_mr(listing: &Value) -> Option<&Value> {
    listing.as_array()?.iter().find(|mr| {
        let source = mr.get("source_project_id").and_then(Value::as_u64);
        source.is_some() && source == mr.get("target_project_id").and_then(Value::as_u64)
    })
}

/// The line charter puts in the body of every pull request it writes, so it can tell its own
/// from one a person opened from the same branch.
pub const MARKER: &str = "<!-- charter-save -->";

/// What `open_or_update` found or made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub pr: Pr,
    /// Whether the PR is charter's: one it opened now, or an open one whose head branch is
    /// under `charter/` or whose body carries [`MARKER`]. A PR that is not charter's was left
    /// exactly as it was, and nothing may be asked of the forge about it — auto-merge above all.
    pub ours: bool,
}

/// Whether an open PR is charter's to rewrite: its head branch is one charter names, or its
/// body carries [`MARKER`].
pub(super) fn is_ours(head: &str, body: Option<&str>) -> bool {
    head.starts_with("charter/") || body.is_some_and(|b| b.contains(MARKER))
}

/// Where a pull or merge request stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Open,
    /// Merged, as `commit` on the target branch: the merge commit, the squash commit, or the
    /// last commit a rebase merge wrote. `None` when the forge names none.
    Merged {
        commit: Option<String>,
    },
    /// Closed without merging.
    Closed,
}

/// The [`State`] one pull or merge request record says, read the same way wherever it came from.
pub(super) fn state_of(kind: Kind, record: &Value, doing: &str) -> Result<State, String> {
    let word = record["state"].as_str().unwrap_or("");
    // GitHub says `closed` for a merged PR too; `merged` (and `merged_at`) tell the two apart.
    let merged = record["merged"] == Value::Bool(true) || record["merged_at"].is_string();
    // The commit the merge made on the target: GitHub's `merge_commit_sha` (the merge commit,
    // the squash commit, or a rebase's last commit); GitLab's squash commit, else its merge
    // commit. A fast-forward merge on GitLab names neither.
    let named = |key: &str| {
        record[key]
            .as_str()
            .filter(|sha| !sha.is_empty())
            .map(str::to_string)
    };
    let commit = match kind {
        Kind::GitHub => named("merge_commit_sha"),
        Kind::GitLab => named("squash_commit_sha").or_else(|| named("merge_commit_sha")),
    };
    match (kind, word) {
        (Kind::GitHub, "open") | (Kind::GitLab, "opened" | "locked") => Ok(State::Open),
        (Kind::GitHub, "closed") if merged => Ok(State::Merged { commit }),
        (Kind::GitHub, "closed") | (Kind::GitLab, "closed") => Ok(State::Closed),
        (Kind::GitLab, "merged") => Ok(State::Merged { commit }),
        _ => Err(format!("{doing}: the forge answered the state {word:?}")),
    }
}

/// A pull or merge request found by its head branch: what `charter change show` reads of each
/// member (ADR 0060).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub number: u64,
    pub url: String,
    pub state: State,
    /// The commit the request's branch is at, as the forge says. Checks are read at exactly
    /// this commit and at no other.
    pub head: String,
}

/// GitHub's methods in charter's order of preference: `(setting, GraphQL enum)`. Rebase keeps
/// each save's commit as it was made; a merge commit keeps them too; squash rewrites them into
/// one, which is why it comes last (ADR 0051).
pub(super) const GITHUB_METHODS: [(&str, &str); 3] = [
    ("rebaseMergeAllowed", "REBASE"),
    ("mergeCommitAllowed", "MERGE"),
    ("squashMergeAllowed", "SQUASH"),
];

/// GitHub's refusals that mean "nothing to wait for": a PR that could merge now (`clean`), or
/// one whose only checks are not required (`unstable`), cannot be put on auto-merge.
pub(super) const GITHUB_NOTHING_TO_WAIT_FOR: [&str; 2] = ["clean status", "unstable status"];

/// A GitLab pipeline still to finish, so there is something to wait for. GitLab 19.4 lists
/// thirteen statuses (`doc/api/pipelines.md`); these are the ones that have not started or
/// are still going, `waiting_for_callback` among them.
pub(super) const GITLAB_ACTIVE: [&str; 7] = [
    "created",
    "waiting_for_resource",
    "preparing",
    "waiting_for_callback",
    "pending",
    "running",
    "scheduled",
];

/// GitLab's refusals of the merge call that mean "this MR cannot be merged now": 405 (not
/// mergeable) and 422 (it cannot be set to merge). Any other refusal — a 409 for a head that
/// moved past `sha`, a 401 — is an error.
pub(super) const GITLAB_NOT_MERGEABLE: [&str; 2] = ["(HTTP 405)", "(HTTP 422)"];

/// `Queued` for a call that succeeded, `NotQueued` with the forge's words for a refusal that
/// names one of `nothing_to_wait_for`, and an error for every other failure.
pub(super) fn not_queued_when(
    said: Result<Result<(), String>, String>,
    nothing_to_wait_for: &[&str],
    doing: &str,
) -> Result<AutoMerge, String> {
    match said? {
        Ok(()) => Ok(AutoMerge::Queued),
        Err(why) if nothing_to_wait_for.iter().any(|w| why.contains(w)) => {
            Ok(AutoMerge::NotQueued(why))
        }
        Err(why) => Err(format!("{doing} failed: {why}")),
    }
}
