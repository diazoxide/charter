//! A teammate's project grant is told when it arrives, and this machine's yes is bound to
//! what it accepted (#1506, V100-62). Every expected answer is written out.

use std::cell::RefCell;
use std::path::Path;

use super::*;
use crate::dispatchgrant::{InForce, Pair, acknowledge_any, acknowledge_pair, decline};

const NONE: &str = "schema = 1\n";
const ONE: &str = "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\"]\n";
const TWO: &str = "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"qa\"]\n";
const STAR: &str = "schema = 1\n\n[dispatch.grants]\nsteward = [\"*\"]\n";

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

fn persona(root: &Path, name: &str) {
    let dir = root.join("personas").join(name);
    std::fs::create_dir_all(&dir).expect("its folder");
    std::fs::write(
        dir.join("persona.md"),
        format!("---\nname: {name}\ndelegate-when: {name} work\n---\n\n# {name}\n"),
    )
    .expect("its definition");
}

fn write(root: &Path, text: &str) {
    std::fs::write(crate::names::manifest(root), text).expect("the project file");
}

/// A project whose file is `text`, with the personas steward, devops and qa.
fn project(text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a project");
    write(dir.path(), text);
    for name in ["steward", "devops", "qa"] {
        persona(dir.path(), name);
    }
    dir
}

fn arrived(asking: &str, target: &str) -> Arrived {
    Arrived {
        asking: asking.to_owned(),
        target: target.to_owned(),
        undefined: None,
        again: false,
    }
}

/// A history that answers what a test hands it, and counts what it was asked.
struct Told {
    head: Option<&'static str>,
    between: Between,
    asked: RefCell<Vec<(String, String)>>,
}

impl Told {
    /// No repository: there is no commit to read.
    fn none() -> Self {
        Self::at(None, Between::Unanswered)
    }

    fn at(head: Option<&'static str>, between: Between) -> Self {
        Self {
            head,
            between,
            asked: RefCell::default(),
        }
    }
}

impl History for Told {
    fn head(&self) -> Option<String> {
        self.head.map(str::to_owned)
    }

    fn between(&self, from: &str, to: &str) -> Between {
        self.asked
            .borrow_mut()
            .push((from.to_owned(), to.to_owned()));
        self.between.clone()
    }
}

fn versions(from: &str, since: &[Option<&str>]) -> Between {
    Between::Versions {
        from: Some(from.to_owned()),
        since: since.iter().map(|one| one.map(str::to_owned)).collect(),
    }
}

fn in_force(root: &Path) -> Vec<Pair> {
    InForce::read(root, Vec::new()).project
}

// ---- arrival ---------------------------------------------------------------------------------

