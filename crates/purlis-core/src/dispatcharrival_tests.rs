//! A teammate's project grant is told when it arrives, and this machine's yes is bound to
//! what it accepted (#1506, V100-62). Every expected answer is written out.

use std::cell::RefCell;
use std::path::Path;
use std::sync::atomic::AtomicUsize;

use super::*;
use crate::dispatchgrant::{InForce, Pair, acknowledge_any, acknowledge_pair, decline};

const NONE: &str = "schema = 1\n";
const ONE: &str = "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\"]\n";
const TWO: &str = "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"qa\"]\n";
const STAR: &str = "schema = 1\n\n[dispatch.grants]\nsteward = [\"*\"]\n";

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PAIR: &str = "steward -> devops";

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

fn persona_saying(root: &Path, name: &str, body: &str) {
    let dir = root.join("personas").join(name);
    std::fs::create_dir_all(&dir).expect("its folder");
    std::fs::write(
        dir.join("persona.md"),
        format!("---\nname: {name}\ndelegate-when: {name} work\n---\n\n# {name}\n{body}"),
    )
    .expect("its definition");
}

fn persona(root: &Path, name: &str) {
    persona_saying(root, name, "");
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
        again: None,
    }
}

fn again(asking: &str, target: &str, why: Again) -> Arrived {
    Arrived {
        again: Some(why),
        ..arrived(asking, target)
    }
}

/// A history that answers what a test hands it, and keeps what it was asked.
struct Told {
    head: Head,
    between: Between,
    asked: RefCell<Vec<(String, String)>>,
}

impl Told {
    /// No repository: there is no history.
    fn none() -> Self {
        Self::of(Head::None, Between::Unanswered)
    }

    fn at(head: &str, between: Between) -> Self {
        Self::of(Head::At(head.to_owned()), between)
    }

    fn of(head: Head, between: Between) -> Self {
        Self {
            head,
            between,
            asked: RefCell::default(),
        }
    }
}

impl History for Told {
    fn head(&self, _until: Instant) -> Head {
        self.head.clone()
    }

    fn between(&self, from: &str, to: &str, _until: Instant) -> Between {
        self.asked
            .borrow_mut()
            .push((from.to_owned(), to.to_owned()));
        self.between.clone()
    }
}

fn took_out(said: &[&str]) -> Between {
    Between::TakenOut(said.iter().map(|one| (*one).to_owned()).collect())
}

/// The project's pairs in force, by the last settling.
fn in_force(root: &Path) -> Vec<Pair> {
    InForce::read(root, Vec::new()).project
}

fn bound(root: &Path) -> local::DispatchBound {
    local::dispatch_bound(root)
}

