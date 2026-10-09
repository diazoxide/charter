//! A teammate's project grant, when it arrives (#1506): what waits when the project opens and
//! when its file moves, what Accept and Not on my machine do and audit, that no Notice's
//! answer accepts "any persona", an answer that comes after the list moved, and a grant a
//! commit took out and put back. Driven at the functions the window's commands and the
//! watcher call.

use super::table::{audit, file, kinds, known, persona, world_with};
use super::*;

const ONE: &str = "\n[dispatch.grants]\nsteward = [\"devops\"]\n";
const TEAMS: &str = "\n[dispatch.grants]\nsteward = [\"devops\", \"*\"]\nqa = [\"devops\"]\n";

fn write(world: &World, grants: &str) {
    std::fs::write(
        purlis_core::names::manifest(world.root()),
        format!("schema = 1\n{grants}"),
    )
    .expect("the project file");
}

/// What waits, each as `asking -> target`.
fn waiting(world: &World) -> Vec<String> {
    arrival_of(world.root())
        .waiting
        .iter()
        .map(|one| format!("{} -> {}", one.asking, one.target))
        .collect()
}

/// The ids the Notice would send for what waits: every grant, or the named pairs only.
fn ids(world: &World, pairs_only: bool) -> Vec<String> {
    arrival_of(world.root())
        .waiting
        .into_iter()
        .filter(|one| !(pairs_only && one.any))
        .map(|one| one.id)
        .collect()
}

fn answer(
    world: &World,
    store: &Store,
    accepted: bool,
    shown: &[String],
) -> Result<Option<String>, String> {
    answer_of(world, store, accepted, shown, &ids(world, false))
}

/// An answer for `shown` from a Notice that listed `listed`.
fn answer_of(
    world: &World,
    store: &Store,
    accepted: bool,
    shown: &[String],
    listed: &[String],
) -> Result<Option<String>, String> {
    let known = known(world);
    world.on(|ground| answer_arrival(store, ground, &known, accepted, shown, listed))
}

/// The grants in force, by the last settling of this machine's acceptances.
fn in_force(world: &World) -> InForce {
    InForce::read(world.root(), Vec::new())
}

/// The grants in force after a settling, as the dispatch path settles before it decides.
fn settled_in_force(world: &World) -> InForce {
    purlis_core::dispatcharrival::settle(world.root());
    in_force(world)
}

// ---- arrival -----------------------------------------------------------------------------------

#[test]
fn what_the_project_grants_is_waiting_when_the_project_opens_and_in_force_for_nobody() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, answered) = store();

    // The first read, as the window makes it when the project opens.
    let arrival = arrival_of(world.root());

    assert_eq!(
        arrival
            .waiting
            .iter()
            .map(|one| (one.asking.as_str(), one.target.as_str(), one.any))
            .collect::<Vec<_>>(),
        [
            ("steward", "*", true),
            ("steward", "devops", false),
            ("qa", "devops", false)
        ]
    );
    assert!(
        arrival
            .waiting
            .iter()
            .all(|one| one.undefined.is_none() && one.again.is_none())
    );
    assert_eq!(arrival.gone, []);
    assert!(!arrival.unread);
    // Nothing listed is in force: a chat that needs one meanwhile is held, and a chat nobody
    // is at is refused.
    assert_eq!(in_force(&world), InForce::default());
    pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    assert!(matches!(
        world
            .request_unattended(&store, chat(4, Some("steward")), "qa", BRIEF)
            .0,
        Requested::Refused(_)
    ));
    assert!(answered.lock().unwrap().is_empty());
    assert!(world.audited().is_empty(), "reading it answers nothing");
}

#[test]
fn a_pair_the_file_gains_is_waiting_at_the_next_read_and_the_watcher_knows_which_changes_matter() {
    let world = world_with(&["steward", "devops"], "");
    assert_eq!(arrival_of(world.root()), DispatchArrival::default());

    write(&world, ONE);

    assert_eq!(waiting(&world), ["steward -> devops"]);
    assert_eq!(in_force(&world), InForce::default());
    // The project's own file, and a burst the watcher could not place, are settled for; a
    // todo is not.
    let classify = |path: std::path::PathBuf| {
        purlis_core::planechange::classify(world.root(), &path).expect("a path of the project's")
    };
    let its_file = classify(purlis_core::names::manifest(world.root()));
    let a_todo = classify(world.root().join("workspaces/alpha/todos/one.md"));
    assert!(concerns_grants(Some(&[a_todo.clone(), its_file])));
    assert!(!concerns_grants(Some(&[a_todo])));
    assert!(concerns_grants(None));
}

