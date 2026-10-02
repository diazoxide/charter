use std::sync::Arc;

use serde_json::{Value, json};

use super::*;
use crate::forge::Kind;
use crate::forge::recorded::Recorded;
use crate::work::list;
use crate::work::log::Op;

const DEVICE: &str = "01K6H0Z8Y3V1N3G4QK0A9T5B7C";
const CHAT: &str = "01K6H10000AAAAAAAAAAAAAAAA";

fn stamp(second: u32) -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 10, 2)
        .unwrap()
        .and_hms_opt(8, 0, second)
        .unwrap()
}

/// A project with workspace `alpha` holding `repos`, each in the inventory on GitHub.
fn project(repos: &[&str]) -> (tempfile::TempDir, Workspace) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    let ws = Plane::open(root).workspace("alpha").unwrap();
    std::fs::create_dir_all(ws.dir()).unwrap();
    let members: Vec<Value> = repos.iter().map(|r| json!({"name": r})).collect();
    std::fs::write(
        ws.dir().join("workspace.json"),
        json!({"name": "alpha", "repos": members}).to_string(),
    )
    .unwrap();
    let records: Vec<Value> = repos
        .iter()
        .map(|r| {
            json!({"name": r, "path_with_namespace": format!("acme/{r}"), "forge": "github",
                   "web_url": format!("https://github.com/acme/{r}")})
        })
        .collect();
    std::fs::create_dir_all(root.join("inventory")).unwrap();
    std::fs::write(
        crate::inventory::path(root),
        json!({"group": "acme", "repos": records}).to_string(),
    )
    .unwrap();
    (tmp, ws)
}

fn exchange(method: Option<&str>, path: &str, fields: Value, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": method, "path": path}}, "fields": fields},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn about(visibility: &str, has_issues: bool) -> Value {
    exchange(
        None,
        "repos/acme/api",
        json!([]),
        json!({"visibility": visibility, "has_issues": has_issues,
               "permissions": {"pull": true, "push": true}}),
    )
}

fn created(fields: Value) -> Value {
    exchange(
        Some("POST"),
        "repos/acme/api/issues",
        fields,
        json!({"id": 1012, "node_id": "I_kwDOAcme12", "number": 12, "title": "Port the picker",
               "state": "open", "html_url": "https://github.com/acme/api/issues/12"}),
    )
}

fn github(exchanges: Value) -> (Box<dyn ForgeBackend>, Arc<Recorded>) {
    let text = json!({"source": "GitHub REST API version 2022-11-28", "exchanges": exchanges});
    let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
    let backend = Forge::default_of(Kind::GitHub).backend_over(recorded.clone());
    (backend, recorded)
}