/// Accepts steward to devops in a project at commit [`A`].
fn accepted_at_a(root: &Path) {
    local::accept_dispatch(root, PAIR, &|| true).expect("accepted");
    assert_eq!(
        settle_with(root, &Told::at(A, Between::Unanswered)),
        Verdict { read: true }
    );
    assert_eq!(bound(root).at.as_deref(), Some(A));
    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

// ---- which commits took a grant out ----------------------------------------------------------

fn step(holds: &[&str], parents: &[&[&str]]) -> Step {
    let own = |all: &[&str]| all.iter().map(|one| (*one).to_owned()).collect::<Vec<_>>();
    Step {
        holds: own(holds),
        parents: parents.iter().map(|one| own(one)).collect(),
    }
}

#[test]
fn a_commit_takes_out_what_it_lacks_and_a_parent_of_it_holds() {
    // Taken out, then put back.
    assert_eq!(
        taken_out(&[step(&[PAIR], &[&[]]), step(&[], &[&[PAIR]])]),
        [PAIR]
    );
    // Added, and never taken out.
    assert_eq!(
        taken_out(&[
            step(&[PAIR, "qa -> devops"], &[&[PAIR]]),
            step(&[PAIR], &[&[]])
        ]),
        Vec::<String>::new()
    );
    // A branch that predates the grant, merged later: no commit of it took anything out.
    assert_eq!(
        taken_out(&[step(&[PAIR], &[&[PAIR], &[]]), step(&[], &[&[]])]),
        Vec::<String>::new()
    );
    // A merge that keeps the grant does not undo a removal on the line it merged.
    assert_eq!(
        taken_out(&[
            step(&[PAIR], &[&[PAIR], &[PAIR]]),
            step(&[PAIR], &[&[]]),
            step(&[], &[&[PAIR]]),
        ]),
        [PAIR]
    );
    // A merge that itself loses what one side held took it out.
    assert_eq!(taken_out(&[step(&[], &[&[], &[PAIR]])]), [PAIR]);
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

// ---- absent from the working file is not a removal -------------------------------------------

#[test]
fn a_grant_absent_from_the_file_on_disk_is_not_in_force_and_nothing_is_dropped_or_told() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);
    let before = bound(root);

    // A branch without the grant is checked out, purlis open: same commit history as far as
    // any removal goes, another file on disk.
    write(root, NONE);
    let away = Told::at(B, took_out(&[]));
    assert_eq!(
        arrival_with(root, &away),
        Arrival::default(),
        "nothing waits and nothing was taken away"
    );
    assert_eq!(in_force(root), []);
    assert_eq!(
        bound(root),
        local::DispatchBound {
            at: Some(B.to_owned()),
            ..before
        },
        "the acceptance is kept"
    );

    // And back: in force again, with nothing asked.
    write(root, ONE);
    assert_eq!(
        arrival_with(root, &Told::at(A, took_out(&[]))),
        Arrival::default()
    );
    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

#[test]
fn a_project_file_that_does_not_read_for_a_moment_drops_nothing() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    write(root, "<<<<<<< ours\nschema = 1\n=======\n");
    assert_eq!(
        arrival_with(root, &Told::at(A, took_out(&[]))),
        Arrival::default()
    );
    assert_eq!(in_force(root), []);
    std::fs::remove_file(crate::names::manifest(root)).expect("removed");
    assert_eq!(
        arrival_with(root, &Told::at(A, took_out(&[]))),
        Arrival::default()
    );
    write(root, ONE);

    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

// ---- a commit took it out --------------------------------------------------------------------

#[test]
fn a_pair_a_commit_took_out_is_dropped_and_its_going_is_told_once() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    // A pull brings the commit that takes it out.
    write(root, NONE);
    let pulled = Told::at(B, took_out(&[PAIR]));
    assert_eq!(
        arrival_with(root, &pulled),
        Arrival {
            gone: vec![PAIR.to_owned()],
            ..Arrival::default()
        }
    );
    assert_eq!(*pulled.asked.borrow(), [(A.to_owned(), B.to_owned())]);
    assert_eq!(bound(root).seen, Vec::<String>::new());
    // It stands until it is read, then is not said again.
    assert_eq!(arrival_with(root, &pulled).gone, [PAIR]);
    told_gone(root, &[PAIR.to_owned()]).expect("told");
    assert_eq!(arrival_with(root, &pulled), Arrival::default());

    // Put back later: it is the project's again, and nobody here has said yes to it.
    write(root, ONE);
    assert_eq!(
        arrival_with(root, &pulled).waiting,
        [arrived("steward", "devops")]
    );
    assert_eq!(in_force(root), []);
}

#[test]
fn a_pair_taken_out_and_put_back_between_two_reads_waits_for_a_new_yes_and_says_why() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    // Two commits arrive in one pull: the first takes the pair out, the second puts it back.
    // The file reads exactly as it did when the pair was accepted.
    let pulled = Told::at(B, took_out(&[PAIR]));

    assert_eq!(
        arrival_with(root, &pulled),
        Arrival {
            waiting: vec![again("steward", "devops", Again::TakenOut)],
            ..Arrival::default()
        }
    );
    assert_eq!(in_force(root), []);

    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted again");
    assert_eq!(arrival_with(root, &Told::none()), Arrival::default());
    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

#[test]
fn any_persona_a_commit_took_out_and_put_back_waits_for_a_new_yes() {
    let dir = project(STAR);
    let root = dir.path();
    local::accept_dispatch(root, "steward -> *", &|| true).expect("accepted");
    settle_with(root, &Told::at(A, Between::Unanswered));
    assert_eq!(InForce::read(root, Vec::new()).project_any, ["steward"]);

    let pulled = Told::at(B, took_out(&["steward -> *"]));

    assert_eq!(
        arrival_with(root, &pulled).waiting,
        [again("steward", "*", Again::TakenOut)]
    );
    assert_eq!(
        InForce::read(root, Vec::new()).project_any,
        Vec::<String>::new()
    );
}

#[test]
fn a_pair_no_commit_took_out_stays_accepted_and_is_bound_to_the_new_commit() {
    let dir = project(TWO);
    let root = dir.path();
    accepted_at_a(root);

    // The persona's line changed around it; another grant was taken out.
    let pulled = Told::at(B, took_out(&["qa -> devops"]));
    assert_eq!(settle_with(root, &pulled), Verdict { read: true });

    assert_eq!(
        bound(root),
        local::DispatchBound {
            seen: vec![PAIR.to_owned()],
            at: Some(B.to_owned()),
            ..Default::default()
        }
    );
    // What is new waits.
    assert_eq!(
        arrival_with(root, &pulled).waiting,
        [arrived("steward", "qa")]
    );
}

