//! The GitLab backend: every operation of [`super::backend`]'s areas, as GitLab's REST API
//! (v4, checked against the GitLab 19.4 documentation) and its GraphQL API spell it. Each
//! request is built once, as a [`Call`], and sent by whichever transport the backend was built
//! over: `glab api`, or the native transport ([`super::http`]) with charter's own sign-in.

use serde_json::Value;

use super::backend::{
    About, Asker, Caller, Capabilities, Capability, ForgeRef, Issues, NewWorkItem, Owner,
    PushProtection, Reach, RepoRecord, Repos, Requests, Support, UnknownWhy, Visibility, WorkItems,
};
use super::checks::{self, Checks};
use super::pr::{
    AutoMerge, GITLAB_ACTIVE, GITLAB_NOT_MERGEABLE, MergeAs, MergedAt, Opened, Pr, Request, State,
    commit_named, is_ours, not_queued_when, own_mr, pr_of, unknown_state,
};
use super::transport::{Call, Field, Method};
use super::{
    ForgeError, Kind, LIST_TIMEOUT, Raised, falsy, first_field, listed_branch, listed_description,
    listed_id, listed_str, listed_topics, mapped, quote, truthy, word_of,
};

mod read;
// `read` calls the boards listing; the rest of the work items wait for their first caller
// (#1202), and until then only their tests reach them, so the compiler would call them dead.
#[allow(dead_code)]
pub(crate) mod work;

/// The GitLab backend.
pub(super) struct GitLab(pub(super) Asker);

/// Whether a merge request record says GitLab will merge it later, by either spelling.
fn set_to_merge_later(record: &Value) -> bool {
    truthy(&record["merge_when_pipeline_succeeds"]) || truthy(&record["auto_merge_enabled"])
}

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

/// One GitLab repo as a neutral record (Python's `_normalize`). Its name is its `path`, the
/// name in its URL, else its display `name`.
fn normalize(raw: &Value) -> RepoRecord {
    let name = Some(listed_str(raw, "path"))
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| listed_str(raw, "name"));
    RepoRecord {
        id: listed_id(raw),
        name,
        path_with_namespace: listed_str(raw, "path_with_namespace"),
        default_branch: listed_branch(raw),
        description: listed_description(raw),
        web_url: listed_str(raw, "web_url"),
        ssh_url: listed_str(raw, "ssh_url_to_repo"),
        topics: listed_topics(raw),
        forge: Kind::GitLab,
    }
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
    fn owned(&self, caller: &Caller, owner: &Owner) -> Result<Vec<RepoRecord>, ForgeError> {
        let owner = owner.as_str();
        let enc = quote(owner);
        let raw = self.paged(caller, owner, |page| {
            format!(
                "groups/{enc}/projects?per_page=100&page={page}&include_subgroups=true&archived=false"
            )
        })?;
        Ok(raw.iter().map(normalize).collect())
    }

    fn reachable(&self, caller: &Caller, owner: &Owner) -> Result<Vec<RepoRecord>, ForgeError> {
        let raw = self.paged(caller, owner.as_str(), |page| {
            format!("projects?membership=true&archived=false&per_page=100&page={page}")
        })?;
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
        let Some(ForgeRef(id)) = &repo.id else {
            return Err(ForgeError::new(format!(
                "GitLab gave no id for {}, so its tree cannot be listed",
                repo.path_with_namespace
            )));
        };
        let rid = quote(id);
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

    fn about(&self, caller: &Caller, repo: &RepoRecord) -> Result<About, ForgeError> {
        let path = repo.path_with_namespace.as_str();
        let api = format!("projects/{}", quote(path));
        let doing = format!("reading {path}");
        let answer = self.0.ask(caller, &Call::get(&api, LIST_TIMEOUT), &doing)?;
        let visibility = answer["visibility"]
            .as_str()
            .and_then(Visibility::parse)
            .ok_or_else(|| ForgeError::new(format!("{doing}: GitLab named no visibility")))?;
        // `issues_access_level` is `disabled`, `private` (members only) or `enabled`; the older
        // `issues_enabled` says only whether they are on. A member of the repo has an access
        // level in `permissions`, given on the repo itself (`project_access`, GitLab's word) or
        // through its group.
        let member = ["project_access", "group_access"].iter().any(|via| {
            answer["permissions"][via]["access_level"]
                .as_u64()
                .is_some()
        });
        let issues = match answer["issues_access_level"].as_str() {
            _ if answer["archived"].as_bool() == Some(true) => Issues::Archived,
            Some("disabled") => Issues::Off,
            _ if answer["issues_enabled"].as_bool() == Some(false) => Issues::Off,
            Some("private") if !member => Issues::NoRight,
            _ => Issues::Open,
        };
        // Named `pre_receive_secret_detection_enabled` before GitLab 18.0, and in the answer
        // only for an account that may change it.
        let push_protection = PushProtection::of(
            answer["secret_push_protection_enabled"]
                .as_bool()
                .or_else(|| answer["pre_receive_secret_detection_enabled"].as_bool()),
        );
        Ok(About {
            visibility,
            issues,
            push_protection,
        })
    }
}

