//! A teammate's project grant, when it arrives (#1506): what waits when the project opens and
//! when the watcher says its file moved, what Accept and Not on my machine do and audit, an
//! answer that comes after the list moved, and a grant taken away and put back. Driven at the
//! functions the window's commands and the watcher call.

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

/// The watcher saying the project's own file moved.
fn the_file_moved(world: &World) {
    let moved = purlis_core::planechange::classify(
        world.root(),
        &purlis_core::names::manifest(world.root()),
    )
    .expect("a path of the project's");
    assert_eq!(moved.kind, purlis_core::planechange::Kind::Project);
    project_moved(world.root(), Some(&[moved]));
}

/// What waits, each as `asking -> target`.
fn waiting(world: &World) -> Vec<String> {
    arrival_of(world.root())
        .waiting
        .iter()
        .map(|one| format!("{} -> {}", one.asking, one.target))
        .collect()
}

fn ids(world: &World) -> Vec<String> {
    arrival_of(world.root())
        .waiting
        .into_iter()
        .map(|one| one.id)
        .collect()
}

fn answer(
    world: &World,
    store: &Store,
    accepted: bool,
    shown: &[String],
) -> Result<Option<String>, String> {
    let known = known(world);
    world.on(|ground| answer_arrival(store, ground, &known, accepted, shown))
}

fn in_force(world: &World) -> InForce {
    InForce::read(world.root(), Vec::new())
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
            .all(|one| one.undefined.is_none() && !one.again)
    );
    assert_eq!(arrival.gone, []);
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
fn a_pair_a_pull_brings_in_is_waiting_once_the_watcher_says_the_file_moved() {
    let world = world_with(&["steward", "devops"], "");
    assert_eq!(arrival_of(world.root()), DispatchArrival::default());

    write(&world, ONE);
    the_file_moved(&world);

    assert_eq!(waiting(&world), ["steward -> devops"]);
    assert_eq!(in_force(&world), InForce::default());
}

#[test]
fn a_grant_naming_a_persona_the_project_does_not_define_is_said_to_and_never_accepted() {
    let world = world_with(
        &["steward", "devops"],
        "\n[dispatch.grants]\nsteward = [\"ghost\"]\n",
    );
    let (store, _) = store();
    let shown = ids(&world);
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
fn accept_puts_everything_listed_in_force_audits_each_and_starts_what_waited() {
    let world = world_with(&["steward", "devops", "qa", "prod"], TEAMS);
    let (store, answered) = store();
    let held = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let before = file(&world);

    assert_eq!(answer(&world, &store, true, &ids(&world)), Ok(None));

    assert_eq!(
        kinds(&world),
        [
            audit("trust.dispatch.grant", "steward", "*", "project"),
            audit("trust.dispatch.grant", "steward", "devops", "project"),
            audit("trust.dispatch.grant", "qa", "devops", "project"),
        ]
    );
    // The person's, from the window: no chat is named as having made it.
    assert!(world.audited().iter().all(|one| one.0.is_none()));
    let now = in_force(&world);
    assert_eq!(now.project_any, ["steward"]);
    assert_eq!(now.project.len(), 2);
    assert_eq!(
        file(&world),
        before,
        "accepting writes nothing to the team's file"
    );
    assert_eq!(arrival_of(world.root()), DispatchArrival::default());
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
fn not_on_my_machine_is_remembered_audited_shown_in_settings_and_never_asked_again() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, answered) = store();
    let before = file(&world);

    assert_eq!(answer(&world, &store, false, &ids(&world)), Ok(None));

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
    // Not told again, at the next open or the next time the file moves.
    assert_eq!(arrival_of(world.root()), DispatchArrival::default());
    the_file_moved(&world);
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
    // A chat that needs the pair is still asked on its own tab.
    pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
}