#[test]
fn a_grant_naming_a_persona_the_project_does_not_define_is_said_to_and_never_accepted() {
    let world = world_with(
        &["steward", "devops"],
        "\n[dispatch.grants]\nsteward = [\"ghost\"]\n",
    );
    let (store, _) = store();
    let shown = ids(&world, false);
    assert_eq!(
        arrival_of(world.root()).waiting[0].undefined.as_deref(),
        Some("ghost")
    );

    assert_eq!(answer(&world, &store, true, &shown), Ok(None));

    assert!(world.audited().is_empty(), "nothing was accepted");
    assert_eq!(in_force(&world), InForce::default());
    assert_eq!(waiting(&world), ["steward -> ghost"]);

    // The persona arrives later: the grant is shown another way now, so an answer to the
    // earlier list does not accept it.
    persona(&world, "ghost");
    let said = answer(&world, &store, true, &shown).expect("answered");
    assert!(said.is_some_and(|said| said.starts_with("Nothing was accepted")));
    assert_eq!(in_force(&world), InForce::default());
    assert_eq!(arrival_of(world.root()).waiting[0].undefined, None);
}

// ---- accept, and not on my machine -------------------------------------------------------------

#[test]
fn accept_puts_the_named_pairs_in_force_audits_each_writes_no_file_and_starts_what_waited() {
    let world = world_with(&["steward", "devops", "qa", "prod"], TEAMS);
    let (store, answered) = store();
    let held = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let before = file(&world);

    assert_eq!(answer(&world, &store, true, &ids(&world, true)), Ok(None));

    assert_eq!(
        kinds(&world),
        [
            audit("trust.dispatch.grant", "steward", "devops", "project"),
            audit("trust.dispatch.grant", "qa", "devops", "project"),
        ]
    );
    // The person's, from the window: no chat is named as having made it.
    assert!(world.audited().iter().all(|one| one.0.is_none()));
    let now = in_force(&world);
    assert_eq!(now.project.len(), 2);
    assert_eq!(now.project_any, Vec::<String>::new());
    assert_eq!(
        file(&world),
        before,
        "accepting writes nothing to the team's file"
    );
    // The project's "any persona" still waits: told, and accepted in Settings only.
    assert_eq!(waiting(&world), ["steward -> *"]);
    // Answering here cleared the question on the chat's tab: its dispatch started.
    assert_eq!(
        answered
            .lock()
            .unwrap()
            .iter()
            .map(|one| (one.pending.id, one.allowed.is_some()))
            .collect::<Vec<_>>(),
        [(held, true)]
    );
    assert!(store.waiting(3).is_empty());
    assert!(matches!(
        world
            .request_unattended(&store, chat(4, Some("qa")), "devops", BRIEF)
            .0,
        Requested::Covered(_)
    ));
}

#[test]
fn no_answer_to_the_arrival_notice_accepts_any_persona() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, _) = store();
    let before = file(&world);
    let star: Vec<String> = arrival_of(world.root())
        .waiting
        .into_iter()
        .filter(|one| one.any)
        .map(|one| one.id)
        .collect();
    assert_eq!(star.len(), 1);

    // Alone, and among the pairs: refused whole, in the command, before anything is audited.
    for shown in [star.clone(), ids(&world, false)] {
        assert_eq!(
            answer(&world, &store, true, &shown),
            Err(ANY_IS_SETTINGS.to_owned())
        );
    }

    assert!(world.audited().is_empty());
    assert_eq!(in_force(&world), InForce::default());
    assert_eq!(file(&world), before);
    assert_eq!(waiting(&world).len(), 3);
    // Not on my machine may answer it: declining narrows.
    assert_eq!(answer(&world, &store, false, &star), Ok(None));
    assert_eq!(waiting(&world), ["steward -> devops", "qa -> devops"]);
}

