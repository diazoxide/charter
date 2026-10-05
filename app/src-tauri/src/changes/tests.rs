//! The changes view's Push and Land, through the commands the window calls, against real clones
//! with local bare remotes and a GitHub stand-in: the recorded transport, which answers only the
//! requests a test wrote down and fails every other one. Nothing reaches a network and nothing is
//! merged anywhere.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use charter_core::change::{Member, Record, store};
use charter_core::forge::recorded::Recorded;
use serde_json::{Value, json};

use super::*;

const SLUG: &str = "api-2";
const BRANCH: &str = "change/api-2";
const HEAD: &str = "6dcb09b5b57875f334f61aebed695e2e4193db5e";
const MERGE: &str = "e5bd3914e2e596debea16f433f57875b5b90bcd6";
const PROBE: &str =
    include_str!("../../../../crates/charter-core/src/forge/github/queries/merge_queue.graphql");

fn git(dir: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    let out = charter_core::forklock::output(&mut command).unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

struct World {
    _dir: tempfile::TempDir,
    top: PathBuf,
    plane: PathBuf,
}

impl World {
    /// A plane with workspace `alpha`, clones of `github.com/acme/<repo>` for each of `repos`,
    /// each with a bare remote and the change's branch one commit ahead and not pushed, and the
    /// change `api-2` over `members` (`(repo, needs)`).
    fn new(members: &[(&str, &[&str])]) -> World {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let plane = top.join("plane");
        std::fs::create_dir_all(plane.join("workspaces/alpha")).unwrap();
        std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
        let world = World {
            _dir: dir,
            top,
            plane,
        };
        let mut record = Record::new(SLUG, "bump the api", "t", "2026-10-02T00:00:00+00:00");
        for (repo, needs) in members {
            world.clone(repo);
            record.members.push(Member {
                repo: (*repo).into(),
                branch: BRANCH.into(),
                needs: needs.iter().map(|n| (*n).to_owned()).collect(),
            });
        }
        store::write(&world.plane, "alpha", &record).unwrap();
        world
    }

    fn clone(&self, repo: &str) {
        let clone = self.plane.join("workspaces/alpha").join(repo);
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main", "."]);
        git(&clone, &["config", "user.name", "Fixture"]);
        git(&clone, &["config", "user.email", "fixture@example.invalid"]);
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "one"]);
        let bare = self.bare(repo);
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare", "-b", "main", "."]);
        git(
            &clone,
            &[
                "remote",
                "add",
                "origin",
                &format!("git@github.com:acme/{repo}.git"),
            ],
        );
        let at = bare.display().to_string();
        git(&clone, &["push", "-q", &at, "main"]);
        git(
            &clone,
            &["fetch", "-q", &at, "+refs/heads/*:refs/remotes/origin/*"],
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
    }

    fn bare(&self, repo: &str) -> PathBuf {
        self.top.join(format!("forge/acme/{repo}.git"))
    }

    fn remote_has_branch(&self, repo: &str) -> bool {
        let mut command = Command::new("git");
        command.arg("-C").arg(self.bare(repo)).args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{BRANCH}"),
        ]);
        charter_core::forklock::output(&mut command)
            .unwrap()
            .status
            .success()
    }

    /// `then`, with the forge answering only `exchanges` and every push routed to its bare
    /// remote; and the recording, to see that every exchange was asked.
    fn with<T>(&self, exchanges: Value, then: impl FnOnce(&Reach) -> T) -> (T, Arc<Recorded>) {
        let text = json!({"source": "the changes view tests", "exchanges": exchanges});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let transport = recorded.clone();
        let top = self.top.clone();
        let answer = then(&Reach {
            backend_of: &|repo: &Repo| repo.forge.backend_over(transport.clone()),
            route: &|https: &str| {
                let path = https.strip_prefix("https://github.com/").unwrap();
                format!("file://{}/forge/{path}", top.display())
            },
            host: "laptop",
        });
        (answer, recorded)
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

fn request(repo: &str, n: u64, state: &str) -> Value {
    let merged = state == "merged";
    get(
        &format!("repos/acme/{repo}/pulls?state=all&head=acme:change%2Fapi-2&per_page=1"),
        json!([{"number": n, "html_url": format!("https://github.com/acme/{repo}/pull/{n}"),
                "state": if state == "open" { "open" } else { "closed" },
                "merged": merged,
                "merge_commit_sha": if merged { Value::from(MERGE) } else { Value::Null },
                "head": {"sha": HEAD, "ref": BRANCH,
                         "repo": {"full_name": format!("acme/{repo}")}}}]),
    )
}

/// The forge's side of a first push of `widget` alone: no request yet, one opened, the block
/// written.
fn first_push() -> Value {
    let new_body = "bump the api

<!-- BEGIN purlis change — GENERATED by `purlis change push`; do not edit by hand. -->
**Cross-repo change: `api-2`** — bump the api

| repo | request | needs |
|---|---|---|
| widget | — | — |
<!-- END purlis change -->";
    let with_it = new_body.replace("| widget | — | — |", "| widget | acme/widget#7 | — |");
    json!([
        get(
            "repos/acme/widget/pulls?state=all&head=acme:change%2Fapi-2&per_page=1",
            json!([])
        ),
        get(
            "repos/acme/widget/pulls?state=open&head=acme:change%2Fapi-2&base=main&per_page=1",
            json!([])
        ),
        write(
            "POST",
            "repos/acme/widget/pulls",
            json!([{"text": ["head", BRANCH]}, {"text": ["base", "main"]},
                   {"text": ["title", "api-2: widget"]}, {"text": ["body", new_body]}]),
            json!({"number": 7, "html_url": "https://github.com/acme/widget/pull/7",
                   "state": "open"}),
        ),
        get(
            "repos/acme/widget/pulls/7",
            json!({"number": 7, "body": new_body})
        ),
        write(
            "PATCH",
            "repos/acme/widget/pulls/7",
            json!([{"text": ["body", with_it]}]),
            json!({"number": 7})
        ),
    ])
}

/// The gates' reads for `widget`'s #7: open at [`HEAD`], one check passed, no merge queue.
fn ready() -> Vec<Value> {
    let base = format!("repos/acme/widget/commits/{HEAD}");
    vec![
        request("widget", 7, "open"),
        get(
            &format!("{base}/check-runs?per_page=100"),
            json!({"total_count": 1, "check_runs":
                   [{"id": 4, "status": "completed", "conclusion": "success"}]}),
        ),
        get(
            &format!("{base}/status?per_page=100"),
            json!({"state": "success", "total_count": 0, "statuses": []}),
        ),
        json!({"call": {"endpoint": "graphql", "fields":
                  [{"text": ["query", PROBE]}, {"text": ["owner", "acme"]},
                   {"text": ["name", "widget"]}, {"typed": ["number", "7"]}]},
               "reply": {"code": 0, "out": json!({"data": {"repository": {"pullRequest":
                   {"id": "PR_kw", "isMergeQueueEnabled": false}}}}).to_string()}}),
    ]
}

/// What the push question names for widget: its branch at the commit its clone holds.
fn widget_destination(world: &World) -> PushDestination {
    let head = git(
        &world.plane.join("workspaces/alpha/widget"),
        &["rev-parse", BRANCH],
    );
    PushDestination {
        repo: "widget".into(),
        branch: BRANCH.into(),
        head_short: head[..12].to_owned(),
        head,
        to: "https://github.com/acme/widget.git".into(),
        base: Some("main".into()),
        forge: ForgeKind::Github,
        request: "pull request".into(),
    }
}

#[test]
fn push_then_land_asks_first_names_what_it_will_do_and_does_only_that() {
    let world = World::new(&[("widget", &[])]);

    // Push asks first: each repo, branch and destination, and nothing pushed or asked.
    let (question, recorded) = world.with(json!([]), |reach| {
        push_question_in(&world.plane, "alpha", SLUG, reach)
    });
    assert_eq!(recorded.unspent(), Vec::new());
    let question = question.expect("the push question");
    assert_eq!(question.destinations, vec![widget_destination(&world)]);
    assert_eq!(question.not_pushed, Vec::<String>::new());
    assert!(!world.remote_has_branch("widget"), "asking pushed it");

    // The yes pushes exactly that.
    let (pushed, recorded) = world.with(first_push(), |reach| {
        push_in(
            &world.plane,
            "alpha",
            SLUG,
            question.destinations.clone(),
            reach,
        )
    });
    let pushed = pushed.expect("the push");
    assert_eq!(recorded.unspent(), Vec::new(), "{pushed:?}");
    assert!(world.remote_has_branch("widget"));

    // Land asks first: the request, the head its checks passed at, and merged now. No merge
    // call is recorded, so asking one would fail the run.
    let (question, recorded) = world.with(json!(ready()), |reach| {
        land_question_in(&world.plane, "alpha", SLUG, "widget", reach)
    });
    assert_eq!(recorded.unspent(), Vec::new());
    let question = question.expect("the land question");
    assert_eq!(
        (
            question.number,
            question.head.as_str(),
            question.through,
            question.request.as_str(),
            question.sigil.as_str(),
            question.how.as_str(),
            question.squash,
        ),
        (
            7,
            HEAD,
            LandThrough::Merge,
            "pull request",
            "#",
            "charter merges it now, at 6dcb09b5b578 and no other.",
            true
        )
    );
    assert!(
        question
            .said
            .iter()
            .any(|l| l.contains("checks PASSED at 6dcb09b5b578")),
        "{:?}",
        question.said
    );

    // The yes merges at that head and no other, and the landing is logged.
    let mut exchanges = ready();
    exchanges.push(write(
        "PUT",
        "repos/acme/widget/pulls/7/merge",
        json!([{"text": ["merge_method", "merge"]},
               {"text": ["commit_title", "api-2: widget (#7)"]},
               {"text": ["commit_message", "bump the api\n\nPurlis-Change: api-2"]},
               {"text": ["sha", HEAD]}]),
        json!({"sha": MERGE, "merged": true}),
    ));
    exchanges.push(request("widget", 7, "merged"));
    let (landed, recorded) = world.with(json!(exchanges), |reach| {
        land_in(&world.plane, "alpha", SLUG, &question, false, reach)
    });
    let landed = landed.expect("the landing");
    assert_eq!(recorded.unspent(), Vec::new(), "{landed:?}");
    assert!(
        landed
            .iter()
            .any(|l| l.contains("merged #7 as e5bd3914e2e5")),
        "{landed:?}"
    );
}

#[test]
fn land_on_a_blocked_member_shows_the_named_refusal_and_merges_nothing() {
    let world = World::new(&[("widget", &[]), ("gadget", &["widget"])]);
    let exchanges = json!([request("gadget", 8, "open"), request("widget", 7, "open")]);

    let (question, recorded) = world.with(exchanges, |reach| {
        land_question_in(&world.plane, "alpha", SLUG, "gadget", reach)
    });

    assert_eq!(recorded.unspent(), Vec::new());
    assert_eq!(
        question,
        Err("gadget: blocker widget has not landed (its request is open).".into())
    );
}

#[test]
fn a_land_question_edited_on_its_way_back_is_refused_and_nothing_is_merged() {
    let world = World::new(&[("widget", &[])]);
    let (question, _) = world.with(json!(ready()), |reach| {
        land_question_in(&world.plane, "alpha", SLUG, "widget", reach)
    });
    let mut question = question.unwrap();
    question.head = "0123456789abcdef0123456789abcdef01234567".into();

    // Only the gates' reads are recorded: a merge would be a request nobody wrote down.
    let (landed, recorded) = world.with(json!(ready()), |reach| {
        land_in(&world.plane, "alpha", SLUG, &question, false, reach)
    });

    assert_eq!(recorded.unspent(), Vec::new());
    let refused = landed.expect_err("a landing nobody was shown");
    assert!(
        refused.contains("this is not the landing you confirmed")
            && refused.contains("Nothing was merged"),
        "{refused}"
    );
}

#[test]
fn a_second_push_or_landing_of_a_change_while_one_runs_is_refused_in_words() {
    let busy = Busy::default();
    let root = Path::new("/plane");
    let first = busy.claim(root, "alpha", SLUG).expect("the first");

    let second = busy.claim(root, "alpha", SLUG).err();

    assert_eq!(
        second.as_deref(),
        Some("charter is already pushing or landing api-2 in alpha. Wait for it to finish.")
    );
    assert!(
        busy.claim(root, "alpha", "other").is_ok(),
        "another change is free"
    );
    drop(first);
    assert!(
        busy.claim(root, "alpha", SLUG).is_ok(),
        "free once it finished"
    );
}

#[test]
fn push_and_land_are_on_the_windows_ipc_allow_list_and_hand_no_secret_back() {
    let (value_free, vault_values): (&[&str], &[&str]) = app_commands!(command_names);
    for name in [
        "change_push_question",
        "change_push",
        "change_land_question",
        "change_land",
    ] {
        assert!(
            value_free.contains(&name),
            "{name} is not on the allow-list"
        );
        assert!(!vault_values.contains(&name));
    }
}

/// Every exchange of a landing of widget's #7 merged by `method`.
fn landed_by(method: &str) -> Value {
    let mut exchanges = ready();
    exchanges.push(write(
        "PUT",
        "repos/acme/widget/pulls/7/merge",
        json!([{"text": ["merge_method", method]},
               {"text": ["commit_title", "api-2: widget (#7)"]},
               {"text": ["commit_message", "bump the api\n\nPurlis-Change: api-2"]},
               {"text": ["sha", HEAD]}]),
        json!({"sha": MERGE, "merged": true}),
    ));
    exchanges.push(request("widget", 7, "merged"));
    json!(exchanges)
}

#[test]
fn the_squash_box_reaches_the_merge_the_forge_is_asked_for() {
    let world = World::new(&[("widget", &[])]);
    let (question, _) = world.with(json!(ready()), |reach| {
        land_question_in(&world.plane, "alpha", SLUG, "widget", reach)
    });
    let question = question.unwrap();

    // Only a squash merge is recorded: a plain one would be a request nobody wrote down.
    let (landed, recorded) = world.with(landed_by("squash"), |reach| {
        land_in(&world.plane, "alpha", SLUG, &question, true, reach)
    });

    assert_eq!(recorded.unspent(), Vec::new(), "{landed:?}");
    assert!(
        landed.unwrap().iter().any(|l| l.contains("(squash)")),
        "the landing was not a squash"
    );
}

#[test]
fn squash_is_never_asked_where_the_question_did_not_offer_it() {
    let world = World::new(&[("widget", &[])]);
    let (question, _) = world.with(json!(ready()), |reach| {
        land_question_in(&world.plane, "alpha", SLUG, "widget", reach)
    });
    // As a question about a GitHub merge queue says it: squash is not charter's there. The
    // compared fields are untouched, so this is the landing that was confirmed.
    let question = LandQuestion {
        squash: false,
        ..question.unwrap()
    };

    let (landed, recorded) = world.with(landed_by("merge"), |reach| {
        land_in(&world.plane, "alpha", SLUG, &question, true, reach)
    });

    assert_eq!(recorded.unspent(), Vec::new(), "{landed:?}");
}
