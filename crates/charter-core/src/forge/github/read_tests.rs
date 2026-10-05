//! `WorkItems::read` on GitHub: an issue's REST answer, the GraphQL answer for what REST does not
//! hold, and its dependency listings, mapped onto the neutral work model (FW-6a).

use std::sync::Arc;

use serde_json::{Value, json};

use super::graphql::work_item;
use crate::forge::backend::{Caller, ForgeRef};
use crate::forge::pr::Pr;
use crate::forge::recorded::Recorded;
use crate::forge::{Failure, Forge, ForgeBackend, Kind};
use crate::work::{ClosedAs, Iteration, Kind as ItemKind, Placement, Relation, State, TrackerKey};

fn over(exchanges: Value) -> (Box<dyn ForgeBackend>, Arc<Recorded>) {
    let text = json!({"source": "GitHub REST API version 2022-11-28 and GraphQL schema",
                      "exchanges": exchanges});
    let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
    let backend = Forge::default_of(Kind::GitHub).backend_over(recorded.clone());
    (backend, recorded)
}

fn rest(path: &str, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn gql(out: Value) -> Value {
    json!({"call": {"endpoint": "graphql",
                    "fields": [{"text": ["query", work_item::QUERY]},
                               {"text": ["owner", "acme"]},
                               {"text": ["name", "api"]},
                               {"typed": ["number", "12"]}]},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn key(text: &str) -> TrackerKey {
    TrackerKey::parse(text).unwrap()
}

/// Issue `number` of `repo`, as a REST listing answers it.
fn listed(repo: &str, number: u64) -> Value {
    json!({"id": 2000 + number, "node_id": format!("I_{number}"), "number": number,
           "title": "other", "state": "open",
           "html_url": format!("https://github.com/{repo}/issues/{number}")})
}

fn closed_issue() -> Value {
    json!({"id": 1012, "node_id": "I_kwDOAcme12", "number": 12, "title": "Port the picker",
           "state": "closed", "state_reason": "completed",
           "html_url": "https://github.com/acme/api/issues/12",
           "labels": [{"name": "ws:alpha"}],
           "assignees": [{"login": "octocat"}, {"login": "hubot"}],
           "milestone": {"number": 1, "node_id": "MI_1", "title": "v1", "state": "open",
                         "due_on": "2026-10-31T07:00:00Z"},
           "type": {"id": 7, "node_id": "IT_7", "name": "Bug"},
           "issue_dependencies_summary": {"blocked_by": 0, "blocking": 1,
                                          "total_blocked_by": 1, "total_blocking": 1}})
}

fn links() -> Value {
    json!({"data": {"repository": {"issue": {
        "parent": {"url": "https://github.com/acme/api/issues/3"},
        "subIssues": {"pageInfo": {"hasNextPage": false},
                      "nodes": [{"url": "https://github.com/acme/web/issues/14"}]},
        "closedByPullRequestsReferences": {"pageInfo": {"hasNextPage": false}, "nodes": [
            {"number": 20, "url": "https://github.com/acme/api/pull/20", "state": "MERGED"},
            {"number": 21, "url": "https://github.com/acme/api/pull/21", "state": "CLOSED"},
            {"number": 22, "url": "https://github.com/acme/api/pull/22", "state": "OPEN"}]},
        "projectItems": {"pageInfo": {"hasNextPage": false}, "nodes": [
            {"project": {"id": "PVT_1", "title": "Roadmap",
                         "url": "https://github.com/orgs/acme/projects/1"},
             "status": {"__typename": "ProjectV2ItemFieldSingleSelectValue", "name": "Done"},
             "fieldValues": {"nodes": [
                 {"__typename": "ProjectV2ItemFieldTextValue"},
                 {"__typename": "ProjectV2ItemFieldIterationValue", "iterationId": "it3",
                  "title": "Sprint 3", "startDate": "2026-10-05", "duration": 14}]}},
            {"project": {"id": "PVT_2", "title": "Triage",
                         "url": "https://github.com/orgs/acme/projects/2"},
             "status": null,
             "fieldValues": {"nodes": [
                 {"__typename": "ProjectV2ItemFieldIterationValue", "iterationId": "it9",
                  "title": "Sprint 9", "startDate": "2026-12-01", "duration": 7}]}}]}
    }}}})
}

#[test]
fn an_issue_is_read_with_its_type_reason_assignees_relations_boards_and_iteration() {
    let (backend, recorded) = over(json!([
        rest("repos/acme/api/issues/12", closed_issue()),
        gql(links()),
        rest(
            "repos/acme/api/issues/12/dependencies/blocked_by?per_page=100&page=1",
            json!([listed("acme/api", 11)])
        ),
        rest(
            "repos/acme/api/issues/12/dependencies/blocking?per_page=100&page=1",
            json!([listed("acme/web", 15)])
        ),
    ]));
    let item = backend.read(&Caller::command(), "acme/api", 12).unwrap();
    assert_eq!(recorded.unspent(), Vec::new(), "recorded and never asked");

    assert_eq!(item.key, key("github:github.com/acme/api#12"));
    assert_eq!(item.forge_ref, Some(ForgeRef("I_kwDOAcme12".into())));
    assert_eq!(item.url, "https://github.com/acme/api/issues/12");
    assert_eq!(item.title, "Port the picker");
    assert_eq!(item.kind, ItemKind::Issue);
    assert_eq!(item.issue_type.as_deref(), Some("Bug"));
    assert_eq!(item.state, State::Closed);
    assert_eq!(item.closed_as, Some(ClosedAs::Completed));
    assert_eq!(item.labels, ["ws:alpha"]);
    assert_eq!(item.assignees, ["octocat", "hubot"]);
    assert_eq!(item.milestone.as_ref().unwrap().title, "v1");
    assert!(item.is_sub_issue());
    assert_eq!(
        item.relations,
        [
            Relation::Parent(key("github:github.com/acme/api#3")),
            Relation::Child(key("github:github.com/acme/web#14")),
            Relation::BlockedBy(key("github:github.com/acme/api#11")),
            Relation::Blocks(key("github:github.com/acme/web#15")),
            Relation::ClosedBy(Pr {
                number: 20,
                url: "https://github.com/acme/api/pull/20".into()
            }),
            Relation::ClosedBy(Pr {
                number: 22,
                url: "https://github.com/acme/api/pull/22".into()
            }),
        ],
        "a closed request that never merged closes nothing"
    );
    assert_eq!(
        item.placements,
        [
            Placement {
                board: ForgeRef("PVT_1".into()),
                board_title: "Roadmap".into(),
                board_url: "https://github.com/orgs/acme/projects/1".into(),
                status: Some("Done".into()),
            },
            Placement {
                board: ForgeRef("PVT_2".into()),
                board_title: "Triage".into(),
                board_url: "https://github.com/orgs/acme/projects/2".into(),
                status: None,
            },
        ]
    );
    let day = |m, d| chrono::NaiveDate::from_ymd_opt(2026, m, d);
    assert_eq!(
        item.iteration,
        Some(Iteration {
            forge_ref: Some(ForgeRef("it3".into())),
            title: "Sprint 3".into(),
            start: day(10, 5),
            end: day(10, 18),
        }),
        "the first board's iteration"
    );
}

#[test]
fn an_issue_with_no_dependencies_asks_for_none_and_reopened_is_no_close_reason() {
    let mut issue = closed_issue();
    issue["state"] = json!("open");
    issue["state_reason"] = json!("reopened");
    issue["type"] = Value::Null;
    issue["assignees"] = json!([]);
    issue["issue_dependencies_summary"] =
        json!({"blocked_by": 0, "blocking": 0, "total_blocked_by": 0, "total_blocking": 0});
    let nothing = json!({"data": {"repository": {"issue": {
        "parent": null,
        "subIssues": {"pageInfo": {"hasNextPage": false}, "nodes": []},
        "closedByPullRequestsReferences": {"pageInfo": {"hasNextPage": false}, "nodes": []},
        "projectItems": {"pageInfo": {"hasNextPage": false}, "nodes": []}}}}});
    let (backend, recorded) = over(json!([
        rest("repos/acme/api/issues/12", issue),
        gql(nothing)
    ]));
    let item = backend.read(&Caller::command(), "acme/api", 12).unwrap();
    assert_eq!(recorded.unspent(), Vec::new(), "recorded and never asked");
    assert_eq!(item.state, State::Open);
    assert_eq!(item.closed_as, None);
    assert_eq!(item.issue_type, None);
    assert!(item.assignees.is_empty());
    assert!(item.relations.is_empty());
    assert!(item.placements.is_empty());
    assert_eq!(item.iteration, None);
}

#[test]
fn an_answer_with_no_dependency_summary_asks_for_no_dependencies() {
    // A GitHub Enterprise Server without issue dependencies answers no summary.
    let mut issue = closed_issue();
    issue
        .as_object_mut()
        .unwrap()
        .remove("issue_dependencies_summary");
    let (backend, recorded) = over(json!([
        rest("repos/acme/api/issues/12", issue),
        gql(links())
    ]));
    let item = backend.read(&Caller::command(), "acme/api", 12).unwrap();
    assert_eq!(recorded.unspent(), Vec::new(), "recorded and never asked");
    assert!(
        !item
            .relations
            .iter()
            .any(|r| matches!(r, Relation::BlockedBy(_) | Relation::Blocks(_)))
    );
}

#[test]
fn more_relations_than_one_answer_holds_is_an_error_never_a_shorter_list() {
    let mut more = links();
    more["data"]["repository"]["issue"]["projectItems"]["pageInfo"]["hasNextPage"] = json!(true);
    let (backend, _) = over(json!([
        rest("repos/acme/api/issues/12", closed_issue()),
        gql(more)
    ]));
    let refused = backend
        .read(&Caller::command(), "acme/api", 12)
        .unwrap_err();
    assert!(refused.said().contains("boards"), "{}", refused.said());
}

#[test]
fn an_issue_graphql_cannot_find_is_not_found() {
    let (backend, _) = over(json!([
        rest("repos/acme/api/issues/12", closed_issue()),
        gql(json!({"data": {"repository": {"issue": null}}})),
    ]));
    let refused = backend
        .read(&Caller::command(), "acme/api", 12)
        .unwrap_err();
    assert!(
        matches!(refused.failure(), Failure::NotFound),
        "{refused:?}"
    );
}