#[test]
fn a_decline_goes_when_a_commit_takes_its_grant_out_and_not_when_a_checkout_does() {
    let dir = project(ONE);
    let root = dir.path();
    decline(root, "steward", "devops").expect("declined");
    settle_with(root, &Told::at(A, Between::Unanswered));
    assert_eq!(bound(root).at.as_deref(), Some(A));

    // Away on a branch without it, and back: still declined, still not told.
    write(root, NONE);
    settle_with(root, &Told::at(B, took_out(&[])));
    write(root, ONE);
    assert_eq!(
        arrival_with(root, &Told::at(A, took_out(&[]))),
        Arrival::default()
    );

    // A commit takes it out and another puts it back: a new grant, told again.
    assert_eq!(
        arrival_with(root, &Told::at(B, took_out(&[PAIR]))).waiting,
        [arrived("steward", "devops")]
    );
    assert_eq!(in_force(root), []);
}

// ---- a history that does not answer ----------------------------------------------------------

#[test]
fn a_history_that_cannot_be_read_keeps_no_acceptance_and_says_that_is_why() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    assert_eq!(
        arrival_with(root, &Told::at(B, Between::Unreadable)),
        Arrival {
            waiting: vec![again("steward", "devops", Again::Unread)],
            ..Arrival::default()
        }
    );
    assert_eq!(in_force(root), []);
    assert_eq!(bound(root).seen, Vec::<String>::new());
}

#[test]
fn a_repository_that_is_no_longer_one_keeps_no_acceptance_bound_to_a_commit() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);

    assert_eq!(
        arrival_with(root, &Told::none()).waiting,
        [again("steward", "devops", Again::Unread)]
    );
    assert_eq!(bound(root).at, None);
}

#[test]
fn where_git_cannot_be_run_nothing_stored_changes_and_nothing_is_in_force_for_that_read() {
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);
    let before = bound(root);

    for unanswered in [
        Told::at(B, Between::Unanswered),
        Told::of(Head::Unanswered, Between::Unanswered),
    ] {
        let told = arrival_with(root, &unanswered);
        assert_eq!(
            told,
            Arrival {
                unread: true,
                ..Arrival::default()
            }
        );
        assert_eq!(in_force(root), [], "not in force for this read");
        assert_eq!(bound(root), before, "and nothing stored changed");
    }

    // It answers again: the history is read from the commit that was kept.
    let later = Told::at(B, took_out(&[]));
    assert_eq!(settle_with(root, &later), Verdict { read: true });
    assert_eq!(*later.asked.borrow(), [(A.to_owned(), B.to_owned())]);
    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

#[cfg(unix)]
#[test]
fn a_drop_that_could_not_be_written_leaves_nothing_in_force() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = project(ONE);
    let root = dir.path();
    accepted_at_a(root);
    let state = local::path(root)
        .parent()
        .expect("its folder")
        .to_path_buf();
    let writable = std::fs::metadata(&state).expect("there").permissions();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o555)).expect("read-only");

    let verdict = settle_with(root, &Told::at(B, took_out(&[PAIR])));
    let stored = bound(root);
    let now = in_force(root);
    std::fs::set_permissions(&state, writable).expect("writable again");

    // A user no permission stops wrote it all the same: then there is nothing to show.
    if stored.seen.is_empty() {
        return;
    }
    assert_eq!(verdict, Verdict { read: false });
    assert_eq!(stored.seen, [PAIR], "still on disk");
    assert_eq!(now, [], "and in force for nobody");
}

#[test]
fn the_history_is_read_once_per_commit_and_not_at_all_where_nothing_is_kept() {
    let dir = project(ONE);
    let root = dir.path();
    let idle = Told::of(Head::Unanswered, Between::Unreadable);
    assert_eq!(settle_with(root, &idle), Verdict { read: true });
    assert_eq!(bound(root).at, None);

    accepted_at_a(root);
    let same = Told::at(A, Between::Unreadable);
    settle_with(root, &same);
    settle_with(root, &same);

    assert_eq!(*same.asked.borrow(), []);
    assert_eq!(bound(root).seen, [PAIR]);
}

#[test]
fn an_acceptance_from_before_commits_were_kept_is_bound_from_the_first_read_on() {
    let dir = project(ONE);
    let root = dir.path();
    local::acknowledge_dispatch(root, &[PAIR.to_owned()]).expect("an earlier build's record");

    let first = Told::at(A, Between::Unreadable);
    assert_eq!(settle_with(root, &first), Verdict { read: true });

    assert_eq!(*first.asked.borrow(), []);
    assert_eq!(bound(root).at.as_deref(), Some(A));
    assert_eq!(in_force(root), [pair("steward", "devops")]);
}

