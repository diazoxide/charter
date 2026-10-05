//! `charter change push` on GitLab (GL-3b): the same scenes as the GitHub ones above, with each
//! member's origin on GitLab in a subgroup, and each question spelt as GitLab's REST API (v4)
//! spells it. Recorded against the GitLab 19.4 documentation, `doc/api/merge_requests.md`:
//! "List project merge requests", "Create a merge request", "Get single MR" and "Update a
//! merge request". The bare remotes are local, and nothing reaches a network.
//!
//! The helpers here are named for merge requests so they never shadow the GitHub ones above.

use super::*;

/// The group every GitLab member is in: a subgroup, so a repo's path has two slashes.
const GROUP: &str = "acme/plat";

/// A self-managed GitLab, declared in `charter.toml`.
const SELF_MANAGED: &str =
    "schema = 1\n\n[[forge]]\nkind = \"gitlab\"\nhost = \"gitlab.acme.test\"\n";

/// The API path of a repo's merge requests: the repo named by its URL-encoded path.
fn mrs_of(repo: &str) -> String {
    format!("projects/acme%2Fplat%2F{repo}/merge_requests")
}

/// One merge request as GitLab lists it, built up from a repo's own open one.
struct Mr {
    host: &'static str,
    repo: &'static str,
    iid: u64,
    state: &'static str,
    /// The repo the source branch is in: 3 is the target repo itself, anything else a fork.
    source_project_id: u64,
    extra: Value,
}

impl Mr {
    /// `repo`'s own open merge request `!iid` on gitlab.com.
    fn own(repo: &'static str, iid: u64) -> Mr {
        Mr {
            host: "gitlab.com",
            repo,
            iid,
            state: "opened",
            source_project_id: 3,
            extra: json!({}),
        }
    }

    fn state(self, state: &'static str) -> Mr {
        Mr { state, ..self }
    }

    fn opened_in_a_fork(self) -> Mr {
        Mr {
            source_project_id: 41,
            ..self
        }
    }

    fn with(self, extra: Value) -> Mr {
        Mr { extra, ..self }
    }

    fn record(&self) -> Value {
        let mut record = json!({"iid": self.iid, "id": 900 + self.iid,
                                "web_url": mr_web(self.host, self.repo, self.iid),
                                "state": self.state, "sha": SHA, "source_branch": BRANCH,
                                "source_project_id": self.source_project_id,
                                "target_project_id": 3});
        for (k, v) in self.extra.as_object().unwrap() {
            record[k] = v.clone();
        }
        record
    }
}

fn mr_web(host: &str, repo: &str, iid: u64) -> String {
    format!("https://{host}/{GROUP}/{repo}/-/merge_requests/{iid}")
}

/// `by_head`'s lookup, answered with `mrs`.
fn mrs_from_the_branch(repo: &str, mrs: &[Mr]) -> Value {
    get(
        &format!(
            "{}?source_branch=change%2Fapi-2&state=all&per_page=100",
            mrs_of(repo)
        ),
        Value::Array(mrs.iter().map(Mr::record).collect()),
    )
}

fn no_mr_from_the_branch(repo: &str) -> Value {
    mrs_from_the_branch(repo, &[])
}

/// `open_or_update`'s lookup for an open merge request into `main`, finding none.
fn no_open_mr_into_main(repo: &str) -> Value {
    get(
        &format!(
            "{}?state=opened&source_branch=change%2Fapi-2&target_branch=main&per_page=100",
            mrs_of(repo)
        ),
        json!([]),
    )
}

/// The `POST` that opens `repo`'s merge request, answered with `reply`.
fn mr_post(repo: &str, body: &str, reply: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": "POST", "path": mrs_of(repo)}},
                    "fields": [{"text": ["source_branch", BRANCH]},
                               {"text": ["target_branch", "main"]},
                               {"text": ["title", format!("api-2: {repo}")]},
                               {"text": ["description", body]}]},
           "reply": reply})
}

/// The open-or-update of a merge request that does not exist yet: the lookup, then the `POST`.
fn mr_opened_on(host: &str, repo: &str, iid: u64, body: &str) -> [Value; 2] {
    let out = json!({"iid": iid, "id": 900 + iid, "web_url": mr_web(host, repo, iid),
                     "state": "opened"});
    [
        no_open_mr_into_main(repo),
        mr_post(repo, body, json!({"code": 0, "out": out.to_string()})),
    ]
}

fn mr_opened(repo: &str, iid: u64, body: &str) -> [Value; 2] {
    mr_opened_on("gitlab.com", repo, iid, body)
}

