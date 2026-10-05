//! GitHub's issues mapped onto the neutral work model (FW-6a, FI4): what [`super::WorkItems`]
//! answers.
//!
//! An issue is read in at most four requests:
//!
//! 1. its REST answer: title, state and close reason, issue type, labels, milestone, assignees,
//!    and how many issues block it and it blocks;
//! 2. one GraphQL query (`queries/work_item.graphql`) for what REST does not hold in one
//!    answer: its parent and sub-issues, the pull requests that close it, and the Projects v2
//!    boards that hold it with its status and iteration on each;
//! 3. and 4. its `blocked_by` and `blocking` listings, each only when the REST answer counted
//!    any. GitHub's GraphQL schema has no dependency field yet, so they stay REST.
//!
//! Every related item is keyed by the page GitHub gives for it, as the item itself is, so an
//! item in another repo is named by that repo.

use super::GitHub;
use super::graphql::{self, work_item};
use super::work::Issue;
use crate::forge::backend::{Caller, ForgeRef};
use crate::forge::pr::Pr;
use crate::forge::transport::Field;
use crate::forge::{Failure, ForgeError};
use crate::work::{
    ClosedAs, Iteration, Kind as ItemKind, Milestone, Placement, Relation, State, TrackerKey,
    WorkItem,
};

/// The tracker key of the GitHub issue whose page is `url`.
pub(super) fn key_of_page(url: &str, doing: &str) -> Result<TrackerKey, ForgeError> {
    let unnamed = || {
        ForgeError::new(format!(
            "{doing}: GitHub's answer names no page charter can key the issue by: {url}"
        ))
    };
    let (host, parts) = crate::work::key::page_of(url).ok_or_else(unnamed)?;
    let [owner, repo, "issues", number] = parts.as_slice() else {
        return Err(unnamed());
    };
    let number: u64 = number.parse().map_err(|_| unnamed())?;
    TrackerKey::github(&host, &format!("{owner}/{repo}"), number)
        .map_err(|why| ForgeError::new(format!("{doing}: {why}")))
}

/// The fields of `issue`'s own REST answer, in the neutral model: no relation or board yet.
pub(super) fn item_of(issue: Issue, doing: &str) -> Result<WorkItem, ForgeError> {
    let key = key_of_page(&issue.html_url, doing)?;
    let mut item = WorkItem::new(key, ItemKind::Issue, issue.title);
    item.forge_ref = Some(ForgeRef(issue.node_id));
    item.url = issue.html_url;
    item.issue_type = issue.issue_type.map(|t| t.name);
    item.state = State::of_forge(&issue.state);
    if item.state == State::Closed {
        item.closed_as = issue.state_reason.as_deref().and_then(ClosedAs::of_forge);
    }
    item.labels = issue.labels.into_iter().map(|l| l.name).collect();
    item.assignees = issue.assignees.into_iter().map(|a| a.login).collect();
    item.milestone = issue
        .milestone
        .map(|m| Milestone::of_forge(m.node_id.map(ForgeRef), m.title, m.due_on.as_deref()));
    Ok(item)
}

impl GitHub {
    /// Issue `number` of the repo at `path`, every field the neutral model has.
    pub(super) fn read_item(
        &self,
        caller: &Caller,
        path: &str,
        number: u64,
    ) -> Result<WorkItem, ForgeError> {
        let doing = format!("reading issue #{number} of {path}");
        let issue = self.issue(caller, path, number)?;
        let summary = issue.issue_dependencies_summary;
        let mut item = item_of(issue, &doing)?;

        let (owner, name) = path.split_once('/').unwrap_or((path, ""));
        let data: work_item::ResponseData = self.graphql(
            caller,
            graphql::work_item::QUERY,
            vec![
                Field::text("owner", owner),
                Field::text("name", name),
                Field::typed("number", &number.to_string()),
            ],
            &doing,
        )?;
        let links = data
            .repository
            .and_then(|r| r.issue)
            .ok_or_else(|| ForgeError::of(Failure::NotFound, format!("{doing}: no such issue")))?;
        let whole = |more: bool, what: &str| {
            if more {
                Err(ForgeError::new(format!(
                    "{doing}: it has more {what} than one answer holds, and charter reads them \
                     all or none"
                )))
            } else {
                Ok(())
            }
        };
        whole(links.sub_issues.page_info.has_next_page, "sub-issues")?;
        whole(
            links
                .closed_by_pull_requests_references
                .as_ref()
                .is_some_and(|c| c.page_info.has_next_page),
            "closing pull requests",
        )?;
        whole(links.project_items.page_info.has_next_page, "boards")?;

        if let Some(parent) = links.parent {
            item.relations
                .push(Relation::Parent(key_of_page(&parent.url, &doing)?));
        }
        for child in links.sub_issues.nodes.into_iter().flatten().flatten() {
            item.relations
                .push(Relation::Child(key_of_page(&child.url, &doing)?));
        }
        if let Some(summary) = summary {
            if summary.total_blocked_by > 0 {
                for blocker in self.blocked_by(caller, path, number)? {
                    item.relations
                        .push(Relation::BlockedBy(key_of_page(&blocker.html_url, &doing)?));
                }
            }
            if summary.total_blocking > 0 {
                for blocked in self.blocking(caller, path, number)? {
                    item.relations
                        .push(Relation::Blocks(key_of_page(&blocked.html_url, &doing)?));
                }
            }
        }
        use work_item::PullRequestState as Pull;
        for pr in links
            .closed_by_pull_requests_references
            .and_then(|c| c.nodes)
            .into_iter()
            .flatten()
            .flatten()
        {
            // A request closed without merging closes nothing.
            if matches!(pr.state, Pull::OPEN | Pull::MERGED) {
                item.relations.push(Relation::ClosedBy(Pr {
                    number: u64::try_from(pr.number).unwrap_or_default(),
                    url: pr.url,
                }));
            }
        }

        use work_item::WorkItemRepositoryIssueProjectItemsNodesFieldValuesNodes as Value;
        use work_item::WorkItemRepositoryIssueProjectItemsNodesStatus as Status;
        for on in links.project_items.nodes.into_iter().flatten().flatten() {
            let status = match on.status {
                Some(Status::ProjectV2ItemFieldSingleSelectValue(v)) => v.name,
                _ => None,
            };
            item.placements.push(Placement {
                board: ForgeRef(on.project.id),
                board_title: on.project.title,
                board_url: on.project.url,
                status,
            });
            if item.iteration.is_none() {
                item.iteration = on
                    .field_values
                    .nodes
                    .into_iter()
                    .flatten()
                    .flatten()
                    .find_map(|v| match v {
                        Value::ProjectV2ItemFieldIterationValue(i) => Some(Iteration::lasting(
                            Some(ForgeRef(i.iteration_id)),
                            i.title,
                            &i.start_date,
                            u32::try_from(i.duration).unwrap_or_default(),
                        )),
                        _ => None,
                    });
            }
        }
        Ok(item)
    }
}
