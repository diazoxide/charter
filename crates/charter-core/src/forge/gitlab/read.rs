//! GitLab's issues mapped onto the neutral work model (FW-6b, FI4): what [`super::WorkItems`]
//! answers.
//!
//! An issue is read in three requests:
//!
//! 1. its REST answer (`GET projects/:id/issues/:iid`): key, ids, title, state, labels,
//!    milestone, assignees;
//! 2. one GraphQL query ([`WORK_ITEM`]) for its work item's widgets, which REST does not hold:
//!    its type, parent and child items, blocking links, the merge requests that close it, its
//!    iteration and its own status;
//! 3. the repo's issue boards (`GET projects/:id/boards`), each read for the label list the
//!    issue sits in.
//!
//! **Tiers.** Epics, iterations, blocking links and work item status are GitLab Premium
//! features. On a Free namespace GitLab leaves the widget out of the answer, or answers it empty,
//! and the field stays empty; [`super::GitLab`]'s capabilities say which (W10).
//!
//! **Schema.** The query is checked against GitLab 19.4's GraphQL reference
//! (`doc/api/graphql/reference/_index.md`). `WorkItemWidgetStatus.status` came in GitLab 17.11,
//! so an older self-managed GitLab refuses the query; FG-2's version probe (#802) is where that
//! is told apart.
//!
//! Every related item is keyed by the page GitLab gives for it, as the item itself is, so an item
//! in another repo, or a group's epic, is named by where it lives.

use serde_json::Value;

use super::GitLab;
use super::work::Board;
use crate::forge::backend::{Caller, ForgeRef};
use crate::forge::pr::Pr;
use crate::forge::transport::{Call, Field};
use crate::forge::{Failure, ForgeError, LIST_TIMEOUT};
use crate::work::{ClosedAs, Iteration, Placement, Relation, State, TrackerKey, WorkItem};

/// An issue's work item and the widgets charter maps: `Project.workItems(iid:)` (GitLab 15.1),
/// the hierarchy (parent, children), linked items (`blocks`, `is_blocked_by`; `relates_to` is
/// no relation the model has), iteration, status (17.11) and development (closing merge
/// requests) widgets. Each list is read whole in one answer, or the read fails.
pub(crate) const WORK_ITEM: &str = "query($path:ID!,$iid:String!){project(fullPath:$path){workItems(iid:$iid,first:1){nodes{workItemType{name} duplicatedToWorkItemUrl widgets{__typename ... on WorkItemWidgetHierarchy{parent{webUrl} children(first:100){pageInfo{hasNextPage} nodes{webUrl}}} ... on WorkItemWidgetLinkedItems{linkedItems(first:100){pageInfo{hasNextPage} nodes{linkType workItem{webUrl}}}} ... on WorkItemWidgetIteration{iteration{id title startDate dueDate}} ... on WorkItemWidgetStatus{status{name category}} ... on WorkItemWidgetDevelopment{closingMergeRequests(first:100){pageInfo{hasNextPage} nodes{mergeRequest{iid webUrl state}}}}}}}}}";

/// The tracker key of the GitLab item whose page is `url`: a repo's issue or task
/// (`group/repo/-/issues/12`, `group/repo/-/work_items/12`), or a group's epic
/// (`groups/group/-/epics/3`, `groups/group/-/work_items/3`; only epics live at a group).
pub(super) fn key_of_page(url: &str, doing: &str) -> Result<TrackerKey, ForgeError> {
    let unnamed = || {
        ForgeError::new(format!(
            "{doing}: GitLab's answer names no page charter can key the item by: {url}"
        ))
    };
    let (host, parts) = crate::work::key::page_of(url).ok_or_else(unnamed)?;
    let dash = parts.iter().position(|p| *p == "-").ok_or_else(unnamed)?;
    let (place, rest) = parts.split_at(dash);
    let [_, what, number] = rest else {
        return Err(unnamed());
    };
    let number: u64 = number.parse().map_err(|_| unnamed())?;
    let key = match (place, *what) {
        (["groups", group @ ..], "epics" | "work_items") if !group.is_empty() => {
            TrackerKey::gitlab_epic(&host, &group.join("/"), number)
        }
        (["groups", ..], _) | ([], _) => return Err(unnamed()),
        (repo, "issues" | "work_items") => TrackerKey::gitlab_issue(&host, &repo.join("/"), number),
        _ => return Err(unnamed()),
    };
    key.map_err(|why| ForgeError::new(format!("{doing}: {why}")))
}