#[test]
fn a_pair_the_project_gained_is_waiting_and_in_force_for_nobody_until_it_is_accepted() {
    let dir = project(NONE);
    let root = dir.path();
    assert_eq!(arrival_with(root, &Told::none()), Arrival::default());

    write(root, ONE);

    assert_eq!(
        arrival_with(root, &Told::none()).waiting,
        [arrived("steward", "devops")]
    );
    assert_eq!(in_force(root), []);

    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    assert_eq!(arrival_with(root, &Told::none()), Arrival::default());
    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

#[test]
fn several_pairs_and_any_persona_arrive_together_with_any_persona_first() {
    let dir = project(
        "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"qa\"]\nqa = [\"devops\", \"*\"]\n",
    );

    assert_eq!(
        arrival_with(dir.path(), &Told::none()).waiting,
        [
            arrived("qa", "*"),
            arrived("steward", "devops"),
            arrived("steward", "qa"),
            arrived("qa", "devops"),
        ]
    );
}

#[test]
fn a_declined_grant_is_never_told_as_arrived_again() {
    let dir = project(TWO);
    let root = dir.path();

    decline(root, "steward", "devops").expect("declined");

    assert_eq!(
        arrival_with(root, &Told::none()).waiting,
        [arrived("steward", "qa")]
    );
    assert_eq!(in_force(root), []);
}

#[test]
fn a_grant_naming_a_persona_this_checkout_does_not_define_is_said_to_and_not_as_usable() {
    let dir = project(
        "schema = 1\n\n[dispatch.grants]\nsteward = [\"ghost\"]\nphantom = [\"devops\", \"*\"]\n",
    );

    assert_eq!(
        arrival_with(dir.path(), &Told::none()).waiting,
        [
            Arrived {
                undefined: Some("phantom".to_owned()),
                ..arrived("phantom", "*")
            },
            Arrived {
                undefined: Some("ghost".to_owned()),
                ..arrived("steward", "ghost")
            },
            Arrived {
                undefined: Some("phantom".to_owned()),
                ..arrived("phantom", "devops")
            },
        ]
    );
}

// ---- removal and return ----------------------------------------------------------------------

#[test]
fn a_pair_removed_and_put_back_waits_for_a_new_yes_and_its_going_is_told_once() {
    let dir = project(ONE);
    let root = dir.path();
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    // Taken out: seen at the next read of the grants in force.
    write(root, NONE);
    assert_eq!(in_force(root), []);
    assert_eq!(
        arrival_with(root, &Told::none()),
        Arrival {
            waiting: Vec::new(),
            gone: vec!["steward -> devops".to_owned()],
        }
    );

    told_gone(root, &["steward -> devops".to_owned()]).expect("told");
    assert_eq!(arrival_with(root, &Told::none()), Arrival::default());

    // Put back: it is the project's again, and nobody here has said yes to it.
    write(root, ONE);
    assert_eq!(in_force(root), []);
    assert_eq!(
        arrival_with(root, &Told::none()).waiting,
        [arrived("steward", "devops")]
    );
}

#[test]
fn a_pair_put_back_before_its_going_was_told_is_asked_again_and_said_to_be() {
    let dir = project(ONE);
    let root = dir.path();
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");
    write(root, NONE);
    settle_with(root, &Told::none());

    write(root, ONE);

    assert_eq!(
        arrival_with(root, &Told::none()),
        Arrival {
            waiting: vec![Arrived {
                again: true,
                ..arrived("steward", "devops")
            }],
            gone: Vec::new(),
        }
    );
    assert_eq!(in_force(root), []);

    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted again");
    assert_eq!(arrival_with(root, &Told::none()), Arrival::default());
    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

#[test]
fn any_persona_removed_and_put_back_waits_for_a_new_yes() {
    let dir = project(STAR);
    let root = dir.path();
    acknowledge_any(root, "steward").expect("accepted");
    assert_eq!(InForce::read(root, Vec::new()).project_any, ["steward"]);

    write(root, NONE);
    assert_eq!(
        arrival_with(root, &Told::none()).gone,
        ["steward -> *".to_owned()]
    );
    write(root, STAR);

    assert_eq!(
        InForce::read(root, Vec::new()).project_any,
        Vec::<String>::new()
    );
    assert_eq!(
        arrival_with(root, &Told::none()).waiting,
        [Arrived {
            again: true,
            ..arrived("steward", "*")
        }]
    );
}

#[test]
fn a_decline_goes_with_the_grant_it_declined_so_one_put_back_is_told_again() {
    let dir = project(ONE);
    let root = dir.path();
    decline(root, "steward", "devops").expect("declined");

    write(root, NONE);
    assert_eq!(arrival_with(root, &Told::none()), Arrival::default());
    write(root, ONE);

    assert_eq!(
        arrival_with(root, &Told::none()).waiting,
        [arrived("steward", "devops")]
    );
    assert_eq!(in_force(root), []);
}

#[test]
fn a_project_file_that_does_not_read_for_a_moment_drops_nothing() {
    let dir = project(ONE);
    let root = dir.path();
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    write(root, "<<<<<<< ours\nschema = 1\n=======\n");
    settle_with(root, &Told::none());
    std::fs::remove_file(crate::names::manifest(root)).expect("removed");
    settle_with(root, &Told::none());
    write(root, ONE);

    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

// ---- both between two reads ------------------------------------------------------------------

/// Accepts steward to devops in a project at commit [`A`].
fn accepted_at_a(root: &Path) {
    let history = Told::at(Some(A), Between::Unanswered);
    settle_with(root, &history);
    crate::sandbox::local::acknowledge_dispatch(root, &["steward -> devops".to_owned()])
        .expect("accepted");
    settle_with(root, &history);
    assert_eq!(
        crate::sandbox::local::dispatch_bound(root).at.as_deref(),
        Some(A)
    );
}

#[test]
fn a_pair_removed_and_put_back_between_two_reads_waits_for_a_new_yes() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    // Two commits arrive in one pull: the first takes the pair out, the second puts it back.
    // The file reads exactly as it did when the pair was accepted.
    let pulled = Told::at(Some(B), versions(ONE, &[Some(NONE), Some(ONE)]));

    assert_eq!(
        arrival_with(root, &pulled),
        Arrival {
            waiting: vec![Arrived {
                again: true,
                ..arrived("steward", "devops")
            }],
            gone: Vec::new(),
        }
    );
    assert_eq!(*pulled.asked.borrow(), [(A.to_owned(), B.to_owned())]);
    assert_eq!(in_force(root), []);
    assert_eq!(crate::sandbox::local::dispatch_bound(root).at, None);
}

#[test]
fn a_pair_every_version_in_between_holds_stays_accepted_and_is_bound_to_the_new_commit() {
    let dir = project(TWO);
    let root = dir.path();
    accepted_at_a(root);

    // The persona's line changed around it; the accepted pair was there the whole time.
    let pulled = Told::at(Some(B), versions(ONE, &[Some(TWO)]));
    settle_with(root, &pulled);

    assert_eq!(
        crate::sandbox::local::dispatch_bound(root),
        crate::sandbox::local::DispatchBound {
            seen: vec!["steward -> devops".to_owned()],
            at: Some(B.to_owned()),
            ..Default::default()
        }
    );
    // What is new waits.
    assert_eq!(
        arrival_with(root, &Told::at(Some(B), Between::Unreadable)).waiting,
        [arrived("steward", "qa")]
    );
}

#[test]
fn the_history_is_read_once_per_commit_and_not_at_all_where_nothing_is_accepted() {
    let dir = project(ONE);
    let root = dir.path();
    let idle = Told::at(Some(A), Between::Unreadable);
    settle_with(root, &idle);
    assert_eq!(crate::sandbox::local::dispatch_bound(root).at, None);

    accepted_at_a(root);
    let same = Told::at(Some(A), Between::Unreadable);
    settle_with(root, &same);
    settle_with(root, &same);

    assert_eq!(*same.asked.borrow(), []);
    assert_eq!(
        crate::sandbox::local::dispatch_bound(root).seen,
        ["steward -> devops"]
    );
}

#[test]
fn a_version_with_no_project_file_or_one_that_does_not_parse_held_no_grant() {
    for between in [None, Some("not = [toml")] {
        let dir = project(ONE);
        let root = dir.path();
        accepted_at_a(root);

        settle_with(
            root,
            &Told::at(Some(B), versions(ONE, &[between, Some(ONE)])),
        );

        assert_eq!(
            crate::sandbox::local::dispatch_bound(root).seen,
            Vec::<String>::new(),
            "{between:?}"
        );
    }
}

#[test]
fn a_history_that_cannot_be_read_keeps_no_acceptance() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    settle_with(root, &Told::at(Some(B), Between::Unreadable));

    assert_eq!(
        arrival_with(root, &Told::at(Some(B), Between::Unreadable)).waiting,
        [Arrived {
            again: true,
            ..arrived("steward", "devops")
        }]
    );
}

#[test]
fn a_history_that_does_not_answer_changes_nothing_and_is_asked_again() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    settle_with(root, &Told::at(Some(B), Between::Unanswered));
    settle_with(root, &Told::at(None, Between::Unanswered));

    assert_eq!(
        crate::sandbox::local::dispatch_bound(root),
        crate::sandbox::local::DispatchBound {
            seen: vec!["steward -> devops".to_owned()],
            at: Some(A.to_owned()),
            ..Default::default()
        }
    );
    let later = Told::at(Some(B), versions(ONE, &[Some(NONE), Some(ONE)]));
    settle_with(root, &later);
    assert_eq!(*later.asked.borrow(), [(A.to_owned(), B.to_owned())]);
    assert_eq!(in_force(root), []);
}

