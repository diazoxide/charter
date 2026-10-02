//! `charter change land` against real clones and the recorded transport, which answers only the
//! requests a test wrote down and fails every other one. Nothing reaches a network and nothing
//! is merged anywhere: each clone's `origin` names a forge in the SSH form, and the "forge" is
//! the recording.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};

use super::*;
use crate::change::landing::{self, LOG_FIELDS};
use crate::change::pending::{self, Pending, Stage, Via};
use crate::change::{Exclusion, Member, Record, store};
use crate::forge::recorded::Recorded;

const SLUG: &str = "api-2";
const BRANCH: &str = "change/api-2";
/// The head every member's request is at, unless a test moves it.
const HEAD: &str = "6dcb09b5b57875f334f61aebed695e2e4193db5e";
/// Where a head moved to.
const MOVED: &str = "0123456789abcdef0123456789abcdef01234567";
/// The commit a merge made.
const MERGE: &str = "e5bd3914e2e596debea16f433f57875b5b90bcd6";

const PROBE: &str = include_str!("../../forge/github/queries/merge_queue.graphql");
const ENQUEUE: &str = include_str!("../../forge/github/queries/enqueue.graphql");

fn git(dir: &Path, args: &[&str]) -> String {
    let done = crate::testgit::run(dir, args);
    assert!(done.ok(), "git {args:?} failed: {done:?}");
    done.out.trim().to_string()
}

struct World {
    _dir: tempfile::TempDir,
    plane: PathBuf,
}

impl World {
    fn new() -> World {
        let dir = tempfile::tempdir().unwrap();
        let plane = dir.path().canonicalize().unwrap().join("plane");
        std::fs::create_dir_all(plane.join("workspaces/alpha")).unwrap();
        std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
        World { _dir: dir, plane }
    }

    /// A clone `alpha/<repo>` whose origin is `git@<host>:<namespace>/<repo>.git`, with
    /// `origin/HEAD` at `origin/main` and the change's branch one commit ahead of `main`.
    fn clone_on(&self, host: &str, namespace: &str, repo: &str) -> PathBuf {
        let clone = self.plane.join("workspaces/alpha").join(repo);
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main", "."]);
        git(&clone, &["config", "user.name", "Fixture"]);
        git(&clone, &["config", "user.email", "fixture@example.invalid"]);
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "one"]);
        let ssh = format!("git@{host}:{namespace}/{repo}.git");
        git(&clone, &["remote", "add", "origin", &ssh]);
        git(
            &clone,
            &["update-ref", "refs/remotes/origin/main", "refs/heads/main"],
        );
        git(
            &clone,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        git(&clone, &["switch", "-q", "-c", BRANCH]);
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "two"]);
        git(&clone, &["switch", "-q", "main"]);
        clone
    }

    fn clone(&self, repo: &str) -> PathBuf {
        self.clone_on("github.com", "acme", repo)
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
        record.excluded.push(Exclusion {
            repo: "docs".into(),
            why: "nothing to change there".into(),
            at: "2026-10-02T00:00:00+00:00".into(),
        });
        store::write(&self.plane, "alpha", &record).unwrap();
    }

    /// A merge commit on the clone's `origin/main`, as if it had been fetched after a merge.
    fn merged_upstream(&self, repo: &str) -> String {
        let clone = self.plane.join("workspaces/alpha").join(repo);
        git(&clone, &["switch", "-q", "main"]);
        git(
            &clone,
            &["commit", "-q", "--allow-empty", "-m", "the merge"],
        );
        let sha = git(&clone, &["rev-parse", "HEAD"]);
        git(&clone, &["update-ref", "refs/remotes/origin/main", &sha]);
        sha
    }

    /// The landing log, every line of every host.
    fn log(&self) -> Vec<Value> {
        let dir = landing::log_dir(&self.plane, "alpha");
        let Ok(files) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        files
            .filter_map(Result::ok)
            .filter(|f| f.path().extension().is_some_and(|e| e == "jsonl"))
            .flat_map(|f| {
                std::fs::read_to_string(f.path())
                    .unwrap()
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect::<Vec<Value>>()
            })
            .collect()
    }

    /// `charter change land` of `repos`, every forge question answered by `exchanges`, on
    /// GitHub's words unless `source` says otherwise.
    fn land(&self, repos: &[&str], how: How, exchanges: Value) -> (u8, String, Arc<Recorded>) {
        self.land_breaking(repos, how, exchanges, "")
    }

    /// [`World::land`], with the pending-landing directory made unwritable (a link out of the
    /// plane) the moment a request whose path ends in `break_at` is sent.
    fn land_breaking(
        &self,
        repos: &[&str],
        how: How,
        exchanges: Value,
        break_at: &str,
    ) -> (u8, String, Arc<Recorded>) {
        let text = json!({"source": "the land tests", "exchanges": exchanges});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let breaking = Arc::new(Breaking {
            recorded: recorded.clone(),
            at: break_at.to_string(),
            dir: crate::change::pending::pending_dir(&self.plane, "alpha"),
            outside: self._dir.path().join("outside"),
        });
        let mut said = String::new();
        let mut say = |line: Say| {
            said.push_str(&line.to_string());
            said.push('\n');
        };
        let repos: Vec<String> = repos.iter().map(|r| (*r).to_string()).collect();
        let when = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 2, 9, 0, 0).unwrap();
        let transport = breaking.clone();
        let code = land_with(
            &self.plane,
            "alpha",
            SLUG,
            &repos,
            how,
            &|repo: &Repo| repo.forge.backend_over(transport.clone()),
            "laptop",
            when,
            &mut say,
        );
        (code, said, recorded)
    }
}

