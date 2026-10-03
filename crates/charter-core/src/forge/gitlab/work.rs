//! GitLab's work items as GitLab has them: issues, blocking links, child items, milestones,
//! epics and iterations (Premium), boards and pipelines (FW-2b, FI4).
//!
//! **GitLab's own types, crate-private methods.** FW-5 defines the neutral work model, and
//! FW-6b maps these onto it behind the `WorkItems` area trait (ADR 0070 §1). Until then nothing
//! outside the core calls them, and every method still takes the [`Caller`] and sends its
//! requests as [`Call`]s, so either transport carries them and the routing rules hold.
//!
//! **Every field is a literal string unless charter wrote it** (charter #323): a title, a
//! description, a label, a path or a global id is a [`Field::Text`]; only numbers charter holds
//! are [`Field::Typed`].
//!
//! **REST where GitLab's v4 has the operation, GraphQL where only work items do.** A child item
//! (GitLab's hierarchy, its sub-issues) and an issue's iteration are written through GraphQL;
//! everything else is REST v4. Epics use v4's epics endpoints, which GitLab deprecated in 17.0
//! in favour of work items and still serves in v4; FW-6b moves them when it maps epics.
//!
//! A repo or group is addressed by its full path, encoded as one segment (`acme%2Fapi`), which
//! is what GitLab asks for and what keeps a nested group's names out of the network log.

use serde::Deserialize;
use serde_json::Value;

use super::GitLab;
use crate::forge::backend::{Caller, ForgeRef};
use crate::forge::transport::{Call, Field, Method};
use crate::forge::{ForgeError, LIST_TIMEOUT, quote};

/// An issue: the fields charter reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    /// The instance-wide id, which an epic and a work item name.
    pub id: u64,
    /// The number within its repo, which the repo's own paths name.
    pub iid: u64,
    pub project_id: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// `opened` or `closed`.
    pub state: String,
    pub web_url: String,
    /// GitLab lists an issue's labels by name.
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub milestone: Option<Milestone>,
    #[serde(default)]
    pub iteration: Option<Iteration>,
    /// `issue`, `incident`, `test_case` or `task`.
    #[serde(default)]
    pub issue_type: Option<String>,
}

impl Issue {
    /// The id the seam carries for it: GitLab's instance-wide id.
    pub fn forge_ref(&self) -> ForgeRef {
        ForgeRef(self.id.to_string())
    }

    /// Its work item's GraphQL id: an issue is a work item, under the same id.
    pub fn work_item_id(&self) -> String {
        format!("gid://gitlab/WorkItem/{}", self.id)
    }
}

/// An issue linked to another, and the link.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Linked {
    #[serde(flatten)]
    pub issue: Issue,
    /// The link's own id, which removing it names.
    #[serde(rename = "issue_link_id")]
    pub link_id: u64,
    /// `relates_to`, `blocks` or `is_blocked_by`, said from the asking issue's side.
    pub link_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Milestone {
    pub id: u64,
    pub iid: u64,
    pub title: String,
    /// `active` or `closed`.
    pub state: String,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
}

/// An iteration of a group's cadence (Premium).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Iteration {
    pub id: u64,
    #[serde(default)]
    pub title: Option<String>,
    pub start_date: String,
    pub due_date: String,
    #[serde(default)]
    pub web_url: Option<String>,
}

impl Iteration {
    /// Its GraphQL id, which setting an issue's iteration names.
    pub fn global_id(&self) -> String {
        format!("gid://gitlab/Iteration/{}", self.id)
    }
}

/// An epic of a group (Premium).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Epic {
    pub id: u64,
    pub iid: u64,
    pub group_id: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub state: String,
    pub web_url: String,
    #[serde(default)]
    pub labels: Vec<String>,
}

/// A work item under another, as GitLab's hierarchy lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    /// Its GraphQL id.
    pub id: String,
    pub iid: String,
    pub title: String,
    /// `OPEN` or `CLOSED`.
    pub state: String,
    pub web_url: String,
    /// Its type's name: `Issue`, `Task`, `Epic`, …
    pub kind: String,
}

/// A board of a repo, with its lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub id: u64,
    pub name: String,
    pub lists: Vec<BoardList>,
}

/// One list of a board. A label list holds the issues carrying its label; a list with no label
/// (an assignee's, a milestone's, an iteration's) is not one charter moves issues onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardList {
    pub id: u64,
    pub label: Option<String>,
    pub position: Option<i64>,
}