#[test]
fn an_answer_nobody_recorded_puts_nothing_in_force() {
    let world = World {
        logging: false,
        ..world_with(&["steward", "devops"], ONE)
    };
    let (store, _) = store();

    assert!(answer(&world, &store, true, &ids(&world)).is_err());

    assert_eq!(in_force(&world), InForce::default());
    assert_eq!(waiting(&world), ["steward -> devops"]);
}

// ---- an answer that comes late -----------------------------------------------------------------

#[test]
fn a_late_accept_answers_only_what_is_still_as_shown_and_says_the_list_moved() {
    let world = world_with(&["steward", "devops", "qa", "prod"], TEAMS);
    let (store, _) = store();
    let shown = ids(&world);

    // While the Notice stands, a pull takes one grant out, leaves one, and adds two.
    write(
        &world,
        "\n[dispatch.grants]\nsteward = [\"devops\", \"prod\"]\nqa = [\"*\"]\n",
    );

    let said = answer(&world, &store, true, &shown).expect("answered");

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
    let now = in_force(&world);
    assert_eq!(now.project.len(), 1);
    assert_eq!(
        now.project_any,
        Vec::<String>::new(),
        "no star the person did not see"
    );
    assert_eq!(waiting(&world), ["qa -> *", "steward -> prod"]);
}

#[test]
fn a_late_answer_to_a_list_that_is_wholly_gone_answers_nothing_and_says_so() {
    let world = world_with(&["steward", "devops", "qa"], ONE);
    let (store, _) = store();
    let shown = ids(&world);
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

// ---- taken away, and put back ------------------------------------------------------------------

#[test]
fn a_pair_taken_out_and_put_back_with_nothing_read_in_between_waits_for_a_new_yes() {
    let world = world_with(&["steward", "devops"], ONE);
    let (store, _) = store();
    answer(&world, &store, true, &ids(&world)).expect("accepted");
    assert_eq!(in_force(&world).project.len(), 1);

    // The file loses the pair and gets it back. Nothing reads the grants in between: only
    // the watcher sees each change.
    write(&world, "");
    the_file_moved(&world);
    write(&world, ONE);
    the_file_moved(&world);

    assert_eq!(in_force(&world), InForce::default());
    pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let arrival = arrival_of(world.root());
    assert_eq!(arrival.gone, []);
    assert_eq!(arrival.waiting.len(), 1);
    assert!(arrival.waiting[0].again, "said to be asked a second time");

    // The new yes is a new audit.
    answer(&world, &store, true, &ids(&world)).expect("accepted again");
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
fn a_change_that_is_not_the_project_s_file_settles_nothing() {
    let world = world_with(&["steward", "devops"], ONE);
    let (store, _) = store();
    answer(&world, &store, true, &ids(&world)).expect("accepted");
    write(&world, "");

    let todo = world.root().join("workspaces/alpha/todos/one.md");
    let moved = purlis_core::planechange::classify(world.root(), &todo).expect("a todo");
    project_moved(world.root(), Some(&[moved]));

    assert_eq!(
        sandbox::local::dispatch_bound(world.root()).seen,
        ["steward -> devops"],
        "not read, so not settled yet"
    );
    // A burst the watcher could not place may be the file: settled.
    project_moved(world.root(), None);
    assert_eq!(
        sandbox::local::dispatch_bound(world.root()).seen,
        Vec::<String>::new()
    );
}

#[test]
fn what_the_project_took_away_is_told_once_and_asks_nothing() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, _) = store();
    answer(&world, &store, true, &ids(&world)).expect("accepted");

    write(&world, "\n[dispatch.grants]\nqa = [\"devops\"]\n");
    the_file_moved(&world);

    let arrival = arrival_of(world.root());
    assert_eq!(arrival.waiting, []);
    assert_eq!(
        arrival
            .gone
            .iter()
            .map(|one| (one.asking.as_str(), one.target.as_str(), one.any))
            .collect::<Vec<_>>(),
        [("steward", "*", true), ("steward", "devops", false)]
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