/// The recorded transport, which breaks the pending-landing directory when a request to `at`
/// is sent.
struct Breaking {
    recorded: Arc<Recorded>,
    at: String,
    dir: PathBuf,
    outside: PathBuf,
}

impl crate::forge::transport::Transport for Breaking {
    fn send(
        &self,
        forge: &crate::forge::Forge,
        call: &crate::forge::transport::Call,
    ) -> Result<crate::forge::transport::Reply, crate::forge::transport::NoAnswer> {
        if !self.at.is_empty() && call.path().ends_with(&self.at) {
            std::fs::remove_dir_all(&self.dir).unwrap();
            std::fs::create_dir_all(&self.outside).unwrap();
            std::os::unix::fs::symlink(&self.outside, &self.dir).unwrap();
        }
        self.recorded.send(forge, call)
    }

    fn check_auth(&self, forge: &crate::forge::Forge) -> Result<(), crate::forge::ForgeError> {
        self.recorded.check_auth(forge)
    }
}

fn get(path: &str, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn refused(path: &str, method: Option<&str>, fields: Value, err: &str) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": method, "path": path}}, "fields": fields},
           "reply": {"code": 1, "out": "", "err": err}})
}

fn write(method: &str, path: &str, fields: Value, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": method, "path": path}}, "fields": fields},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn graphql(fields: Value, out: Value) -> Value {
    json!({"call": {"endpoint": "graphql", "fields": fields},
           "reply": {"code": 0, "out": out.to_string()}})
}

/// GitHub's pull request `n` from the change's branch of `repo`, as `by_head` reads it.
fn github_request(repo: &str, n: u64, state: &str, head: &str) -> Value {
    let merged = state == "merged";
    get(
        &format!("repos/acme/{repo}/pulls?state=all&head=acme:change%2Fapi-2&per_page=1"),
        json!([{"number": n, "html_url": format!("https://github.com/acme/{repo}/pull/{n}"),
                "state": if state == "open" { "open" } else { "closed" },
                "merged": merged,
                "merge_commit_sha": if merged { Value::from(MERGE) } else { Value::Null },
                "head": {"sha": head, "ref": BRANCH,
                         "repo": {"full_name": format!("acme/{repo}")}}}]),
    )
}

fn no_github_request(repo: &str) -> Value {
    get(
        &format!("repos/acme/{repo}/pulls?state=all&head=acme:change%2Fapi-2&per_page=1"),
        json!([]),
    )
}

/// The checks at `head` of `repo`: `runs` check runs and `statuses` commit statuses.
fn github_checks(repo: &str, head: &str, runs: Value, statuses: Value) -> [Value; 2] {
    let base = format!("repos/acme/{repo}/commits/{head}");
    let runs_n = runs.as_array().map_or(0, Vec::len);
    let statuses_n = statuses.as_array().map_or(0, Vec::len);
    [
        get(
            &format!("{base}/check-runs?per_page=100"),
            json!({"total_count": runs_n, "check_runs": runs}),
        ),
        get(
            &format!("{base}/status?per_page=100"),
            json!({"state": "success", "total_count": statuses_n, "statuses": statuses}),
        ),
    ]
}

