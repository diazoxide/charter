//! `WorkItems::read` on GitLab: an issue's REST answer, one GraphQL answer for its work item's
//! widgets, and the repo's boards, mapped onto the neutral work model (FW-6b).

use std::sync::Arc;

use serde_json::{Value, json};

use super::WORK_ITEM;
use crate::forge::backend::{Caller, ForgeRef};
use crate::forge::pr::Pr;
use crate::forge::recorded::Recorded;
use crate::forge::{Forge, ForgeBackend, Kind};
use crate::work::{ClosedAs, Iteration, Placement, Relation, State, TrackerKey};

fn over(exchanges: Value) -> (Box<dyn ForgeBackend>, Arc<Recorded>) {
    let text = json!({"source": "GitLab 19.4 REST API docs (issues.md, boards.md) and its \
                                 GraphQL reference (Project.workItems, the work item widgets)",
                      "exchanges": exchanges});
    let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
    let backend = Forge::default_of(Kind::GitLab).backend_over(recorded.clone());
    (backend, recorded)
}

fn rest(path: &str, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn gql(out: Value) -> Value {
    json!({"call": {"endpoint": "graphql",
                    "fields": [{"text": ["query", WORK_ITEM]},
                               {"text": ["path", "acme/api"]},
                               {"text": ["iid", "12"]}]},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn boards(out: Value) -> Value {
    rest("projects/acme%2Fapi/boards?per_page=100&page=1", out)
}

fn key(text: &str) -> TrackerKey {
    TrackerKey::parse(text).unwrap()
}

fn issue(state: &str, labels: Value) -> Value {
    json!({"id": 84012, "iid": 12, "project_id": 3, "title": "Port the picker",
           "state": state, "labels": labels,
           "assignees": [{"username": "octocat"}],
           "milestone": {"id": 501, "iid": 1, "title": "v1", "state": "active",
                         "due_date": "2026-10-31"},
           "web_url": "https://gitlab.com/acme/api/-/issues/12",
           "references": {"full": "acme/api#12"}})
}

/// The work item's answer with these widgets, of type `kind`.
fn item(kind: &str, extra: Value, widgets: Value) -> Value {
    let mut node = json!({"workItemType": {"name": kind}, "duplicatedToWorkItemUrl": null,
                          "widgets": widgets});
    for (k, v) in extra.as_object().unwrap() {
        node[k] = v.clone();
    }
    json!({"data": {"project": {"workItems": {"nodes": [node]}}}})
}

fn widgets() -> Value {
    json!([
        {"__typename": "WorkItemWidgetAssignees"},
        {"__typename": "WorkItemWidgetHierarchy",
         "parent": {"webUrl": "https://gitlab.com/groups/acme/-/epics/3"},
         "children": {"pageInfo": {"hasNextPage": false}, "nodes": [
             {"webUrl": "https://gitlab.com/acme/web/-/work_items/14"}]}},
        {"__typename": "WorkItemWidgetLinkedItems",
         "linkedItems": {"pageInfo": {"hasNextPage": false}, "nodes": [
             {"linkType": "is_blocked_by",
              "workItem": {"webUrl": "https://gitlab.com/acme/api/-/issues/11"}},
             {"linkType": "relates_to",
              "workItem": {"webUrl": "https://gitlab.com/acme/api/-/issues/9"}},
             {"linkType": "blocks",
              "workItem": {"webUrl": "https://gitlab.com/acme/api/-/issues/15"}},
             {"linkType": "blocks", "workItem": null}]}},
        {"__typename": "WorkItemWidgetIteration",
         "iteration": {"id": "gid://gitlab/Iteration/41", "title": null,
                       "startDate": "2026-10-05T00:00:00Z", "dueDate": "2026-10-18T00:00:00Z"}},
        {"__typename": "WorkItemWidgetStatus",
         "status": {"name": "Won't do", "category": "CANCELED"}},
        {"__typename": "WorkItemWidgetDevelopment",
         "closingMergeRequests": {"pageInfo": {"hasNextPage": false}, "nodes": [
             {"mergeRequest": {"iid": "20", "state": "merged",
                               "webUrl": "https://gitlab.com/acme/api/-/merge_requests/20"}},
             {"mergeRequest": {"iid": "21", "state": "closed",
                               "webUrl": "https://gitlab.com/acme/api/-/merge_requests/21"}},
             {"mergeRequest": {"iid": "22", "state": "opened",
                               "webUrl": "https://gitlab.com/acme/api/-/merge_requests/22"}}]}}
    ])
}

fn read(exchanges: Value) -> Result<crate::work::WorkItem, crate::forge::ForgeError> {
    let (backend, recorded) = over(exchanges);
    let read = backend.read(&Caller::command(), "acme/api", 12);
    if read.is_ok() {
        assert_eq!(recorded.unspent(), Vec::new(), "recorded and never asked");
    }
    read
}

#[test]
fn an_issue_is_read_with_its_type_relations_iteration_and_own_status() {
    let item = read(json!([
        rest(
            "projects/acme%2Fapi/issues/12",
            issue("closed", json!(["bug"]))
        ),
        gql(item("Issue", json!({}), widgets())),
        boards(json!([])),
    ]))
    .unwrap();
    assert_eq!(item.state, State::Closed);
    assert_eq!(item.issue_type.as_deref(), Some("Issue"));
    assert_eq!(item.assignees, ["octocat"]);
    assert_eq!(
        item.relations,
        [
            Relation::Parent(key("gitlab:gitlab.com/acme&3")),
            Relation::Child(key("gitlab:gitlab.com/acme/web#14")),
            Relation::BlockedBy(key("gitlab:gitlab.com/acme/api#11")),
            Relation::Blocks(key("gitlab:gitlab.com/acme/api#15")),
            Relation::ClosedBy(Pr {
                number: 20,
                url: "https://gitlab.com/acme/api/-/merge_requests/20".into()
            }),
            Relation::ClosedBy(Pr {
                number: 22,
                url: "https://gitlab.com/acme/api/-/merge_requests/22".into()
            }),
        ],
        "a related link is no relation the model has, and a request closed unmerged closes \
         nothing"
    );
    // A cadence's iteration has no title: it is named by its dates.
    assert_eq!(
        item.iteration,
        Some(Iteration::of_forge(
            Some(ForgeRef("gid://gitlab/Iteration/41".into())),
            None,
            Some("2026-10-05"),
            Some("2026-10-18"),
        ))
    );
    assert_eq!(item.status.as_deref(), Some("Won't do"));
    assert_eq!(item.closed_as, Some(ClosedAs::NotPlanned));
    assert!(item.placements.is_empty());
}

#[test]
fn a_closed_item_is_closed_as_its_status_category_says_and_a_duplicate_above_all() {
    let closed = |category: &str, duplicate: Value| {
        let widgets = json!([{"__typename": "WorkItemWidgetStatus",
                              "status": {"name": "x", "category": category}}]);
        read(json!([
            rest("projects/acme%2Fapi/issues/12", issue("closed", json!([]))),
            gql(item(
                "Issue",
                json!({"duplicatedToWorkItemUrl": duplicate}),
                widgets
            )),
            boards(json!([])),
        ]))
        .unwrap()
        .closed_as
    };
    assert_eq!(closed("DONE", Value::Null), Some(ClosedAs::Completed));
    assert_eq!(closed("CANCELED", Value::Null), Some(ClosedAs::NotPlanned));
    assert_eq!(closed("TO_DO", Value::Null), None);
    assert_eq!(
        closed("CANCELED", json!("https://gitlab.com/acme/api/-/issues/2")),
        Some(ClosedAs::Duplicate)
    );
}

#[test]
fn an_open_item_has_no_close_reason_whatever_its_status() {
    let widgets = json!([{"__typename": "WorkItemWidgetStatus",
                          "status": {"name": "Done", "category": "DONE"}}]);
    let item = read(json!([
        rest("projects/acme%2Fapi/issues/12", issue("opened", json!([]))),
        gql(item("Issue", json!({}), widgets)),
        boards(json!([])),
    ]))
    .unwrap();
    assert_eq!(
        (item.closed_as, item.status.as_deref()),
        (None, Some("Done"))
    );
}

#[test]
fn a_gitlab_with_no_status_or_iteration_widget_leaves_them_empty() {
    // GitLab's Free tier: the work item has no status or iteration widget at all.
    let item = read(json!([
        rest("projects/acme%2Fapi/issues/12", issue("closed", json!([]))),
        gql(item(
            "Task",
            json!({}),
            json!([{"__typename": "WorkItemWidgetHierarchy",
             "parent": null,
             "children": {"pageInfo": {"hasNextPage": false}, "nodes": []}}])
        )),
        boards(json!([])),
    ]))
    .unwrap();
    assert_eq!(item.issue_type.as_deref(), Some("Task"));
    assert_eq!(
        (item.status, item.iteration, item.closed_as),
        (None, None, None)
    );
    assert!(item.relations.is_empty());
}

fn board(id: u64, name: &str, scope: Value, lists: &[&str]) -> Value {
    let lists: Vec<Value> = lists
        .iter()
        .enumerate()
        .map(|(n, label)| {
            json!({"id": id * 10 + n as u64, "position": n,
                                 "label": {"name": label}})
        })
        .collect();
    let mut board = json!({"id": id, "name": name, "lists": lists});
    for (k, v) in scope.as_object().unwrap() {
        board[k] = v.clone();
    }
    board
}

#[test]
fn an_open_issue_sits_on_each_board_in_the_first_label_list_it_carries() {
    let item = read(json!([
        rest(
            "projects/acme%2Fapi/issues/12",
            issue(
                "opened",
                json!(["bug", "workflow::review", "workflow::doing"])
            )
        ),
        gql(item("Issue", json!({}), json!([]))),
        boards(json!([
            board(
                1,
                "Development",
                json!({}),
                &["workflow::doing", "workflow::review"]
            ),
            board(2, "Triage", json!({}), &["needs-info"]),
            board(
                3,
                "Other milestone",
                json!({"milestone": {"id": 777, "title": "v2"}}),
                &["workflow::doing"]
            ),
            board(
                4,
                "Frontend",
                json!({"labels": [{"name": "frontend"}]}),
                &["workflow::doing"]
            ),
            board(
                5,
                "Mine",
                json!({"assignee": {"username": "octocat"},
                                     "milestone": {"id": 501, "title": "v1"}}),
                &["bug"]
            ),
        ])),
    ]))
    .unwrap();
    let on = |id: &str, title: &str, status: Option<&str>| Placement {
        board: ForgeRef(id.into()),
        board_title: title.into(),
        board_url: format!("https://gitlab.com/acme/api/-/boards/{id}"),
        status: status.map(str::to_string),
        iteration: None,
    };
    assert_eq!(
        item.placements,
        [
            on("1", "Development", Some("workflow::doing")),
            on("2", "Triage", None),
            on("5", "Mine", Some("bug")),
        ],
        "a board whose milestone, labels or assignee leave the issue out does not hold it"
    );
    assert_eq!(
        item.status.as_deref(),
        Some("workflow::doing"),
        "with no status of its own, the item's is its first board's"
    );
}

#[test]
fn a_closed_issue_sits_in_no_label_list() {
    // GitLab shows a closed issue only in a board's Closed list (boards.md, issue_board.md).
    let item = read(json!([
        rest(
            "projects/acme%2Fapi/issues/12",
            issue("closed", json!(["workflow::doing"]))
        ),
        gql(item("Issue", json!({}), json!([]))),
        boards(json!([board(
            1,
            "Development",
            json!({}),
            &["workflow::doing"]
        )])),
    ]))
    .unwrap();
    assert_eq!(item.placements.len(), 1);
    assert_eq!(
        (item.placements[0].status.as_ref(), item.status),
        (None, None)
    );
}

#[test]
fn more_children_links_or_closing_requests_than_one_answer_holds_is_an_error() {
    for (widget, what) in [
        (
            json!({"__typename": "WorkItemWidgetHierarchy", "parent": null,
                   "children": {"pageInfo": {"hasNextPage": true}, "nodes": []}}),
            "child items",
        ),
        (
            json!({"__typename": "WorkItemWidgetLinkedItems",
                   "linkedItems": {"pageInfo": {"hasNextPage": true}, "nodes": []}}),
            "linked items",
        ),
        (
            json!({"__typename": "WorkItemWidgetDevelopment",
                   "closingMergeRequests": {"pageInfo": {"hasNextPage": true}, "nodes": []}}),
            "closing merge requests",
        ),
    ] {
        let refused = read(json!([
            rest("projects/acme%2Fapi/issues/12", issue("opened", json!([]))),
            gql(item("Issue", json!({}), json!([widget]))),
        ]))
        .unwrap_err();
        assert!(refused.said().contains(what), "{what}: {}", refused.said());
    }
}

#[test]
fn a_work_item_graphql_does_not_find_is_not_found() {
    let refused = read(json!([
        rest("projects/acme%2Fapi/issues/12", issue("opened", json!([]))),
        gql(json!({"data": {"project": {"workItems": {"nodes": []}}}})),
    ]))
    .unwrap_err();
    assert_eq!(refused.failure(), &crate::forge::Failure::NotFound);
}

#[test]
fn a_related_item_is_keyed_by_its_page_and_a_page_charter_cannot_key_is_an_error() {
    let widgets = json!([{"__typename": "WorkItemWidgetHierarchy",
                          "parent": {"webUrl": "https://gitlab.com/acme/api/-/wikis/home"},
                          "children": {"pageInfo": {"hasNextPage": false}, "nodes": []}}]);
    let refused = read(json!([
        rest("projects/acme%2Fapi/issues/12", issue("opened", json!([]))),
        gql(item("Issue", json!({}), widgets)),
    ]))
    .unwrap_err();
    assert!(refused.said().contains("wikis/home"), "{}", refused.said());
}

#[test]
fn an_answer_with_no_title_is_refused_as_malformed() {
    let mut answer = issue("opened", json!([]));
    answer.as_object_mut().unwrap().remove("title");
    let refused = read(json!([rest("projects/acme%2Fapi/issues/12", answer)])).unwrap_err();
    assert!(refused.said().contains("no title"), "{}", refused.said());
}