/// `body`: the merge request's own record, its `description` as GitLab answers it.
fn mr_description_is(repo: &str, iid: u64, description: Value) -> Value {
    get(
        &format!("{}/{iid}", mrs_of(repo)),
        json!({"iid": iid, "description": description}),
    )
}

fn mr_body_is(repo: &str, iid: u64, body: &str) -> Value {
    mr_description_is(repo, iid, json!(body))
}

/// `set_body`'s `PUT`, carrying the description and nothing else, so a draft's title, its
/// labels and its reviewers are never touched; answered with `reply`.
fn mr_put(repo: &str, iid: u64, body: &str, reply: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": "PUT",
                                          "path": format!("{}/{iid}", mrs_of(repo))}},
                    "fields": [{"text": ["description", body]}]},
           "reply": reply})
}

fn mr_body_set(repo: &str, iid: u64, body: &str) -> Value {
    mr_put(
        repo,
        iid,
        body,
        json!({"code": 0, "out": json!({"iid": iid}).to_string()}),
    )
}

/// A refusal as the forge CLI reports one.
fn refused(said: &str) -> Value {
    json!({"code": 1, "out": "", "err": said})
}

/// The block once both merge requests are open: a GitLab cross-repo reference names the full
/// path.
const BOTH_MRS: &str = "bump the api

<!-- BEGIN purlis change — GENERATED by `purlis change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | acme/plat/widget!7 | — |
| gadget | acme/plat/gadget!9 | widget |
<!-- END purlis change -->";

/// The block with only `widget`, before its merge request is open.
const LONE_NEW: &str = "bump the api

<!-- BEGIN purlis change — GENERATED by `purlis change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | — | — |
<!-- END purlis change -->";

/// [`LONE_NEW`] once `widget`'s merge request `!7` is open.
fn lone_mr() -> String {
    LONE_NEW.replace("| widget | — |", "| widget | acme/plat/widget!7 |")
}

impl World {
    fn gitlab_clone(&self, repo: &str) -> PathBuf {
        self.clone_on("gitlab.com", GROUP, repo)
    }
}

fn first_mr_push(world: &World) -> (u8, String, Arc<Recorded>) {
    let [w1, w2] = mr_opened("widget", 7, NEW_BODY);
    let [g1, g2] = mr_opened("gadget", 9, NEW_BODY);
    world.push(json!([
        no_mr_from_the_branch("widget"),
        w1,
        w2,
        no_mr_from_the_branch("gadget"),
        g1,
        g2,
        mr_body_is("gadget", 9, NEW_BODY),
        mr_body_set("gadget", 9, BOTH_MRS),
        mr_body_is("widget", 7, NEW_BODY),
        mr_body_set("widget", 7, BOTH_MRS),
    ]))
}

/// `widget` alone, with `exchanges` answering after its branch is looked up by `found`.
fn lone_mr_push(world: &World, found: &[Mr], then: Vec<Value>) -> (u8, String, Arc<Recorded>) {
    let mut exchanges = vec![mrs_from_the_branch("widget", found)];
    exchanges.extend(then);
    world.push(Value::Array(exchanges))
}

#[test]
fn every_members_branch_is_pushed_and_its_merge_request_opened_carrying_the_cross_link_block() {
    let world = World::new("schema = 1\n");
    let widget = world.gitlab_clone("widget");
    world.gitlab_clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let (code, said, recorded) = first_mr_push(&world);
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    let head = run(&widget, &["rev-parse", BRANCH]).trim().to_string();
    assert_eq!(world.remote_in_has(GROUP, "widget", BRANCH), Some(head));
    assert!(world.remote_in_has(GROUP, "gadget", BRANCH).is_some());
    assert!(
        said.contains(
            "widget  change/api-2 → https://gitlab.com/acme/plat/widget.git, its request into main"
        ),
        "{said}"
    );
    assert!(said.contains("opened → !7"), "{said}");
    assert!(said.contains("cross-link block written into 2"), "{said}");
}