fn github_passed(repo: &str, head: &str) -> [Value; 2] {
    github_checks(
        repo,
        head,
        json!([{"id": 4, "status": "completed", "conclusion": "success"}]),
        json!([]),
    )
}

fn github_probe(repo: &str, n: u64, queue: bool) -> Value {
    graphql(
        json!([{"text": ["query", PROBE]}, {"text": ["owner", "acme"]}, {"text": ["name", repo]},
               {"typed": ["number", n.to_string()]}]),
        json!({"data": {"repository": {"pullRequest": {"id": "PR_kw", "isMergeQueueEnabled": queue}}}}),
    )
}

fn github_merge_fields(repo: &str, n: u64, method: &str) -> Value {
    json!([{"text": ["merge_method", method]},
           {"text": ["commit_title", format!("api-2: {repo} (#{n})")]},
           {"text": ["commit_message", "bump the api\n\nCharter-Change: api-2"]},
           {"text": ["sha", HEAD]}])
}

fn github_merged(repo: &str, n: u64, method: &str) -> Value {
    write(
        "PUT",
        &format!("repos/acme/{repo}/pulls/{n}/merge"),
        github_merge_fields(repo, n, method),
        json!({"sha": MERGE, "merged": true, "message": "Pull Request successfully merged"}),
    )
}

/// One member, `widget`, open at [`HEAD`] with its checks passed and no merge queue.
fn ready(repo: &str, n: u64) -> Vec<Value> {
    let mut out = vec![github_request(repo, n, "open", HEAD)];
    out.extend(github_passed(repo, HEAD));
    out.push(github_probe(repo, n, false));
    out
}

fn a_landing(world: &World, repo: &str, merge: &str) {
    landing::append(
        &world.plane,
        "alpha",
        "laptop",
        &landing::Landing {
            ts: "2026-10-01T00:00:00+00:00".into(),
            change: SLUG.into(),
            repo: repo.into(),
            number: 3,
            merge: merge.into(),
            head: HEAD.into(),
        },
    )
    .unwrap();
}

#[test]
fn a_member_lands_at_the_head_its_checks_passed_on_and_the_landing_is_logged() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = ready("widget", 7);
    exchanges.push(github_merged("widget", 7, "merge"));
    exchanges.push(github_request("widget", 7, "merged", HEAD));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!(code, 0, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("checks PASSED at 6dcb09b5b578 (1 check)"),
        "{said}"
    );
    assert!(
        said.contains("merged #7 as e5bd3914e2e5, trailer Charter-Change: api-2"),
        "{said}"
    );
    let log = world.log();
    assert_eq!(log.len(), 1, "{log:?}");
    let mut keys: Vec<&str> = log[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, LOG_FIELDS);
    assert_eq!(
        (
            &log[0]["repo"],
            &log[0]["number"],
            &log[0]["head"],
            &log[0]["merge"]
        ),
        (&json!("widget"), &json!(7), &json!(HEAD), &json!(MERGE))
    );
}

#[test]
fn squash_is_asked_of_the_forge_and_rebase_is_refused_before_any_forge_is_asked() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = ready("widget", 7);
    exchanges.push(github_merged("widget", 7, "squash"));
    exchanges.push(github_request("widget", 7, "merged", HEAD));
    let (code, said, recorded) = world.land(&["widget"], How::Squash, json!(exchanges));
    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");

    let (code, said, _) = world.land(&["widget"], How::Rebase, json!([]));
    assert_eq!(code, REFUSED, "{said}");
    assert!(said.contains("charter does not land by rebase"), "{said}");
    assert!(
        !said.contains("nothing recorded"),
        "no forge was asked: {said}"
    );
}

#[test]
fn more_than_one_member_asked_for_is_refused_naming_each_and_no_forge_is_asked() {
    let world = World::new();
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &[])]);

    let (code, said, _) = world.land(&["widget", "gadget"], How::Merge, json!([]));

    assert_eq!(code, REFUSED, "{said}");
    assert!(
        said.contains("one member per landing, and 2 were named: widget, gadget"),
        "{said}"
    );
    assert!(
        !said.contains("nothing recorded"),
        "no forge was asked: {said}"
    );
    let (code, said, _) = world.land(&[], How::Merge, json!([]));
    assert_eq!(code, REFUSED, "{said}");
    assert!(
        said.contains("name the one member to land: --repo <name>"),
        "{said}"
    );
    assert!(world.log().is_empty());
}