#[test]
fn not_on_my_machine_is_remembered_audited_shown_in_settings_and_never_asked_again() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, answered) = store();
    let before = file(&world);

    assert_eq!(answer(&world, &store, false, &ids(&world, false)), Ok(None));

    assert_eq!(
        kinds(&world),
        [
            audit("trust.dispatch.decline", "steward", "*", "project"),
            audit("trust.dispatch.decline", "steward", "devops", "project"),
            audit("trust.dispatch.decline", "qa", "devops", "project"),
        ]
    );
    assert_eq!(file(&world), before, "the team's file is not touched");
    assert_eq!(in_force(&world), InForce::default());
    assert!(answered.lock().unwrap().is_empty());
    // Not told again, at the next open or the next read.
    assert_eq!(arrival_of(world.root()), DispatchArrival::default());
    // Settings' table is where each is changed.
    let rows = world.listed(&store);
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter().all(|one| one.waiting && one.declined),
        "{rows:?}"
    );
    let any = standing_of(world.root()).any;
    assert_eq!(any.len(), 1);
    assert!(any[0].declined);
    // A chat that needs the pair is still asked on its own tab, and is not offered the
    // project's level there: that would undo the decline by another name.
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let asked = store.waiting(3);
    assert_eq!(asked[0].id, id);
    assert_eq!(
        told(&PlaneId::for_tests(world.root()), world.root(), &asked[0]).levels,
        [GrantLevel::Chat, GrantLevel::You]
    );
}

#[test]
fn an_answer_nobody_recorded_puts_nothing_in_force() {
    let world = World {
        logging: false,
        ..world_with(&["steward", "devops"], ONE)
    };
    let (store, _) = store();

    assert!(answer(&world, &store, true, &ids(&world, true)).is_err());

    assert_eq!(in_force(&world), InForce::default());
    assert_eq!(waiting(&world), ["steward -> devops"]);
}

// ---- an answer that comes late -----------------------------------------------------------------

#[test]
fn a_late_accept_answers_only_what_is_still_as_shown_and_says_the_list_moved() {
    let world = world_with(&["steward", "devops", "qa", "prod"], TEAMS);
    let (store, _) = store();
    let shown = ids(&world, true);
    let listed = ids(&world, false);

    // While the Notice stands, a pull takes one pair out, leaves one, and adds two grants.
    write(
        &world,
        "\n[dispatch.grants]\nsteward = [\"devops\", \"prod\"]\nqa = [\"*\"]\n",
    );

    let said = answer_of(&world, &store, true, &shown, &listed).expect("answered");

    assert_eq!(
        said.as_deref(),
        Some(
            "The project's dispatch grants changed after this was shown, so only what was \
             still as shown was accepted. This is what waits now."
        )
    );
    assert_eq!(
        kinds(&world),
        [audit(
            "trust.dispatch.grant",
            "steward",
            "devops",
            "project"
        )]
    );
    assert_eq!(in_force(&world).project.len(), 1);
    assert_eq!(waiting(&world), ["qa -> *", "steward -> prod"]);
}

#[test]
fn a_late_answer_to_a_list_that_is_wholly_gone_answers_nothing_and_says_so() {
    let world = world_with(&["steward", "devops", "qa"], ONE);
    let (store, _) = store();
    let shown = ids(&world, true);
    write(&world, "\n[dispatch.grants]\nsteward = [\"qa\"]\n");

    for (accepted, word) in [(true, "accepted"), (false, "declined")] {
        assert_eq!(
            answer(&world, &store, accepted, &shown),
            Ok(Some(format!(
                "Nothing was {word}: the project's dispatch grants changed after this was \
                 shown. This is what waits now."
            )))
        );
    }
    assert!(world.audited().is_empty());
    assert_eq!(in_force(&world), InForce::default());
    assert_eq!(waiting(&world), ["steward -> qa"]);
}

#[test]
fn an_answer_names_what_was_shown_and_no_other_spelling_of_a_grant_is_one() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, _) = store();

    // The grants as the record spells them, a bare star, and nothing at all: none is an id
    // the Notice showed.
    let forged: Vec<String> = ["steward -> *", "steward -> devops", "*", "", "qa -> devops"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let said = answer(&world, &store, true, &forged).expect("answered");

    assert!(said.is_some_and(|said| said.starts_with("Nothing was accepted")));
    assert!(world.audited().is_empty());
    assert_eq!(in_force(&world), InForce::default());
}

// ---- away on disk, and taken out by a commit ---------------------------------------------------

#[test]
fn a_grant_the_file_on_disk_loses_and_gets_back_drops_nothing_and_tells_nothing() {
    let world = world_with(&["steward", "devops"], ONE);
    let (store, _) = store();
    answer(&world, &store, true, &ids(&world, true)).expect("accepted");
    assert_eq!(in_force(&world).project.len(), 1);
    let kept = sandbox::local::dispatch_bound(world.root());

    // A branch without it is checked out, purlis open.
    write(&world, "");
    assert_eq!(arrival_of(world.root()), DispatchArrival::default());
    assert_eq!(settled_in_force(&world), InForce::default());
    pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    assert_eq!(sandbox::local::dispatch_bound(world.root()), kept);

    // And back: in force as it was, and nobody is asked about the project's grant again.
    write(&world, ONE);
    assert_eq!(arrival_of(world.root()), DispatchArrival::default());
    assert_eq!(settled_in_force(&world).project.len(), 1);
    assert_eq!(world.audited().len(), 1, "one acceptance, and no more");
}

