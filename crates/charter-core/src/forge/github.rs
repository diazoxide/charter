//! The GitHub backend: every operation of [`super::backend`]'s areas, as GitHub's REST and
//! GraphQL APIs (version `2022-11-28`) spell it. Each request is built once, as a
//! [`Call`], and sent by whichever transport the backend was built over.

use serde_json::Value;

use super::backend::{Asker, Caller, Repos, Requests};
use super::checks::{self, Checks};
use super::pr::{
    AutoMerge, GITHUB_METHODS, GITHUB_NOTHING_TO_WAIT_FOR, Opened, Pr, Request, State, first,
    is_ours, not_queued_when, pr_of, state_of,
};
use super::transport::{Call, Field, Method, NoAnswer};
use super::{
    ForgeError, Kind, LIST_TIMEOUT, Raised, STATUS_TIMEOUT, falsy, first_field, mapped, parse,
    quote, truthy, word_of,
};

/// The GitHub backend.
pub(super) struct GitHub(pub(super) Asker);

/// GitHub's `statusCheckRollup.state` → charter's neutral vocabulary. Python's
/// `github._CI_MAP`; anything unlisted is no answer rather than an invented one.
const CI: [(&str, &str); 5] = [
    ("SUCCESS", "success"),
    ("FAILURE", "failed"),
    ("ERROR", "failed"),
    ("PENDING", "pending"),
    ("EXPECTED", "pending"),
];

/// The one GraphQL document the status line sends, byte for byte as `github._ROLLUP_QUERY`
/// spells it — leading newline included, because it is a value on a command line a recorded
/// `gh` matches against.
///
/// GraphQL and not REST because **there is no single CI status in GitHub's REST API**: a
/// commit carries N check runs plus legacy commit statuses, and GitHub computes the rollup
/// only here. Inventing an aggregation would either lose information or state something
/// GitHub does not.
const ROLLUP_QUERY: &str = "\nquery($owner:String!, $name:String!, $ref:String!) {\n  \
                            repository(owner:$owner, name:$name) {\n    \
                            ref(qualifiedName:$ref) { target { ... on Commit {\n      \
                            statusCheckRollup { state } } } }\n  }\n}\n";

/// What a GitHub repo allows, and the PR's node id, in one question.
const SETTINGS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){autoMergeAllowed rebaseMergeAllowed mergeCommitAllowed squashMergeAllowed pullRequest(number:$number){id}}}";

/// Turn auto-merge on for one PR, at one head: `expectedHeadOid` makes GitHub refuse if the
/// branch has moved past the commit this save pushed.
const ENABLE: &str = "mutation($id:ID!,$method:PullRequestMergeMethod!,$head:GitObjectID!){enablePullRequestAutoMerge(input:{pullRequestId:$id,mergeMethod:$method,expectedHeadOid:$head}){clientMutationId}}";

enum Paged {
    NotAnOrg,
    Failed(ForgeError),
}

fn owner_name(path: &str) -> (&str, &str) {
    path.split_once('/').unwrap_or((path, ""))
}