// ---- one settling at a time ------------------------------------------------------------------

/// A history whose first answer waits until it is let go, and that counts its answers.
struct Slow {
    asked: AtomicUsize,
    entered: mpsc::Sender<()>,
    go: Mutex<mpsc::Receiver<()>>,
}

impl History for Slow {
    fn head(&self, _until: Instant) -> Head {
        if self.asked.fetch_add(1, Ordering::SeqCst) == 0 {
            let _ = self.entered.send(());
            let _ = self.go.lock().expect("held").recv();
        }
        Head::At(A.to_owned())
    }

    fn between(&self, _from: &str, _to: &str, _until: Instant) -> Between {
        Between::Unreadable
    }
}

#[test]
fn a_settling_asked_for_while_one_runs_waits_for_it_and_takes_its_answer() {
    let dir = project(ONE);
    let root = dir.path().to_path_buf();
    accepted_at_a(&root);
    let (entered, has_entered) = mpsc::channel();
    let (go, wait) = mpsc::channel();
    let slow = Arc::new(Slow {
        asked: AtomicUsize::new(0),
        entered,
        go: Mutex::new(wait),
    });

    let first = {
        let (root, slow) = (root.clone(), Arc::clone(&slow));
        std::thread::spawn(move || settle_with(&root, &*slow))
    };
    has_entered
        .recv_timeout(Duration::from_secs(30))
        .expect("the first is reading the history");
    let second = {
        let (root, slow) = (root.clone(), Arc::clone(&slow));
        std::thread::spawn(move || settle_with(&root, &*slow))
    };
    // The second has asked, and is waiting on the first.
    std::thread::sleep(Duration::from_millis(200));
    go.send(()).expect("let go");

    assert_eq!(first.join().expect("first"), Verdict { read: true });
    assert_eq!(second.join().expect("second"), Verdict { read: true });
    assert_eq!(
        slow.asked.load(Ordering::SeqCst),
        1,
        "one read of the history"
    );
}

// ---- the record --------------------------------------------------------------------------------

#[test]
fn an_acceptance_is_added_in_one_write_and_brings_nothing_dropped_back() {
    let dir = project(TWO);
    let root = dir.path();
    accepted_at_a(root);
    // Dropped by a settling, as another thread's would between a read and a write.
    settle_with(root, &Told::at(B, took_out(&[PAIR])));

    acknowledge_pair(root, &pair("steward", "qa")).expect("accepted");

    assert_eq!(bound(root).seen, ["steward -> qa"]);
    assert_eq!(
        bound(root).gone,
        [Gone {
            said: PAIR.to_owned(),
            why: Gone::REMOVED.to_owned(),
        }]
    );
}

#[test]
fn nothing_is_accepted_that_the_project_s_file_does_not_hold() {
    let dir = project(NONE);
    let root = dir.path();

    let pair_refused = acknowledge_pair(root, &pair("steward", "devops")).expect_err("refused");
    let any_refused = acknowledge_any(root, "steward").expect_err("refused");

    assert_eq!(
        pair_refused.to_string(),
        crate::dispatchgrant::NO_SUCH_GRANT
    );
    assert_eq!(any_refused.to_string(), crate::dispatchgrant::NO_SUCH_GRANT);
    assert_eq!(bound(root), local::DispatchBound::default());
    assert_eq!(arrival_with(root, &Told::none()), Arrival::default());
}

// ---- a persona that went and came back (#1504's rule, told here) -----------------------------

