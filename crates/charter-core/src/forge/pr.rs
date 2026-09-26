//! Opening or updating a pull request — a merge request on GitLab — and asking the forge to
//! merge it once it may. The forge adapters' first write that is not a push (ADR 0051).
//!
//! Two calls, the same on both forges:
//!
//! - [`open_or_update`]: the open PR from `head` into `base`, updated with a new title and body,
//!   or a new one when there is none. Never a second one for the same pair.
//! - [`request_auto_merge`]: the forge queues it to merge when its checks allow, by the
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
//! The same as the rest of [`crate::forge`]: every question is a `gh api` or `glab api` call
//! through the one runner there, so the token stays in the forge's CLI and is never read,
//! passed or printed here. Every value that came from a repo or a person — a branch, a title,
//! a body — goes by `-f`, which sends it as a literal string; `-F` would read a leading `@` as
//! a file to upload (charter #323). `-F` carries only values charter writes itself: a number
//! and `true`/`false`.
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

use super::{Forge, Kind, LIST_TIMEOUT, NoAnswer, call, detail, quote, strings};
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

/// What [`request_auto_merge`] got the forge to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoMerge {
    /// The forge will merge the PR, at the pushed commit, once its checks pass.
    Queued,
    /// The forge would not queue it because there is nothing to wait for — no checks, or a
    /// pipeline already finished — and nothing was merged. Carries the forge's reason.
    NotQueued(String),
}

impl Repo {
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

    pub(super) fn owner_name(&self) -> (&str, &str) {
        self.path.split_once('/').unwrap_or((&self.path, ""))
    }

    /// Run the forge's CLI with `args` and read its answer as JSON. Any failure is an error
    /// in the CLI's own words; nothing here reads a failure as "none".
    pub(super) fn ask(&self, args: Vec<String>, doing: &str) -> Result<Value, String> {
        let kind = self.forge.kind;
        let answer = match call(kind, &args, LIST_TIMEOUT) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => return Err(format!("{doing}: {why}")),
            Err(NoAnswer::Missing(why)) => return Err(why),
        };
        if answer.code != 0 {
            return Err(format!("{doing} failed: {}", detail(kind, &answer)));
        }
        serde_json::from_str(&answer.out)
            .map_err(|e| format!("{doing}: {} answered malformed JSON: {e}", kind.cli()))
    }

    /// Run a write and keep the forge's refusal apart from a CLI that could not answer: the
    /// outer error is "no answer", the inner one is the forge's own words.
    fn said(&self, args: Vec<String>) -> Result<Result<(), String>, String> {
        let kind = self.forge.kind;
        match call(kind, &args, LIST_TIMEOUT) {
            Ok(answer) if answer.code == 0 => Ok(Ok(())),
            Ok(answer) => Ok(Err(detail(kind, &answer))),
            Err(NoAnswer::Timeout(why)) | Err(NoAnswer::Missing(why)) => Err(why),
        }
    }

    /// `gh api --hostname H [-X METHOD] <path> [fields…]`, or glab's equivalent.
    pub(super) fn api(&self, method: Option<&str>, path: &str, fields: &[Field]) -> Vec<String> {
        let host = self.forge.host.as_str();
        let mut args = match self.forge.kind {
            Kind::GitHub => strings(&["api", "--hostname", host]),
            Kind::GitLab => strings(&["--hostname", host, "api"]),
        };
        if let Some(method) = method {
            args.extend(strings(&["-X", method]));
        }
        args.push(path.to_string());
        with_fields(args, fields)
    }

    /// `gh api graphql --hostname H -f query=… <variables…>`.
    fn graphql(&self, query: &str, fields: &[Field]) -> Vec<String> {
        let args = strings(&["api", "graphql", "--hostname", &self.forge.host]);
        with_fields(args, &[&[Field::Text("query", query)], fields].concat())
    }
}