impl GitHub {
    fn paged(&self, base: &str, owner: &str, org_probe: bool) -> Result<Vec<Value>, Paged> {
        let mut out = Vec::new();
        let mut page = 1;
        let joint = if base.contains('?') { '&' } else { '?' };
        loop {
            let path = format!("{base}{joint}per_page=100&page={page}");
            let answer = match self.0.send(&Call::get(&path, LIST_TIMEOUT)) {
                Ok(answer) => answer,
                Err(NoAnswer::Timeout(why)) => {
                    return Err(Paged::Failed(ForgeError(format!(
                        "listing repos for GitHub owner '{owner}' {why}"
                    ))));
                }
                Err(NoAnswer::Missing(why)) => return Err(Paged::Failed(ForgeError(why))),
            };
            if !answer.ok() {
                let blob = answer.both();
                if org_probe
                    && page == 1
                    && (blob.contains("HTTP 404") || blob.contains("\"status\":\"404\""))
                {
                    return Err(Paged::NotAnOrg);
                }
                return Err(Paged::Failed(ForgeError(format!(
                    "listing repos for GitHub owner '{owner}' failed ({path}): {}",
                    answer.said(Kind::GitHub)
                ))));
            }
            let batch = parse(Kind::GitHub, &answer.out, &path).map_err(Paged::Failed)?;
            let items = batch.as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                break;
            }
            let short = items.len() < 100;
            out.extend(items);
            if short {
                break;
            }
            page += 1;
        }
        Ok(out)
    }

    /// The GitHub rollup, over GraphQL.
    ///
    /// **Literal fields, never typed ones** (charter #323). None of these three values is
    /// charter's: `branch` is read out of the tree's `HEAD` and `path` out of `git remote
    /// get-url origin`, so both are written by whoever wrote the repo. A typed field gives a
    /// value magic meaning in the CLI transport — a leading `@` names a file to read it from.
    ///
    /// **Not percent-encoded**: these are GraphQL variables, JSON strings GitHub never
    /// decodes. Encoding would send `feature%2Fx` for `feature/x` and match no ref.
    fn rollup(&self, path: &str, branch: &str) -> Result<Option<String>, Raised> {
        let (owner, name) = owner_name(path);
        let call = Call::graphql(
            ROLLUP_QUERY,
            vec![
                Field::text("owner", owner),
                Field::text("name", name),
                Field::text("ref", branch),
            ],
            STATUS_TIMEOUT,
        );
        let Ok(answer) = self.0.send(&call) else {
            return Ok(None);
        };
        if !answer.ok() {
            return Ok(None);
        }
        let Ok(data) = serde_json::from_str::<Value>(&answer.out) else {
            return Ok(None);
        };
        // `(x or {}).get(…)` five times over. The `or {}` is what makes a MISSING key
        // harmless; a key that is present and is not an object is where the next `.get`
        // makes Python raise, and that distinction is the whole of this loop.
        let mut node = data;
        for key in ["data", "repository", "ref", "target", "statusCheckRollup"] {
            let Some(map) = node.as_object() else {
                return Err(Raised);
            };
            let found = map.get(key).cloned().unwrap_or(Value::Null);
            node = if falsy(&found) {
                Value::Object(serde_json::Map::new())
            } else {
                found
            };
        }
        let Some(rollup) = node.as_object() else {
            return Err(Raised);
        };
        Ok(mapped(&CI, word_of(rollup.get("state"))))
    }
}

/// One GitHub repo in the shape every backend produces (Python's `_normalize`).
fn normalize(raw: &Value) -> Value {
    let get = |key: &str| raw.get(key).cloned().unwrap_or(Value::Null);
    let text_or_empty = |key: &str| match raw.get(key) {
        Some(v) if truthy(v) => v.clone(),
        _ => Value::String(String::new()),
    };
    let topics = match raw.get("topics") {
        Some(v) if truthy(v) => v.clone(),
        _ => Value::Array(Vec::new()),
    };
    serde_json::json!({
        "id": get("id"),
        "name": get("name"),
        "path_with_namespace": get("full_name"),
        "default_branch": get("default_branch"),
        "description": text_or_empty("description"),
        "web_url": text_or_empty("html_url"),
        "ssh_url": text_or_empty("ssh_url"),
        "topics": topics,
        "forge": Kind::GitHub.word(),
    })
}

impl Repos for GitHub {
    fn owned(&self, _caller: &Caller, owner: &str) -> Result<Vec<Value>, ForgeError> {
        let enc = quote(owner);
        let raw = match self.paged(&format!("orgs/{enc}/repos"), owner, true) {
            Err(Paged::NotAnOrg) => {
                // A personal account 404s on the org endpoint with an identical record shape
                // on the user one. Only a real 404 falls back; any other failure is a failure.
                match self.paged(&format!("users/{enc}/repos"), owner, false) {
                    Ok(items) => items,
                    Err(Paged::Failed(e)) => return Err(e),
                    Err(Paged::NotAnOrg) => unreachable!("only the org probe says this"),
                }
            }
            Err(Paged::Failed(e)) => return Err(e),
            Ok(items) => items,
        };
        Ok(raw.iter().map(normalize).collect())
    }

    fn reachable(&self, _caller: &Caller, owner: &str) -> Result<Vec<Value>, ForgeError> {
        let raw = match self.paged(
            "user/repos?affiliation=owner,collaborator,organization_member",
            owner,
            false,
        ) {
            Ok(items) => items,
            Err(Paged::Failed(e)) => return Err(e),
            Err(Paged::NotAnOrg) => unreachable!("only the org probe says this"),
        };
        Ok(super::under_owner(raw.iter().map(normalize), owner))
    }

