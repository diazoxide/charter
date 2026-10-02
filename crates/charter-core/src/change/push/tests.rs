//! `charter change push` against real clones, each with a local bare remote, and a GitHub
//! backend over the recorded transport, which answers only the requests a test wrote down and
//! fails every other one. Nothing reaches a network: each clone's `origin` is in the SSH form,
//! and `push_with`'s route hands git the bare repository in place of the HTTPS URL printed. No
//! clone's own config rewrites a URL, since a clone whose config would is refused.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};

use super::*;
use crate::change::{Member, store};
use crate::forge::recorded::Recorded;

const SLUG: &str = "api-2";
const BRANCH: &str = "change/api-2";
const SHA: &str = "6dcb09b5b57875f334f61aebed695e2e4193db5e";

fn run(dir: &Path, args: &[&str]) -> String {
    let done = crate::testgit::run(dir, args);
    assert!(done.ok(), "git {args:?} failed: {done:?}");
    done.out
}

struct World {
    _dir: tempfile::TempDir,
    plane: PathBuf,
    top: PathBuf,
    /// The host of every forge request the last push sent, in order.
    asked_hosts: std::cell::RefCell<Vec<String>>,
}

/// The recorded transport, noting the host of every request sent through it.
struct NotingHosts {
    recorded: Arc<Recorded>,
    hosts: std::sync::Mutex<Vec<String>>,
}

impl crate::forge::transport::Transport for NotingHosts {
    fn send(
        &self,
        forge: &crate::forge::Forge,
        call: &crate::forge::transport::Call,
    ) -> Result<crate::forge::transport::Reply, crate::forge::transport::NoAnswer> {
        self.hosts.lock().unwrap().push(forge.host.clone());
        self.recorded.send(forge, call)
    }

    fn check_auth(&self, forge: &crate::forge::Forge) -> Result<(), crate::forge::ForgeError> {
        self.recorded.check_auth(forge)
    }
}

impl World {
    /// A plane saying `toml`, with a workspace `alpha` and no clones yet.
    fn new(toml: &str) -> World {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let plane = top.join("plane");
        std::fs::create_dir_all(plane.join("workspaces/alpha")).unwrap();
        std::fs::write(plane.join("charter.toml"), toml).unwrap();
        World {
            _dir: dir,
            plane,
            top,
            asked_hosts: std::cell::RefCell::default(),
        }
    }

    /// A clone `alpha/<repo>` of `github.com/acme/<repo>`: [`World::clone_on`].
    fn clone(&self, repo: &str) -> PathBuf {
        self.clone_on("github.com", "acme", repo)
    }