#[test]
fn a_grant_made_here_and_not_committed_yet_is_not_dropped_by_commits_that_never_held_it() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    // The commit it was accepted at had no such pair: it was this machine's own edit.
    let pulled = Told::at(Some(B), versions(NONE, &[Some(NONE)]));
    settle_with(root, &pulled);

    assert_eq!(
        crate::sandbox::local::dispatch_bound(root),
        crate::sandbox::local::DispatchBound {
            seen: vec!["steward -> devops".to_owned()],
            at: Some(B.to_owned()),
            ..Default::default()
        }
    );
}

#[test]
fn an_acceptance_from_before_commits_were_kept_is_bound_from_the_first_read_on() {
    let dir = project(ONE);
    let root = dir.path();
    crate::sandbox::local::acknowledge_dispatch(root, &["steward -> devops".to_owned()])
        .expect("an earlier build's record");

    let first = Told::at(Some(A), Between::Unreadable);
    settle_with(root, &first);

    assert_eq!(*first.asked.borrow(), []);
    assert_eq!(
        crate::sandbox::local::dispatch_bound(root).at.as_deref(),
        Some(A)
    );
    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

// ---- git's own history ------------------------------------------------------------------------

/// git in `root`, for a fixture.
fn git(root: &Path, args: &[&str]) -> String {
    let mut all = vec![
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    all.extend(args);
    let run = crate::testgit::run(root, &all);
    assert_eq!(run.code, Some(0), "git {args:?}: {}", run.err);
    run.out.trim().to_owned()
}

/// Commits the project file as `text`, and answers the commit.
fn commit(root: &Path, text: &str) -> String {
    write(root, text);
    let file = crate::names::manifest(root);
    let file = file
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a name");
    git(root, &["add", "--", file]);
    git(root, &["commit", "-q", "--allow-empty", "-m", "settings"]);
    git(root, &["rev-parse", "HEAD"])
}

/// A project in a repository of its own, its file committed as `text`. The repository is kept
/// beside the project, so nothing of git's is written inside it.
fn repository(text: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let dir = project(NONE);
    let kept = tempfile::tempdir().expect("a place for the repository");
    let at = format!("--separate-git-dir={}", kept.path().join("git").display());
    git(dir.path(), &["init", "-q", &at]);
    commit(dir.path(), text);
    (dir, kept)
}

#[test]
fn git_says_every_version_between_two_commits_and_the_one_before_them() {
    let (dir, _kept) = repository(ONE);
    let root = dir.path();
    let history = Git(root);
    let first = history.head().expect("a commit");

    let out = commit(root, NONE);
    let back = commit(root, ONE);

    assert_eq!(history.head().as_deref(), Some(back.as_str()));
    assert_eq!(
        history.between(&first, &back),
        versions(ONE, &[Some(ONE), Some(NONE)])
    );
    assert_eq!(history.between(&out, &back), versions(NONE, &[Some(ONE)]));
    assert_eq!(history.between(&back, &back), versions(ONE, &[]));
}

#[test]
fn a_pair_two_pulled_commits_took_out_and_put_back_waits_for_a_new_yes_in_a_real_repository() {
    let (dir, _kept) = repository(ONE);
    let root = dir.path();
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");
    assert_eq!(in_force(root), [pair("steward", "devops")]);

    // Nothing reads the grants between the two commits.
    commit(root, NONE);
    commit(root, ONE);

    assert_eq!(in_force(root), []);
    assert_eq!(
        arrival(root).waiting,
        [Arrived {
            again: true,
            ..arrived("steward", "devops")
        }]
    );
}

#[test]
fn a_commit_that_leaves_the_grants_alone_keeps_the_acceptance_in_a_real_repository() {
    let (dir, _kept) = repository(ONE);
    let root = dir.path();
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    let head = commit(root, TWO);

    assert_eq!(in_force(root), [pair("steward", "devops")]);
    assert_eq!(crate::sandbox::local::dispatch_bound(root).at, Some(head));
}

#[test]
fn a_version_on_a_branch_that_was_merged_in_counts_though_the_merge_changed_nothing() {
    let (dir, _kept) = repository(ONE);
    let root = dir.path();
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");
    let ours = Git(root).head().expect("a commit");

    // A teammate's line of history takes the pair out and puts it back; this machine has a
    // commit of its own, so the pull is a merge whose result is the file as it was.
    git(root, &["checkout", "-q", "-b", "theirs"]);
    commit(root, NONE);
    commit(root, ONE);
    git(root, &["checkout", "-q", "-"]);
    std::fs::write(root.join("notes.md"), "mine\n").expect("a file of this machine's");
    git(root, &["add", "--", "notes.md"]);
    git(root, &["commit", "-q", "-m", "mine"]);
    git(root, &["merge", "-q", "--no-ff", "-m", "pull", "theirs"]);
    assert_ne!(Git(root).head().expect("the merge"), ours);

    assert_eq!(in_force(root), []);
}

#[test]
fn a_stored_commit_that_is_no_commit_id_is_never_handed_to_git() {
    let (dir, _kept) = repository(ONE);
    let root = dir.path();
    let head = Git(root).head().expect("a commit");

    for forged in ["--output=x", "HEAD", "", "main..other", &"g".repeat(40)] {
        assert_eq!(Git(root).between(forged, &head), Between::Unreadable);
    }
    // One git does not know: nothing is assumed to have stayed.
    assert_eq!(Git(root).between(A, &head), Between::Unreadable);
}

#[test]
fn a_project_in_no_repository_has_no_commit_to_read() {
    let dir = project(ONE);
    // A fixture's folder may sit inside some checkout of the machine's; only where it does
    // not is there nothing to read.
    if Git(dir.path()).head().is_none() {
        acknowledge_pair(dir.path(), &pair("steward", "devops")).expect("accepted");
        assert_eq!(crate::sandbox::local::dispatch_bound(dir.path()).at, None);
        assert_eq!(in_force(dir.path()), [pair("steward", "devops")]);
    }
}