#[test]
fn a_repo_that_is_not_a_member_excluded_or_with_no_request_is_refused_by_name() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);

    let (code, said, _) = world.land(&["gizmo"], How::Merge, json!([]));
    assert_eq!(code, REFUSED);
    assert!(said.contains("gizmo is not a member of api-2"), "{said}");

    let (code, said, _) = world.land(&["docs"], How::Merge, json!([]));
    assert_eq!(code, REFUSED);
    assert!(
        said.contains("docs was excluded from api-2: nothing to change there"),
        "{said}"
    );

    let (code, said, _) = world.land(
        &["widget"],
        How::Merge,
        json!([no_github_request("widget")]),
    );
    assert_eq!(code, REFUSED);
    assert!(
        said.contains(
            "widget: no request from branch change/api-2. Push it first: charter change push api-2"
        ),
        "{said}"
    );

    let closed = github_request("widget", 7, "closed", HEAD);
    let (code, said, _) = world.land(&["widget"], How::Merge, json!([closed]));
    assert_eq!(code, REFUSED);
    assert!(said.contains("#7 is REJECTED"), "{said}");
}

#[test]
fn a_blocker_that_has_not_landed_is_refused_by_name_and_nothing_is_merged() {
    let world = World::new();
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let exchanges = json!([
        github_request("gadget", 8, "open", HEAD),
        github_request("widget", 7, "open", HEAD),
    ]);

    let (code, said, recorded) = world.land(&["gadget"], How::Merge, exchanges);

    assert_eq!(code, REFUSED, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains("gadget: blocker widget has not landed (its request is open)"),
        "{said}"
    );
    assert!(world.log().is_empty());
}

#[test]
fn a_blocker_charter_landed_and_the_default_branch_still_holds_lets_its_dependent_land() {
    let world = World::new();
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let merge = world.merged_upstream("widget");
    a_landing(&world, "widget", &merge);
    let mut exchanges = vec![
        github_request("gadget", 8, "open", HEAD),
        github_request("widget", 7, "merged", HEAD),
    ];
    exchanges.extend(github_passed("gadget", HEAD));
    exchanges.push(github_probe("gadget", 8, false));
    exchanges.push(github_merged("gadget", 8, "merge"));
    exchanges.push(github_request("gadget", 8, "merged", HEAD));

    let (code, said, recorded) = world.land(&["gadget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");
    assert_eq!(world.log().len(), 2);
}

#[test]
fn a_blocker_whose_logged_merge_the_default_branch_no_longer_holds_has_not_landed() {
    let world = World::new();
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    // A commit no branch of the clone holds: reverted, or never fetched.
    a_landing(&world, "widget", MOVED);
    let exchanges = json!([
        github_request("gadget", 8, "open", HEAD),
        github_request("widget", 7, "merged", HEAD),
    ]);

    let (code, said, _) = world.land(&["gadget"], How::Merge, exchanges);

    assert_eq!(code, REFUSED, "{said}");
    assert!(
        said.contains("blocker widget has not landed (merged as 0123456789ab, which this clone's main does not contain"),
        "{said}"
    );
}

#[test]
fn checks_that_did_not_pass_are_refused_each_in_its_own_words_and_nothing_is_merged() {
    let failed = json!([{"id": 4, "status": "completed", "conclusion": "failure"}]);
    let running = json!([{"id": 4, "status": "in_progress", "conclusion": null}]);
    let scenes: [(&str, Vec<Value>); 4] = [
        (
            "checks FAILED at 6dcb09b5b578 (1 check).",
            github_checks("widget", HEAD, failed, json!([])).to_vec(),
        ),
        (
            "checks RUNNING at 6dcb09b5b578 (1 check) — not a verdict yet.",
            github_checks("widget", HEAD, running, json!([])).to_vec(),
        ),
        (
            "checks NOT RUN at 6dcb09b5b578 — this head has no check run and no commit status.",
            github_checks("widget", HEAD, json!([]), json!([])).to_vec(),
        ),
        (
            "checks UNKNOWN at 6dcb09b5b578 — charter could not read them",
            vec![refused(
                &format!("repos/acme/widget/commits/{HEAD}/check-runs?per_page=100"),
                None,
                json!([]),
                "HTTP 502",
            )],
        ),
    ];
    for (words, checks) in scenes {
        let world = World::new();
        world.clone("widget");
        world.change(&[("widget", &[])]);
        let mut exchanges = vec![github_request("widget", 7, "open", HEAD)];
        exchanges.extend(checks);

        let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

        assert_eq!(code, REFUSED, "{words}: {said}");
        assert_eq!(recorded.unspent(), Vec::new(), "{words}: {said}");
        assert!(
            said.contains(&format!("widget: {words}")),
            "{words}: {said}"
        );
        assert!(world.log().is_empty());
    }
}

#[test]
fn a_head_that_moved_since_the_check_is_refused_and_nothing_is_logged() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = ready("widget", 7);
    exchanges.push(refused(
        "repos/acme/widget/pulls/7/merge",
        Some("PUT"),
        github_merge_fields("widget", 7, "merge"),
        "Head branch was modified. Review and try the merge again. (HTTP 409)",
    ));
    exchanges.push(github_request("widget", 7, "open", MOVED));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!(code, REFUSED, "{said}");
    assert_eq!(recorded.unspent(), Vec::new(), "{said}");
    assert!(
        said.contains(
            "widget: the head moved since the check: checks PASSED at 6dcb09b5b578, and the \
             branch is now at 0123456789ab. Nothing was merged."
        ),
        "{said}"
    );
    assert!(world.log().is_empty());

    // The refused landing is no evidence: a later merge of that head by somebody else is not
    // recorded as charter's.
    let (code, said, _) = world.land(
        &["widget"],
        How::Merge,
        json!([github_request("widget", 7, "merged", HEAD)]),
    );
    assert_eq!(code, REFUSED, "{said}");
    assert!(
        said.contains("already merged, and not by charter"),
        "{said}"
    );
    assert!(world.log().is_empty());
}

