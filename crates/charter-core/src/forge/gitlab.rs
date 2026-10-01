//! The GitLab backend: every operation of [`super::backend`]'s areas, as GitLab's REST API
//! (v4, checked against the GitLab 19.4 documentation) spells it. Each request is built once,
//! as a [`Call`], and sent by whichever transport the backend was built over.

use serde_json::Value;

use super::backend::{
    Asker, Caller, Capabilities, Capability, Reach, Reason, Repos, Requests, Support, Unavailable,
};
use super::checks::{self, Checks};
use super::pr::{
    AutoMerge, GITLAB_ACTIVE, GITLAB_NOT_MERGEABLE, Opened, Pr, Request, State, commit_named,
    is_ours, not_queued_when, own_mr, pr_of, unknown_state,
};
use super::transport::{Call, Field, Method};
use super::{
    ForgeError, Kind, LIST_TIMEOUT, Raised, falsy, first_field, mapped, py_str, quote, truthy,
    word_of,
};

/// The GitLab backend.
pub(super) struct GitLab(pub(super) Asker);

/// GitLab's pipeline `status` → charter's neutral vocabulary. Python's `gitlab._CI_MAP`, plus
/// the two statuses GitLab added after it (GitLab 19.4 `doc/api/pipelines.md`, the `status`
/// filter): `waiting_for_callback` waits as `pending` does, and `canceling` is on its way to
/// `canceled`.
const CI: [(&str, &str); 13] = [
    ("success", "success"),
    ("failed", "failed"),
    ("running", "running"),
    ("canceled", "canceled"),
    ("canceling", "canceled"),
    ("skipped", "skipped"),
    ("manual", "manual"),
    ("pending", "pending"),
    ("created", "pending"),
    ("preparing", "pending"),
    ("waiting_for_resource", "pending"),
    ("waiting_for_callback", "pending"),
    ("scheduled", "pending"),
];