/// git in `world`'s project, for a fixture.
fn git(world: &World, args: &[&str]) -> String {
    let mut all = vec![
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    all.extend(args);
    let run = purlis_core::worktree::git::run_untimed(world.root(), &all).expect("git runs");
    assert_eq!(run.code, Some(0), "git {args:?}: {}", run.err);
    run.out.trim().to_owned()
}

/// Makes `world`'s project a repository kept beside it, with one commit of what is there.
fn committed(world: &World) -> tempfile::TempDir {
    let kept = tempfile::tempdir().expect("a place for the repository");
    let nothing = kept.path().join("template");
    std::fs::create_dir_all(&nothing).expect("an empty template");
    git(
        world,
        &[
            "init",
            "-q",
            &format!("--template={}", nothing.display()),
            &format!("--separate-git-dir={}", kept.path().join("git").display()),
        ],
    );
    commit(world);
    kept
}

fn commit(world: &World) {
    git(world, &["add", "-A", "."]);
    git(world, &["commit", "-q", "--allow-empty", "-m", "settings"]);
}

#[test]
fn a_pair_two_commits_took_out_and_put_back_waits_for_a_new_yes_and_says_why() {
    let world = world_with(&["steward", "devops"], ONE);
    let _kept = committed(&world);
    let (store, _) = store();
    answer(&world, &store, true, &ids(&world, true)).expect("accepted");
    assert_eq!(settled_in_force(&world).project.len(), 1);

    // Two commits arrive together. Nothing reads the grants in between, and the file reads
    // as it did when the pair was accepted.
    write(&world, "");
    commit(&world);
    write(&world, ONE);
    commit(&world);

    assert_eq!(settled_in_force(&world), InForce::default());
    pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let arrival = arrival_of(world.root());
    assert_eq!(arrival.gone, []);
    assert_eq!(arrival.waiting.len(), 1);
    assert_eq!(arrival.waiting[0].again, Some(DispatchAgain::TakenOut));

    // The new yes is a new audit.
    answer(&world, &store, true, &ids(&world, true)).expect("accepted again");
    assert_eq!(
        kinds(&world),
        [
            audit("trust.dispatch.grant", "steward", "devops", "project"),
            audit("trust.dispatch.grant", "steward", "devops", "project"),
        ]
    );
    assert_eq!(in_force(&world).project.len(), 1);
}

#[test]
fn what_a_commit_took_away_is_settled_off_the_watcher_s_thread_told_once_and_asks_nothing() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let _kept = committed(&world);
    let (store, _) = store();
    answer(&world, &store, true, &ids(&world, true)).expect("accepted");

    write(&world, "\n[dispatch.grants]\nqa = [\"devops\"]\n");
    commit(&world);
    // The watcher's event: it comes back at once, and the settling lands on its own thread.
    project_moved(&PlaneId::for_tests(world.root()), world.root(), None);
    let landed = (0..600).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        sandbox::local::dispatch_bound(world.root()).seen == ["qa -> devops"]
    });
    assert!(landed, "the settling after the watcher's event landed");

    let arrival = arrival_of(world.root());
    assert_eq!(arrival.waiting, []);
    assert_eq!(
        arrival
            .gone
            .iter()
            .map(|one| (one.asking.as_str(), one.target.as_str(), one.any))
            .collect::<Vec<_>>(),
        [("steward", "devops", false)]
    );
    assert_eq!(
        arrival_of(world.root()),
        arrival,
        "it stands until it is read"
    );
    let audited = world.audited().len();

    let shown: Vec<String> = arrival.gone.into_iter().map(|one| one.id).collect();
    purlis_core::dispatcharrival::told_gone(world.root(), &shown).expect("read");

    assert_eq!(arrival_of(world.root()), DispatchArrival::default());
    assert_eq!(world.audited().len(), audited, "reading it is no answer");
    assert_eq!(
        in_force(&world).project.len(),
        1,
        "what stayed is still accepted"
    );
}

// ---- a persona that went and came back ---------------------------------------------------------