#[test]
fn a_grant_naming_a_name_that_changed_hands_waits_again_and_says_the_persona_was_away() {
    let dir = project(ONE);
    let root = dir.path();
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");
    crate::dispatchdormant::judged(root, &[]).expect("a dispatch judged while both are there");

    // devops is removed, seen gone, and another persona is made under the name.
    std::fs::remove_dir_all(root.join("personas/devops")).expect("removed");
    crate::dispatchdormant::noticed(root, &[]).expect("seen gone");
    persona_saying(root, "devops", "Another persona altogether.\n");

    // Before any dispatch is judged the acceptance is still in the record, and waits.
    let waits = [again(
        "steward",
        "devops",
        Again::Persona("devops".to_owned()),
    )];
    assert_eq!(arrival_with(root, &Told::none()).waiting, waits);

    // A dispatch is judged: #1504 sets the acceptance aside. It waits the same way.
    crate::dispatchdormant::judged(root, &[]).expect("judged");
    assert_eq!(bound(root).seen, Vec::<String>::new());
    assert_eq!(arrival_with(root, &Told::none()).waiting, waits);

    // One answer: accepted here, it is not waiting to be given back in Settings as well.
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");
    assert_eq!(arrival_with(root, &Told::none()), Arrival::default());
    assert_eq!(local::accepted_aside_dispatch(root), []);
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

fn head_of(root: &Path) -> String {
    git(root, &["rev-parse", "HEAD"])
}

/// Commits everything in the project's folder, and answers the commit.
fn commit_all(root: &Path) -> String {
    git(root, &["add", "-A", "."]);
    git(root, &["commit", "-q", "--allow-empty", "-m", "settings"]);
    head_of(root)
}

/// Commits the project file as `text`, and answers the commit.
fn commit(root: &Path, text: &str) -> String {
    write(root, text);
    commit_all(root)
}

/// A project in a repository of its own, with one commit whose project file is `text`. The
/// repository is kept beside the project, so nothing of git's is written inside it.
fn repository(text: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let dir = project(text);
    let kept = tempfile::tempdir().expect("a place for the repository");
    let at = format!("--separate-git-dir={}", kept.path().join("git").display());
    git(dir.path(), &["init", "-q", &at]);
    commit_all(dir.path());
    (dir, kept)
}

/// A repository where steward to devops is committed and accepted.
fn accepted_in_a_repository() -> (tempfile::TempDir, tempfile::TempDir) {
    let made = repository(ONE);
    acknowledge_pair(made.0.path(), &pair("steward", "devops")).expect("accepted");
    assert_eq!(settled_in_force(made.0.path()), [pair("steward", "devops")]);
    made
}

/// The project's pairs in force after a settling against git, as the app settles before a
/// dispatch is decided.
fn settled_in_force(root: &Path) -> Vec<Pair> {
    settle(root);
    in_force(root)
}

/// The project's file under its other name.
fn other_name(root: &Path) -> std::path::PathBuf {
    let now = crate::names::manifest_name(root);
    let other = crate::names::PLANE_MANIFEST
        .spellings()
        .find(|name| *name != now)
        .expect("a second spelling");
    root.join(other)
}

#[test]
fn two_pulled_commits_that_took_a_pair_out_and_put_it_back_make_it_wait_for_a_new_yes() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();

    // Nothing reads the grants between the two commits.
    commit(root, NONE);
    commit(root, ONE);

    assert_eq!(settled_in_force(root), []);
    assert_eq!(
        arrival(root).waiting,
        [again("steward", "devops", Again::TakenOut)]
    );
}

#[test]
fn a_commit_that_leaves_the_grant_alone_keeps_the_acceptance_bound_to_it() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();

    let head = commit(root, TWO);

    assert_eq!(settled_in_force(root), [pair("steward", "devops")]);
    assert_eq!(bound(root).at, Some(head));
}

#[test]
fn a_grant_made_here_then_committed_then_taken_out_and_put_back_by_a_pull_waits_again() {
    let (dir, _kept) = repository(NONE);
    let root = dir.path();
    // Made here: in the file, accepted, and in no commit yet.
    write(root, ONE);
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");
    // Committed while nothing reads the grants, then a teammate's two commits.
    commit_all(root);
    commit(root, NONE);
    commit(root, ONE);

    assert_eq!(settled_in_force(root), []);
    assert_eq!(
        arrival(root).waiting,
        [again("steward", "devops", Again::TakenOut)]
    );
}

#[test]
fn a_removal_is_not_hidden_by_giving_the_project_s_file_its_other_name() {
    for back_under_the_first_name in [false, true] {
        let (dir, _kept) = accepted_in_a_repository();
        let root = dir.path();
        let first = crate::names::manifest(root);
        let second = other_name(root);

        // Taken out, and put back in a file under the other name.
        std::fs::remove_file(&first).expect("removed");
        std::fs::write(&second, NONE).expect("under the other name");
        commit_all(root);
        std::fs::write(&second, ONE).expect("put back");
        commit_all(root);
        if back_under_the_first_name {
            std::fs::remove_file(&second).expect("removed");
            std::fs::write(&first, ONE).expect("back");
            commit_all(root);
        }

        assert_eq!(settled_in_force(root), [], "{back_under_the_first_name}");
        assert_eq!(
            arrival(root).waiting,
            [again("steward", "devops", Again::TakenOut)]
        );
    }
}

#[test]
fn a_project_file_renamed_with_its_grants_kept_takes_nothing_out() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();
    let first = crate::names::manifest(root);
    let second = other_name(root);

    std::fs::remove_file(&first).expect("removed");
    std::fs::write(&second, ONE).expect("the same grants, under the other name");
    commit_all(root);

    assert_eq!(settled_in_force(root), [pair("steward", "devops")]);
    assert_eq!(arrival(root), Arrival::default());
}

