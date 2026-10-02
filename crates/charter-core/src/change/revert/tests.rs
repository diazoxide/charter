//! `charter change revert` against real clones, each landed for real on its own `main` and
//! pushed to a local bare remote, with charter's landing-log line beside it. Ported from the
//! behaviour of `cli-final`'s `tests/test_change_revert.py` (ADR 0060 §8, D7). Nothing reaches
//! a network: revert asks no forge at all, and the test that takes the seeded change on
//! through `push` and `land` answers every forge question from a recording.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::*;
use crate::change::landing::{self, Landing};
use crate::change::{Member, Record, store};

const SLUG: &str = "api-2";
const BRANCH: &str = "change/api-2";
const REVERTED: &str = "revert-api-2";
const REVERT_BRANCH: &str = "change/revert-api-2";

fn git(dir: &Path, args: &[&str]) -> String {
    let done = crate::testgit::run(dir, args);
    assert!(done.ok(), "git {args:?} failed: {done:?}");
    done.out.trim().to_string()
}

fn at(second: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 2, 9, 0, second).unwrap()
}

struct World {
    _dir: tempfile::TempDir,
    top: PathBuf,
    plane: PathBuf,
}

impl World {
    fn new() -> World {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let plane = top.join("plane");
        std::fs::create_dir_all(plane.join("workspaces/alpha")).unwrap();
        std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
        World {
            _dir: dir,
            top,
            plane,
        }
    }

    fn clone_dir(&self, repo: &str) -> PathBuf {
        self.plane.join("workspaces/alpha").join(repo)
    }

    fn bare(&self, repo: &str) -> PathBuf {
        self.top.join(format!("forge/acme/{repo}.git"))
    }

