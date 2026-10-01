//! `charter change push` against real clones, each with a local bare remote, and a GitHub
//! backend over the recorded transport, which answers only the requests a test wrote down and
//! fails every other one. Nothing reaches a network: each clone's `origin` is in the SSH form,
//! and an `insteadOf` in its own config maps the HTTPS URL charter pushes to onto the bare
//! repository, as `reposave`'s tests do.

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
        }
    }

    /// A clone `alpha/<repo>` whose `main` is on its bare remote, with the change's branch
    /// one commit ahead of it and not pushed.
    fn clone(&self, repo: &str) -> PathBuf {
        let clone = self.plane.join("workspaces/alpha").join(repo);
        std::fs::create_dir_all(&clone).unwrap();
        run(&clone, &["init", "-q", "-b", "main", "."]);
        run(&clone, &["config", "user.name", "Fixture"]);
        run(&clone, &["config", "user.email", "fixture@example.invalid"]);
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        run(&clone, &["add", "-A"]);
        run(&clone, &["commit", "-q", "-m", "one"]);
        let bare = self.bare(repo);
        std::fs::create_dir_all(&bare).unwrap();
        run(&bare, &["init", "-q", "--bare", "-b", "main", "."]);
        let ssh = format!("git@github.com:acme/{repo}.git");
        run(&clone, &["remote", "add", "origin", &ssh]);
        let base = format!("file://{}/", bare.parent().unwrap().display());
        run(
            &clone,
            &[
                "config",
                &format!("url.{base}.insteadOf"),
                "https://github.com/acme/",
            ],
        );
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

    fn bare(&self, repo: &str) -> PathBuf {
        self.top.join(format!("forge/acme/{repo}.git"))
    }

    /// The commit the bare remote's `branch` is at, if it has one.
    fn remote_has(&self, repo: &str, branch: &str) -> Option<String> {
        let found = crate::testgit::run(
            &self.bare(repo),
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
        let text = json!({"source": "GitHub REST API version 2022-11-28, pulls",
                          "exchanges": exchanges});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let transport = recorded.clone();
        let mut said = String::new();
        let mut say = |line: Say| {
            said.push_str(&line.to_string());
            said.push('\n');
        };
        let code = push_with(
            &self.plane,
            "alpha",
            SLUG,
            &|repo: &Repo| repo.forge.backend_over(transport.clone()),
            &mut say,
        );
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
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD"
            ],
            vec![
                "push",
                "https://github.com/acme/widget.git",
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
fn a_member_with_no_clone_is_refused_by_name_and_the_others_still_pushed() {
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
    assert_eq!(code, REFUSED, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("gadget: no clone in workspace 'alpha'"),
        "{said}"
    );
    assert!(world.remote_has("widget", BRANCH).is_some());
}

#[test]
fn a_repo_whose_mode_is_off_is_still_pushed() {
    let world = World::new("schema = 1\n\n[repos.widget]\nmode = \"off\"\n");
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
    let block = block(&record, &BTreeMap::new());
    assert!(block.contains("— a \\| b"), "{block}");
    assert_eq!(block.lines().count(), 7, "{block}");
}
