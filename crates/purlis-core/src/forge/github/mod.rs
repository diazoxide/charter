//! The GitHub backend: every operation of [`super::backend`]'s areas, as GitHub's REST and
//! GraphQL APIs (version `2022-11-28`) spell it. Each request is built once, as a
//! [`Call`], and sent by whichever transport the backend was built over.

use serde_json::Value;

use super::backend::{
    About, Asker, Caller, Capabilities, Capability, Issues, NewWorkItem, Owner, PushProtection,
    Reach, Reason, RepoRecord, Repos, Requests, Support, Unavailable, UnknownWhy, Visibility,
    WorkItems,
};
use super::checks::{self, Checks};
use super::pr::{
    AutoMerge, GITHUB_METHODS, GITHUB_NOTHING_TO_WAIT_FOR, MergeAs, MergedAt, Opened, Pr, Request,
    State, commit_named, first, is_ours, not_queued_when, pr_of, unknown_state,
};
use super::transport::{Call, Field, Method, NoAnswer};
use super::{
    ForgeError, Kind, LIST_TIMEOUT, Raised, STATUS_TIMEOUT, falsy, first_field, listed_branch,
    listed_description, listed_id, listed_str, listed_topics, mapped, parse, quote, word_of,
};

// `WorkItems::read` (FW-6a) calls the reads; the writes, and the board queries, have only their
// tests until the seam takes them, so the compiler would call them dead.
#[allow(dead_code)]
mod graphql;
mod read;
#[cfg(test)]
mod read_tests;
#[allow(dead_code)]
pub(crate) mod work;

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
    /// The pull request's node id and whether its base branch has a merge queue, in one
    /// question.
    fn queue_probe(&self, caller: &Caller, path: &str, pr: &Pr) -> Result<Value, ForgeError> {
        let (owner, name) = owner_name(path);
        let number = pr.number.to_string();
        let doing = format!(
            "reading whether #{} of {path} lands through a merge queue",
            pr.number
        );
        let answer = self.0.ask(
            caller,
            &Call::graphql(
                graphql::merge_queue::QUERY,
                vec![
                    Field::text("owner", owner),
                    Field::text("name", name),
                    Field::typed("number", &number),
                ],
                LIST_TIMEOUT,
            ),
            &doing,
        )?;
        let found = &answer["data"]["repository"]["pullRequest"];
        if !found.is_object() {
            return Err(ForgeError::new(format!(
                "{doing}: no pull request #{}",
                pr.number
            )));
        }
        Ok(found.clone())
    }

    fn paged(
        &self,
        caller: &Caller,
        base: &str,
        owner: &str,
        org_probe: bool,
    ) -> Result<Vec<Value>, Paged> {
        let mut out = Vec::new();
        let mut page = 1;
        let joint = if base.contains('?') { '&' } else { '?' };
        loop {
            let path = format!("{base}{joint}per_page=100&page={page}");
            let answer = match self.0.send(caller, &Call::get(&path, LIST_TIMEOUT)) {
                Ok(answer) => answer,
                Err(NoAnswer::Timeout(why)) => {
                    return Err(Paged::Failed(ForgeError::transport(format!(
                        "listing repos for GitHub owner '{owner}' {why}"
                    ))));
                }
                Err(
                    no @ (NoAnswer::Missing(_) | NoAnswer::Refused(_) | NoAnswer::HeldBack { .. }),
                ) => {
                    return Err(Paged::Failed(no.error(no.said().to_string())));
                }
            };
            if !answer.ok() {
                let blob = answer.both();
                if org_probe
                    && page == 1
                    && (answer.status == Some(404)
                        || blob.contains("HTTP 404")
                        || blob.contains("\"status\":\"404\""))
                {
                    return Err(Paged::NotAnOrg);
                }
                return Err(Paged::Failed(ForgeError::of(
                    answer.failure(),
                    format!(
                        "listing repos for GitHub owner '{owner}' failed ({path}): {}",
                        answer.said(Kind::GitHub)
                    ),
                )));
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
    fn rollup(&self, caller: &Caller, path: &str, branch: &str) -> Result<Option<String>, Raised> {
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
        let Ok(answer) = self.0.send(caller, &call) else {
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

/// One GitHub repo as a neutral record (Python's `_normalize`).
fn normalize(raw: &Value) -> RepoRecord {
    RepoRecord {
        id: listed_id(raw),
        name: listed_str(raw, "name"),
        path_with_namespace: listed_str(raw, "full_name"),
        default_branch: listed_branch(raw),
        description: listed_description(raw),
        web_url: listed_str(raw, "html_url"),
        ssh_url: listed_str(raw, "ssh_url"),
        topics: listed_topics(raw),
        forge: Kind::GitHub,
    }
}

/// The [`State`] a GitHub pull request record says. GitHub says `closed` for a merged PR too;
/// `merged` (and `merged_at`) tell the two apart, and `merge_commit_sha` is the merge commit,
/// the squash commit, or a rebase's last commit.
fn state_of(record: &Value, doing: &str) -> Result<State, ForgeError> {
    let merged = record["merged"] == Value::Bool(true) || record["merged_at"].is_string();
    match record["state"].as_str().unwrap_or("") {
        "open" => Ok(State::Open),
        "closed" if merged => Ok(State::Merged {
            commit: commit_named(record, "merge_commit_sha"),
        }),
        "closed" => Ok(State::Closed),
        _ => Err(unknown_state(record, doing)),
    }
}

impl Repos for GitHub {
    fn owned(&self, caller: &Caller, owner: &Owner) -> Result<Vec<RepoRecord>, ForgeError> {
        let owner = owner.as_str();
        let enc = quote(owner);
        let raw = match self.paged(caller, &format!("orgs/{enc}/repos"), owner, true) {
            Err(Paged::NotAnOrg) => {
                // A personal account 404s on the org endpoint with an identical record shape
                // on the user one. Only a real 404 falls back; any other failure is a failure.
                match self.paged(caller, &format!("users/{enc}/repos"), owner, false) {
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

    fn reachable(&self, caller: &Caller, owner: &Owner) -> Result<Vec<RepoRecord>, ForgeError> {
        let raw = match self.paged(
            caller,
            "user/repos?affiliation=owner,collaborator,organization_member",
            owner.as_str(),
            false,
        ) {
            Ok(items) => items,
            Err(Paged::Failed(e)) => return Err(e),
            Err(Paged::NotAnOrg) => unreachable!("only the org probe says this"),
        };
        Ok(raw
            .iter()
            .map(normalize)
            .filter(|r| owner.holds(r))
            .collect())
    }

    fn top_level(
        &self,
        caller: &Caller,
        repo: &RepoRecord,
        git_ref: Option<&str>,
    ) -> Result<Vec<String>, ForgeError> {
        let path = repo.path_with_namespace.as_str();
        let (owner, name) = owner_name(path);
        let git_ref = git_ref
            .filter(|r| !r.is_empty())
            .or(repo.default_branch.as_deref())
            .unwrap_or("HEAD");
        let api = format!(
            "repos/{}/{}/git/trees/{}",
            quote(owner),
            quote(name),
            quote(git_ref)
        );
        let answer = match self.0.send(caller, &Call::get(&api, LIST_TIMEOUT)) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => {
                return Err(ForgeError::transport(format!(
                    "listing tree for {path}@{git_ref} {why}"
                )));
            }
            Err(no @ (NoAnswer::Missing(_) | NoAnswer::Refused(_) | NoAnswer::HeldBack { .. })) => {
                return Err(no.error(no.said().to_string()));
            }
        };
        if !answer.ok() {
            return Err(ForgeError::new(format!(
                "listing tree for {path}@{git_ref} failed: {}",
                answer.said(Kind::GitHub)
            )));
        }
        if answer.out.trim().is_empty() {
            return Ok(Vec::new());
        }
        let data: Value = serde_json::from_str(&answer.out).map_err(|e| {
            ForgeError::new(format!(
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

    fn about(&self, caller: &Caller, repo: &RepoRecord) -> Result<About, ForgeError> {
        let path = repo.path_with_namespace.as_str();
        let (owner, name) = owner_name(path);
        let api = format!("repos/{}/{}", quote(owner), quote(name));
        let doing = format!("reading {path}");
        let answer = self.0.ask(caller, &Call::get(&api, LIST_TIMEOUT), &doing)?;
        // `visibility` names `internal` too; a reply without it still says `private`.
        let visibility = match answer["visibility"].as_str() {
            Some(word) => Visibility::parse(word),
            None => answer["private"].as_bool().map(|private| {
                if private {
                    Visibility::Private
                } else {
                    Visibility::Public
                }
            }),
        }
        .ok_or_else(|| ForgeError::new(format!("{doing}: GitHub named no visibility")))?;
        // Anyone who can read a repo can open an issue in it, unless its issues are off. A
        // `permissions` block that denies reading is the account's own answer.
        let issues = if answer["archived"].as_bool() == Some(true) {
            Issues::Archived
        } else if answer["has_issues"].as_bool() == Some(false) {
            Issues::Off
        } else if answer["permissions"]["pull"].as_bool() == Some(false) {
            Issues::NoRight
        } else {
            Issues::Open
        };
        // `security_and_analysis` is in the answer only for an account with admin rights.
        let push_protection = PushProtection::of(
            match answer["security_and_analysis"]["secret_scanning_push_protection"]["status"]
                .as_str()
            {
                Some("enabled") => Some(true),
                Some("disabled") => Some(false),
                _ => None,
            },
        );
        Ok(About {
            visibility,
            issues,
            push_protection,
        })
    }
}

impl WorkItems for GitHub {
    fn create(
        &self,
        caller: &Caller,
        path: &str,
        new: &NewWorkItem,
    ) -> Result<crate::work::WorkItem, ForgeError> {
        let issue = self.create_issue(
            caller,
            path,
            &work::NewIssue {
                title: new.title.clone(),
                body: Some(new.body.clone()),
                labels: new
                    .workspace_label
                    .iter()
                    .map(|ws| super::backend::workspace_label(Kind::GitHub, ws))
                    .collect(),
                ..work::NewIssue::default()
            },
        )?;
        read::item_of(issue, &format!("opening an issue in {path}"))
    }

    fn read(
        &self,
        caller: &Caller,
        path: &str,
        number: u64,
    ) -> Result<crate::work::WorkItem, ForgeError> {
        self.read_item(caller, path, number)
    }
}

impl Requests for GitHub {
    fn open_or_update(
        &self,
        caller: &Caller,
        path: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<Opened, ForgeError> {
        let (owner, name) = owner_name(path);
        let pulls = format!("repos/{}/{}/pulls", quote(owner), quote(name));
        let lookup = format!(
            "{pulls}?state=open&head={}:{}&base={}&per_page=1",
            quote(owner),
            quote(head),
            quote(base)
        );
        let doing = format!("looking for an open pull request from {head} into {base}");
        let found = self
            .0
            .ask(caller, &Call::get(&lookup, LIST_TIMEOUT), &doing)?;
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
        let pr = pr_of(
            &self.0.ask(caller, &call, &doing)?,
            "number",
            "html_url",
            &doing,
        )?;
        Ok(Opened { pr, ours: true })
    }

    fn state(&self, caller: &Caller, path: &str, pr: &Pr) -> Result<State, ForgeError> {
        let (owner, name) = owner_name(path);
        let api = format!("repos/{}/{}/pulls/{}", quote(owner), quote(name), pr.number);
        let doing = format!("reading pull request #{} of {path}", pr.number);
        let record = self.0.ask(caller, &Call::get(&api, LIST_TIMEOUT), &doing)?;
        state_of(&record, &doing)
    }

    fn body(&self, caller: &Caller, path: &str, pr: &Pr) -> Result<String, ForgeError> {
        let (owner, name) = owner_name(path);
        let api = format!("repos/{}/{}/pulls/{}", quote(owner), quote(name), pr.number);
        let doing = format!(
            "reading pull request #{}'s description in {path}",
            pr.number
        );
        let record = self.0.ask(caller, &Call::get(&api, LIST_TIMEOUT), &doing)?;
        // GitHub answers `"body": null` for a pull request opened with none.
        Ok(record["body"].as_str().unwrap_or_default().to_string())
    }

    fn set_body(&self, caller: &Caller, path: &str, pr: &Pr, body: &str) -> Result<(), ForgeError> {
        let (owner, name) = owner_name(path);
        let api = format!("repos/{}/{}/pulls/{}", quote(owner), quote(name), pr.number);
        let doing = format!(
            "writing pull request #{}'s description in {path}",
            pr.number
        );
        let call = Call::write(Method::Patch, api, vec![Field::text("body", body)]);
        self.0.ask(caller, &call, &doing).map(|_| ())
    }

    fn by_head(
        &self,
        caller: &Caller,
        path: &str,
        branch: &str,
    ) -> Result<Option<Request>, ForgeError> {
        let (owner, name) = owner_name(path);
        let api = format!(
            "repos/{}/{}/pulls?state=all&head={}:{}&per_page=1",
            quote(owner),
            quote(name),
            quote(owner),
            quote(branch)
        );
        let doing = format!("finding the pull request from {branch} in {path}");
        let listing = self.0.ask(caller, &Call::get(&api, LIST_TIMEOUT), &doing)?;
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
                return Err(ForgeError::new(format!(
                    "{doing}: the forge answered a pull request from {}:{}, not from this \
                     branch",
                    from.1.unwrap_or("?"),
                    from.0.unwrap_or("?")
                )));
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
            state: state_of(record, &doing)?,
            head: head.to_string(),
        }))
    }

    fn request_auto_merge(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
    ) -> Result<AutoMerge, ForgeError> {
        let (owner, name) = owner_name(path);
        let number = pr.number.to_string();
        let doing = format!("reading how {path} merges");
        let answer = self.0.ask(
            caller,
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
            return Err(ForgeError::new(format!(
                "{path} does not allow auto-merge. Turn on \"Allow auto-merge\" in its \
                 settings, or choose the pr mode"
            )));
        }
        let Some(&(_, method)) = GITHUB_METHODS
            .iter()
            .find(|(setting, _)| settings[*setting] == Value::Bool(true))
        else {
            return Err(ForgeError::new(format!("{path} allows no merge method")));
        };
        let Some(id) = settings["pullRequest"]["id"].as_str() else {
            return Err(ForgeError::new(format!(
                "{doing}: no pull request #{number}"
            )));
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
            self.0.said(caller, &call),
            &GITHUB_NOTHING_TO_WAIT_FOR,
            &format!("asking {path} to auto-merge #{number}"),
        )
    }

    /// `isMergeQueueEnabled`: GitHub's own answer for the pull request's base branch, so a
    /// queue that a ruleset turns on is seen as well as one a branch protection rule does.
    fn lands_through_queue(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
    ) -> Result<bool, ForgeError> {
        let found = self.queue_probe(caller, path, pr)?;
        match found["isMergeQueueEnabled"].as_bool() {
            Some(queued) => Ok(queued),
            None => Err(ForgeError::new(format!(
                "reading whether #{} of {path} lands through a merge queue: GitHub did not say",
                pr.number
            ))),
        }
    }

    /// REST's merge, with `sha`: GitHub answers 409 and merges nothing when the head is no
    /// longer that commit. The subject and body are charter's, so the trailer is on the commit.
    fn merge_at(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
        how: &MergeAs,
    ) -> Result<MergedAt, ForgeError> {
        let (owner, name) = owner_name(path);
        let api = format!(
            "repos/{}/{}/pulls/{}/merge",
            quote(owner),
            quote(name),
            pr.number
        );
        let method = if how.squash { "squash" } else { "merge" };
        let call = Call::write(
            Method::Put,
            api,
            vec![
                Field::text("merge_method", method),
                Field::text("commit_title", &how.title),
                Field::text("commit_message", &how.message),
                Field::text("sha", head_sha),
            ],
        );
        let doing = format!("merging pull request #{} of {path} ({method})", pr.number);
        let answer = self.0.ask(caller, &call, &doing)?;
        if answer["merged"] != Value::Bool(true) {
            return Err(ForgeError::new(format!(
                "{doing}: GitHub did not confirm a merge ({})",
                answer["message"].as_str().unwrap_or("no message")
            )));
        }
        Ok(MergedAt::Now)
    }

    /// `enqueuePullRequest` with `expectedHeadOid`. The queue merges by the method its own
    /// rule sets and writes its own message, so `how` is not GitHub's to take here.
    fn enqueue_at(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
        _how: &MergeAs,
    ) -> Result<(), ForgeError> {
        let found = self.queue_probe(caller, path, pr)?;
        let Some(id) = found["id"].as_str() else {
            return Err(ForgeError::new(format!(
                "reading #{} of {path}: GitHub named no pull request id",
                pr.number
            )));
        };
        let doing = format!(
            "putting #{} of {path} in its merge queue at {head_sha}",
            pr.number
        );
        let answer = self.0.ask(
            caller,
            &Call::graphql(
                graphql::enqueue::QUERY,
                vec![Field::text("id", id), Field::text("head", head_sha)],
                LIST_TIMEOUT,
            ),
            &doing,
        )?;
        let entry = &answer["data"]["enqueuePullRequest"]["mergeQueueEntry"];
        if entry["headCommit"]["oid"].as_str() != Some(head_sha) {
            return Err(ForgeError::new(format!(
                "{doing}: GitHub did not answer with an entry at that commit"
            )));
        }
        Ok(())
    }

    /// By the commit alone: GitHub's check runs and commit statuses hang off the sha, not the
    /// pull request, so the request's number is not asked for.
    fn checks_at(&self, caller: &Caller, path: &str, sha: &str, _request: u64) -> Checks {
        checks::guarded(sha, || {
            let (owner, name) = owner_name(path);
            let base = format!("repos/{}/{}/commits/{sha}", quote(owner), quote(name));
            let runs = self.0.ask(
                caller,
                &Call::get(format!("{base}/check-runs?per_page=100"), LIST_TIMEOUT),
                &format!("reading the check runs at {sha} of {path}"),
            )?;
            let statuses = self.0.ask(
                caller,
                &Call::get(format!("{base}/status?per_page=100"), LIST_TIMEOUT),
                &format!("reading the commit statuses at {sha} of {path}"),
            )?;
            Ok(checks::github(&runs, &statuses)?)
        })
    }

    fn open_on_branch(
        &self,
        caller: &Caller,
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
        first_field(self.0.best_effort(caller, &asked), "number")
    }

    fn ci_word(&self, caller: &Caller, path: &str, branch: &str) -> Result<Option<String>, Raised> {
        self.rollup(caller, path, branch)
    }
}

impl Capabilities for GitHub {
    /// What GitHub is known to have without asking. github.com has sub-issues, dependencies,
    /// boards and iterations, said of the instance only: whether one owner or repo may use them
    /// turns on its plan and settings, which FG-2's probes ask, so a narrower reach is
    /// `Unknown` until then and takes its fallback. A GHES has what its version has: `Unknown`.
    /// GitHub has no epics anywhere; sub-issues are how it nests work.
    fn support(&self, _caller: &Caller, at: &Reach, what: Capability) -> Support {
        let dotcom = self.0.forge.host.eq_ignore_ascii_case("github.com");
        match what {
            // GitHub has no epics, and an issue has no status of its own, only a Projects v2
            // board's.
            Capability::Epics | Capability::ItemStatus => {
                Support::Unavailable(Unavailable::because(what, Reason::NotOnThisForge))
            }
            Capability::SubIssues
            | Capability::Dependencies
            | Capability::Boards
            | Capability::Iterations
            | Capability::CloseReasons
                if dotcom && *at == Reach::Instance =>
            {
                Support::Available
            }
            _ => Support::Unknown(UnknownWhy::NotProbed),
        }
    }
}

#[cfg(test)]
mod capability_tests {
    use std::sync::Arc;

    use super::*;
    use crate::forge::Forge;
    use crate::forge::backend::{Fallback, Fixed, Taken};
    use crate::forge::recorded::Recorded;

    fn on(host: &str) -> GitHub {
        let recorded = Recorded::parse(r#"{"source": "none", "exchanges": []}"#).unwrap();
        let mut forge = Forge::default_of(Kind::GitHub);
        forge.host = host.to_string();
        GitHub(Asker {
            forge,
            transports: Arc::new(Fixed(Arc::new(recorded))),
        })
    }

    #[test]
    fn github_com_has_sub_issues_said_of_the_instance_and_unknown_of_a_repo() {
        let github = on("github.com");
        let me = Caller::window();
        assert_eq!(
            github.support(&me, &Reach::Instance, Capability::SubIssues),
            Support::Available
        );
        let repo = Reach::Repo("o/r".into());
        assert_eq!(
            github.support(&me, &repo, Capability::SubIssues),
            Support::Unknown(UnknownWhy::NotProbed)
        );
        assert_eq!(
            github
                .support(&me, &repo, Capability::SubIssues)
                .taken(Capability::SubIssues),
            Taken::Fallback(Capability::SubIssues.fallback())
        );
    }

    #[test]
    fn github_com_keeps_a_close_reason_and_has_no_epics_at_all() {
        let github = on("github.com");
        let me = Caller::window();
        assert_eq!(
            github.support(&me, &Reach::Instance, Capability::CloseReasons),
            Support::Available
        );
        for at in [Reach::Instance, Reach::Repo("o/r".into())] {
            assert_eq!(
                github.support(&me, &at, Capability::Epics),
                Support::Unavailable(Unavailable::because(
                    Capability::Epics,
                    Reason::NotOnThisForge
                ))
            );
        }
        assert_eq!(Capability::CloseReasons.fallback(), Fallback::Hidden);
    }

    #[test]
    fn an_issue_has_no_status_of_its_own_on_any_github_only_its_boards() {
        for host in ["github.com", "ghe.example.com"] {
            let said =
                on(host).support(&Caller::window(), &Reach::Instance, Capability::ItemStatus);
            assert_eq!(
                said,
                Support::Unavailable(Unavailable::because(
                    Capability::ItemStatus,
                    Reason::NotOnThisForge
                )),
                "{host}"
            );
            assert_eq!(
                said.taken(Capability::ItemStatus),
                Taken::Fallback(Fallback::BoardStatus)
            );
        }
    }

    #[test]
    fn a_ghes_has_what_its_version_has_which_charter_has_not_asked() {
        let github = on("ghe.example.com");
        for what in Capability::ALL {
            assert_ne!(
                github.support(&Caller::window(), &Reach::Instance, what),
                Support::Available,
                "{what:?}"
            );
        }
    }

    #[test]
    fn an_unknown_capability_takes_its_fallback_and_never_the_feature() {
        for what in Capability::ALL {
            assert_eq!(
                Support::Unknown(UnknownWhy::NotProbed).taken(what),
                Taken::Fallback(what.fallback())
            );
            assert_eq!(Support::Available.taken(what), Taken::Feature);
        }
    }
}

#[cfg(test)]
mod create_mapping_tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::forge::Forge;
    use crate::forge::Kind;
    use crate::forge::backend::{Caller, ForgeRef, NewWorkItem};
    use crate::forge::recorded::Recorded;
    use crate::work::Milestone;

    fn created(answer: serde_json::Value) -> crate::work::WorkItem {
        let call = json!({"endpoint": {"rest": {"method": "POST", "path": "repos/acme/api/issues"}},
                          "fields": [{"text": ["title", "Port the picker"]},
                                     {"text": ["body", "Why."]}]});
        let text = json!({"source": "GitHub REST API version 2022-11-28, issues: Create an issue",
                          "exchanges": [{"call": call, "reply": {"code": 0, "out": answer.to_string()}}]});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let backend = Forge::default_of(Kind::GitHub).backend_over(recorded);
        let new = NewWorkItem {
            title: "Port the picker".into(),
            body: "Why.".into(),
            workspace_label: None,
        };
        backend
            .create(&Caller::command(), "acme/api", &new)
            .unwrap()
    }

    fn issue() -> serde_json::Value {
        json!({"id": 1012, "node_id": "I_kwDOAcme12", "number": 12, "title": "Port the picker",
               "state": "open", "html_url": "https://github.com/acme/api/issues/12"})
    }

    #[test]
    fn a_milestone_in_githubs_answer_is_mapped_with_its_node_id_and_due_day() {
        let mut answer = issue();
        answer["milestone"] = json!({"number": 1, "node_id": "MI_kwDOAcme1", "title": "v1",
                                     "state": "open", "due_on": "2026-10-31T07:00:00Z"});
        assert_eq!(
            created(answer).milestone,
            Some(Milestone {
                forge_ref: Some(ForgeRef("MI_kwDOAcme1".into())),
                title: "v1".into(),
                due: chrono::NaiveDate::from_ymd_opt(2026, 10, 31),
            })
        );
    }

    #[test]
    fn no_milestone_or_no_due_date_in_githubs_answer_maps_to_none() {
        assert_eq!(created(issue()).milestone, None);
        let mut answer = issue();
        answer["milestone"] = json!({"number": 1, "title": "v1", "state": "open", "due_on": null});
        let milestone = created(answer).milestone.unwrap();
        assert_eq!((milestone.forge_ref, milestone.due), (None, None));
    }
}