    /// A clone `alpha/<repo>` of `git@github.com:acme/<repo>.git`, whose `main` is on a local
    /// bare remote and fetched, with `origin/HEAD` at `origin/main`, and the change's branch
    /// one commit ahead of `main` adding `<repo>.rs`. The checkout is left on `main`.
    fn clone(&self, repo: &str) -> PathBuf {
        let clone = self.clone_dir(repo);
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main", "."]);
        git(&clone, &["config", "user.name", "Fixture"]);
        git(&clone, &["config", "user.email", "fixture@example.invalid"]);
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "one"]);
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
        self.publish(repo);
        git(
            &clone,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        git(&clone, &["switch", "-q", "-c", BRANCH]);
        std::fs::write(clone.join(format!("{repo}.rs")), "pub fn two() {}\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "two"]);
        git(&clone, &["switch", "-q", "main"]);
        clone
    }

    /// `main` pushed to the bare remote and fetched back, as after a merge on the forge.
    fn publish(&self, repo: &str) {
        let clone = self.clone_dir(repo);
        let at = self.bare(repo).display().to_string();
        git(&clone, &["push", "-q", &at, "main"]);
        git(
            &clone,
            &["fetch", "-q", &at, "+refs/heads/*:refs/remotes/origin/*"],
        );
    }

    /// The change's branch of `repo` landed on `main` as a merge commit, or one squashed commit,
    /// carrying the trailer, published. Answers the commit the landing made.
    fn land(&self, repo: &str, squash: bool) -> String {
        let clone = self.clone_dir(repo);
        let message = format!("api-2: {repo} (#7)\n\nbump the api\n\nCharter-Change: api-2");
        if squash {
            git(&clone, &["merge", "-q", "--squash", BRANCH]);
            git(&clone, &["commit", "-q", "-m", &message]);
        } else {
            git(&clone, &["merge", "-q", "--no-ff", "-m", &message, BRANCH]);
        }
        self.publish(repo);
        git(&clone, &["rev-parse", "HEAD"])
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

    /// Charter's landing-log line for `repo`, naming `merge`.
    fn logged(&self, repo: &str, merge: &str) {
        landing::append(
            &self.plane,
            "alpha",
            "laptop",
            &Landing::new(SLUG, repo, 7, &"a".repeat(40), merge, at(1)),
        )
        .unwrap();
    }

    /// One member, `widget`, landed for real and logged. Answers its clone and the landing.
    fn one_landed(&self, squash: bool) -> (PathBuf, String) {
        let clone = self.clone("widget");
        self.change(&[("widget", &[])]);
        let merge = self.land("widget", squash);
        self.logged("widget", &merge);
        (clone, merge)
    }

    /// `charter change revert`: the exit code and what was said.
    fn revert(&self) -> (u8, String) {
        let mut said = String::new();
        let mut say = |line: Say| {
            said.push_str(&line.to_string());
            said.push('\n');
        };
        let code = revert(&self.plane, "alpha", SLUG, "t", at(9), &mut say);
        (code, said)
    }

    fn reverted(&self) -> Record {
        store::read(&self.plane, "alpha", REVERTED).unwrap()
    }
}

/// The commit `rev` names in `clone`.
fn sha(clone: &Path, rev: &str) -> String {
    git(clone, &["rev-parse", rev])
}

/// Whether `clone` has the local branch `branch`.
fn has_branch(clone: &Path, branch: &str) -> bool {
    crate::testgit::run(
        clone,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .ok()
}

/// Whether `file` is in the tree of `rev`.
fn in_tree(clone: &Path, rev: &str, file: &str) -> bool {
    crate::testgit::run(clone, &["cat-file", "-e", &format!("{rev}:{file}")]).ok()
}

// ---- a revert is a new change -----------------------------------------------------------

#[test]
fn a_merge_landing_is_reverted_on_a_new_branch_of_a_new_change() {
    let world = World::new();
    let (clone, merge) = world.one_landed(false);
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    let record = world.reverted();
    assert_eq!(record.change, REVERTED);
    assert_eq!(
        record.members,
        vec![Member {
            repo: "widget".into(),
            branch: REVERT_BRANCH.into(),
            needs: vec![],
        }]
    );
    // Off `main` as the remote has it, one commit ahead, and that commit undoes the file.
    assert_eq!(sha(&clone, &format!("{REVERT_BRANCH}~1")), merge);
    assert!(in_tree(&clone, "main", "widget.rs"));
    assert!(!in_tree(&clone, REVERT_BRANCH, "widget.rs"));
    let body = git(&clone, &["log", "-1", "--format=%B", REVERT_BRANCH]);
    assert!(
        body.contains(&format!("This reverts commit {merge}")),
        "{body}"
    );
    assert!(said.contains("widget"), "{said}");
}

/// Every git argv the revert ran in `clone`, from the `before`th on.
fn asked(clone: &Path, before: usize) -> Vec<Vec<String>> {
    crate::worktree::git::tally::asked(clone)[before..].to_vec()
}

/// The git verb of `argv`: its first word after any `-c <setting>` and global option.
fn verb(argv: &[String]) -> &str {
    let mut words = argv.iter().map(String::as_str);
    loop {
        match words.next() {
            Some("-c") => {
                words.next();
            }
            Some(word) if word.starts_with('-') => {}
            Some(word) => return word,
            None => return "",
        }
    }
}

/// The `revert` argv the revert ran, without its `--abort`.
fn reverts(argvs: &[Vec<String>]) -> Vec<Vec<String>> {
    argvs
        .iter()
        .filter(|a| verb(a) == "revert" && !a.iter().any(|w| w == "--abort"))
        .cloned()
        .collect()
}

#[test]
fn a_squash_landing_is_reverted_without_m_because_git_says_it_has_one_parent() {
    let world = World::new();
    let (clone, merge) = world.one_landed(true);
    let before = crate::worktree::git::tally::asked(&clone).len();
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    let ran = reverts(&asked(&clone, before));
    assert_eq!(ran.len(), 1, "{ran:?}");
    assert!(!ran[0].iter().any(|w| w == "-m"), "{ran:?}");
    // A repo that does not say `sign = true` gets an unsigned revert, as its saves do.
    assert!(
        ran[0].iter().any(|w| w == "commit.gpgsign=false"),
        "{ran:?}"
    );
    assert_eq!(ran[0].last(), Some(&merge));
    assert!(!in_tree(&clone, REVERT_BRANCH, "widget.rs"));
}

#[test]
fn a_merge_landing_is_reverted_with_m_1_because_git_says_it_has_two() {
    let world = World::new();
    let (clone, merge) = world.one_landed(false);
    let before = crate::worktree::git::tally::asked(&clone).len();
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    let ran = reverts(&asked(&clone, before));
    assert_eq!(ran.len(), 1, "{ran:?}");
    let m = ran[0].iter().position(|w| w == "-m").expect("-m passed");
    assert_eq!(ran[0][m + 1], "1");
    assert_eq!(ran[0].last(), Some(&merge));
}

/// The four acts ADR 0060 §8 refuses are never built: every argv a revert makes is recorded,
/// each is one of [`VERBS`], and none pushes, forces, deletes a branch or resets. No forge is
/// reachable at all: `revert` takes no backend. The remote is exactly as it was.
#[test]
fn every_git_argv_a_revert_makes_is_recorded_and_holds_none_of_the_four_refused_acts() {
    let world = World::new();
    let (clone, _) = world.one_landed(false);
    let remote_before = git(&world.bare("widget"), &["for-each-ref"]);
    let before = crate::worktree::git::tally::asked(&clone).len();
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    let ran = asked(&clone, before);
    assert!(
        ran.iter().any(|a| verb(a) == "revert"),
        "nothing ran: {ran:?}"
    );
    for argv in &ran {
        assert!(
            VERBS.contains(&verb(argv)),
            "{argv:?} is not a revert's verb"
        );
        for word in argv {
            assert!(
                !word.starts_with('+')
                    && !word.starts_with("--force")
                    && !matches!(word.as_str(), "-f" | "-D" | "-d" | "--delete" | "-B" | "-C"),
                "{argv:?}"
            );
        }
        assert!(
            !matches!(
                verb(argv),
                "push" | "branch" | "reset" | "update-ref" | "clean"
            ),
            "{argv:?}"
        );
    }
    assert_eq!(git(&world.bare("widget"), &["for-each-ref"]), remote_before);
    // The checkout is back where it was, and nothing on `main` moved.
    assert_eq!(git(&clone, &["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(sha(&clone, "main"), sha(&clone, "origin/main"));
}

#[test]
fn the_original_record_is_untouched_and_the_new_why_names_it() {
    let world = World::new();
    world.one_landed(false);
    let path = store::path_for(&world.plane, "alpha", SLUG).unwrap();
    let before = std::fs::read(&path).unwrap();
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let record = world.reverted();
    assert_eq!(record.why, "reverts change 'api-2': bump the api");
    assert_eq!(record.by, "t");
    assert_eq!(record.created, "2026-10-02T09:00:09+00:00");
}

// ---- only what charter landed, at the commit it logged ------------------------------------

#[test]
fn a_change_charter_landed_nothing_of_is_refused_and_no_change_is_created() {
    let world = World::new();
    let clone = world.clone("widget");
    world.change(&[("widget", &[])]);
    // Merged, even: but not by charter, so there is no line and nothing is guessed.
    world.land("widget", false);
    let (code, said) = world.revert();
    assert_eq!(code, REFUSED, "{said}");
    assert!(said.contains("nothing to revert"), "{said}");
    assert!(!store::exists(&world.plane, "alpha", REVERTED));
    assert!(!has_branch(&clone, REVERT_BRANCH));
}

#[test]
fn a_member_with_no_landing_line_is_named_as_a_persons_and_left_out() {
    let world = World::new();
    world.clone("widget");
    world.clone("gadget");
    world.change(&[("widget", &[]), ("gadget", &[])]);
    let merge = world.land("widget", false);
    world.logged("widget", &merge);
    world.land("gadget", false);
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    assert!(said.contains("gadget: no landing record"), "{said}");
    let members: Vec<String> = world
        .reverted()
        .members
        .into_iter()
        .map(|m| m.repo)
        .collect();
    assert_eq!(members, vec!["widget"]);
    assert!(!has_branch(&world.clone_dir("gadget"), REVERT_BRANCH));
}

#[test]
fn a_member_charter_started_landing_and_has_not_recorded_is_named_with_the_way_to_record_it() {
    let world = World::new();
    world.clone("gadget");
    world.one_landed(false);
    let mut record = store::read(&world.plane, "alpha", SLUG).unwrap();
    record.members.push(Member {
        repo: "gadget".into(),
        branch: BRANCH.into(),
        needs: vec![],
    });
    store::write(&world.plane, "alpha", &record).unwrap();
    pending::append(
        &world.plane,
        "alpha",
        "laptop",
        &pending::Pending::new(
            SLUG,
            "gadget",
            9,
            &"b".repeat(40),
            pending::Via::Queue,
            pending::Stage::Asked,
            at(2),
        ),
    )
    .unwrap();
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    assert!(
        said.contains("gadget: a landing charter started is not recorded yet")
            && said.contains("charter change land api-2 --repo gadget"),
        "{said}"
    );
}

/// `_SHA_RE`: the log is a local file, and what it names reaches a git argv.
#[test]
fn a_malformed_sha_in_the_log_is_refused_by_name_and_never_reaches_git() {
    for bad in [
        "--upload-pack=touch /tmp/pwned",
        "e0c9d13\n",
        "E0C9D13",
        "e0c9d1",
        "HEAD~1",
    ] {
        let world = World::new();
        let clone = world.clone("widget");
        world.change(&[("widget", &[])]);
        world.land("widget", false);
        world.logged("widget", bad);
        let before = crate::worktree::git::tally::asked(&clone).len();
        let (code, said) = world.revert();
        assert_eq!(code, 1, "{bad:?}: {said}");
        assert!(said.contains("which is not a commit id"), "{bad:?}: {said}");
        assert!(
            !asked(&clone, before).iter().flatten().any(|w| w == bad),
            "{bad:?} reached git"
        );
        assert!(!store::exists(&world.plane, "alpha", REVERTED));
    }
}

#[test]
fn a_logged_commit_the_default_branch_no_longer_holds_is_refused_by_name() {
    let world = World::new();
    let (clone, merge) = world.one_landed(false);
    // The remote's `main` rewritten past the landing, and fetched.
    git(
        &clone,
        &["update-ref", "refs/remotes/origin/main", "main~1"],
    );
    let (code, said) = world.revert();
    assert_eq!(code, 1, "{said}");
    assert!(
        said.contains(&format!(
            "refs/remotes/origin/main no longer holds {}",
            &merge[..12]
        )),
        "{said}"
    );
    assert!(!has_branch(&clone, REVERT_BRANCH));
    assert!(!store::exists(&world.plane, "alpha", REVERTED));
}

#[test]
fn a_logged_commit_git_does_not_know_is_named_rather_than_reverted() {
    let world = World::new();
    world.clone("widget");
    world.change(&[("widget", &[])]);
    world.logged("widget", &"0".repeat(40));
    let (code, said) = world.revert();
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("git does not know the commit"), "{said}");
}

#[test]
fn a_default_branch_charter_cannot_tell_stops_the_member() {
    let world = World::new();
    let (clone, _) = world.one_landed(false);
    git(
        &clone,
        &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"],
    );
    let (code, said) = world.revert();
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("will not guess"), "{said}");
}

// ---- the new change -----------------------------------------------------------------------

/// Undoing goes the other way round: `gadget` needed `widget` to land first, so the revert of
/// `widget` waits for the revert of `gadget`.
#[test]
fn the_ordering_is_the_originals_reversed() {
    let world = World::new();
    world.clone("widget");
    world.clone("gadget");
    world.clone("doodad");
    world.change(&[
        ("widget", &[]),
        ("gadget", &["widget"]),
        ("doodad", &["widget"]),
    ]);
    for repo in ["widget", "gadget"] {
        let merge = world.land(repo, false);
        world.logged(repo, &merge);
    }
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    let needs: Vec<(String, Vec<String>)> = world
        .reverted()
        .members
        .into_iter()
        .map(|m| (m.repo, m.needs))
        .collect();
    // `doodad` was not landed, so it is in neither the revert nor anybody's needs.
    assert_eq!(
        needs,
        vec![
            ("widget".to_string(), vec!["gadget".to_string()]),
            ("gadget".to_string(), vec![]),
        ]
    );
}

#[test]
fn a_second_revert_of_the_same_change_is_refused_by_name() {
    let world = World::new();
    world.one_landed(false);
    assert_eq!(world.revert().0, 0);
    let before = std::fs::read(store::path_for(&world.plane, "alpha", REVERTED).unwrap()).unwrap();
    let (code, said) = world.revert();
    assert_eq!(code, REFUSED, "{said}");
    assert!(said.contains("'revert-api-2' already exists"), "{said}");
    assert_eq!(
        std::fs::read(store::path_for(&world.plane, "alpha", REVERTED).unwrap()).unwrap(),
        before
    );
}

// ---- every way a seed can fail is named ---------------------------------------------------

#[test]
fn uncommitted_work_is_refused_before_any_branch_and_the_record_still_names_the_member() {
    let world = World::new();
    let (clone, _) = world.one_landed(false);
    std::fs::write(clone.join("README.md"), "work in progress\n").unwrap();
    let (code, said) = world.revert();
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("uncommitted changes"), "{said}");
    assert!(!has_branch(&clone, REVERT_BRANCH));
    assert_eq!(
        std::fs::read_to_string(clone.join("README.md")).unwrap(),
        "work in progress\n"
    );
    let members: Vec<String> = world
        .reverted()
        .members
        .into_iter()
        .map(|m| m.repo)
        .collect();
    assert_eq!(members, vec!["widget"]);
}

#[test]
fn a_branch_that_already_exists_is_neither_reused_nor_replaced() {
    let world = World::new();
    let (clone, _) = world.one_landed(false);
    git(&clone, &["branch", REVERT_BRANCH, "main~1"]);
    let was = sha(&clone, REVERT_BRANCH);
    let (code, said) = world.revert();
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("already exists here"), "{said}");
    assert_eq!(sha(&clone, REVERT_BRANCH), was);
}

#[test]
fn a_conflicted_revert_is_aborted_named_and_the_checkout_put_back() {
    let world = World::new();
    let (clone, _) = world.one_landed(true);
    // Later work on the file the landing added, published, so undoing the landing conflicts.
    std::fs::write(clone.join("widget.rs"), "pub fn three() {}\n").unwrap();
    git(&clone, &["commit", "-q", "-am", "later work"]);
    world.publish("widget");
    let (code, said) = world.revert();
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("finish it by hand"), "{said}");
    assert_eq!(git(&clone, &["status", "--porcelain"]), "");
    assert_eq!(git(&clone, &["symbolic-ref", "--short", "HEAD"]), "main");
    // The branch is there for a person, at the base, with no half-made revert on it.
    assert_eq!(sha(&clone, REVERT_BRANCH), sha(&clone, "origin/main"));
}