impl WorkItems for GitLab {
    fn create(
        &self,
        caller: &Caller,
        path: &str,
        new: &NewWorkItem,
    ) -> Result<crate::work::WorkItem, ForgeError> {
        let mut fields = vec![
            Field::text("title", &new.title),
            Field::text("description", &new.body),
        ];
        if let Some(ws) = &new.workspace_label {
            fields.push(Field::text(
                "labels",
                &super::backend::workspace_label(Kind::GitLab, ws),
            ));
        }
        let doing = format!("opening an issue in {path}");
        let call = Call::write(
            Method::Post,
            format!("projects/{}/issues", quote(path)),
            fields,
        );
        let issue = self.0.ask(caller, &call, &doing)?;
        item_of(&issue, Some(&new.title), &doing)
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

/// GitLab's answer for an issue, in the neutral model: its key, ids, title, state, labels and
/// milestone. A title it leaves out is `sent`, the one charter sent; with none sent, an answer
/// with no title is malformed, as GitHub's is.
fn item_of(
    issue: &serde_json::Value,
    sent: Option<&str>,
    doing: &str,
) -> Result<crate::work::WorkItem, ForgeError> {
    let url = issue["web_url"].as_str().unwrap_or_default().to_string();
    let iid = issue["iid"].as_u64();
    // The path as GitLab spells it, from its own full reference: `group/sub/repo#12`.
    let full = issue["references"]["full"].as_str().unwrap_or_default();
    let place = full.rsplit_once('#').map(|(place, _)| place);
    let host = crate::work::key::page_of(&url).map(|(host, _)| host);
    let (Some(iid), Some(place), Some(host)) = (iid, place, host) else {
        return Err(ForgeError::new(format!(
            "{doing}: GitLab's answer names no page and reference purlis can key the issue by"
        )));
    };
    let key = crate::work::TrackerKey::gitlab_issue(&host, place, iid)
        .map_err(|why| ForgeError::new(format!("{doing}: {why}")))?;
    // GitHub's answer without a title is refused as malformed. GitLab's is read field by field:
    // a title it leaves out of a create's answer is the one charter sent, and a read's is refused.
    let Some(title) = issue["title"].as_str().or(sent) else {
        return Err(ForgeError::new(format!(
            "{doing}: GitLab's answer names no title"
        )));
    };
    let mut item = crate::work::WorkItem::new(key, crate::work::Kind::Issue, title);
    item.forge_ref = issue["id"].as_u64().map(|id| ForgeRef(id.to_string()));
    item.url = url;
    item.state = crate::work::State::of_forge(issue["state"].as_str().unwrap_or_default());
    item.labels = issue["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l.as_str().map(str::to_string))
        .collect();
    let milestone = &issue["milestone"];
    item.milestone = milestone["title"].as_str().map(|title| {
        crate::work::Milestone::of_forge(
            milestone["id"].as_u64().map(|id| ForgeRef(id.to_string())),
            title,
            milestone["due_date"].as_str(),
        )
    });
    Ok(item)
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
                 purlis reads no further"
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

    /// The repo's `merge_trains_enabled`. **A repo that does not name it fails closed**: the
    /// field is missing when the token cannot read the repo's settings or the GitLab tier does
    /// not report it, and merging directly where a train may exist would skip the train.
    fn lands_through_queue(
        &self,
        caller: &Caller,
        path: &str,
        _pr: &Pr,
    ) -> Result<bool, ForgeError> {
        let repo = format!("projects/{}", quote(path));
        let doing = format!("reading whether {path} lands through a merge train");
        let settings = self
            .0
            .ask(caller, &Call::get(&repo, LIST_TIMEOUT), &doing)?;
        settings["merge_trains_enabled"].as_bool().ok_or_else(|| {
            ForgeError::new(format!(
                "{doing}: GitLab did not say. Its answer has no merge_trains_enabled, which \
                 GitLab leaves out for a token that cannot read the repo's settings and on a \
                 tier that does not report merge trains. purlis will not merge directly where a \
                 train may exist, so this member is landed by a person"
            ))
        })
    }

    /// The merge call with `sha`, and **no** `merge_when_pipeline_succeeds` or `auto_merge`:
    /// either would have GitLab merge later, at whatever head the branch then has, and a
    /// later push does not undo it. GitLab answers 409 and merges nothing when the head has
    /// moved past `sha`. Its merge method is the repo's own, so the only choice is squash.
    ///
    /// Should GitLab answer that it set the merge request to merge later anyway, charter
    /// cancels that at once and says so: a merge charter did not see happen is one it cannot
    /// vouch for.
    fn merge_at(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
        how: &MergeAs,
    ) -> Result<MergedAt, ForgeError> {
        let mr = format!("projects/{}/merge_requests/{}", quote(path), pr.number);
        let message = format!("{}\n\n{}", how.title, how.message);
        let mut fields = vec![
            Field::typed("squash", if how.squash { "true" } else { "false" }),
            Field::text("sha", head_sha),
            Field::text("merge_commit_message", &message),
        ];
        if how.squash {
            fields.push(Field::text("squash_commit_message", &message));
        }
        let doing = format!("merging merge request !{} of {path}", pr.number);
        let answer = self.0.ask(
            caller,
            &Call::write(Method::Put, format!("{mr}/merge"), fields),
            &doing,
        )?;
        if answer["state"] == "merged" {
            return Ok(MergedAt::Now);
        }
        if !set_to_merge_later(&answer) {
            return Err(ForgeError::new(format!(
                "{doing}: GitLab did not confirm a merge (state {:?})",
                answer["state"].as_str().unwrap_or("unknown")
            )));
        }
        let cancel = Call::write(
            Method::Post,
            format!("{mr}/cancel_merge_when_pipeline_succeeds"),
            Vec::new(),
        );
        if let Err(why) = match self.0.said(caller, &cancel) {
            Ok(said) => said,
            Err(no) => Err(no.said().to_string()),
        } {
            return Ok(MergedAt::Later(format!(
                "GitLab set !{} to merge later, and purlis could not cancel that ({why})",
                pr.number
            )));
        }
        // Confirmed by reading it again, not by the cancel's answer alone.
        let read = format!(
            "reading !{} of {path} after cancelling its merge",
            pr.number
        );
        match self.0.ask(caller, &Call::get(&mr, LIST_TIMEOUT), &read) {
            Ok(after) if !set_to_merge_later(&after) => Err(ForgeError::new(format!(
                "{doing}: GitLab set it to merge later instead of merging it, and purlis \
                 cancelled that"
            ))),
            Ok(_) => Ok(MergedAt::Later(format!(
                "GitLab set !{} to merge later, and it is still set after purlis cancelled it",
                pr.number
            ))),
            Err(why) => Ok(MergedAt::Later(format!(
                "GitLab set !{} to merge later, and purlis could not confirm the cancel ({})",
                pr.number,
                why.said()
            ))),
        }
    }

    /// Added to the merge train with `sha` and no `auto_merge`, so GitLab adds it now or
    /// refuses; the train then runs its own pipeline and merges it.
    fn enqueue_at(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
        how: &MergeAs,
    ) -> Result<(), ForgeError> {
        let api = format!(
            "projects/{}/merge_trains/merge_requests/{}",
            quote(path),
            pr.number
        );
        let doing = format!(
            "adding merge request !{} of {path} to its merge train at {head_sha}",
            pr.number
        );
        let answer = self.0.ask(
            caller,
            &Call::write(
                Method::Post,
                api,
                vec![
                    Field::typed("squash", if how.squash { "true" } else { "false" }),
                    Field::text("sha", head_sha),
                ],
            ),
            &doing,
        )?;
        let on_it = answer.as_array().is_some_and(|cars| {
            cars.iter()
                .any(|car| car["merge_request"]["iid"].as_u64() == Some(pr.number))
        });
        if !on_it {
            return Err(ForgeError::new(format!(
                "{doing}: GitLab's answer names no car for !{}",
                pr.number
            )));
        }
        Ok(())
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
    /// What GitLab is known to have without asking. gitlab.com has epics, iterations, boards,
    /// child items, blocking links, work item types, an item's own status and close reasons, said
    /// of the instance only: epics, iterations, blocking links and work item status (whose
    /// category says why a closed item was closed) need a Premium namespace, and whether one
    /// group or repo has them turns on its plan and settings, which FG-2's probes ask, so a
    /// narrower reach is `Unknown` until then and takes its fallback. On Free, an item marked a
    /// duplicate is still read as one. A self-managed GitLab has what its edition, licence and
    /// version have, which it shows a non-admin nowhere: `Unknown`.
    fn support(&self, _caller: &Caller, at: &Reach, what: Capability) -> Support {
        let dotcom = self.0.forge.host.eq_ignore_ascii_case("gitlab.com");
        match what {
            Capability::Epics
            | Capability::Iterations
            | Capability::Boards
            | Capability::SubIssues
            | Capability::Dependencies
            | Capability::IssueTypes
            | Capability::CloseReasons
            | Capability::ItemStatus
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
    use crate::forge::backend::{Fixed, Taken};
    use crate::forge::recorded::Recorded;

    fn on(host: &str) -> GitLab {
        let recorded = Recorded::parse(r#"{"source": "none", "exchanges": []}"#).unwrap();
        let mut forge = Forge::default_of(Kind::GitLab);
        forge.host = host.to_string();
        GitLab(Asker {
            forge,
            transports: Arc::new(Fixed(Arc::new(recorded))),
        })
    }

    #[test]
    fn gitlab_com_has_epics_said_of_the_instance_and_unknown_of_a_group_or_repo() {
        let gitlab = on("gitlab.com");
        let me = Caller::window();
        for what in [
            Capability::Epics,
            Capability::Iterations,
            Capability::Boards,
            Capability::IssueTypes,
            Capability::CloseReasons,
            Capability::ItemStatus,
        ] {
            assert_eq!(
                gitlab.support(&me, &Reach::Instance, what),
                Support::Available
            );
            for narrower in [Reach::Owner("acme".into()), Reach::Repo("acme/api".into())] {
                let said = gitlab.support(&me, &narrower, what);
                assert_eq!(said, Support::Unknown(UnknownWhy::NotProbed));
                assert_eq!(said.taken(what), Taken::Fallback(what.fallback()));
            }
        }
    }

    #[test]
    fn a_self_managed_gitlab_has_what_its_licence_has_which_charter_has_not_asked() {
        let gitlab = on("git.example.com");
        for what in Capability::ALL {
            assert_eq!(
                gitlab.support(&Caller::window(), &Reach::Instance, what),
                Support::Unknown(UnknownWhy::NotProbed),
                "{what:?}"
            );
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
        let call = json!({"endpoint": {"rest": {"method": "POST", "path": "projects/acme%2Fapi/issues"}},
                          "fields": [{"text": ["title", "Port the picker"]},
                                     {"text": ["description", "Why."]}]});
        let text = json!({"source": "GitLab REST API v4, issues: New issue",
                          "exchanges": [{"call": call, "reply": {"code": 0, "out": answer.to_string()}}]});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let backend = Forge::default_of(Kind::GitLab).backend_over(recorded);
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
        json!({"id": 84012, "iid": 12, "project_id": 3, "title": "Port the picker",
               "state": "opened", "labels": [], "web_url": "https://gitlab.com/acme/api/-/issues/12",
               "references": {"full": "acme/api#12"}})
    }

    #[test]
    fn a_milestone_in_gitlabs_answer_is_mapped_with_its_id_and_due_day() {
        let mut answer = issue();
        answer["milestone"] = json!({"id": 501, "iid": 1, "title": "v1", "state": "active",
                                     "due_date": "2026-10-31"});
        assert_eq!(
            created(answer).milestone,
            Some(Milestone {
                forge_ref: Some(ForgeRef("501".into())),
                title: "v1".into(),
                due: chrono::NaiveDate::from_ymd_opt(2026, 10, 31),
            })
        );
    }

    #[test]
    fn no_milestone_or_no_due_date_in_gitlabs_answer_maps_to_none() {
        assert_eq!(created(issue()).milestone, None);
        let mut answer = issue();
        answer["milestone"] = json!({"id": 501, "iid": 1, "title": "v1", "state": "active",
                                     "due_date": null});
        assert_eq!(created(answer).milestone.unwrap().due, None);
    }
}