/// A pipeline of a repo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pipeline {
    pub id: u64,
    pub sha: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub status: String,
    pub web_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewIssue {
    pub title: String,
    pub description: Option<String>,
    pub labels: Vec<String>,
    pub milestone_id: Option<u64>,
    pub issue_type: Option<String>,
}

/// A change to an issue: only what is `Some` is sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueEdit {
    pub title: Option<String>,
    pub description: Option<String>,
    /// `close` or `reopen`.
    pub state_event: Option<String>,
    /// Every label it carries afterwards; an empty list clears them.
    pub labels: Option<Vec<String>>,
    pub milestone_id: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewMilestone {
    pub title: String,
    pub description: Option<String>,
    pub due_date: Option<String>,
    pub start_date: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewEpic {
    pub title: String,
    pub description: Option<String>,
    pub labels: Vec<String>,
}

/// A work item's children, a page at a time.
pub(crate) const CHILDREN: &str = "query($id:WorkItemID!,$after:String){workItem(id:$id){widgets{__typename ... on WorkItemWidgetHierarchy{children(first:100,after:$after){pageInfo{hasNextPage endCursor} nodes{id iid title state webUrl workItemType{name}}}}}}}";

/// Put a work item under a parent, or take it out from under one (`$parent` null).
pub(crate) const SET_PARENT: &str = "mutation($id:WorkItemID!,$parent:WorkItemID){workItemUpdate(input:{id:$id,hierarchyWidget:{parentId:$parent}}){errors}}";

/// Set an issue's iteration, or clear it (`$iteration` null).
pub(crate) const SET_ITERATION: &str = "mutation($path:ID!,$iid:String!,$iteration:IterationID){issueSetIteration(input:{projectPath:$path,iid:$iid,iterationId:$iteration}){errors issue{iteration{id}}}}";

/// A listing longer than this many pages is refused rather than read without end.
const MOST_PAGES: usize = 1000;

/// GitLab's comma-separated label list, refused for a label that holds a comma: GitLab would
/// read it as two.
fn labels_of(labels: &[String], doing: &str) -> Result<String, ForgeError> {
    if let Some(bad) = labels.iter().find(|l| l.contains(',')) {
        return Err(ForgeError::new(format!(
            "{doing}: GitLab reads a comma in a label as two labels, so '{bad}' cannot be sent"
        )));
    }
    Ok(labels.join(","))
}

impl GitLab {
    fn repo_of(repo: &str) -> String {
        format!("projects/{}", quote(repo))
    }

    fn group_of(group: &str) -> String {
        format!("groups/{}", quote(group))
    }

    /// `call`'s answer read as `T`.
    fn typed<T: serde::de::DeserializeOwned>(
        &self,
        caller: &Caller,
        call: &Call,
        doing: &str,
    ) -> Result<T, ForgeError> {
        let answer = self.0.ask(caller, call, doing)?;
        serde_json::from_value(answer).map_err(|e| {
            ForgeError::new(format!(
                "{doing}: the answer is not what charter reads: {e}"
            ))
        })
    }

    /// Every page of a REST listing at `base`, a hundred at a time, until a short page.
    fn pages<T: serde::de::DeserializeOwned>(
        &self,
        caller: &Caller,
        base: &str,
        doing: &str,
    ) -> Result<Vec<T>, ForgeError> {
        let joint = if base.contains('?') { '&' } else { '?' };
        let mut out = Vec::new();
        for page in 1..=MOST_PAGES {
            let call = Call::get(
                format!("{base}{joint}per_page=100&page={page}"),
                LIST_TIMEOUT,
            );
            let batch: Vec<T> = self.typed(caller, &call, doing)?;
            let short = batch.len() < 100;
            out.extend(batch);
            if short {
                return Ok(out);
            }
        }
        Err(ForgeError::new(format!(
            "{doing}: ran past {MOST_PAGES} pages"
        )))
    }

    /// A GraphQL mutation, failed in GitLab's words when its payload names errors: GitLab
    /// answers a mutation it refuses with `200` and a list of `errors` beside the data.
    fn mutate(
        &self,
        caller: &Caller,
        query: &str,
        name: &str,
        variables: Vec<Field>,
        doing: &str,
    ) -> Result<Value, ForgeError> {
        let answer = self.0.ask(
            caller,
            &Call::graphql(query, variables, LIST_TIMEOUT),
            doing,
        )?;
        let payload = answer["data"][name].clone();
        if !payload.is_object() {
            return Err(ForgeError::new(format!(
                "{doing}: GitLab answered no {name}"
            )));
        }
        let errors: Vec<&str> = payload["errors"]
            .as_array()
            .map(|all| all.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !errors.is_empty() {
            return Err(ForgeError::new(format!("{doing}: {}", errors.join(", "))));
        }
        Ok(payload)
    }

    /// Issue `iid` of `repo`.
    pub(crate) fn issue(&self, caller: &Caller, repo: &str, iid: u64) -> Result<Issue, ForgeError> {
        self.typed(
            caller,
            &Call::get(
                format!("{}/issues/{iid}", Self::repo_of(repo)),
                LIST_TIMEOUT,
            ),
            &format!("reading issue #{iid} of {repo}"),
        )
    }

    /// Every issue of `repo` in `state` (`opened`, `closed` or `all`).
    pub(crate) fn issues(
        &self,
        caller: &Caller,
        repo: &str,
        state: &str,
    ) -> Result<Vec<Issue>, ForgeError> {
        self.pages(
            caller,
            &format!("{}/issues?state={}", Self::repo_of(repo), quote(state)),
            &format!("listing the issues of {repo}"),
        )
    }

    pub(crate) fn create_issue(
        &self,
        caller: &Caller,
        repo: &str,
        new: &NewIssue,
    ) -> Result<Issue, ForgeError> {
        let doing = format!("opening an issue in {repo}");
        let mut fields = vec![Field::text("title", &new.title)];
        if let Some(description) = &new.description {
            fields.push(Field::text("description", description));
        }
        if !new.labels.is_empty() {
            fields.push(Field::text("labels", &labels_of(&new.labels, &doing)?));
        }
        if let Some(milestone) = new.milestone_id {
            fields.push(Field::typed("milestone_id", &milestone.to_string()));
        }
        if let Some(kind) = &new.issue_type {
            fields.push(Field::text("issue_type", kind));
        }
        self.typed(
            caller,
            &Call::write(
                Method::Post,
                format!("{}/issues", Self::repo_of(repo)),
                fields,
            ),
            &doing,
        )
    }

    pub(crate) fn update_issue(
        &self,
        caller: &Caller,
        repo: &str,
        iid: u64,
        edit: &IssueEdit,
    ) -> Result<Issue, ForgeError> {
        let doing = format!("changing issue #{iid} of {repo}");
        let mut fields = Vec::new();
        for (name, value) in [
            ("title", &edit.title),
            ("description", &edit.description),
            ("state_event", &edit.state_event),
        ] {
            if let Some(value) = value {
                fields.push(Field::text(name, value));
            }
        }
        if let Some(labels) = &edit.labels {
            fields.push(Field::text("labels", &labels_of(labels, &doing)?));
        }
        if let Some(milestone) = edit.milestone_id {
            fields.push(Field::typed("milestone_id", &milestone.to_string()));
        }
        self.typed(
            caller,
            &Call::write(
                Method::Put,
                format!("{}/issues/{iid}", Self::repo_of(repo)),
                fields,
            ),
            &doing,
        )
    }

    /// The issues that block issue `iid`: its links whose type, from its side, is
    /// `is_blocked_by`.
    pub(crate) fn blocked_by(
        &self,
        caller: &Caller,
        repo: &str,
        iid: u64,
    ) -> Result<Vec<Linked>, ForgeError> {
        let links: Vec<Linked> = self.typed(
            caller,
            &Call::get(
                format!("{}/issues/{iid}/links", Self::repo_of(repo)),
                LIST_TIMEOUT,
            ),
            &format!("listing what blocks #{iid} of {repo}"),
        )?;
        Ok(links
            .into_iter()
            .filter(|l| l.link_type == "is_blocked_by")
            .collect())
    }

    /// Mark issue `iid` of `repo` as blocked by issue `blocker_iid` of `blocker_repo` (a full
    /// path or an id, as GitLab takes either).
    pub(crate) fn add_blocked_by(
        &self,
        caller: &Caller,
        repo: &str,
        iid: u64,
        blocker_repo: &str,
        blocker_iid: u64,
    ) -> Result<(), ForgeError> {
        self.0
            .ask(
                caller,
                &Call::write(
                    Method::Post,
                    format!("{}/issues/{iid}/links", Self::repo_of(repo)),
                    vec![
                        Field::text("target_project_id", blocker_repo),
                        Field::typed("target_issue_iid", &blocker_iid.to_string()),
                        Field::text("link_type", "is_blocked_by"),
                    ],
                ),
                &format!("marking #{iid} of {repo} blocked"),
            )
            .map(|_| ())
    }

    /// Remove link `link_id` from issue `iid`.
    pub(crate) fn remove_link(
        &self,
        caller: &Caller,
        repo: &str,
        iid: u64,
        link_id: u64,
    ) -> Result<(), ForgeError> {
        self.0
            .ask(
                caller,
                &Call::write(
                    Method::Delete,
                    format!("{}/issues/{iid}/links/{link_id}", Self::repo_of(repo)),
                    Vec::new(),
                ),
                &format!("removing a link from #{iid} of {repo}"),
            )
            .map(|_| ())
    }

    /// The work items under the one whose GraphQL id is `parent`, every page.
    pub(crate) fn children(&self, caller: &Caller, parent: &str) -> Result<Vec<Child>, ForgeError> {
        let doing = format!("listing the child items of {parent}");
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..MOST_PAGES {
            let mut variables = vec![Field::text("id", parent)];
            if let Some(cursor) = &after {
                variables.push(Field::text("after", cursor));
            }
            let answer = self.0.ask(
                caller,
                &Call::graphql(CHILDREN, variables, LIST_TIMEOUT),
                &doing,
            )?;
            let item = &answer["data"]["workItem"];
            if !item.is_object() {
                return Err(ForgeError::new(format!(
                    "{doing}: GitLab found no such item"
                )));
            }
            let hierarchy = item["widgets"]
                .as_array()
                .and_then(|w| {
                    w.iter()
                        .find(|w| w["__typename"] == "WorkItemWidgetHierarchy")
                })
                .ok_or_else(|| {
                    ForgeError::new(format!("{doing}: the item has no hierarchy on this GitLab"))
                })?;
            let children = &hierarchy["children"];
            for node in children["nodes"].as_array().into_iter().flatten() {
                let text = |key: &str| node[key].as_str().unwrap_or_default().to_string();
                out.push(Child {
                    id: text("id"),
                    iid: text("iid"),
                    title: text("title"),
                    state: text("state"),
                    web_url: text("webUrl"),
                    kind: node["workItemType"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                });
            }
            let info = &children["pageInfo"];
            match (info["hasNextPage"].as_bool(), info["endCursor"].as_str()) {
                (Some(true), Some(cursor)) => after = Some(cursor.to_string()),
                _ => return Ok(out),
            }
        }
        Err(ForgeError::new(format!(
            "{doing}: ran past {MOST_PAGES} pages"
        )))
    }

    /// Put the work item `child` under `parent`, or take it out from under its parent (`None`).
    pub(crate) fn set_parent(
        &self,
        caller: &Caller,
        child: &str,
        parent: Option<&str>,
    ) -> Result<(), ForgeError> {
        let mut variables = vec![Field::text("id", child)];
        variables.push(match parent {
            Some(parent) => Field::text("parent", parent),
            None => Field::typed("parent", "null"),
        });
        let doing = match parent {
            Some(parent) => format!("putting {child} under {parent}"),
            None => format!("taking {child} out from under its parent"),
        };
        self.mutate(caller, SET_PARENT, "workItemUpdate", variables, &doing)
            .map(|_| ())
    }

    /// Every milestone of `repo`, active and closed.
    pub(crate) fn milestones(
        &self,
        caller: &Caller,
        repo: &str,
    ) -> Result<Vec<Milestone>, ForgeError> {
        self.pages(
            caller,
            &format!("{}/milestones", Self::repo_of(repo)),
            &format!("listing the milestones of {repo}"),
        )
    }

    pub(crate) fn create_milestone(
        &self,
        caller: &Caller,
        repo: &str,
        new: &NewMilestone,
    ) -> Result<Milestone, ForgeError> {
        let mut fields = vec![Field::text("title", &new.title)];
        for (name, value) in [
            ("description", &new.description),
            ("due_date", &new.due_date),
            ("start_date", &new.start_date),
        ] {
            if let Some(value) = value {
                fields.push(Field::text(name, value));
            }
        }
        self.typed(
            caller,
            &Call::write(
                Method::Post,
                format!("{}/milestones", Self::repo_of(repo)),
                fields,
            ),
            &format!("making a milestone in {repo}"),
        )
    }

    /// Every epic of `group` in `state` (`opened`, `closed` or `all`). Premium.
    pub(crate) fn epics(
        &self,
        caller: &Caller,
        group: &str,
        state: &str,
    ) -> Result<Vec<Epic>, ForgeError> {
        self.pages(
            caller,
            &format!("{}/epics?state={}", Self::group_of(group), quote(state)),
            &format!("listing the epics of {group}"),
        )
    }

    pub(crate) fn create_epic(
        &self,
        caller: &Caller,
        group: &str,
        new: &NewEpic,
    ) -> Result<Epic, ForgeError> {
        let doing = format!("making an epic in {group}");
        let mut fields = vec![Field::text("title", &new.title)];
        if let Some(description) = &new.description {
            fields.push(Field::text("description", description));
        }
        if !new.labels.is_empty() {
            fields.push(Field::text("labels", &labels_of(&new.labels, &doing)?));
        }
        self.typed(
            caller,
            &Call::write(
                Method::Post,
                format!("{}/epics", Self::group_of(group)),
                fields,
            ),
            &doing,
        )
    }

    /// Put the issue whose instance-wide id is `issue_id` in epic `epic_iid` of `group`.
    pub(crate) fn add_to_epic(
        &self,
        caller: &Caller,
        group: &str,
        epic_iid: u64,
        issue_id: u64,
    ) -> Result<(), ForgeError> {
        self.0
            .ask(
                caller,
                &Call::write(
                    Method::Post,
                    format!(
                        "{}/epics/{epic_iid}/issues/{issue_id}",
                        Self::group_of(group)
                    ),
                    Vec::new(),
                ),
                &format!("putting an issue in epic &{epic_iid} of {group}"),
            )
            .map(|_| ())
    }

    /// The iterations `group` has in `state` (`opened`, `upcoming`, `current`, `closed` or
    /// `all`), its ancestors' cadences included. Premium.
    pub(crate) fn iterations(
        &self,
        caller: &Caller,
        group: &str,
        state: &str,
    ) -> Result<Vec<Iteration>, ForgeError> {
        self.pages(
            caller,
            &format!(
                "{}/iterations?state={}&include_ancestors=true",
                Self::group_of(group),
                quote(state)
            ),
            &format!("listing the iterations of {group}"),
        )
    }

    /// Set issue `iid` of `repo` to the iteration whose GraphQL id is `iteration`, or clear it.
    pub(crate) fn set_iteration(
        &self,
        caller: &Caller,
        repo: &str,
        iid: u64,
        iteration: Option<&str>,
    ) -> Result<(), ForgeError> {
        let iid_text = iid.to_string();
        let variables = vec![
            Field::text("path", repo),
            Field::text("iid", &iid_text),
            match iteration {
                Some(iteration) => Field::text("iteration", iteration),
                None => Field::typed("iteration", "null"),
            },
        ];
        self.mutate(
            caller,
            SET_ITERATION,
            "issueSetIteration",
            variables,
            &format!("setting the iteration of #{iid} of {repo}"),
        )
        .map(|_| ())
    }

    /// Every board of `repo`, with its lists.
    pub(crate) fn boards(&self, caller: &Caller, repo: &str) -> Result<Vec<Board>, ForgeError> {
        let raw: Vec<Value> = self.pages(
            caller,
            &format!("{}/boards", Self::repo_of(repo)),
            &format!("listing the boards of {repo}"),
        )?;
        Ok(raw
            .iter()
            .map(|board| Board {
                id: board["id"].as_u64().unwrap_or_default(),
                name: board["name"].as_str().unwrap_or_default().to_string(),
                lists: board["lists"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|list| BoardList {
                        id: list["id"].as_u64().unwrap_or_default(),
                        label: list["label"]["name"].as_str().map(str::to_string),
                        position: list["position"].as_i64(),
                    })
                    .collect(),
            })
            .collect())
    }

    /// Move issue `iid` onto `list` by giving it the list's label. A list with no label is
    /// refused: there is nothing to give.
    pub(crate) fn put_on_list(
        &self,
        caller: &Caller,
        repo: &str,
        iid: u64,
        list: &BoardList,
    ) -> Result<(), ForgeError> {
        let doing = format!("moving #{iid} of {repo} onto a board list");
        let Some(label) = &list.label else {
            return Err(ForgeError::new(format!(
                "{doing}: list {} is not a label list",
                list.id
            )));
        };
        self.0
            .ask(
                caller,
                &Call::write(
                    Method::Put,
                    format!("{}/issues/{iid}", Self::repo_of(repo)),
                    vec![Field::text(
                        "add_labels",
                        &labels_of(std::slice::from_ref(label), &doing)?,
                    )],
                ),
                &doing,
            )
            .map(|_| ())
    }

    /// Every pipeline of `repo` at `git_ref`, newest first.
    pub(crate) fn pipelines(
        &self,
        caller: &Caller,
        repo: &str,
        git_ref: &str,
    ) -> Result<Vec<Pipeline>, ForgeError> {
        self.pages(
            caller,
            &format!("{}/pipelines?ref={}", Self::repo_of(repo), quote(git_ref)),
            &format!("listing the pipelines of {repo} at {git_ref}"),
        )
    }
}

#[cfg(test)]
#[path = "work_tests.rs"]
mod tests;