#[test]
fn the_push_to_gitlab_is_one_refspec_to_the_printed_destination_with_no_force() {
    let world = World::new("schema = 1\n");
    let widget = world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let before = crate::worktree::git::tally::asked(&widget).len();
    let [w1, w2] = mr_opened("widget", 7, LONE_NEW);
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[],
        vec![
            w1,
            w2,
            mr_body_is("widget", 7, LONE_NEW),
            mr_body_set("widget", 7, &lone_mr()),
        ],
    );
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    let asked = crate::worktree::git::tally::asked(&widget)[before..].to_vec();
    let refspec = format!("refs/heads/{BRANCH}:refs/heads/{BRANCH}");
    let push = asked
        .iter()
        .find(|argv| argv.iter().any(|a| a == "push"))
        .unwrap_or_else(|| panic!("no push in {asked:?}"));
    assert_eq!(
        push,
        &[
            "-c",
            "push.followTags=false",
            "-c",
            "push.pushOption=",
            "-c",
            "push.recurseSubmodules=no",
            "push",
            world
                .route("https://gitlab.com/acme/plat/widget.git")
                .as_str(),
            refspec.as_str(),
        ]
        .map(String::from)
    );
}

#[test]
fn a_second_push_opens_no_second_merge_request_and_writes_no_description_that_is_current() {
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.gitlab_clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let (code, said, recorded) = first_mr_push(&world);
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    // No POST and no PUT is recorded: either one fails the run as a request nobody wrote down.
    let (code, said, recorded) = world.push(json!([
        mrs_from_the_branch("widget", &[Mr::own("widget", 7)]),
        mrs_from_the_branch("gadget", &[Mr::own("gadget", 9)]),
        mr_body_is("gadget", 9, BOTH_MRS),
        mr_body_is("widget", 7, BOTH_MRS),
    ]));
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(said.contains("already current in 2"), "{said}");
}

#[test]
fn when_membership_changes_the_block_in_a_merge_request_is_replaced_in_place() {
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let theirs = format!("Reviewed by Ann.\n\n{BOTH_MRS}\n\nShip after Friday.");
    let ours = format!("Reviewed by Ann.\n\n{}\n\nShip after Friday.", lone_mr());
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[Mr::own("widget", 7)],
        vec![
            mr_body_is("widget", 7, &theirs),
            mr_body_set("widget", 7, &ours),
        ],
    );
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
}

#[test]
fn a_draft_merge_request_is_adopted_and_only_its_description_is_written() {
    // GitLab keeps a draft in its title (`Draft: `) and its `draft` flag. Charter writes the
    // description alone, so the merge request stays a draft and keeps its author's title.
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let draft = Mr::own("widget", 7)
        .with(json!({"draft": true, "work_in_progress": true, "title": "Draft: widget api"}));
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[draft],
        vec![
            mr_body_is("widget", 7, LONE_NEW),
            mr_body_set("widget", 7, &lone_mr()),
        ],
    );
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(said.contains("pushed → !7"), "{said}");
}

#[test]
fn a_merge_request_from_a_fork_with_the_same_branch_name_is_never_adopted() {
    // GitLab matches `source_branch` by name in forks too. A stranger's merge request from
    // their fork's `change/api-2` is not this member's: charter opens the member's own, and
    // writes nothing into the stranger's.
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let [w1, w2] = mr_opened("widget", 7, LONE_NEW);
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[Mr::own("widget", 5).opened_in_a_fork()],
        vec![
            w1,
            w2,
            mr_body_is("widget", 7, LONE_NEW),
            mr_body_set("widget", 7, &lone_mr()),
        ],
    );
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(!said.contains("!5"), "{said}");
}

#[test]
fn a_merged_merge_request_is_adopted_and_said_as_merged() {
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let merged = Mr::own("widget", 7)
        .state("merged")
        .with(json!({"merge_commit_sha": "e5bd3914e2e596debea16f433f57875b5b90bcd6"}));
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[merged],
        vec![
            mr_body_is("widget", 7, LONE_NEW),
            mr_body_set("widget", 7, &lone_mr()),
        ],
    );
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(said.contains("pushed → !7 (merged as e5bd391"), "{said}");
}

#[test]
fn a_closed_merge_request_is_adopted_and_said_as_rejected_and_no_second_one_is_opened() {
    // D-0004: a request already on the branch is the member's, in any state. A closed one is
    // shown as REJECTED and gets the block; no POST is recorded, so none is opened.
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[Mr::own("widget", 7).state("closed")],
        vec![
            mr_body_is("widget", 7, LONE_NEW),
            mr_body_set("widget", 7, &lone_mr()),
        ],
    );
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(said.contains("pushed → !7 (REJECTED)"), "{said}");
}

#[test]
fn a_merge_request_with_no_description_is_left_alone_and_named() {
    // GitLab answers `"description": null` for a merge request opened with none: there is no
    // block in it for charter to write into.
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[Mr::own("widget", 7)],
        vec![mr_description_is("widget", 7, Value::Null)],
    );
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("!7's description has no single charter block"),
        "{said}"
    );
}