/// Why a closed item was closed. GitLab keeps no reason word: an item marked a duplicate names
/// what it duplicates (`duplicatedToWorkItemUrl`), and a Premium status says its category
/// (`WorkItemStatusCategoryEnum`): `DONE` is completed, `CANCELED` (`Won't do`) is not planned.
fn closed_as(node: &Value, status: Option<&Value>) -> Option<ClosedAs> {
    if node["duplicatedToWorkItemUrl"].is_string() {
        return Some(ClosedAs::Duplicate);
    }
    match status?["category"].as_str()? {
        "DONE" => Some(ClosedAs::Completed),
        "CANCELED" => Some(ClosedAs::NotPlanned),
        _ => None,
    }
}

impl GitLab {
    /// Issue `iid` of the repo at `path`, every field the neutral model has.
    pub(super) fn read_item(
        &self,
        caller: &Caller,
        path: &str,
        iid: u64,
    ) -> Result<WorkItem, ForgeError> {
        let doing = format!("reading issue #{iid} of {path}");
        let call = Call::get(
            format!("projects/{}/issues/{iid}", crate::forge::quote(path)),
            LIST_TIMEOUT,
        );
        let issue = self.0.ask(caller, &call, &doing)?;
        let mut item = super::item_of(&issue, None, &doing)?;
        item.assignees = issue["assignees"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["username"].as_str().map(str::to_string))
            .collect();

        let answer = self.0.ask(
            caller,
            &Call::graphql(
                WORK_ITEM,
                vec![
                    Field::text("path", path),
                    Field::text("iid", &iid.to_string()),
                ],
                LIST_TIMEOUT,
            ),
            &doing,
        )?;
        let node = &answer["data"]["project"]["workItems"]["nodes"][0];
        if !node.is_object() {
            return Err(ForgeError::of(
                Failure::NotFound,
                format!("{doing}: GitLab found no such work item"),
            ));
        }
        let widget = |name: &str| {
            node["widgets"]
                .as_array()
                .and_then(|all| all.iter().find(|w| w["__typename"] == name))
        };
        // A list longer than one answer holds is refused rather than read short.
        let whole = |list: &Value, what: &str| {
            if list["pageInfo"]["hasNextPage"].as_bool() == Some(true) {
                Err(ForgeError::new(format!(
                    "{doing}: it has more {what} than one answer holds, and charter reads them \
                     all or none"
                )))
            } else {
                Ok(list["nodes"].as_array().cloned().unwrap_or_default())
            }
        };
        let hierarchy = widget("WorkItemWidgetHierarchy");
        let children = match hierarchy {
            Some(h) => whole(&h["children"], "child items")?,
            None => Vec::new(),
        };
        let linked = match widget("WorkItemWidgetLinkedItems") {
            Some(l) => whole(&l["linkedItems"], "linked items")?,
            None => Vec::new(),
        };
        let closing = match widget("WorkItemWidgetDevelopment") {
            Some(d) => whole(&d["closingMergeRequests"], "closing merge requests")?,
            None => Vec::new(),
        };

        item.issue_type = node["workItemType"]["name"].as_str().map(str::to_string);
        if let Some(parent) = hierarchy.and_then(|h| h["parent"]["webUrl"].as_str()) {
            item.relations
                .push(Relation::Parent(key_of_page(parent, &doing)?));
        }
        for child in &children {
            if let Some(url) = child["webUrl"].as_str() {
                item.relations
                    .push(Relation::Child(key_of_page(url, &doing)?));
            }
        }
        // Said from this item's side (`LinkedWorkItemType.linkType`, as `issue_links.md`'s
        // `link_type`). A linked item the caller may not see comes back as null and is left out.
        for link in &linked {
            let Some(url) = link["workItem"]["webUrl"].as_str() else {
                continue;
            };
            match link["linkType"].as_str() {
                Some("is_blocked_by") => item
                    .relations
                    .push(Relation::BlockedBy(key_of_page(url, &doing)?)),
                Some("blocks") => item
                    .relations
                    .push(Relation::Blocks(key_of_page(url, &doing)?)),
                _ => {}
            }
        }
        for mr in closing.iter().map(|c| &c["mergeRequest"]) {
            // A request closed without merging closes nothing, as on GitHub.
            if !matches!(mr["state"].as_str(), Some("opened" | "locked" | "merged")) {
                continue;
            }
            let (Some(number), Some(url)) = (
                mr["iid"].as_str().and_then(|n| n.parse().ok()),
                mr["webUrl"].as_str(),
            ) else {
                continue;
            };
            item.relations.push(Relation::ClosedBy(Pr {
                number,
                url: url.to_string(),
            }));
        }
        // GraphQL's `Iteration.startDate` and `dueDate` are `Time`s; each is read by its day.
        let iteration = widget("WorkItemWidgetIteration").map(|w| &w["iteration"]);
        item.iteration = iteration.filter(|i| i.is_object()).map(|i| {
            Iteration::of_forge(
                i["id"].as_str().map(|id| ForgeRef(id.to_string())),
                i["title"].as_str(),
                i["startDate"].as_str(),
                i["dueDate"].as_str(),
            )
        });
        let status = widget("WorkItemWidgetStatus")
            .map(|w| &w["status"])
            .filter(|s| s.is_object());
        item.status = status.and_then(|s| s["name"].as_str()).map(str::to_string);
        if item.state == State::Closed {
            item.closed_as = closed_as(node, status);
        }

        let boards = self.boards(caller, path)?;
        // A board's page: the item's own page names the repo's, before GitLab's `/-/`.
        let repo_page = item.url.split_once("/-/").map(|(repo, _)| repo.to_string());
        for board in &boards {
            if !holds(board, &item) {
                continue;
            }
            item.placements.push(Placement {
                board: ForgeRef(board.id.to_string()),
                board_title: board.name.clone(),
                board_url: repo_page
                    .as_deref()
                    .map(|repo| format!("{repo}/-/boards/{}", board.id))
                    .unwrap_or_default(),
                status: list_of(board, &item),
                iteration: None,
            });
        }
        if item.status.is_none() {
            item.status = item.placements.iter().find_map(|p| p.status.clone());
        }
        Ok(item)
    }
}