#[test]
fn a_merge_the_forge_refuses_is_said_in_its_words_and_nothing_is_logged() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = ready("widget", 7);
    exchanges.push(refused(
        "repos/acme/widget/pulls/7/merge",
        Some("PUT"),
        github_merge_fields("widget", 7, "merge"),
        "Pull Request is not mergeable (HTTP 405)",
    ));
    exchanges.push(github_request("widget", 7, "open", HEAD));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (1, Vec::new()), "{said}");
    assert!(
        said.contains("Pull Request is not mergeable (HTTP 405)"),
        "{said}"
    );
    assert!(said.contains("Nothing was merged"), "{said}");
    assert!(world.log().is_empty());
}

#[test]
fn a_merge_the_read_back_does_not_confirm_is_not_logged() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = ready("widget", 7);
    exchanges.push(github_merged("widget", 7, "merge"));
    exchanges.push(github_request("widget", 7, "open", HEAD));

    let (code, said, _) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!(code, 1, "{said}");
    assert!(
        said.contains("did not confirm the merge on a read-back"),
        "{said}"
    );
    assert!(
        said.contains("Run charter change land api-2 --repo widget again to record it"),
        "{said}"
    );
    assert!(world.log().is_empty());

    // The next run finds it merged at the head charter asked for, and records it.
    let (code, said, recorded) = world.land(
        &["widget"],
        How::Merge,
        json!([github_request("widget", 7, "merged", HEAD)]),
    );
    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");
    assert!(
        said.contains("recorded the landing charter made: #7 merged as e5bd3914e2e5"),
        "{said}"
    );
    assert_eq!(world.log().len(), 1);
}

#[test]
fn a_merge_at_another_head_than_the_one_verified_is_never_logged() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = ready("widget", 7);
    exchanges.push(github_merged("widget", 7, "merge"));
    exchanges.push(github_request("widget", 7, "merged", MOVED));
    let (code, said, _) = world.land(&["widget"], How::Merge, json!(exchanges));
    assert_eq!(code, 1, "{said}");
    assert!(
        said.contains("did not confirm the merge on a read-back"),
        "{said}"
    );
    assert!(world.log().is_empty());
}