#[test]
fn a_merge_request_gitlab_refuses_to_open_is_said_in_its_words_and_the_push_kept() {
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[],
        vec![
            no_open_mr_into_main("widget"),
            mr_post("widget", LONE_NEW, refused("HTTP 403: 403 Forbidden")),
        ],
    );
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("widget: pushed, but its request into main was not opened"),
        "{said}"
    );
    assert!(said.contains("403 Forbidden"), "{said}");
    assert!(world.remote_in_has(GROUP, "widget", BRANCH).is_some());
}

#[test]
fn a_description_gitlab_refuses_to_write_is_said_and_the_exit_is_one() {
    let world = World::new("schema = 1\n");
    world.gitlab_clone("widget");
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[Mr::own("widget", 7)],
        vec![
            mr_body_is("widget", 7, LONE_NEW),
            mr_put("widget", 7, &lone_mr(), refused("HTTP 409: 409 Conflict")),
        ],
    );
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("widget: !7's cross-link block was not written"),
        "{said}"
    );
    assert!(said.contains("409 Conflict"), "{said}");
}

#[test]
fn a_repo_whose_default_branch_is_unknown_is_pushed_and_no_merge_request_opened() {
    // With no `origin/HEAD` and no inventory, charter cannot say where a merge request goes,
    // so it opens none: no lookup into a base and no POST is recorded.
    let world = World::new("schema = 1\n");
    let widget = world.gitlab_clone("widget");
    run(
        &widget,
        &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"],
    );
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = lone_mr_push(&world, &[], vec![]);
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("does not know its default branch") && said.contains("remote set-head"),
        "{said}"
    );
    assert!(world.remote_in_has(GROUP, "widget", BRANCH).is_some());
}

#[test]
fn a_self_managed_gitlab_is_pushed_to_and_asked_at_its_own_host() {
    let world = World::new(SELF_MANAGED);
    let widget = world.clone_on("gitlab.acme.test", GROUP, "widget");
    world.change(&[("widget", &[])]);
    let [w1, w2] = mr_opened_on("gitlab.acme.test", "widget", 7, LONE_NEW);
    let (code, said, recorded) = lone_mr_push(
        &world,
        &[],
        vec![
            w1,
            w2,
            mr_body_is("widget", 7, LONE_NEW),
            mr_body_set("widget", 7, &lone_mr()),
        ],
    );
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("widget  change/api-2 → https://gitlab.acme.test/acme/plat/widget.git"),
        "{said}"
    );
    let hosts = world.asked_hosts.borrow().clone();
    assert_eq!(hosts.len(), 5, "{hosts:?}");
    assert!(
        hosts.iter().all(|h| h == "gitlab.acme.test"),
        "every API request goes to the declared host: {hosts:?}"
    );
    let head = run(&widget, &["rev-parse", BRANCH]).trim().to_string();
    assert_eq!(world.remote_in_has(GROUP, "widget", BRANCH), Some(head));
}

#[test]
fn a_member_on_another_host_is_linked_by_its_requests_url_not_by_a_reference() {
    // `acme/widget#7` in a GitLab description is GitLab's own issue 7 of a GitLab repo
    // `acme/widget`, and `acme/plat/gadget!9` in a GitHub description is no link at all. A
    // reference only means the member's request on the description's own host, so a member
    // elsewhere is named by the URL its forge gave.
    let world = World::new(SELF_MANAGED);
    world.clone("widget");
    world.clone_on("gitlab.acme.test", GROUP, "gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let [w1, w2] = opened("widget", 7, NEW_BODY);
    let [g1, g2] = mr_opened_on("gitlab.acme.test", "gadget", 9, NEW_BODY);
    let in_github = "bump the api

<!-- BEGIN purlis change — GENERATED by `purlis change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | acme/widget#7 | — |
| gadget | https://gitlab.acme.test/acme/plat/gadget/-/merge_requests/9 | widget |
<!-- END purlis change -->";
    let in_gitlab = "bump the api

<!-- BEGIN purlis change — GENERATED by `purlis change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | https://github.com/acme/widget/pull/7 | — |
| gadget | acme/plat/gadget!9 | widget |
<!-- END purlis change -->";
    let (code, said, recorded) = world.push(json!([
        none_from_the_branch("widget"),
        w1,
        w2,
        no_mr_from_the_branch("gadget"),
        g1,
        g2,
        mr_body_is("gadget", 9, NEW_BODY),
        mr_body_set("gadget", 9, in_gitlab),
        body_is("widget", 7, NEW_BODY),
        body_set("widget", 7, in_github),
    ]));
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
}