#[test]
fn a_detached_checkout_is_put_back_where_it_was() {
    let world = World::new();
    let (clone, _) = world.one_landed(false);
    git(&clone, &["switch", "-q", "--detach", "main~1"]);
    let was = sha(&clone, "HEAD");
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    assert_eq!(sha(&clone, "HEAD"), was);
    assert!(crate::testgit::run(&clone, &["symbolic-ref", "-q", "HEAD"]).code == Some(1));
}

#[test]
fn the_revert_commit_is_signed_exactly_as_the_repos_saves_are() {
    let world = World::new();
    let (clone, _) = world.one_landed(false);
    // A signer that refuses, in the clone's own config, so no developer's signer is asked.
    git(&clone, &["config", "gpg.program", "/usr/bin/false"]);
    git(&clone, &["config", "gpg.ssh.program", "/usr/bin/false"]);
    std::fs::write(
        world.plane.join("charter.toml"),
        "schema = 1\n\n[repos.widget]\nsign = true\n",
    )
    .unwrap();
    let before = crate::worktree::git::tally::asked(&clone).len();
    let (code, said) = world.revert();
    // No signer here: asked to sign, git cannot, and the revert is aborted and named.
    let ran = reverts(&asked(&clone, before));
    assert!(ran[0].iter().any(|w| w == "commit.gpgsign=true"), "{ran:?}");
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("finish it by hand"), "{said}");
    assert_eq!(sha(&clone, REVERT_BRANCH), sha(&clone, "origin/main"));
}