    /// A clone `alpha/<repo>` whose origin is `<repo>` under `namespace` on `host`, in the SSH
    /// form, and whose `main` is on its bare remote, with the change's branch one commit ahead
    /// of it and not pushed.
    fn clone_on(&self, host: &str, namespace: &str, repo: &str) -> PathBuf {
        let clone = self.plane.join("workspaces/alpha").join(repo);
        std::fs::create_dir_all(&clone).unwrap();
        run(&clone, &["init", "-q", "-b", "main", "."]);
        run(&clone, &["config", "user.name", "Fixture"]);
        run(&clone, &["config", "user.email", "fixture@example.invalid"]);
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        run(&clone, &["add", "-A"]);
        run(&clone, &["commit", "-q", "-m", "one"]);
        let bare = self.bare_in(namespace, repo);
        std::fs::create_dir_all(&bare).unwrap();
        run(&bare, &["init", "-q", "--bare", "-b", "main", "."]);
        let ssh = format!("git@{host}:{namespace}/{repo}.git");
        run(&clone, &["remote", "add", "origin", &ssh]);
        let at = bare.display().to_string();
        run(&clone, &["push", "-q", &at, "main"]);
        run(
            &clone,
            &["fetch", "-q", &at, "+refs/heads/*:refs/remotes/origin/*"],
        );
        run(
            &clone,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        run(&clone, &["switch", "-q", "-c", BRANCH]);
        std::fs::write(clone.join("api.rs"), "pub fn two() {}\n").unwrap();
        run(&clone, &["add", "-A"]);
        run(&clone, &["commit", "-q", "-m", "two"]);
        crate::repos::clone_at(&self.plane, "alpha", repo)
            .expect("the clone is the workspace's")
            .path
    }

    /// The bare remote a printed `https://<host>/<namespace>/<repo>.git` stands for, as a
    /// `file://` URL: every host's repos share one tree here, by namespace.
    fn route(&self, https: &str) -> String {
        let (_, path) = https
            .strip_prefix("https://")
            .and_then(|rest| rest.split_once('/'))
            .unwrap_or_else(|| panic!("not an HTTPS destination: {https}"));
        format!("file://{}/forge/{path}", self.top.display())
    }

    fn bare(&self, repo: &str) -> PathBuf {
        self.bare_in("acme", repo)
    }

    /// The bare remote of `<repo>` under `namespace`.
    fn bare_in(&self, namespace: &str, repo: &str) -> PathBuf {
        self.top.join(format!("forge/{namespace}/{repo}.git"))
    }

    /// The commit the bare remote's `branch` is at, if it has one.
    fn remote_has(&self, repo: &str, branch: &str) -> Option<String> {
        self.remote_in_has("acme", repo, branch)
    }

    /// [`World::remote_has`] of `<repo>` under `namespace`.
    fn remote_in_has(&self, namespace: &str, repo: &str, branch: &str) -> Option<String> {
        let found = crate::testgit::run(
            &self.bare_in(namespace, repo),
            &["rev-parse", &format!("refs/heads/{branch}")],
        );
        found.ok().then(|| found.out.trim().to_string())
    }

    /// The change, over `members`: `(repo, needs)`.
    fn change(&self, members: &[(&str, &[&str])]) {
        let mut record = Record::new(SLUG, "bump the api", "t", "2026-10-02T00:00:00+00:00");
        for (repo, needs) in members {
            record.members.push(Member {
                repo: (*repo).into(),
                branch: BRANCH.into(),
                needs: needs.iter().map(|n| (*n).to_string()).collect(),
            });
        }
        store::write(&self.plane, "alpha", &record).unwrap();
    }

    /// `charter change push`, every forge question answered by `exchanges`. The exit code,
    /// what was said, and the recording, to check nothing recorded went unasked.
    fn push(&self, exchanges: Value) -> (u8, String, Arc<Recorded>) {
        self.push_as(None, exchanges)
    }

    /// What the window names before its Push asks the operator (#474), and what was said.
    fn destinations(&self) -> (Result<Vec<Destination>, u8>, String) {
        let mut said = String::new();
        let found = destinations_with(
            &self.plane,
            "alpha",
            SLUG,
            &|https: &str| self.route(https),
            &mut |line: Say| {
                said.push_str(&line.to_string());
                said.push('\n');
            },
        );
        (found, said)
    }

    /// [`World::push`], or, given what the operator `confirmed`, the window's Push.
    fn push_as(
        &self,
        confirmed: Option<&[Destination]>,
        exchanges: Value,
    ) -> (u8, String, Arc<Recorded>) {
        let text = json!({"source": "GitHub REST API version 2022-11-28, pulls",
                          "exchanges": exchanges});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let transport = Arc::new(NotingHosts {
            recorded: recorded.clone(),
            hosts: std::sync::Mutex::default(),
        });
        let mut said = String::new();
        let mut say = |line: Say| {
            said.push_str(&line.to_string());
            said.push('\n');
        };
        let backend_of = |repo: &Repo| repo.forge.backend_over(transport.clone());
        let route = |https: &str| self.route(https);
        let code = match confirmed {
            None => push_with(&self.plane, "alpha", SLUG, &backend_of, &route, &mut say),
            Some(confirmed) => push_confirmed_with(
                &self.plane,
                "alpha",
                SLUG,
                confirmed,
                &backend_of,
                &route,
                &mut say,
            ),
        };
        *self.asked_hosts.borrow_mut() = transport.hosts.lock().unwrap().clone();
        (code, said, recorded)
    }
}

fn get(path: &str, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn write(method: &str, path: &str, fields: Value, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": method, "path": path}}, "fields": fields},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn none_from_the_branch(repo: &str) -> Value {
    get(
        &format!("repos/acme/{repo}/pulls?state=all&head=acme:change%2Fapi-2&per_page=1"),
        json!([]),
    )
}

fn open_from_the_branch(repo: &str, n: u64) -> Value {
    get(
        &format!("repos/acme/{repo}/pulls?state=all&head=acme:change%2Fapi-2&per_page=1"),
        json!([{"number": n, "html_url": format!("https://github.com/acme/{repo}/pull/{n}"),
                "state": "open",
                "head": {"sha": SHA, "ref": BRANCH, "repo": {"full_name": format!("acme/{repo}")}}}]),
    )
}

/// The open-or-update of a request that does not exist yet: the lookup, then the `POST`.
fn opened(repo: &str, n: u64, body: &str) -> [Value; 2] {
    [
        get(
            &format!(
                "repos/acme/{repo}/pulls?state=open&head=acme:change%2Fapi-2&base=main&per_page=1"
            ),
            json!([]),
        ),
        write(
            "POST",
            &format!("repos/acme/{repo}/pulls"),
            json!([{"text": ["head", BRANCH]}, {"text": ["base", "main"]},
                   {"text": ["title", format!("api-2: {repo}")]}, {"text": ["body", body]}]),
            json!({"number": n, "html_url": format!("https://github.com/acme/{repo}/pull/{n}"),
                   "state": "open"}),
        ),
    ]
}

fn body_is(repo: &str, n: u64, body: &str) -> Value {
    get(
        &format!("repos/acme/{repo}/pulls/{n}"),
        json!({"number": n, "body": body}),
    )
}

fn body_set(repo: &str, n: u64, body: &str) -> Value {
    write(
        "PATCH",
        &format!("repos/acme/{repo}/pulls/{n}"),
        json!([{"text": ["body", body]}]),
        json!({"number": n}),
    )
}

/// What charter writes when it opens a request: the `why`, and a block with no request yet.
const NEW_BODY: &str = "bump the api

<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | — | — |
| gadget | — | widget |
<!-- END charter change -->";

/// The same once both requests are open.
const BOTH: &str = "bump the api

<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | acme/widget#7 | — |
| gadget | acme/gadget#9 | widget |
<!-- END charter change -->";

fn first_push(world: &World) -> (u8, String, Arc<Recorded>) {
    let [w1, w2] = opened("widget", 7, NEW_BODY);
    let [g1, g2] = opened("gadget", 9, NEW_BODY);
    world.push(json!([
        none_from_the_branch("widget"),
        w1,
        w2,
        none_from_the_branch("gadget"),
        g1,
        g2,
        body_is("gadget", 9, NEW_BODY),
        body_set("gadget", 9, BOTH),
        body_is("widget", 7, NEW_BODY),
        body_set("widget", 7, BOTH),
    ]))
}

#[test]
fn every_members_branch_is_pushed_and_its_request_opened_carrying_the_cross_link_block() {
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let (code, said, recorded) = first_push(&world);
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    let head = run(&widget, &["rev-parse", BRANCH]).trim().to_string();
    assert_eq!(world.remote_has("widget", BRANCH), Some(head));
    assert!(world.remote_has("gadget", BRANCH).is_some());
    assert!(said.contains("widget#7") || said.contains("#7"), "{said}");
    assert!(said.contains("cross-link block written into 2"), "{said}");
}

#[test]
fn each_repo_branch_and_destination_is_said_before_anything_is_pushed() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let (_, said, _) = first_push(&world);
    let plan = said
        .find("widget  change/api-2 → https://github.com/acme/widget.git, its request into main")
        .unwrap_or_else(|| panic!("no plan line for widget:\n{said}"));
    assert!(
        said.contains("gadget  change/api-2 → https://github.com/acme/gadget.git"),
        "{said}"
    );
    let first_result = said.find("opened").expect("a request opened");
    assert!(plan < first_result, "the plan comes first:\n{said}");
}

#[test]
fn a_second_push_opens_no_second_request_and_writes_no_description_that_is_current() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let (code, said, _) = first_push(&world);
    assert_eq!(code, 0, "{said}");
    // No POST and no PATCH is recorded: either one fails the run as a request nobody wrote
    // down.
    let (code, said, recorded) = world.push(json!([
        open_from_the_branch("widget", 7),
        open_from_the_branch("gadget", 9),
        body_is("gadget", 9, BOTH),
        body_is("widget", 7, BOTH),
    ]));
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(said.contains("already current in 2"), "{said}");
}