/// Whether `board`'s scope (Premium: a milestone, labels, an assignee; `boards.md`) holds the
/// item. A milestone scope GitLab names by a special id (none, any, upcoming, started: zero or
/// less) is not one charter decides, so the board holds the item. A scope by iteration or weight
/// is not in REST's answer, and is not read.
fn holds(board: &Board, item: &WorkItem) -> bool {
    if let Some(milestone) = board.milestone_id.filter(|id| *id > 0) {
        let mine = item.milestone.as_ref().and_then(|m| m.forge_ref.as_ref());
        if mine.map(|r| r.0.as_str()) != Some(milestone.to_string().as_str()) {
            return false;
        }
    }
    if !board.scope_labels.iter().all(|l| item.labels.contains(l)) {
        return false;
    }
    board
        .assignee
        .as_ref()
        .is_none_or(|who| item.assignees.contains(who))
}

/// The label of the first list, by position, that holds the item: an open item carrying the
/// list's label. A label list holds open issues only (`issue_board.md`, "Label list: all open
/// issues for a label"), so a closed item, or one in no label list, has no status there.
fn list_of(board: &Board, item: &WorkItem) -> Option<String> {
    if item.state == State::Closed {
        return None;
    }
    let mut lists: Vec<_> = board.lists.iter().collect();
    lists.sort_by_key(|l| l.position.unwrap_or(i64::MAX));
    lists
        .into_iter()
        .filter_map(|l| l.label.as_ref())
        .find(|label| item.labels.contains(label))
        .cloned()
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