#[test]
fn a_member_whose_branch_has_a_merge_queue_is_queued_at_its_head_and_logged_once_it_merged() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = vec![github_request("widget", 7, "open", HEAD)];
    exchanges.extend(github_passed("widget", HEAD));
    exchanges.push(github_probe("widget", 7, true));
    exchanges.push(github_probe("widget", 7, true));
    exchanges.push(graphql(
        json!([{"text": ["query", ENQUEUE]}, {"text": ["id", "PR_kw"]}, {"text": ["head", HEAD]}]),
        json!({"data": {"enqueuePullRequest": {"mergeQueueEntry": {"state": "QUEUED", "headCommit": {"oid": HEAD}}}}}),
    ));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");
    assert!(
        said.contains("queued #7 in its merge queue at 6dcb09b5b578"),
        "{said}"
    );
    assert!(world.log().is_empty(), "nothing has merged yet");

    // Once the queue merged it, landing it again records it: charter's pending landing is
    // the evidence it was charter's.
    let exchanges = vec![github_request("widget", 7, "merged", HEAD)];
    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));
    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");
    assert!(
        said.contains("recorded the landing its merge queue made: #7 merged as e5bd3914e2e5"),
        "{said}"
    );
    assert_eq!(world.log().len(), 1);

    // And a third time there is nothing to do.
    let (code, said, _) = world.land(
        &["widget"],
        How::Merge,
        json!([github_request("widget", 7, "merged", HEAD)]),
    );
    assert_eq!(code, REFUSED, "{said}");
    assert!(
        said.contains("already landed, and the landing log has it as e5bd3914e2e5"),
        "{said}"
    );
}

#[test]
fn a_request_merged_with_no_pending_landing_of_charters_is_never_recorded() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);

    // Merged in the browser, or by a queue charter never put it in.
    let (code, said, recorded) = world.land(
        &["widget"],
        How::Merge,
        json!([github_request("widget", 7, "merged", HEAD)]),
    );
    assert_eq!((code, recorded.unspent()), (REFUSED, Vec::new()), "{said}");
    assert!(
        said.contains("#7 is already merged, and not by charter"),
        "{said}"
    );

    // A pending landing at another head is not this one.
    pending::append(
        &world.plane,
        "alpha",
        "laptop",
        &Pending::new(
            SLUG,
            "widget",
            7,
            MOVED,
            Via::Queue,
            Stage::Asked,
            chrono::Utc::now(),
        ),
    )
    .unwrap();
    let (code, said, _) = world.land(
        &["widget"],
        How::Merge,
        json!([github_request("widget", 7, "merged", HEAD)]),
    );
    assert_eq!(code, REFUSED, "{said}");
    assert!(
        said.contains("already merged, and not by charter"),
        "{said}"
    );
    assert!(world.log().is_empty());
}

#[test]
fn a_blocker_merged_outside_charter_is_refused_by_name() {
    let world = World::new();
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    let exchanges = json!([
        github_request("gadget", 8, "open", HEAD),
        github_request("widget", 7, "merged", HEAD),
    ]);

    let (code, said, recorded) = world.land(&["gadget"], How::Merge, exchanges);

    assert_eq!((code, recorded.unspent()), (REFUSED, Vec::new()), "{said}");
    assert!(
        said.contains("gadget: blocker widget has not landed (merged outside charter:"),
        "{said}"
    );
}

#[test]
fn a_blocker_charter_queued_and_found_merged_is_recorded_and_lets_its_dependent_land() {
    let world = World::new();
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &["widget"])]);
    pending::append(
        &world.plane,
        "alpha",
        "laptop",
        &Pending::new(
            SLUG,
            "widget",
            7,
            HEAD,
            Via::Queue,
            Stage::Asked,
            chrono::Utc::now(),
        ),
    )
    .unwrap();
    let mut exchanges = vec![
        github_request("gadget", 8, "open", HEAD),
        github_request("widget", 7, "merged", HEAD),
    ];
    exchanges.extend(github_passed("gadget", HEAD));
    exchanges.push(github_probe("gadget", 8, false));
    exchanges.push(github_merged("gadget", 8, "merge"));
    exchanges.push(github_request("gadget", 8, "merged", HEAD));

    let (code, said, recorded) = world.land(&["gadget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");
    let repos: Vec<Value> = world.log().iter().map(|l| l["repo"].clone()).collect();
    assert_eq!(repos, vec![json!("widget"), json!("gadget")], "{said}");
}