#[test]
fn when_membership_changes_the_block_is_replaced_in_place_and_the_rest_of_the_description_kept() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[])]);
    let theirs = format!("Reviewed by Ann.\n\n{BOTH}\n\nShip after Friday.");
    let ours = "Reviewed by Ann.

bump the api

<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | acme/widget#7 | — |
<!-- END charter change -->

Ship after Friday.";
    let (code, said, recorded) = world.push(json!([
        open_from_the_branch("widget", 7),
        body_is("widget", 7, &theirs),
        body_set("widget", 7, ours),
    ]));
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
}

#[test]
fn a_description_without_charters_block_is_left_alone_and_named() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = world.push(json!([
        open_from_the_branch("widget", 7),
        body_is("widget", 7, "A person's own words."),
    ]));
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(said.contains("left it as it is"), "{said}");
}

#[test]
fn every_git_call_is_recorded_and_the_push_is_one_refspec_with_no_force() {
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let before = crate::worktree::git::tally::asked(&widget).len();
    let (code, said, _) = first_push(&world);
    assert_eq!(code, 0, "{said}");
    let asked = crate::worktree::git::tally::asked(&widget)[before..].to_vec();
    let refspec = format!("refs/heads/{BRANCH}:refs/heads/{BRANCH}");
    let to = world.route("https://github.com/acme/widget.git");
    assert_eq!(
        asked,
        [
            vec!["remote", "get-url", "origin"],
            vec!["remote", "get-url", "origin"],
            vec![
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/heads/change/api-2"
            ],
            vec![
                "config",
                "--get-regexp",
                r"^(url\..*\.pushinsteadof|remote\..*\.pushurl)$"
            ],
            vec![
                "-c",
                "push.followTags=false",
                "-c",
                "push.pushOption=",
                "-c",
                "push.recurseSubmodules=no",
                "ls-remote",
                "--get-url",
                to.as_str()
            ],
            vec![
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD"
            ],
            vec![
                "-c",
                "push.followTags=false",
                "-c",
                "push.pushOption=",
                "-c",
                "push.recurseSubmodules=no",
                "push",
                to.as_str(),
                refspec.as_str()
            ],
            vec![
                "update-ref",
                "refs/remotes/origin/change/api-2",
                "refs/heads/change/api-2"
            ],
        ]
        .map(|argv| argv.iter().map(|a| a.to_string()).collect::<Vec<_>>()),
        "{said}"
    );
    for argv in &asked {
        for arg in argv {
            assert!(
                !arg.starts_with('+') && !arg.starts_with("--force") && arg != "-f",
                "a force in {argv:?}"
            );
            assert!(
                !["--mirror", "--delete", "-d", "--all", "--tags"].contains(&arg.as_str()),
                "{arg} in {argv:?}"
            );
        }
    }
}