/// One field of an API call.
#[derive(Clone, Copy)]
pub(super) enum Field<'a> {
    /// `-f`: sent as a literal string. Every value that is not charter's own.
    Text(&'a str, &'a str),
    /// `-F`: typed, so `true` is a boolean and `12` a number. Only for values charter writes,
    /// because `-F` also reads a leading `@` as a file to upload.
    Typed(&'a str, &'a str),
}

fn with_fields(mut args: Vec<String>, fields: &[Field]) -> Vec<String> {
    for field in fields {
        let (flag, name, value) = match field {
            Field::Text(name, value) => ("-f", name, value),
            Field::Typed(name, value) => ("-F", name, value),
        };
        args.push(flag.to_string());
        args.push(format!("{name}={value}"));
    }
    args
}

/// The request's number and page out of a forge's record of it.
fn pr_of(record: &Value, number_key: &str, url_key: &str, doing: &str) -> Result<Pr, String> {
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
fn first(listing: &Value) -> Option<&Value> {
    listing.as_array().and_then(|items| items.first())
}

/// The first MR in a listing whose source branch is in the project itself. GitLab's lookup
/// matches `source_branch` by name only and lists MRs from forks too, so a stranger's fork
/// MR from a branch of the same name would otherwise be rewritten and set to merge.
fn own_mr(listing: &Value) -> Option<&Value> {
    listing.as_array()?.iter().find(|mr| {
        let source = mr.get("source_project_id").and_then(Value::as_u64);
        source.is_some() && source == mr.get("target_project_id").and_then(Value::as_u64)
    })
}

/// The line charter puts in the body of every pull request it writes, so it can tell its own
/// from one a person opened from the same branch.
pub const MARKER: &str = "<!-- charter-save -->";

/// What [`open_or_update`] found or made.
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
fn is_ours(head: &str, body: Option<&str>) -> bool {
    head.starts_with("charter/") || body.is_some_and(|b| b.contains(MARKER))
}

/// Open a pull request (a merge request on GitLab) from `head` into `base`, or update the one
/// already open for that pair with `title` and `body` — **when that one is charter's**. A PR a
/// person opened from the same branch is never rewritten: its page is answered with
/// [`Opened::ours`] false, and the caller says so.
///
/// A lookup that fails is an error and never "there is none": reading it as none would open a
/// second PR every time the forge hiccuped.
pub fn open_or_update(
    repo: &Repo,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
) -> Result<Opened, String> {
    match repo.forge.kind {
        Kind::GitHub => {
            let (owner, name) = repo.owner_name();
            let pulls = format!("repos/{}/{}/pulls", quote(owner), quote(name));
            let lookup = format!(
                "{pulls}?state=open&head={}:{}&base={}&per_page=1",
                quote(owner),
                quote(head),
                quote(base)
            );
            let doing = format!("looking for an open pull request from {head} into {base}");
            let found = repo.ask(repo.api(None, &lookup, &[]), &doing)?;
            let args = match first(&found) {
                Some(open) => {
                    let pr = pr_of(open, "number", "html_url", &doing)?;
                    if !is_ours(head, open.get("body").and_then(Value::as_str)) {
                        return Ok(Opened { pr, ours: false });
                    }
                    repo.api(
                        Some("PATCH"),
                        &format!("{pulls}/{}", pr.number),
                        &[Field::Text("title", title), Field::Text("body", body)],
                    )
                }
                None => repo.api(
                    Some("POST"),
                    &pulls,
                    &[
                        Field::Text("head", head),
                        Field::Text("base", base),
                        Field::Text("title", title),
                        Field::Text("body", body),
                    ],
                ),
            };
            let doing = format!("opening a pull request from {head} into {base}");
            let pr = pr_of(&repo.ask(args, &doing)?, "number", "html_url", &doing)?;
            Ok(Opened { pr, ours: true })
        }
        Kind::GitLab => {
            let mrs = format!("projects/{}/merge_requests", quote(&repo.path));
            let lookup = format!(
                "{mrs}?state=opened&source_branch={}&target_branch={}&per_page=100",
                quote(head),
                quote(base)
            );
            let doing = format!("looking for an open merge request from {head} into {base}");
            let found = repo.ask(repo.api(None, &lookup, &[]), &doing)?;
            let args = match own_mr(&found) {
                Some(open) => {
                    let mr = pr_of(open, "iid", "web_url", &doing)?;
                    if !is_ours(head, open.get("description").and_then(Value::as_str)) {
                        return Ok(Opened {
                            pr: mr,
                            ours: false,
                        });
                    }
                    repo.api(
                        Some("PUT"),
                        &format!("{mrs}/{}", mr.number),
                        &[
                            Field::Text("title", title),
                            Field::Text("description", body),
                        ],
                    )
                }
                None => repo.api(
                    Some("POST"),
                    &mrs,
                    &[
                        Field::Text("source_branch", head),
                        Field::Text("target_branch", base),
                        Field::Text("title", title),
                        Field::Text("description", body),
                    ],
                ),
            };
            let doing = format!("opening a merge request from {head} into {base}");
            let pr = pr_of(&repo.ask(args, &doing)?, "iid", "web_url", &doing)?;
            Ok(Opened { pr, ours: true })
        }
    }
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

/// Where `pr` stands on the forge: open, merged, or closed without merging.
///
/// A save asks this of the PR it last opened, to learn whether the target branch now holds
/// the plane's work (charter-app#298). A lookup that fails, or an answer that is none of the
/// three, is an error and never "open": the caller then leaves the plane where it is.
pub fn state(repo: &Repo, pr: &Pr) -> Result<State, String> {
    let (path, doing) = match repo.forge.kind {
        Kind::GitHub => {
            let (owner, name) = repo.owner_name();
            (
                format!("repos/{}/{}/pulls/{}", quote(owner), quote(name), pr.number),
                format!("reading pull request #{} of {}", pr.number, repo.path),
            )
        }
        Kind::GitLab => (
            format!(
                "projects/{}/merge_requests/{}",
                quote(&repo.path),
                pr.number
            ),
            format!("reading merge request !{} of {}", pr.number, repo.path),
        ),
    };
    let record = repo.ask(repo.api(None, &path, &[]), &doing)?;
    state_of(repo.forge.kind, &record, &doing)
}

/// The [`State`] one pull or merge request record says, read the same way wherever it came from.
fn state_of(kind: Kind, record: &Value, doing: &str) -> Result<State, String> {
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

/// The newest pull or merge request whose head is `branch`, in any state, or `None` when there
/// is none. A lookup that fails is an error, never "none": a member with no request and a
/// forge charter could not ask are different answers.
pub fn by_head(repo: &Repo, branch: &str) -> Result<Option<Request>, String> {
    let kind = repo.forge.kind;
    let (path, doing) = match kind {
        Kind::GitHub => {
            let (owner, name) = repo.owner_name();
            (
                format!(
                    "repos/{}/{}/pulls?state=all&head={}:{}&per_page=1",
                    quote(owner),
                    quote(name),
                    quote(owner),
                    quote(branch)
                ),
                format!("finding the pull request from {branch} in {}", repo.path),
            )
        }
        Kind::GitLab => (
            format!(
                "projects/{}/merge_requests?source_branch={}&state=all&per_page=100",
                quote(&repo.path),
                quote(branch)
            ),
            format!("finding the merge request from {branch} in {}", repo.path),
        ),
    };
    let listing = repo.ask(repo.api(None, &path, &[]), &doing)?;
    // GitLab matches `source_branch` by name across forks too, so only an MR from the project
    // itself is this member's; GitHub's `owner:branch` already says so.
    let (found, number_key, url_key, head) = match kind {
        Kind::GitHub => {
            let found = first(&listing);
            // Asked, not assumed: `owner:branch` can match a branch in another repo the owner
            // holds, and a head filter GitHub cannot parse is ignored rather than refused. A
            // pull request from anywhere else is not this member's, and printing its checks
            // as this member's would be the one wrong answer worse than none.
            if let Some(pr) = found {
                let from = (
                    pr["head"]["ref"].as_str(),
                    pr["head"]["repo"]["full_name"].as_str(),
                );
                if from != (Some(branch), Some(repo.path.as_str())) {
                    return Err(format!(
                        "{doing}: the forge answered a pull request from {}:{}, not from this \
                         branch",
                        from.1.unwrap_or("?"),
                        from.0.unwrap_or("?")
                    ));
                }
            }
            let head = found.and_then(|r| r["head"]["sha"].as_str());
            (found, "number", "html_url", head)
        }
        Kind::GitLab => {
            let found = own_mr(&listing);
            // A full page with none of the project's own is a page charter could not see past,
            // which is not "no merge request".
            let full = listing.as_array().is_some_and(|all| all.len() >= 100);
            if found.is_none() && full {
                return Err(format!(
                    "{doing}: a hundred merge requests from forks share this branch name, and \
                     charter reads no further"
                ));
            }
            let head = found.and_then(|r| r["sha"].as_str());
            (found, "iid", "web_url", head)
        }
    };
    let Some(record) = found else {
        return Ok(None);
    };
    let pr = pr_of(record, number_key, url_key, &doing)?;
    let head = head
        .filter(|sha| !sha.is_empty())
        .ok_or_else(|| format!("{doing}: the forge named no head commit"))?;
    Ok(Some(Request {
        number: pr.number,
        url: pr.url,
        state: state_of(kind, record, &doing)?,
        head: head.to_string(),
    }))
}

/// What a GitHub repo allows, and the PR's node id, in one question.
const SETTINGS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){autoMergeAllowed rebaseMergeAllowed mergeCommitAllowed squashMergeAllowed pullRequest(number:$number){id}}}";

/// Turn auto-merge on for one PR, at one head: `expectedHeadOid` makes GitHub refuse if the
/// branch has moved past the commit this save pushed.
const ENABLE: &str = "mutation($id:ID!,$method:PullRequestMergeMethod!,$head:GitObjectID!){enablePullRequestAutoMerge(input:{pullRequestId:$id,mergeMethod:$method,expectedHeadOid:$head}){clientMutationId}}";

/// GitHub's methods in charter's order of preference: `(setting, GraphQL enum)`. Rebase keeps
/// each save's commit as it was made; a merge commit keeps them too; squash rewrites them into
/// one, which is why it comes last (ADR 0051).
const GITHUB_METHODS: [(&str, &str); 3] = [
    ("rebaseMergeAllowed", "REBASE"),
    ("mergeCommitAllowed", "MERGE"),
    ("squashMergeAllowed", "SQUASH"),
];

/// GitHub's refusals that mean "nothing to wait for": a PR that could merge now (`clean`), or
/// one whose only checks are not required (`unstable`), cannot be put on auto-merge.
const GITHUB_NOTHING_TO_WAIT_FOR: [&str; 2] = ["clean status", "unstable status"];

/// A GitLab pipeline still to finish, so there is something to wait for.
const GITLAB_ACTIVE: [&str; 6] = [
    "created",
    "waiting_for_resource",
    "preparing",
    "pending",
    "running",
    "scheduled",
];

/// GitLab's refusals of the merge call that mean "this MR cannot be merged now": 405 (not
/// mergeable) and 422 (it cannot be set to merge). Any other refusal — a 409 for a head that
/// moved past `sha`, a 401 — is an error.
const GITLAB_NOT_MERGEABLE: [&str; 2] = ["(HTTP 405)", "(HTTP 422)"];

/// Ask the forge to merge `pr` once its checks pass, by the repo's allowed method (rebase,
/// else a merge commit, else squash), and only while its head is `head_sha`, the commit this
/// save pushed.
///
/// Never merges. `Ok(NotQueued)` when the forge has nothing to wait for; `Err` for anything
/// that is a failure — a CLI that could not answer, auto-merge turned off, a head that moved.
///
/// - **GitHub:** `enablePullRequestAutoMerge` with `expectedHeadOid`.
/// - **GitLab:** merge when the pipeline succeeds, with `sha`. GitLab merges *at once* when
///   told that while no pipeline is running, so the MR's pipeline is read first and a finished
///   or missing one is `NotQueued`. A GitLab project has one merge method, so the only choice
///   is squash, asked for only when the project always squashes.
///   `merge_when_pipeline_succeeds` rather than the newer `auto_merge`: it is deprecated but
///   still accepted, and an older GitLab ignores a parameter it does not know, so sending it
///   `auto_merge` would merge at once rather than refuse.
pub fn request_auto_merge(repo: &Repo, pr: &Pr, head_sha: &str) -> Result<AutoMerge, String> {
    match repo.forge.kind {
        Kind::GitHub => {
            let (owner, name) = repo.owner_name();
            let number = pr.number.to_string();
            let doing = format!("reading how {} merges", repo.path);
            let answer = repo.ask(
                repo.graphql(
                    SETTINGS,
                    &[
                        Field::Text("owner", owner),
                        Field::Text("name", name),
                        Field::Typed("number", &number),
                    ],
                ),
                &doing,
            )?;
            let settings = &answer["data"]["repository"];
            if settings["autoMergeAllowed"] != Value::Bool(true) {
                return Err(format!(
                    "{} does not allow auto-merge. Turn on \"Allow auto-merge\" in its \
                     settings, or choose the pr mode",
                    repo.path
                ));
            }
            let Some(&(_, method)) = GITHUB_METHODS
                .iter()
                .find(|(setting, _)| settings[*setting] == Value::Bool(true))
            else {
                return Err(format!("{} allows no merge method", repo.path));
            };
            let Some(id) = settings["pullRequest"]["id"].as_str() else {
                return Err(format!("{doing}: no pull request #{number}"));
            };
            let args = repo.graphql(
                ENABLE,
                &[
                    Field::Text("id", id),
                    Field::Text("method", method),
                    Field::Text("head", head_sha),
                ],
            );
            not_queued_when(
                repo.said(args),
                &GITHUB_NOTHING_TO_WAIT_FOR,
                &format!("asking {} to auto-merge #{number}", repo.path),
            )
        }
        Kind::GitLab => {
            let project = format!("projects/{}", quote(&repo.path));
            let doing = format!("reading how {} merges", repo.path);
            let settings = repo.ask(repo.api(None, &project, &[]), &doing)?;
            let squash = if settings["squash_option"] == "always" {
                "true"
            } else {
                "false"
            };
            let mr = format!("{project}/merge_requests/{}", pr.number);
            let doing = format!("reading !{} of {}", pr.number, repo.path);
            let record = repo.ask(repo.api(None, &mr, &[]), &doing)?;
            let pipeline = record["head_pipeline"]["status"].as_str().unwrap_or("");
            if !GITLAB_ACTIVE.contains(&pipeline) {
                return Ok(AutoMerge::NotQueued(format!(
                    "!{} has no pipeline running, so there is nothing for GitLab to wait for",
                    pr.number
                )));
            }
            let args = repo.api(
                Some("PUT"),
                &format!("{mr}/merge"),
                &[
                    Field::Typed("merge_when_pipeline_succeeds", "true"),
                    Field::Typed("squash", squash),
                    Field::Text("sha", head_sha),
                ],
            );
            not_queued_when(
                repo.said(args),
                &GITLAB_NOT_MERGEABLE,
                &format!(
                    "asking {} to merge !{} when it passes",
                    repo.path, pr.number
                ),
            )
        }
    }
}

/// `Queued` for a call that succeeded, `NotQueued` with the forge's words for a refusal that
/// names one of `nothing_to_wait_for`, and an error for every other failure.
fn not_queued_when(
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