// ---- GitLab ----------------------------------------------------------------------------------

fn gitlab_request(n: u64, state: &str, head: &str) -> Value {
    get(
        "projects/acme%2Fwidget/merge_requests?source_branch=change%2Fapi-2&state=all&per_page=100",
        json!([{"iid": n, "web_url": format!("https://gitlab.com/acme/widget/-/merge_requests/{n}"),
                "state": state, "sha": head,
                "merge_commit_sha": if state == "merged" { Value::from(MERGE) } else { Value::Null },
                "source_project_id": 3, "target_project_id": 3}]),
    )
}

fn gitlab_ready(n: u64, trains: bool) -> Vec<Value> {
    vec![
        gitlab_request(n, "opened", HEAD),
        get(
            &format!("projects/acme%2Fwidget/merge_requests/{n}/pipelines?per_page=100"),
            json!([{"id": 77, "sha": HEAD, "ref": BRANCH, "status": "success"}]),
        ),
        get(
            "projects/acme%2Fwidget",
            json!({"id": 3, "merge_trains_enabled": trains}),
        ),
    ]
}

fn gitlab_merge_fields(n: u64) -> Value {
    json!([{"typed": ["squash", "false"]}, {"text": ["sha", HEAD]},
           {"text": ["merge_commit_message",
                     format!("api-2: widget (!{n})\n\nbump the api\n\nCharter-Change: api-2")]}])
}