#[test]
fn a_member_that_is_not_a_repo_here_is_refused_by_name_and_the_others_still_pushed() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let one = "bump the api

<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | acme/widget#7 | — |
| gadget | — | widget |
<!-- END charter change -->";
    let [w1, w2] = opened("widget", 7, NEW_BODY);
    let (code, said, recorded) = world.push(json!([
        none_from_the_branch("widget"),
        w1,
        w2,
        body_is("widget", 7, NEW_BODY),
        body_set("widget", 7, one),
    ]));
    // 1, as the Python charter answered: the run did not do everything it was asked.
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("gadget: not a repo in workspace 'alpha'"),
        "{said}"
    );
    assert!(said.contains("charter clone gadget -w alpha"), "{said}");
    assert!(world.remote_has("widget", BRANCH).is_some());
}

#[test]
fn a_repo_whose_mode_is_off_is_still_pushed() {
    // `branch` is where a save's pull request goes. This verb reads no save setting, so the
    // request still goes into the repo's default branch, `main`.
    let world = World::new("schema = 1\n\n[repos.widget]\nmode = \"off\"\nbranch = \"release\"\n");
    let widget = world.clone("widget");
    world.change(&[("widget", &[])]);
    let lone_new = "bump the api

<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | — | — |
<!-- END charter change -->";
    let lone = lone_new.replace("| widget | — |", "| widget | acme/widget#7 |");
    let [w1, w2] = opened("widget", 7, lone_new);
    let (code, said, recorded) = world.push(json!([
        none_from_the_branch("widget"),
        w1,
        w2,
        body_is("widget", 7, lone_new),
        body_set("widget", 7, &lone),
    ]));
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    let head = run(&widget, &["rev-parse", BRANCH]).trim().to_string();
    assert_eq!(world.remote_has("widget", BRANCH), Some(head));
}

