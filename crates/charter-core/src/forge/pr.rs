//! Opening or updating a pull request — a merge request on GitLab — and asking the forge to
//! merge it once it may. The forge adapters' first write that is not a push (ADR 0051).
//!
//! Two calls, the same on both forges:
//!
//! - [`open_or_update`]: the open PR from `head` into `base`, updated with a new title and body,
//!   or a new one when there is none. Never a second one for the same pair.
//! - [`request_auto_merge`]: the forge merges it when its checks allow, by the repo's own
//!   method, preferring rebase, then a merge commit, then squash.
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

impl Repo {
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

    fn owner_name(&self) -> (&str, &str) {
        self.path.split_once('/').unwrap_or((&self.path, ""))
    }

    /// Run the forge's CLI with `args` and read its answer as JSON. Any failure is an error
    /// in the CLI's own words; nothing here reads a failure as "none".
    fn ask(&self, args: Vec<String>, doing: &str) -> Result<Value, String> {
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

    /// `gh api --hostname H [-X METHOD] <path> [fields…]`, or glab's equivalent.
    fn api(&self, method: Option<&str>, path: &str, fields: &[Field]) -> Vec<String> {
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
enum Field<'a> {
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

/// The first record of a listing, or `None` for an empty one.
fn first(listing: &Value) -> Option<&Value> {
    listing.as_array().and_then(|items| items.first())
}

/// Open a pull request (a merge request on GitLab) from `head` into `base`, or update the one
/// already open for that pair with `title` and `body`.
///
/// A lookup that fails is an error and never "there is none": reading it as none would open a
/// second PR every time the forge hiccuped.
pub fn open_or_update(
    repo: &Repo,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
) -> Result<Pr, String> {
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
            pr_of(&repo.ask(args, &doing)?, "number", "html_url", &doing)
        }
        Kind::GitLab => {
            let mrs = format!("projects/{}/merge_requests", quote(&repo.path));
            let lookup = format!(
                "{mrs}?state=opened&source_branch={}&target_branch={}&per_page=1",
                quote(head),
                quote(base)
            );
            let doing = format!("looking for an open merge request from {head} into {base}");
            let found = repo.ask(repo.api(None, &lookup, &[]), &doing)?;
            let args = match first(&found) {
                Some(open) => {
                    let mr = pr_of(open, "iid", "web_url", &doing)?;
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
            pr_of(&repo.ask(args, &doing)?, "iid", "web_url", &doing)
        }
    }
}

/// What a GitHub repo allows, and the PR's node id, in one question.
const SETTINGS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){autoMergeAllowed rebaseMergeAllowed mergeCommitAllowed squashMergeAllowed pullRequest(number:$number){id}}}";

/// Turn auto-merge on for one PR.
const ENABLE: &str = "mutation($id:ID!,$method:PullRequestMergeMethod!){enablePullRequestAutoMerge(input:{pullRequestId:$id,mergeMethod:$method}){clientMutationId}}";

/// GitHub's methods in charter's order of preference: `(setting, GraphQL enum, REST word)`.
/// Rebase keeps each save's commit as it was made; a merge commit keeps them too; squash
/// rewrites them into one, which is why it comes last (ADR 0051).
const GITHUB_METHODS: [(&str, &str, &str); 3] = [
    ("rebaseMergeAllowed", "REBASE", "rebase"),
    ("mergeCommitAllowed", "MERGE", "merge"),
    ("squashMergeAllowed", "SQUASH", "squash"),
];

/// Ask the forge to merge `pr` once it may, by the repo's allowed method: rebase, else a merge
/// commit, else squash.
///
/// - **GitHub:** auto-merge, which the repo must allow. A PR with nothing left to wait for
///   cannot be put on auto-merge (GitHub answers "clean status"), so it is merged at once by
///   the same method: that is what auto-merge would have done a moment later.
/// - **GitLab:** merge when the pipeline succeeds. A GitLab project has one merge method, so
///   there is no choice to make except squash, which is asked for only when the project
///   always squashes.
pub fn request_auto_merge(repo: &Repo, pr: &Pr) -> Result<(), String> {
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
            let Some(&(_, method, rest_method)) = GITHUB_METHODS
                .iter()
                .find(|(setting, _, _)| settings[*setting] == Value::Bool(true))
            else {
                return Err(format!("{} allows no merge method", repo.path));
            };
            let Some(id) = settings["pullRequest"]["id"].as_str() else {
                return Err(format!("{doing}: no pull request #{number}"));
            };
            let doing = format!("asking {} to auto-merge #{number}", repo.path);
            match repo.ask(
                repo.graphql(
                    ENABLE,
                    &[Field::Text("id", id), Field::Text("method", method)],
                ),
                &doing,
            ) {
                Ok(_) => Ok(()),
                Err(said) if said.contains("clean status") => {
                    let merge = format!(
                        "repos/{}/{}/pulls/{number}/merge",
                        quote(owner),
                        quote(name)
                    );
                    let doing = format!("merging #{number} into {}", repo.path);
                    repo.ask(
                        repo.api(
                            Some("PUT"),
                            &merge,
                            &[Field::Text("merge_method", rest_method)],
                        ),
                        &doing,
                    )
                    .map(|_| ())
                }
                Err(said) => Err(said),
            }
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
            let doing = format!(
                "asking {} to merge !{} when it passes",
                repo.path, pr.number
            );
            repo.ask(
                repo.api(
                    Some("PUT"),
                    &format!("{project}/merge_requests/{}/merge", pr.number),
                    &[
                        Field::Typed("merge_when_pipeline_succeeds", "true"),
                        Field::Typed("squash", squash),
                    ],
                ),
                &doing,
            )
            .map(|_| ())
        }
    }
}