#[test]
fn a_removal_on_a_line_that_was_merged_in_counts_though_the_merge_changed_nothing() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();

    // A teammate's line takes the pair out and puts it back; this machine has a commit of
    // its own, so the pull is a merge whose result is the file as it was.
    git(root, &["checkout", "-q", "-b", "theirs"]);
    commit(root, NONE);
    commit(root, ONE);
    git(root, &["checkout", "-q", "-"]);
    std::fs::write(root.join("notes.md"), "mine\n").expect("a file of this machine's");
    commit_all(root);
    git(root, &["merge", "-q", "--no-ff", "-m", "pull", "theirs"]);

    assert_eq!(settled_in_force(root), []);
}

#[test]
fn a_branch_that_predates_the_grant_and_changed_the_file_takes_nothing_out_when_merged() {
    let (dir, _kept) = repository(NONE);
    let root = dir.path();
    // An old branch, cut before the grant existed, that edits the project's file.
    git(root, &["checkout", "-q", "-b", "old"]);
    commit(root, "schema = 1\n# a note\n");
    git(root, &["checkout", "-q", "-"]);
    commit(root, ONE);
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    // Merged, keeping this line's file where the two differ.
    git(
        root,
        &[
            "merge", "-q", "--no-ff", "-X", "ours", "-m", "old work", "old",
        ],
    );

    assert_eq!(settled_in_force(root), [pair("steward", "devops")]);
    assert_eq!(arrival(root), Arrival::default());
}

#[test]
fn switching_to_a_branch_without_the_grant_and_back_drops_nothing_and_tells_nothing() {
    let (dir, _kept) = repository(NONE);
    let root = dir.path();
    git(root, &["branch", "before"]);
    commit(root, ONE);
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    git(root, &["checkout", "-q", "before"]);
    assert_eq!(settled_in_force(root), []);
    assert_eq!(arrival(root), Arrival::default());
    assert_eq!(bound(root).seen, [PAIR]);

    git(root, &["checkout", "-q", "-"]);
    assert_eq!(settled_in_force(root), [pair("steward", "devops")]);
    assert_eq!(arrival(root), Arrival::default());
}

#[test]
fn a_line_rewritten_so_that_no_commit_took_the_grant_out_keeps_the_acceptance() {
    // The limit, pinned: the check is over the history as this machine has it.
    let (dir, _kept) = repository(NONE);
    let root = dir.path();
    git(root, &["branch", "rewritten"]);
    commit(root, ONE);
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    // The line is pushed again from before the grant, with a commit of its own that adds it.
    git(root, &["checkout", "-q", "rewritten"]);
    commit(root, ONE);

    assert_eq!(settled_in_force(root), [pair("steward", "devops")]);
}

#[test]
fn a_history_that_shares_nothing_with_the_one_accepted_under_keeps_no_acceptance() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();

    git(root, &["checkout", "-q", "--orphan", "elsewhere"]);
    commit_all(root);

    assert_eq!(settled_in_force(root), []);
    assert_eq!(
        arrival(root).waiting,
        [again("steward", "devops", Again::Unread)]
    );
}

#[test]
fn an_object_that_stands_in_for_the_removing_commit_hides_nothing() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();
    let accepted = head_of(root);
    let removed = commit(root, NONE);
    commit(root, ONE);
    // Whoever can write the repository says: read the accepted commit where the removing
    // one is asked for.
    git(root, &["replace", &removed, &accepted]);

    assert_eq!(settled_in_force(root), []);
}

#[test]
fn a_repository_with_grafts_or_a_boundary_inside_the_range_is_a_history_that_cannot_be_read() {
    for cut in ["grafts", "shallow"] {
        let (dir, kept) = accepted_in_a_repository();
        let root = dir.path();
        let accepted = head_of(root);
        std::fs::write(root.join("notes.md"), "more\n").expect("a file");
        let head = commit_all(root);
        let git_dir = kept.path().join("git");
        match cut {
            "grafts" => {
                std::fs::create_dir_all(git_dir.join("info")).expect("info");
                std::fs::write(git_dir.join("info/grafts"), format!("{head} {accepted}\n"))
                    .expect("grafts");
            }
            // The newest commit is the boundary: what is under it is not there to read.
            _ => std::fs::write(git_dir.join("shallow"), format!("{head}\n")).expect("shallow"),
        }

        assert_eq!(settled_in_force(root), [], "{cut}");
        assert_eq!(
            arrival(root).waiting,
            [again("steward", "devops", Again::Unread)],
            "{cut}"
        );
    }
}