#[test]
fn a_change_with_no_members_is_refused() {
    let world = World::new("schema = 1\n");
    world.change(&[]);
    let (code, said, _) = world.push(json!([]));
    assert_eq!(code, REFUSED, "{said}");
    assert!(said.contains("has no members"), "{said}");
}

#[test]
fn the_splice_refuses_a_missing_doubled_reversed_or_fenced_block() {
    let block = format!("{BLOCK_BEGIN}\nnew\n{BLOCK_END}");
    let one = format!("a\n{BLOCK_BEGIN}\nold\n{BLOCK_END}\nb");
    assert_eq!(splice(&one, &block), Some(format!("a\n{block}\nb")));
    assert_eq!(splice("no markers", &block), None);
    assert_eq!(splice(&format!("{one}\n{one}"), &block), None);
    assert_eq!(splice(&format!("{BLOCK_END}\n{BLOCK_BEGIN}"), &block), None);
    assert_eq!(splice(&format!("```\n{one}\n```"), &block), None);
    assert_eq!(splice(&format!("~~~\n{one}\n~~~"), &block), None);
}

#[test]
fn a_why_cannot_add_a_row_or_a_column_to_the_block() {
    let mut record = Record::new(SLUG, "a | b", "t", "2026-10-02T00:00:00+00:00");
    record.members.push(Member {
        repo: "widget".into(),
        branch: BRANCH.into(),
        needs: vec![],
    });
    let block = block(&record, &|_| None, "github.com");
    assert!(block.contains("— a \\| b"), "{block}");
    assert_eq!(block.lines().count(), 7, "{block}");
}

/// The lone-member scene: `widget` opened as #7 and its block written.
fn lone_member(world: &World) -> (u8, String, Arc<Recorded>) {
    let lone_new = "bump the api

<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | — | — |
<!-- END charter change -->";
    let lone = lone_new.replace("| widget | — |", "| widget | acme/widget#7 |");
    let [w1, w2] = opened("widget", 7, lone_new);
    world.push(json!([
        none_from_the_branch("widget"),
        w1,
        w2,
        body_is("widget", 7, lone_new),
        body_set("widget", 7, &lone),
    ]))
}

#[test]
fn an_annotated_tag_is_never_sent_even_when_the_repo_follows_tags() {
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    run(&widget, &["config", "push.followTags", "true"]);
    run(&widget, &["tag", "-a", "v9", "-m", "nine"]);
    world.change(&[("widget", &[])]);
    let (code, said, _) = lone_member(&world);
    assert_eq!(code, 0, "{said}");
    assert!(world.remote_has("widget", BRANCH).is_some());
    let tags = run(&world.bare("widget"), &["tag", "-l"]);
    assert_eq!(tags.trim(), "", "a tag went with the branch");
}

#[test]
fn a_push_option_in_the_repos_config_is_never_sent() {
    // The bare remote advertises no push options, so a push that sent one would be refused.
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    run(&widget, &["config", "push.pushOption", "ci.skip"]);
    world.change(&[("widget", &[])]);
    let (code, said, _) = lone_member(&world);
    assert_eq!(code, 0, "{said}");
    assert!(world.remote_has("widget", BRANCH).is_some(), "{said}");
}

#[test]
fn a_repo_whose_config_sends_pushes_elsewhere_is_refused_and_nothing_is_pushed() {
    for key in [
        "url.file:///elsewhere/.pushInsteadOf",
        "remote.origin.pushurl",
    ] {
        let world = World::new("schema = 1\n");
        let widget = world.clone("widget");
        let value = if key.starts_with("url.") {
            "https://github.com/acme/"
        } else {
            "file:///elsewhere/widget.git"
        };
        run(&widget, &["config", key, value]);
        world.change(&[("widget", &[])]);
        let (code, said, recorded) = world.push(json!([]));
        assert_eq!(code, 1, "{key}: {said}");
        assert_eq!(recorded.unspent(), Vec::new());
        assert!(said.contains(&key.to_lowercase()), "{key}: {said}");
        assert_eq!(world.remote_has("widget", BRANCH), None, "{key}: {said}");
    }
}