#[test]
fn a_grant_naming_a_name_that_changed_hands_waits_here_again_and_one_answer_settles_it() {
    let world = world_with(&["steward", "devops"], ONE);
    let (store, _) = store();
    answer(&world, &store, true, &ids(&world, true)).expect("accepted");
    purlis_core::dispatchdormant::judged(world.root(), &[]).expect("judged with both there");

    // devops is removed, seen gone, and another persona is made under the name by a pull.
    std::fs::remove_dir_all(world.root().join("personas/devops")).expect("removed");
    purlis_core::dispatchdormant::noticed(world.root(), &[]).expect("seen gone");
    let made = world.root().join("personas/devops");
    std::fs::create_dir_all(&made).expect("its folder");
    std::fs::write(
        made.join("persona.md"),
        "---\nname: devops\ndelegate-when: other work\n---\n\n# another devops\n",
    )
    .expect("another definition");

    let arrival = arrival_of(world.root());
    assert_eq!(arrival.waiting.len(), 1);
    assert_eq!(
        arrival.waiting[0].again,
        Some(DispatchAgain::Persona {
            name: "devops".to_owned()
        })
    );

    // Accepted on the Notice: in force for the persona of that name now, and not waiting to
    // be given back in Settings as well.
    let shown: Vec<String> = arrival.waiting.into_iter().map(|one| one.id).collect();
    assert_eq!(answer(&world, &store, true, &shown), Ok(None));
    assert_eq!(arrival_of(world.root()), DispatchArrival::default());
    assert_eq!(
        sandbox::local::dispatch_bound(world.root()).seen,
        ["steward -> devops"]
    );
    assert_eq!(sandbox::local::accepted_aside_dispatch(world.root()), []);
}

// ---- what the target works with (#1502's words, joined on train 64) -----------------------------

#[test]
fn a_waiting_pair_says_what_its_target_works_with_in_the_question_s_own_words() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let told = arrival_of(world.root());
    let of = |asking: &str, target: &str| {
        told.waiting
            .iter()
            .find(|one| one.asking == asking && one.target == target)
            .unwrap_or_else(|| panic!("{asking} -> {target} waits"))
            .works_with
            .clone()
    };
    // The one function for the words: what the dispatch question says of the same persona.
    let said = purlis_core::dispatchwants::Access::at(world.root(), &world.locks, "devops").said();
    assert!(
        said.starts_with("devops works with its own access: "),
        "{said}"
    );
    assert_eq!(of("steward", "devops"), Some(said.clone()));
    assert_eq!(of("qa", "devops"), Some(said));
    // "Any persona" names no one persona, so there is no one sentence for it.
    assert_eq!(of("steward", "*"), None);

    // And a grant naming a persona the project does not define can be used by nothing.
    let lone = world_with(&["steward"], ONE);
    let told = arrival_of(lone.root());
    assert_eq!(told.waiting[0].undefined.as_deref(), Some("devops"));
    assert_eq!(told.waiting[0].works_with, None);
}

#[test]
fn a_yes_that_comes_after_what_the_target_works_with_changed_answers_nothing() {
    let world = world_with(&["steward", "devops", "qa"], ONE);
    let (store, _) = store();
    let before = arrival_of(world.root());
    let shown = ids(&world, true);
    assert_eq!(shown.len(), 1);

    // While the Notice stands, devops comes to dispatch onward to qa without asking: a yes
    // to steward -> devops now reaches qa too, which the Notice did not say.
    sandbox::local::grant_dispatch(world.root(), "devops", "qa").expect("a grant of mine");
    let after = arrival_of(world.root());
    assert_ne!(
        before.waiting[0].works_with, after.waiting[0].works_with,
        "the sentence says the onward reach"
    );
    assert!(
        after.waiting[0]
            .works_with
            .as_deref()
            .is_some_and(|said| said.contains("may itself dispatch to qa")),
        "{:?}",
        after.waiting[0].works_with
    );
    assert_ne!(before.waiting[0].id, after.waiting[0].id);

    assert_eq!(
        answer(&world, &store, true, &shown),
        Ok(Some(
            "Nothing was accepted: the project's dispatch grants changed after this was \
             shown. This is what waits now."
                .to_owned()
        ))
    );
    assert!(
        world.audited().is_empty(),
        "nothing was accepted or audited"
    );
    assert!(in_force(&world).project.is_empty());
    assert_eq!(waiting(&world), ["steward -> devops"]);

    // Answered as it reads now, it is accepted.
    assert_eq!(answer(&world, &store, true, &ids(&world, true)), Ok(None));
    assert_eq!(in_force(&world).project.len(), 1);
}