#[test]
fn a_version_of_the_project_s_file_past_the_size_cap_is_a_history_that_cannot_be_read() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();
    let pad = "# pad\n".repeat(usize::try_from(MOST_FILE_BYTES / 6).expect("fits") + 1);

    commit(root, &format!("{ONE}{pad}"));
    commit(root, ONE);

    assert_eq!(settled_in_force(root), []);
    assert_eq!(
        arrival(root).waiting,
        [again("steward", "devops", Again::Unread)]
    );
}

#[test]
fn a_stored_commit_that_is_no_commit_id_is_never_handed_to_git() {
    let (dir, _kept) = repository(ONE);
    let root = dir.path();
    let history = Git::new(root);
    let until = Instant::now() + DEADLINE;
    let Head::At(head) = history.head(until) else {
        panic!("a commit");
    };

    for forged in ["--output=x", "HEAD", "", "main..other", &"g".repeat(40)] {
        assert_eq!(history.between(forged, &head, until), Between::Unreadable);
    }
    // One git does not know: nothing is assumed to have stayed.
    assert_eq!(history.between(A, &head, until), Between::Unreadable);
}

#[test]
fn reading_the_grants_in_force_runs_no_git() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();
    settle(root);
    let before = crate::worktree::git::tally(root).spawned;

    for _ in 0..3 {
        assert_eq!(in_force(root), [pair("steward", "devops")]);
    }

    assert_eq!(crate::worktree::git::tally(root).spawned, before);
}

#[test]
fn a_settling_at_the_commit_it_was_last_checked_through_asks_git_one_thing() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();
    settle(root);
    let before = crate::worktree::git::tally(root).spawned;

    settle(root);

    assert_eq!(crate::worktree::git::tally(root).spawned, before + 1);
}

#[test]
fn a_settling_past_its_deadline_reads_no_history_and_keeps_no_acceptance() {
    let (dir, _kept) = accepted_in_a_repository();
    let root = dir.path();
    let accepted = head_of(root);
    let head = commit(root, TWO);
    let history = Git::new(root);
    assert_eq!(
        history.head(Instant::now() + DEADLINE),
        Head::At(head.clone())
    );

    assert_eq!(
        history.between(&accepted, &head, Instant::now()),
        Between::Unreadable
    );
}

#[test]
fn a_project_in_an_ordinary_clone_is_read_as_one_with_a_repository_kept_apart() {
    // The other tests keep the repository beside the project; this one is `git init` as a
    // person runs it.
    let dir = project(ONE);
    let root = dir.path();
    git(root, &["init", "-q"]);
    commit_all(root);
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    commit(root, NONE);
    commit(root, ONE);

    assert_eq!(settled_in_force(root), []);
    assert_eq!(
        arrival(root).waiting,
        [again("steward", "devops", Again::TakenOut)]
    );
}

#[test]
fn a_project_in_no_repository_has_no_history_and_its_acceptance_stands() {
    let dir = project(ONE);
    let root = dir.path();
    // A fixture's folder may sit inside some checkout of the machine's; only where it does
    // not is there no history.
    if Git::new(root).head(Instant::now() + DEADLINE) != Head::None {
        return;
    }
    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    assert_eq!(bound(root).at, None);
    assert_eq!(settled_in_force(root), [pair("steward", "devops")]);
}

// ---- a grant limited to one workspace is bound as a pair is (#1505 meets #1506) -----------------

const LIMITED: &str =
    "schema = 1\n\n[dispatch.grants]\nsteward = [{ to = \"devops\", in = \"runners\" }]\n";
const IN_RUNNERS: &str = "steward -> devops in runners";

fn limited_one() -> crate::dispatchwithin::Limited {
    crate::dispatchwithin::Limited::new("steward", "devops", "runners").expect("a limited grant")
}

/// The project's grants limited to one workspace in force, by the last settling.
fn limited_in_force(root: &Path) -> Vec<crate::dispatchwithin::Limited> {
    InForce::read(root, Vec::new())
        .limited
        .into_iter()
        .filter(|(level, _)| *level == crate::sandbox::grant::Level::Project)
        .map(|(_, one)| one)
        .collect()
}

/// A project holding the limited grant, with the workspace, accepted here at commit [`A`].
fn limited_accepted_at_a() -> tempfile::TempDir {
    let dir = project(LIMITED);
    let root = dir.path();
    std::fs::create_dir_all(root.join("workspaces").join("runners")).expect("the workspace");
    crate::dispatchwithin::accept(root, &limited_one()).expect("accepted");
    assert_eq!(
        settle_with(root, &Told::at(A, Between::Unanswered)),
        Verdict { read: true }
    );
    assert_eq!(bound(root).at.as_deref(), Some(A));
    assert_eq!(bound(root).seen_in, [IN_RUNNERS]);
    assert_eq!(limited_in_force(root), [limited_one()]);
    dir
}