#[test]
fn a_repo_whose_config_rewrites_the_url_it_pushes_to_is_refused_and_nothing_is_pushed() {
    // An `insteadOf` rewrites a push's URL as well as a fetch's, so git would push somewhere
    // other than the destination printed.
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    let routed = world.route("https://github.com/acme/widget.git");
    run(
        &widget,
        &["config", "url.file:///elsewhere/.insteadOf", &routed],
    );
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = world.push(json!([]));
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("(url.file:///elsewhere/.insteadof), so it is not pushed"),
        "{said}"
    );
    assert_eq!(world.remote_has("widget", BRANCH), None, "{said}");
}

#[test]
fn charters_own_ssh_to_https_rewrite_does_not_stop_a_push() {
    // `charter git-policy --apply` maps the forge's SSH forms onto its HTTPS URL. That rewrites
    // no HTTPS URL, so the destination printed is still the one git pushes to.
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    for ssh in ["git@github.com:", "ssh://git@github.com/"] {
        run(
            &widget,
            &["config", "--add", "url.https://github.com/.insteadOf", ssh],
        );
    }
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = lone_member(&world);
    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(world.remote_has("widget", BRANCH).is_some(), "{said}");
}

#[test]
fn a_push_that_is_not_a_fast_forward_is_refused_and_the_remote_is_unchanged() {
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    // Somebody else's commit on the remote's branch, which the local branch does not have.
    let bare = world.bare("widget").display().to_string();
    run(
        &widget,
        &["push", "-q", &bare, &format!("{BRANCH}:{BRANCH}")],
    );
    let theirs = world.remote_has("widget", BRANCH).unwrap();
    run(&widget, &["reset", "-q", "--hard", "HEAD~1"]);
    std::fs::write(widget.join("api.rs"), "pub fn three() {}\n").unwrap();
    run(&widget, &["add", "-A"]);
    run(&widget, &["commit", "-q", "-m", "three"]);
    world.change(&[("widget", &[])]);
    let (code, said, recorded) = world.push(json!([]));
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new());
    assert!(said.contains("widget: not pushed"), "{said}");
    assert_eq!(world.remote_has("widget", BRANCH), Some(theirs));
}

#[test]
fn no_key_in_a_record_can_steer_where_a_request_goes() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    let path = world.plane.join("workspaces/alpha/changes/api-2.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"change": "api-2", "why": "bump the api", "created": "2026-10-02T00:00:00+00:00", "by": "t", "members": [{"repo": "widget", "branch": "change/api-2", "needs": [], "base": "release"}], "excluded": []}"#,
    )
    .unwrap();
    let (code, said, recorded) = world.push(json!([]));
    assert_eq!(code, 1, "{said}");
    assert_eq!(recorded.unspent(), Vec::new());
    assert!(said.contains("base"), "{said}");
    assert_eq!(world.remote_has("widget", BRANCH), None);
}

#[test]
fn a_tracking_ref_that_cannot_be_moved_is_said_not_dropped() {
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    // A file where the tracking ref's directory would go.
    std::fs::write(
        widget.join(".git/refs/remotes/origin/change"),
        "not a ref\n",
    )
    .unwrap();
    world.change(&[("widget", &[])]);
    let (code, said, _) = lone_member(&world);
    assert_eq!(code, 0, "{said}");
    assert!(
        said.contains("refs/remotes/origin/change/api-2 was not moved"),
        "{said}"
    );
}

#[test]
fn a_why_carrying_charters_marker_cannot_lock_the_block() {
    let mut record = Record::new(
        SLUG,
        "see <!-- END charter change --> here",
        "t",
        "2026-10-02T00:00:00+00:00",
    );
    record.members.push(Member {
        repo: "widget".into(),
        branch: BRANCH.into(),
        needs: vec![],
    });
    let body = new_body(&record, "github.com");
    let spliced = splice(&body, &block(&record, &|_| None, "github.com"));
    assert_eq!(spliced.as_deref(), Some(body.as_str()), "{body}");
    assert!(!body.contains("see <!--"), "{body}");
}