#[test]
fn on_gitlab_the_merge_is_pinned_to_the_head_and_never_asks_to_merge_later() {
    let world = World::new();
    world.clone_on("gitlab.com", "acme", "widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = gitlab_ready(9, false);
    // Exactly these fields: no `merge_when_pipeline_succeeds`, no `auto_merge`. Any other
    // spelling of the call is one nobody recorded, and fails the run.
    exchanges.push(write(
        "PUT",
        "projects/acme%2Fwidget/merge_requests/9/merge",
        gitlab_merge_fields(9),
        json!({"iid": 9, "state": "merged", "merge_commit_sha": MERGE}),
    ));
    exchanges.push(gitlab_request(9, "merged", HEAD));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");
    assert!(said.contains("merged !9 as e5bd3914e2e5"), "{said}");
    assert_eq!(world.log().len(), 1);
}

#[test]
fn on_gitlab_a_merge_set_to_happen_later_is_cancelled_and_not_logged() {
    let world = World::new();
    world.clone_on("gitlab.com", "acme", "widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = gitlab_ready(9, false);
    exchanges.push(write(
        "PUT",
        "projects/acme%2Fwidget/merge_requests/9/merge",
        gitlab_merge_fields(9),
        json!({"iid": 9, "state": "opened", "merge_when_pipeline_succeeds": true}),
    ));
    exchanges.push(write(
        "POST",
        "projects/acme%2Fwidget/merge_requests/9/cancel_merge_when_pipeline_succeeds",
        json!([]),
        json!({"iid": 9, "state": "opened", "merge_when_pipeline_succeeds": false}),
    ));
    // The cancel is confirmed by reading the merge request again.
    exchanges.push(get(
        "projects/acme%2Fwidget/merge_requests/9",
        json!({"iid": 9, "state": "opened", "merge_when_pipeline_succeeds": false}),
    ));
    exchanges.push(gitlab_request(9, "opened", HEAD));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (1, Vec::new()), "{said}");
    assert!(
        said.contains(
            "GitLab set it to merge later instead of merging it, and charter cancelled that"
        ),
        "{said}"
    );
    assert!(world.log().is_empty());
    assert_eq!(
        pending::pendings(&world.plane, "alpha", SLUG)["widget"].stage,
        Stage::Refused
    );
}

#[test]
fn on_gitlab_a_merge_left_to_happen_later_is_said_and_kept_for_doctor() {
    let world = World::new();
    world.clone_on("gitlab.com", "acme", "widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = gitlab_ready(9, false);
    exchanges.push(write(
        "PUT",
        "projects/acme%2Fwidget/merge_requests/9/merge",
        gitlab_merge_fields(9),
        json!({"iid": 9, "state": "opened", "merge_when_pipeline_succeeds": true}),
    ));
    exchanges.push(refused(
        "projects/acme%2Fwidget/merge_requests/9/cancel_merge_when_pipeline_succeeds",
        Some("POST"),
        json!([]),
        "403 Forbidden",
    ));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (1, Vec::new()), "{said}");
    assert!(
        said.contains(
            "GitLab set !9 to merge later, and charter could not cancel that (403 Forbidden)"
        ),
        "{said}"
    );
    assert!(said.contains("Cancel its auto-merge on GitLab"), "{said}");
    assert!(world.log().is_empty());
    assert_eq!(
        pending::pendings(&world.plane, "alpha", SLUG)["widget"].stage,
        Stage::MergeLater
    );
}

#[test]
fn on_gitlab_a_repo_that_does_not_say_whether_it_has_a_merge_train_is_not_merged() {
    let world = World::new();
    world.clone_on("gitlab.com", "acme", "widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = gitlab_ready(9, false);
    exchanges.pop();
    exchanges.push(get("projects/acme%2Fwidget", json!({"id": 3})));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (1, Vec::new()), "{said}");
    assert!(said.contains("GitLab did not say"), "{said}");
    assert!(
        said.contains("will not merge directly where a train may exist"),
        "{said}"
    );
    assert!(said.contains("Nothing was merged"), "{said}");
    assert!(world.log().is_empty());
}

#[test]
fn on_gitlab_a_repo_with_a_merge_train_has_the_request_added_to_it_at_its_head() {
    let world = World::new();
    world.clone_on("gitlab.com", "acme", "widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = gitlab_ready(9, true);
    exchanges.push(write(
        "POST",
        "projects/acme%2Fwidget/merge_trains/merge_requests/9",
        json!([{"typed": ["squash", "false"]}, {"text": ["sha", HEAD]}]),
        json!([{"id": 1, "merge_request": {"iid": 9, "project_id": 3}, "status": "idle"}]),
    ));

    let (code, said, recorded) = world.land(&["widget"], How::Merge, json!(exchanges));

    assert_eq!((code, recorded.unspent()), (0, Vec::new()), "{said}");
    assert!(
        said.contains("queued !9 in its merge train at 6dcb09b5b578"),
        "{said}"
    );
    assert!(world.log().is_empty());
}

#[test]
fn a_refusal_charter_cannot_note_in_its_pending_landing_is_said_and_exits_one() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = ready("widget", 7);
    exchanges.push(refused(
        "repos/acme/widget/pulls/7/merge",
        Some("PUT"),
        github_merge_fields("widget", 7, "merge"),
        "Head branch was modified. Review and try the merge again. (HTTP 409)",
    ));
    exchanges.push(github_request("widget", 7, "open", MOVED));

    let (code, said, recorded) =
        world.land_breaking(&["widget"], How::Merge, json!(exchanges), "pulls/7/merge");

    assert_eq!((code, recorded.unspent()), (1, Vec::new()), "{said}");
    assert!(
        said.contains(
            "charter could not note that in its pending landing, which still says it asked"
        ),
        "{said}"
    );
    assert!(world.log().is_empty());
}

#[test]
fn a_merge_left_for_later_that_charter_cannot_note_is_said_and_exits_one() {
    let world = World::new();
    world.clone_on("gitlab.com", "acme", "widget");
    world.change(&[("widget", &[])]);
    let mut exchanges = gitlab_ready(9, false);
    exchanges.push(write(
        "PUT",
        "projects/acme%2Fwidget/merge_requests/9/merge",
        gitlab_merge_fields(9),
        json!({"iid": 9, "state": "opened", "merge_when_pipeline_succeeds": true}),
    ));
    exchanges.push(refused(
        "projects/acme%2Fwidget/merge_requests/9/cancel_merge_when_pipeline_succeeds",
        Some("POST"),
        json!([]),
        "403 Forbidden",
    ));

    let (code, said, recorded) = world.land_breaking(
        &["widget"],
        How::Merge,
        json!(exchanges),
        "cancel_merge_when_pipeline_succeeds",
    );

    assert_eq!((code, recorded.unspent()), (1, Vec::new()), "{said}");
    assert!(
        said.contains(
            "charter could not note that in its pending landing, which still says it asked"
        ),
        "{said}"
    );
}