fn the_picker(ws: &Workspace) -> String {
    let path = ws
        .add_todo("Port the picker\n\nThe old one is slow.", stamp(0))
        .unwrap();
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

/// FW-5's acceptance, at the core: a todo promoted to a GitHub issue keeps its chat link, and
/// the Work list the board draws (FW-9) shows one item, not two.
#[test]
fn a_todo_promoted_to_a_github_issue_keeps_its_chat_link_and_is_one_item() {
    let (tmp, ws) = project(&["api"]);
    let root = tmp.path();
    let stem = the_picker(&ws);
    let todo = TrackerKey::todo("alpha", &stem).unwrap();
    log::append(
        root,
        "alpha",
        DEVICE,
        stamp(1).and_utc(),
        &Op::link_chat(todo.clone(), CHAT),
    )
    .unwrap();
    let (backend, recorded) = github(json!([
        about("private", true),
        created(json!([{"text": ["title", "Port the picker"]},
                        {"text": ["body", "The old one is slow."]},
                        {"text": ["labels[]", "ws:alpha"]}]))
    ]));
    let target = target(root, "alpha", Some("api")).unwrap();
    let mut told = Vec::new();
    let promoted = promote(
        &ws,
        "port-the-picker",
        &target,
        backend.as_ref(),
        &Caller::command(),
        DEVICE,
        stamp(2),
        &mut |step| {
            told.push(match step {
                Step::Sending {
                    target,
                    about,
                    label,
                } => format!(
                    "sending to {} ({}, {})",
                    target.repo.path_with_namespace,
                    about.visibility.word(),
                    label.as_deref().unwrap_or("no label")
                ),
                Step::Created(item) => format!("created {}", item.key),
            })
        },
    )
    .unwrap();
    assert_eq!(recorded.unspent(), Vec::new());
    let issue = TrackerKey::parse("github:github.com/acme/api#12").unwrap();
    assert_eq!(promoted.item.key, issue);
    assert_eq!(
        told,
        [
            "sending to acme/api (private, ws:alpha)",
            "created github:github.com/acme/api#12"
        ],
        "the repo and whether it is public are said before anything is sent"
    );

    let folded = log::fold(root);
    assert_eq!(
        folded.chat_link(CHAT),
        Some(issue.clone()),
        "it keeps its chat link"
    );
    let listed = list::of(&ws, &folded).unwrap();
    assert_eq!(listed.len(), 1, "one card, not two: {listed:?}");
    assert_eq!(listed[0].key, issue);
    assert_eq!(listed[0].chats, [CHAT]);

    assert!(ws.todos().unwrap().is_empty(), "the todo is closed");
    let journal: Vec<String> = ws
        .memories()
        .unwrap()
        .into_iter()
        .map(|m| m.title)
        .collect();
    assert_eq!(
        journal,
        ["Promoted todo: Port the picker → github:github.com/acme/api#12"]
    );
}

#[test]
fn a_public_repo_is_named_as_public_and_its_issue_carries_no_workspace_label() {
    let (tmp, ws) = project(&["api"]);
    the_picker(&ws);
    let (backend, recorded) = github(json!([
        about("public", true),
        created(json!([{"text": ["title", "Port the picker"]},
                        {"text": ["body", "The old one is slow."]}]))
    ]));
    let target = target(tmp.path(), "alpha", None).unwrap();
    let mut public = None;
    promote(
        &ws,
        "port-the-picker",
        &target,
        backend.as_ref(),
        &Caller::command(),
        DEVICE,
        stamp(2),
        &mut |step| {
            if let Step::Sending { about, label, .. } = step {
                public = Some(about.visibility == Visibility::Public);
                assert_eq!(label, None);
            }
        },
    )
    .unwrap();
    assert_eq!(public, Some(true));
    assert_eq!(recorded.unspent(), Vec::new());
}

#[test]
fn a_repo_that_takes_no_issue_is_refused_before_anything_is_sent() {
    let (tmp, ws) = project(&["api"]);
    let stem = the_picker(&ws);
    let (backend, recorded) = github(json!([about("private", false)]));
    let target = target(tmp.path(), "alpha", Some("api")).unwrap();
    let mut sent = false;
    let refused = promote(
        &ws,
        &stem,
        &target,
        backend.as_ref(),
        &Caller::command(),
        DEVICE,
        stamp(2),
        &mut |_| sent = true,
    )
    .unwrap_err();
    assert!(refused.contains("issues turned off"), "{refused}");
    assert!(!sent, "nothing was said to be sent");
    assert_eq!(recorded.unspent(), Vec::new(), "only the repo was read");
    assert_eq!(ws.todos().unwrap().len(), 1, "the todo stays open");
    assert!(!log::dir_for(tmp.path(), "alpha").exists(), "no alias");
}

#[test]
fn the_repo_is_named_among_the_workspaces_or_is_its_only_one_on_a_forge() {
    let (tmp, _) = project(&["api", "web"]);
    let root = tmp.path();
    assert_eq!(
        target(root, "alpha", Some("web"))
            .unwrap()
            .repo
            .path_with_namespace,
        "acme/web"
    );
    let many = target(root, "alpha", None).unwrap_err();
    assert!(
        many.contains("--repo") && many.contains("api, web"),
        "{many}"
    );
    let stranger = target(root, "alpha", Some("billing")).unwrap_err();
    assert!(
        stranger.contains("is not a repo of workspace 'alpha'"),
        "{stranger}"
    );
    let (one, _) = project(&["api"]);
    let only = target(one.path(), "alpha", None).unwrap();
    assert_eq!(only.name, "api");
    assert_eq!(only.forge, Forge::default_of(Kind::GitHub));
}

/// The target holds the repo as the inventory row reads, typed (#911): what `about` and the
/// caller's sentence are given is that record, not fields dug out of the row's JSON.
#[test]
fn the_target_holds_the_repo_its_inventory_row_reads_as() {
    let (tmp, _) = project(&["api"]);
    let only = target(tmp.path(), "alpha", None).unwrap();
    assert_eq!(
        only.repo,
        crate::forge::RepoRecord {
            id: None,
            name: "api".into(),
            path_with_namespace: "acme/api".into(),
            default_branch: None,
            description: String::new(),
            web_url: "https://github.com/acme/api".into(),
            ssh_url: String::new(),
            topics: Vec::new(),
            forge: Kind::GitHub,
        }
    );
}

#[test]
fn a_promote_that_stopped_after_its_alias_is_closed_at_the_next_look() {
    let (tmp, ws) = project(&["api"]);
    let root = tmp.path();
    let stem = the_picker(&ws);
    let todo = TrackerKey::todo("alpha", &stem).unwrap();
    let issue = TrackerKey::parse("github:github.com/acme/api#12").unwrap();
    log::append_alias(
        root,
        "alpha",
        DEVICE,
        stamp(1).and_utc(),
        todo,
        issue.clone(),
        Cause::Promoted,
    )
    .unwrap();
    let closed = finish_closes(&ws, &log::fold(root), stamp(2)).unwrap();
    assert_eq!(closed, [("Port the picker".to_string(), issue)]);
    assert!(ws.todos().unwrap().is_empty());
    assert!(
        finish_closes(&ws, &log::fold(root), stamp(3))
            .unwrap()
            .is_empty(),
        "nothing is left to close"
    );
}

#[test]
fn a_todo_already_promoted_is_not_promoted_twice() {
    let (tmp, ws) = project(&["api"]);
    let root = tmp.path();
    let stem = the_picker(&ws);
    let todo = TrackerKey::todo("alpha", &stem).unwrap();
    let issue = TrackerKey::parse("github:github.com/acme/api#12").unwrap();
    log::append_alias(
        root,
        "alpha",
        DEVICE,
        stamp(1).and_utc(),
        todo,
        issue,
        Cause::Promoted,
    )
    .unwrap();
    let (backend, recorded) = github(json!([]));
    let target = target(root, "alpha", None).unwrap();
    let refused = promote(
        &ws,
        &stem,
        &target,
        backend.as_ref(),
        &Caller::command(),
        DEVICE,
        stamp(2),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(refused.contains("already promoted"), "{refused}");
    assert_eq!(recorded.unspent(), Vec::new());
}

/// Promote `stem` over `exchanges`, answering the refusal and whether every exchange was asked.
fn refused_over(tmp: &tempfile::TempDir, ws: &Workspace, stem: &str, exchanges: Value) -> String {
    let (backend, recorded) = github(exchanges);
    let target = target(tmp.path(), "alpha", Some("api")).unwrap();
    let refused = promote(
        ws,
        stem,
        &target,
        backend.as_ref(),
        &Caller::command(),
        DEVICE,
        stamp(2),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(
        recorded.unspent(),
        Vec::new(),
        "every recorded exchange was asked"
    );
    refused
}

#[cfg(unix)]
#[test]
fn a_log_that_cannot_be_written_refuses_before_any_issue_exists() {
    let (tmp, ws) = project(&["api"]);
    let stem = the_picker(&ws);
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), log::dir_for(tmp.path(), "alpha")).unwrap();
    // Nothing is recorded: the repo is not even read, and no issue is POSTed.
    let refused = refused_over(&tmp, &ws, &stem, json!([]));
    assert!(refused.contains("nothing was sent"), "{refused}");
    assert_eq!(ws.todos().unwrap().len(), 1);
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn a_device_id_that_is_not_one_refuses_before_any_issue_exists() {
    let (tmp, ws) = project(&["api"]);
    let stem = the_picker(&ws);
    let (backend, recorded) = github(json!([]));
    let target = target(tmp.path(), "alpha", None).unwrap();
    let refused = promote(
        &ws,
        &stem,
        &target,
        backend.as_ref(),
        &Caller::command(),
        "my-laptop",
        stamp(2),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(refused.contains("not a device id"), "{refused}");
    assert_eq!(recorded.unspent(), Vec::new());
}

#[test]
fn an_archived_repo_is_refused_before_anything_is_sent() {
    let (tmp, ws) = project(&["api"]);
    let stem = the_picker(&ws);
    let archived = exchange(
        None,
        "repos/acme/api",
        json!([]),
        json!({"visibility": "private", "archived": true, "has_issues": true}),
    );
    let refused = refused_over(&tmp, &ws, &stem, json!([archived]));
    assert!(refused.contains("is archived"), "{refused}");
}

#[test]
fn an_internal_repo_gets_no_workspace_label() {
    let (tmp, ws) = project(&["api"]);
    let stem = the_picker(&ws);
    let (backend, recorded) = github(json!([
        about("internal", true),
        created(json!([{"text": ["title", "Port the picker"]},
                        {"text": ["body", "The old one is slow."]}]))
    ]));
    let target = target(tmp.path(), "alpha", None).unwrap();
    promote(
        &ws,
        &stem,
        &target,
        backend.as_ref(),
        &Caller::command(),
        DEVICE,
        stamp(2),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(recorded.unspent(), Vec::new());
}

#[test]
fn an_issue_keyed_on_a_host_charter_did_not_ask_is_not_aliased() {
    let (tmp, ws) = project(&["api"]);
    let stem = the_picker(&ws);
    let elsewhere = exchange(
        Some("POST"),
        "repos/acme/api/issues",
        json!([{"text": ["title", "Port the picker"]},
               {"text": ["body", "The old one is slow."]},
               {"text": ["labels[]", "ws:alpha"]}]),
        json!({"id": 1, "node_id": "I_1", "number": 12, "title": "Port the picker",
               "state": "open", "html_url": "https://evil.example/acme/api/issues/12"}),
    );
    let refused = refused_over(&tmp, &ws, &stem, json!([about("private", true), elsewhere]));
    assert!(refused.contains("not trusted"), "{refused}");
    assert_eq!(ws.todos().unwrap().len(), 1, "the todo stays open");
    assert!(!log::fold(tmp.path()).is_aliased(&TrackerKey::todo("alpha", &stem).unwrap()));
}