    fn top_level(
        &self,
        _caller: &Caller,
        repo: &Value,
        git_ref: Option<&str>,
    ) -> Result<Vec<String>, ForgeError> {
        let path = repo
            .get("path_with_namespace")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (owner, name) = owner_name(path);
        let git_ref = git_ref
            .filter(|r| !r.is_empty())
            .or_else(|| {
                repo.get("default_branch")
                    .and_then(Value::as_str)
                    .filter(|r| !r.is_empty())
            })
            .unwrap_or("HEAD");
        let api = format!(
            "repos/{}/{}/git/trees/{}",
            quote(owner),
            quote(name),
            quote(git_ref)
        );
        let answer = match self.0.send(&Call::get(&api, LIST_TIMEOUT)) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => {
                return Err(ForgeError(format!(
                    "listing tree for {path}@{git_ref} {why}"
                )));
            }
            Err(NoAnswer::Missing(why)) => return Err(ForgeError(why)),
        };
        if !answer.ok() {
            return Err(ForgeError(format!(
                "listing tree for {path}@{git_ref} failed: {}",
                answer.said(Kind::GitHub)
            )));
        }
        if answer.out.trim().is_empty() {
            return Ok(Vec::new());
        }
        let data: Value = serde_json::from_str(&answer.out).map_err(|e| {
            ForgeError(format!(
                "GitHub API returned malformed JSON (tree {path}@{git_ref}): {e}"
            ))
        })?;
        Ok(data
            .get("tree")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| e.get("path").and_then(Value::as_str).unwrap_or_default())
                    .filter(|p| !p.contains('/'))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default())
    }
}