#[test]
fn a_fence_closes_only_on_its_own_kind() {
    let block = format!("{BLOCK_BEGIN}\nnew\n{BLOCK_END}");
    let marked = format!("{BLOCK_BEGIN}\nold\n{BLOCK_END}");
    // A `~~~` inside a ``` fence is the fence's text, so the block after the ``` close is not
    // fenced, and the one before it is.
    let after = format!("```\n~~~\n```\n{marked}");
    assert_eq!(
        splice(&after, &block),
        Some(format!("```\n~~~\n```\n{block}"))
    );
    let inside = format!("```\n~~~\n{marked}\n~~~\n```");
    assert_eq!(splice(&inside, &block), None);
}

#[test]
fn the_descriptions_line_endings_and_trailing_newline_are_kept_outside_the_block() {
    let block = format!("{BLOCK_BEGIN}\nnew\n{BLOCK_END}");
    let crlf = format!("a\r\n{BLOCK_BEGIN}\r\nold\r\n{BLOCK_END}\r\nb\r\n");
    assert_eq!(
        splice(&crlf, &block),
        Some(format!("a\r\n{BLOCK_BEGIN}\r\nnew\r\n{BLOCK_END}\r\nb\r\n"))
    );
    let trailing = format!("a\n{BLOCK_BEGIN}\nold\n{BLOCK_END}\n");
    assert_eq!(splice(&trailing, &block), Some(format!("a\n{block}\n")));
    let current = format!("a\r\n{BLOCK_BEGIN}\r\nnew\r\n{BLOCK_END}\r\n");
    assert_eq!(splice(&current, &block).as_deref(), Some(current.as_str()));
}

mod gitlab;

fn destination(repo: &str) -> Destination {
    Destination {
        repo: repo.into(),
        branch: BRANCH.into(),
        to: format!("https://github.com/acme/{repo}.git"),
        base: Some("main".into()),
        kind: crate::forge::Kind::GitHub,
    }
}

#[test]
fn each_members_destination_is_named_for_the_window_and_nothing_is_pushed_or_asked() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);

    let (found, said) = world.destinations();

    assert_eq!(
        found,
        Ok(vec![destination("widget"), destination("gadget")]),
        "{said}"
    );
    assert_eq!(world.remote_has("widget", BRANCH), None, "{said}");
    assert_eq!(world.remote_has("gadget", BRANCH), None, "{said}");
}

#[test]
fn a_member_that_will_not_be_pushed_is_said_in_the_cores_words_beside_the_destinations() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);

    let (found, said) = world.destinations();

    assert_eq!(found, Ok(vec![destination("widget")]), "{said}");
    assert!(
        said.contains("gadget: not a repo in workspace 'alpha', so it is not pushed."),
        "{said}"
    );
}

#[test]
fn a_push_of_the_destinations_the_operator_confirmed_pushes_them() {
    let world = World::new("schema = 1\n");
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let [w1, w2] = opened("widget", 7, NEW_BODY);
    let [g1, g2] = opened("gadget", 9, NEW_BODY);
    let confirmed = [destination("widget"), destination("gadget")];

    let (code, said, recorded) = world.push_as(
        Some(&confirmed),
        json!([
            none_from_the_branch("widget"),
            w1,
            w2,
            none_from_the_branch("gadget"),
            g1,
            g2,
            body_is("gadget", 9, NEW_BODY),
            body_set("gadget", 9, BOTH),
            body_is("widget", 7, NEW_BODY),
            body_set("widget", 7, BOTH),
        ]),
    );

    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");
    assert!(world.remote_has("widget", BRANCH).is_some());
}

#[test]
fn a_push_whose_destinations_changed_since_the_operator_confirmed_them_pushes_nothing() {
    let world = World::new("schema = 1\n");
    let widget = world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let confirmed = [destination("widget"), destination("gadget")];
    // After the question was asked, widget's origin is pointed somewhere else.
    run(
        &widget,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:elsewhere/widget.git",
        ],
    );

    let (code, said, recorded) = world.push_as(Some(&confirmed), json!([]));

    assert_eq!((code, recorded.unspent()), (REFUSED, Vec::new()), "{said}");
    assert!(
        said.contains(
            "what charter would push is not what you confirmed: widget now goes to \
             https://github.com/elsewhere/widget.git. Nothing was pushed."
        ),
        "{said}"
    );
    assert_eq!(world.remote_has("gadget", BRANCH), None, "{said}");
}