impl GitLab {
    /// Every page of a GitLab listing, `path_of` naming each page's path.
    fn paged(
        &self,
        caller: &Caller,
        owner: &str,
        path_of: impl Fn(usize) -> String,
    ) -> Result<Vec<Value>, ForgeError> {
        let mut out = Vec::new();
        let mut page = 1;
        loop {
            let path = path_of(page);
            let batch = self
                .0
                .strict(caller, &path, "GitLab API call")
                .map_err(|e| {
                    let said = format!("listing repos for GitLab group '{owner}' failed: {e}");
                    e.reworded(said)
                })?;
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
}

/// One GitLab project in the shape every backend produces (Python's `_normalize`).
fn normalize(raw: &Value) -> Value {
    let get = |key: &str| raw.get(key).cloned().unwrap_or(Value::Null);
    let text_or_empty = |key: &str| match raw.get(key) {
        Some(v) if truthy(v) => v.clone(),
        _ => Value::String(String::new()),
    };
    let name = match raw.get("path") {
        Some(v) if truthy(v) => v.clone(),
        _ => get("name"),
    };
    let topics = match raw.get("topics") {
        Some(v) if truthy(v) => v.clone(),
        _ => Value::Array(Vec::new()),
    };
    serde_json::json!({
        "id": get("id"),
        "name": name,
        "path_with_namespace": get("path_with_namespace"),
        "default_branch": get("default_branch"),
        "description": text_or_empty("description"),
        "web_url": text_or_empty("web_url"),
        "ssh_url": text_or_empty("ssh_url_to_repo"),
        "topics": topics,
        "forge": Kind::GitLab.word(),
    })
}

/// The [`State`] a GitLab merge request record says. A merged MR names its squash commit, else
/// its merge commit; a fast-forward merge names neither.
fn state_of(record: &Value, doing: &str) -> Result<State, ForgeError> {
    match record["state"].as_str().unwrap_or("") {
        "opened" | "locked" => Ok(State::Open),
        "closed" => Ok(State::Closed),
        "merged" => Ok(State::Merged {
            commit: commit_named(record, "squash_commit_sha")
                .or_else(|| commit_named(record, "merge_commit_sha")),
        }),
        _ => Err(unknown_state(record, doing)),
    }
}

impl Repos for GitLab {
    fn owned(&self, caller: &Caller, owner: &str) -> Result<Vec<Value>, ForgeError> {
        let enc = quote(owner);
        let raw = self.paged(caller, owner, |page| {
            format!(
                "groups/{enc}/projects?per_page=100&page={page}&include_subgroups=true&archived=false"
            )
        })?;
        Ok(raw.iter().map(normalize).collect())
    }

    fn reachable(&self, caller: &Caller, owner: &str) -> Result<Vec<Value>, ForgeError> {
        let raw = self.paged(caller, owner, |page| {
            format!("projects?membership=true&archived=false&per_page=100&page={page}")
        })?;
        Ok(super::under_owner(raw.iter().map(normalize), owner))
    }

    fn top_level(
        &self,
        caller: &Caller,
        repo: &Value,
        git_ref: Option<&str>,
    ) -> Result<Vec<String>, ForgeError> {
        let rid = quote(&py_str(repo.get("id").unwrap_or(&Value::Null)));
        let ref_q = git_ref
            .filter(|r| !r.is_empty())
            .map(|r| format!("&ref={}", quote(r)))
            .unwrap_or_default();
        let mut out = Vec::new();
        let mut page = 1;
        loop {
            let path = format!("projects/{rid}/repository/tree?per_page=100&page={page}{ref_q}");
            let batch = self.0.strict(caller, &path, "GitLab API call")?;
            let items = batch.as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                break;
            }
            let short = items.len() < 100;
            out.extend(items.iter().map(|e| {
                e.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            }));
            if short {
                break;
            }
            page += 1;
        }
        Ok(out)
    }
}

impl Requests for GitLab {
    fn open_or_update(
        &self,
        caller: &Caller,
        path: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<Opened, ForgeError> {
        let mrs = format!("projects/{}/merge_requests", quote(path));
        let lookup = format!(
            "{mrs}?state=opened&source_branch={}&target_branch={}&per_page=100",
            quote(head),
            quote(base)
        );
        let doing = format!("looking for an open merge request from {head} into {base}");
        let found = self
            .0
            .ask(caller, &Call::get(&lookup, LIST_TIMEOUT), &doing)?;
        let call = match own_mr(&found) {
            Some(open) => {
                let mr = pr_of(open, "iid", "web_url", &doing)?;
                if !is_ours(head, open.get("description").and_then(Value::as_str)) {
                    return Ok(Opened {
                        pr: mr,
                        ours: false,
                    });
                }
                Call::write(
                    Method::Put,
                    format!("{mrs}/{}", mr.number),
                    vec![
                        Field::text("title", title),
                        Field::text("description", body),
                    ],
                )
            }
            None => Call::write(
                Method::Post,
                mrs,
                vec![
                    Field::text("source_branch", head),
                    Field::text("target_branch", base),
                    Field::text("title", title),
                    Field::text("description", body),
                ],
            ),
        };
        let doing = format!("opening a merge request from {head} into {base}");
        let pr = pr_of(
            &self.0.ask(caller, &call, &doing)?,
            "iid",
            "web_url",
            &doing,
        )?;
        Ok(Opened { pr, ours: true })
    }

    fn state(&self, caller: &Caller, path: &str, pr: &Pr) -> Result<State, ForgeError> {
        let api = format!("projects/{}/merge_requests/{}", quote(path), pr.number);
        let doing = format!("reading merge request !{} of {path}", pr.number);
        let record = self.0.ask(caller, &Call::get(&api, LIST_TIMEOUT), &doing)?;
        state_of(&record, &doing)
    }

    fn body(&self, caller: &Caller, path: &str, pr: &Pr) -> Result<String, ForgeError> {
        let api = format!("projects/{}/merge_requests/{}", quote(path), pr.number);
        let doing = format!(
            "reading merge request !{}'s description in {path}",
            pr.number
        );
        let record = self.0.ask(caller, &Call::get(&api, LIST_TIMEOUT), &doing)?;
        // GitLab answers `"description": null` for a merge request opened with none.
        Ok(record["description"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    fn set_body(&self, caller: &Caller, path: &str, pr: &Pr, body: &str) -> Result<(), ForgeError> {
        let api = format!("projects/{}/merge_requests/{}", quote(path), pr.number);
        let doing = format!(
            "writing merge request !{}'s description in {path}",
            pr.number
        );
        let call = Call::write(Method::Put, api, vec![Field::text("description", body)]);
        self.0.ask(caller, &call, &doing).map(|_| ())
    }

    fn by_head(
        &self,
        caller: &Caller,
        path: &str,
        branch: &str,
    ) -> Result<Option<Request>, ForgeError> {
        let api = format!(
            "projects/{}/merge_requests?source_branch={}&state=all&per_page=100",
            quote(path),
            quote(branch)
        );
        let doing = format!("finding the merge request from {branch} in {path}");
        let listing = self.0.ask(caller, &Call::get(&api, LIST_TIMEOUT), &doing)?;
        // GitLab matches `source_branch` by name across forks too, so only an MR from the
        // project itself is this member's.
        let found = own_mr(&listing);
        // A full page with none of the project's own is a page charter could not see past,
        // which is not "no merge request".
        let full = listing.as_array().is_some_and(|all| all.len() >= 100);
        if found.is_none() && full {
            return Err(ForgeError::new(format!(
                "{doing}: a hundred merge requests from forks share this branch name, and \
                 charter reads no further"
            )));
        }
        let Some(record) = found else {
            return Ok(None);
        };
        let pr = pr_of(record, "iid", "web_url", &doing)?;
        let head = record["sha"]
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

    /// Merge when the pipeline succeeds, with `sha`. GitLab merges *at once* when told that
    /// while no pipeline is running, so the MR's pipeline is read first and a finished or
    /// missing one is `NotQueued`. A GitLab project has one merge method, so the only choice
    /// is squash, asked for only when the project always squashes.
    ///
    /// `merge_when_pipeline_succeeds` rather than `auto_merge`: GitLab deprecated it in 17.11
    /// and still accepts it in 19.4 (`doc/api/merge_requests.md`, "Merge a merge request"),
    /// and a GitLab older than `auto_merge` ignores a parameter it does not know, so sending
    /// that one would merge at once rather than refuse.
    fn request_auto_merge(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
    ) -> Result<AutoMerge, ForgeError> {
        let project = format!("projects/{}", quote(path));
        let doing = format!("reading how {path} merges");
        let settings = self
            .0
            .ask(caller, &Call::get(&project, LIST_TIMEOUT), &doing)?;
        let squash = if settings["squash_option"] == "always" {
            "true"
        } else {
            "false"
        };
        let mr = format!("{project}/merge_requests/{}", pr.number);
        let doing = format!("reading !{} of {path}", pr.number);
        let record = self.0.ask(caller, &Call::get(&mr, LIST_TIMEOUT), &doing)?;
        let pipeline = record["head_pipeline"]["status"].as_str().unwrap_or("");
        if !GITLAB_ACTIVE.contains(&pipeline) {
            return Ok(AutoMerge::NotQueued(format!(
                "!{} has no pipeline running, so there is nothing for GitLab to wait for",
                pr.number
            )));
        }
        let call = Call::write(
            Method::Put,
            format!("{mr}/merge"),
            vec![
                Field::typed("merge_when_pipeline_succeeds", "true"),
                Field::typed("squash", squash),
                Field::text("sha", head_sha),
            ],
        );
        not_queued_when(
            self.0.said(caller, &call),
            &GITLAB_NOT_MERGEABLE,
            &format!("asking {path} to merge !{} when it passes", pr.number),
        )
    }

    fn checks_at(&self, caller: &Caller, path: &str, sha: &str, request: u64) -> Checks {
        checks::guarded(sha, || {
            let answer = self.0.ask(
                caller,
                &Call::get(
                    format!(
                        "projects/{}/merge_requests/{request}/pipelines?per_page=100",
                        quote(path)
                    ),
                    LIST_TIMEOUT,
                ),
                &format!("reading the pipelines of merge request !{request} of {path}"),
            )?;
            Ok(checks::gitlab(&answer, sha, request)?)
        })
    }

    fn open_on_branch(
        &self,
        caller: &Caller,
        path: &str,
        branch: &str,
    ) -> Result<Option<Value>, Raised> {
        // GitLab matches `source_branch` by name in forks too, so only the project's own merge
        // request is the branch's, as `by_head` reads it. The first page of a hundred is read;
        // past it, the status line shows none rather than a stranger's.
        let asked = format!(
            "projects/{}/merge_requests?state=opened&source_branch={}&per_page=100",
            quote(path),
            quote(branch)
        );
        let Some(answer) = self.0.best_effort(caller, &asked) else {
            return Ok(None);
        };
        if falsy(&answer) {
            return Ok(None);
        }
        if !answer.is_array() {
            return Err(Raised);
        }
        Ok(own_mr(&answer).and_then(|mr| mr.get("iid").cloned()))
    }

    fn ci_word(&self, caller: &Caller, path: &str, branch: &str) -> Result<Option<String>, Raised> {
        let asked = format!(
            "projects/{}/pipelines?ref={}&per_page=1",
            quote(path),
            quote(branch)
        );
        let status = first_field(self.0.best_effort(caller, &asked), "status")?;
        Ok(mapped(&CI, word_of(status.as_ref())))
    }
}

impl Capabilities for GitLab {
    /// Not built yet (W7: the GitLab twin, FW-2b, ships one release after GitHub's): every
    /// capability is unavailable for that reason, and takes its fallback.
    fn support(&self, _caller: &Caller, _at: &Reach, what: Capability) -> Support {
        Support::Unavailable(Unavailable::because(what, Reason::NotYetBuilt))
    }
}