// ---- from here it is an ordinary change ---------------------------------------------------

const PROBE: &str = include_str!("../../forge/github/queries/merge_queue.graphql");

fn get(path: &str, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
           "reply": {"code": 0, "out": out.to_string()}})
}

fn write(method: &str, path: &str, fields: Value, out: Value) -> Value {
    json!({"call": {"endpoint": {"rest": {"method": method, "path": path}}, "fields": fields},
           "reply": {"code": 0, "out": out.to_string()}})
}

/// A recorded GitHub, answering only `exchanges`.
fn forge(exchanges: Value) -> std::sync::Arc<crate::forge::recorded::Recorded> {
    let text = json!({"source": "the revert tests", "exchanges": exchanges});
    std::sync::Arc::new(crate::forge::recorded::Recorded::parse(&text.to_string()).unwrap())
}

/// What `push` writes on the revert's request: its `why`, and the block, before and after
/// the request has a number.
const OPENED: &str = "reverts change 'api-2': bump the api

<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->
**Cross-repo change: `revert-api-2`** — reverts change 'api-2': bump the api

| repo | request | needs |
|---|---|---|
| widget | — | — |
<!-- END charter change -->";

/// The seeded change goes through `push` (T6) and `land` (T7) as any change does, with
/// nothing in either that knows it is a revert: its branch is pushed to the remote and its
/// request opened, then it is landed at the head its checks passed on and logged under its
/// own slug.
#[test]
fn the_seeded_change_is_pushed_and_landed_by_the_ordinary_verbs() {
    let world = World::new();
    let (clone, _) = world.one_landed(false);
    let (code, said) = world.revert();
    assert_eq!(code, 0, "{said}");
    let head = sha(&clone, REVERT_BRANCH);

    let by_head = "repos/acme/widget/pulls?state=all&head=acme:change%2Frevert-api-2&per_page=1";
    let numbered = OPENED.replace("| widget | — |", "| widget | acme/widget#8 |");
    let pushed = forge(json!([
        get(by_head, json!([])),
        get(
            "repos/acme/widget/pulls?state=open&head=acme:change%2Frevert-api-2&base=main&per_page=1",
            json!([])
        ),
        write(
            "POST",
            "repos/acme/widget/pulls",
            json!([{"text": ["head", REVERT_BRANCH]}, {"text": ["base", "main"]},
                   {"text": ["title", "revert-api-2: widget"]}, {"text": ["body", OPENED]}]),
            json!({"number": 8, "html_url": "https://github.com/acme/widget/pull/8",
                   "state": "open"})
        ),
        get(
            "repos/acme/widget/pulls/8",
            json!({"number": 8, "body": OPENED})
        ),
        write(
            "PATCH",
            "repos/acme/widget/pulls/8",
            json!([{"text": ["body", numbered]}]),
            json!({"number": 8})
        ),
    ]));
    let mut said = String::new();
    let mut say = |line: Say| {
        said.push_str(&line.to_string());
        said.push('\n');
    };
    let top = world.top.clone();
    let code = super::super::push::push_with(
        &world.plane,
        "alpha",
        REVERTED,
        &|repo: &crate::forge::pr::Repo| repo.forge.backend_over(pushed.clone()),
        &|https: &str| {
            let path = https.strip_prefix("https://github.com/").unwrap();
            format!("file://{}/forge/{path}", top.display())
        },
        &mut say,
    );
    assert_eq!(code, 0, "{said}");
    assert!(pushed.unspent().is_empty(), "{:?}", pushed.unspent());
    assert_eq!(
        git(
            &world.bare("widget"),
            &["rev-parse", &format!("refs/heads/{REVERT_BRANCH}")]
        ),
        head
    );

    let request = |state: &str| {
        let merged = state == "merged";
        get(
            by_head,
            json!([{"number": 8, "html_url": "https://github.com/acme/widget/pull/8",
                    "state": if merged { "closed" } else { "open" },
                    "merged": merged,
                    "merge_commit_sha": if merged { Value::from("c".repeat(40)) } else { Value::Null },
                    "head": {"sha": head, "ref": REVERT_BRANCH,
                             "repo": {"full_name": "acme/widget"}}}]),
        )
    };
    let checks = format!("repos/acme/widget/commits/{head}");
    let landed = forge(json!([
        request("open"),
        get(
            &format!("{checks}/check-runs?per_page=100"),
            json!({"total_count": 1,
                   "check_runs": [{"id": 4, "status": "completed", "conclusion": "success"}]})
        ),
        get(
            &format!("{checks}/status?per_page=100"),
            json!({"state": "success", "total_count": 0, "statuses": []})
        ),
        json!({"call": {"endpoint": "graphql",
                        "fields": [{"text": ["query", PROBE]}, {"text": ["owner", "acme"]},
                                   {"text": ["name", "widget"]}, {"typed": ["number", "8"]}]},
               "reply": {"code": 0, "out": json!({"data": {"repository": {"pullRequest":
                   {"id": "PR_kw", "isMergeQueueEnabled": false}}}}).to_string()}}),
        write(
            "PUT",
            "repos/acme/widget/pulls/8/merge",
            json!([{"text": ["merge_method", "merge"]},
                   {"text": ["commit_title", "revert-api-2: widget (#8)"]},
                   {"text": ["commit_message",
                             "reverts change 'api-2': bump the api\n\nCharter-Change: revert-api-2"]},
                   {"text": ["sha", head]}]),
            json!({"sha": "c".repeat(40), "merged": true})
        ),
        request("merged"),
    ]));
    let mut said = String::new();
    let mut say = |line: Say| {
        said.push_str(&line.to_string());
        said.push('\n');
    };
    let code = super::super::land::land_with(
        &world.plane,
        "alpha",
        REVERTED,
        &["widget".to_string()],
        super::super::land::How::Merge,
        &|repo: &crate::forge::pr::Repo| repo.forge.backend_over(landed.clone()),
        "laptop",
        at(30),
        &mut say,
    );
    assert_eq!(code, 0, "{said}");
    assert!(landed.unspent().is_empty(), "{:?}", landed.unspent());
    let logged = landing::landings(&world.plane, "alpha", REVERTED);
    assert_eq!(
        (
            logged["widget"].merge.clone(),
            logged["widget"].head.clone()
        ),
        ("c".repeat(40), head)
    );
}