impl Requests for GitHub {
    fn open_or_update(
        &self,
        _caller: &Caller,
        path: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<Opened, String> {
        let (owner, name) = owner_name(path);
        let pulls = format!("repos/{}/{}/pulls", quote(owner), quote(name));
        let lookup = format!(
            "{pulls}?state=open&head={}:{}&base={}&per_page=1",
            quote(owner),
            quote(head),
            quote(base)
        );
        let doing = format!("looking for an open pull request from {head} into {base}");
        let found = self.0.ask(&Call::get(&lookup, LIST_TIMEOUT), &doing)?;
        let call = match first(&found) {
            Some(open) => {
                let pr = pr_of(open, "number", "html_url", &doing)?;
                if !is_ours(head, open.get("body").and_then(Value::as_str)) {
                    return Ok(Opened { pr, ours: false });
                }
                Call::write(
                    Method::Patch,
                    format!("{pulls}/{}", pr.number),
                    vec![Field::text("title", title), Field::text("body", body)],
                )
            }
            None => Call::write(
                Method::Post,
                pulls,
                vec![
                    Field::text("head", head),
                    Field::text("base", base),
                    Field::text("title", title),
                    Field::text("body", body),
                ],
            ),
        };
        let doing = format!("opening a pull request from {head} into {base}");
        let pr = pr_of(&self.0.ask(&call, &doing)?, "number", "html_url", &doing)?;
        Ok(Opened { pr, ours: true })
    }

    fn state(&self, _caller: &Caller, path: &str, pr: &Pr) -> Result<State, String> {
        let (owner, name) = owner_name(path);
        let api = format!("repos/{}/{}/pulls/{}", quote(owner), quote(name), pr.number);
        let doing = format!("reading pull request #{} of {path}", pr.number);
        let record = self.0.ask(&Call::get(&api, LIST_TIMEOUT), &doing)?;
        state_of(Kind::GitHub, &record, &doing)
    }

    fn by_head(
        &self,
        _caller: &Caller,
        path: &str,
        branch: &str,
    ) -> Result<Option<Request>, String> {
        let (owner, name) = owner_name(path);
        let api = format!(
            "repos/{}/{}/pulls?state=all&head={}:{}&per_page=1",
            quote(owner),
            quote(name),
            quote(owner),
            quote(branch)
        );
        let doing = format!("finding the pull request from {branch} in {path}");
        let listing = self.0.ask(&Call::get(&api, LIST_TIMEOUT), &doing)?;
        let found = first(&listing);
        // Asked, not assumed: `owner:branch` can match a branch in another repo the owner
        // holds, and a head filter GitHub cannot parse is ignored rather than refused. A pull
        // request from anywhere else is not this member's, and printing its checks as this
        // member's would be the one wrong answer worse than none.
        if let Some(pr) = found {
            let from = (
                pr["head"]["ref"].as_str(),
                pr["head"]["repo"]["full_name"].as_str(),
            );
            if from != (Some(branch), Some(path)) {
                return Err(format!(
                    "{doing}: the forge answered a pull request from {}:{}, not from this \
                     branch",
                    from.1.unwrap_or("?"),
                    from.0.unwrap_or("?")
                ));
            }
        }
        let Some(record) = found else {
            return Ok(None);
        };
        let pr = pr_of(record, "number", "html_url", &doing)?;
        let head = record["head"]["sha"]
            .as_str()
            .filter(|sha| !sha.is_empty())
            .ok_or_else(|| format!("{doing}: the forge named no head commit"))?;
        Ok(Some(Request {
            number: pr.number,
            url: pr.url,
            state: state_of(Kind::GitHub, record, &doing)?,
            head: head.to_string(),
        }))
    }

    fn request_auto_merge(
        &self,
        _caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
    ) -> Result<AutoMerge, String> {
        let (owner, name) = owner_name(path);
        let number = pr.number.to_string();
        let doing = format!("reading how {path} merges");
        let answer = self.0.ask(
            &Call::graphql(
                SETTINGS,
                vec![
                    Field::text("owner", owner),
                    Field::text("name", name),
                    Field::typed("number", &number),
                ],
                LIST_TIMEOUT,
            ),
            &doing,
        )?;
        let settings = &answer["data"]["repository"];
        if settings["autoMergeAllowed"] != Value::Bool(true) {
            return Err(format!(
                "{path} does not allow auto-merge. Turn on \"Allow auto-merge\" in its \
                 settings, or choose the pr mode"
            ));
        }
        let Some(&(_, method)) = GITHUB_METHODS
            .iter()
            .find(|(setting, _)| settings[*setting] == Value::Bool(true))
        else {
            return Err(format!("{path} allows no merge method"));
        };
        let Some(id) = settings["pullRequest"]["id"].as_str() else {
            return Err(format!("{doing}: no pull request #{number}"));
        };
        let call = Call::graphql(
            ENABLE,
            vec![
                Field::text("id", id),
                Field::text("method", method),
                Field::text("head", head_sha),
            ],
            LIST_TIMEOUT,
        );
        not_queued_when(
            self.0.said(&call),
            &GITHUB_NOTHING_TO_WAIT_FOR,
            &format!("asking {path} to auto-merge #{number}"),
        )
    }

    fn checks_at(&self, _caller: &Caller, path: &str, sha: &str, _request: u64) -> Checks {
        checks::guarded(sha, || {
            let (owner, name) = owner_name(path);
            let base = format!("repos/{}/{}/commits/{sha}", quote(owner), quote(name));
            let runs = self.0.ask(
                &Call::get(format!("{base}/check-runs?per_page=100"), LIST_TIMEOUT),
                &format!("reading the check runs at {sha} of {path}"),
            )?;
            let statuses = self.0.ask(
                &Call::get(format!("{base}/status?per_page=100"), LIST_TIMEOUT),
                &format!("reading the commit statuses at {sha} of {path}"),
            )?;
            checks::github(&runs, &statuses)
        })
    }

    fn open_on_branch(
        &self,
        _caller: &Caller,
        path: &str,
        branch: &str,
    ) -> Result<Option<Value>, Raised> {
        let (owner, name) = owner_name(path);
        let asked = format!(
            "repos/{}/{}/pulls?state=open&head={}:{}&per_page=1",
            quote(owner),
            quote(name),
            quote(owner),
            quote(branch)
        );
        first_field(self.0.best_effort(&asked), "number")
    }

    fn ci_word(
        &self,
        _caller: &Caller,
        path: &str,
        branch: &str,
    ) -> Result<Option<String>, Raised> {
        self.rollup(path, branch)
    }
}