#[test]
fn a_limited_grant_is_one_of_the_grants_a_version_of_the_project_s_file_holds() {
    assert_eq!(grants_in(Some(LIMITED)), [IN_RUNNERS]);
}

#[test]
fn a_limited_grant_taken_out_and_put_back_between_two_reads_waits_for_a_new_yes() {
    let dir = limited_accepted_at_a();
    let root = dir.path();

    // One pull: a commit takes it out, a later one puts it back. The file reads as it did.
    assert_eq!(
        settle_with(root, &Told::at(B, took_out(&[IN_RUNNERS]))),
        Verdict { read: true }
    );

    assert_eq!(limited_in_force(root), []);
    assert_eq!(bound(root).seen_in, Vec::<String>::new());
    assert_eq!(crate::dispatchwithin::unaccepted(root), [limited_one()]);
}

#[test]
fn a_limited_grant_is_not_in_force_where_the_history_cannot_be_read() {
    // Cannot be read: dropped, as a pair is.
    let dir = limited_accepted_at_a();
    let root = dir.path();
    assert_eq!(
        settle_with(root, &Told::at(B, Between::Unreadable)),
        Verdict { read: true }
    );
    assert_eq!(limited_in_force(root), []);
    assert_eq!(bound(root).seen_in, Vec::<String>::new());

    // Cannot be asked at all: nothing stored changes, and it is in force for nobody until a
    // settling answers. The arrival Notice says so.
    let dir = limited_accepted_at_a();
    let root = dir.path();
    let before = bound(root);
    let told = arrival_with(root, &Told::of(Head::Unanswered, Between::Unanswered));
    assert!(told.unread);
    assert_eq!(limited_in_force(root), [], "not in force for that read");
    assert_eq!(bound(root), before, "and nothing stored changed");
    assert_eq!(
        settle_with(root, &Told::at(B, took_out(&[]))),
        Verdict { read: true }
    );
    assert_eq!(limited_in_force(root), [limited_one()]);
}

#[test]
fn a_limited_grant_absent_from_the_file_on_disk_is_not_in_force_and_nothing_is_dropped() {
    let dir = limited_accepted_at_a();
    let root = dir.path();
    // A branch without it, and reads.
    write(root, NONE);
    assert_eq!(limited_in_force(root), []);
    assert_eq!(limited_in_force(root), []);
    assert_eq!(bound(root).seen_in, [IN_RUNNERS], "nothing dropped");
    // Back: in force with no new yes.
    write(root, LIMITED);
    assert_eq!(limited_in_force(root), [limited_one()]);
}

#[test]
fn what_was_set_aside_for_a_name_is_bound_to_the_history_too() {
    // Accepted, then set aside with the name's grants; a commit took it out meanwhile.
    for between in [took_out(&[PAIR]), Between::Unreadable] {
        let dir = project(ONE);
        let root = dir.path();
        std::fs::create_dir_all(local::path(root).parent().expect("a folder")).expect("made");
        std::fs::write(
            local::path(root),
            format!(
                "{{\"dispatch_accepted_aside\": [{{\"said\": \"{PAIR}\", \"was\": \"devops\"}}], \
                 \"dispatch_seen_at\": \"{A}\"}}"
            ),
        )
        .expect("the record");
        assert_eq!(bound(root).aside, [PAIR]);

        assert_eq!(
            settle_with(root, &Told::at(B, between)),
            Verdict { read: true }
        );

        assert_eq!(bound(root).aside, Vec::<String>::new());
        assert_eq!(local::accepted_aside_dispatch(root), []);
    }
}

#[test]
fn a_first_read_runs_no_git_on_its_thread_and_counts_nothing_accepted_until_a_settling_lands() {
    let dir = project(ONE);
    let root = dir.path();
    // Accepted by an earlier run of the app: nothing settled in this process yet.
    local::accept_dispatch(root, PAIR, &|| true).expect("accepted");

    // Not in force for this read: a settling was started off this thread.
    assert_eq!(for_read(root), Verdict { read: false });
    assert_eq!(in_force(root), []);

    // Once it lands, by the verdict it came to.
    let deadline = Instant::now() + Duration::from_secs(30);
    while for_read(root) == (Verdict { read: false }) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(for_read(root), Verdict { read: true });

    // A project that keeps nothing reads as settled at once.
    let empty = project(ONE);
    assert_eq!(for_read(empty.path()), Verdict { read: true });
}
