//! Settings' table of dispatch grants (#1504): every grant with where it comes from, what each
//! action takes back and what it leaves alone, a project grant accepted and declined on this
//! machine, the pairs chats are kept blocked for, and what becomes of a removed persona's
//! grants. Driven at the functions the window's commands call.

use super::*;

/// Writes the persona `name` into `world`'s project, as a hand or a pull would.
fn persona(world: &World, name: &str) {
    let dir = world.root().join("personas").join(name);
    std::fs::create_dir_all(&dir).expect("its folder");
    std::fs::write(
        dir.join("persona.md"),
        format!("---\nname: {name}\ndelegate-when: {name} work\n---\n\n# {name}\n"),
    )
    .expect("its definition");
}

/// A world whose project has a project file holding `grants` and these personas.
fn world_with(personas: &[&str], grants: &str) -> World {
    let world = World::new();
    std::fs::write(
        purlis_core::names::manifest(world.root()),
        format!("schema = 1\n{grants}"),
    )
    .expect("the project file");
    for name in personas {
        persona(&world, name);
    }
    world
}

fn file(world: &World) -> String {
    std::fs::read_to_string(purlis_core::names::manifest(world.root())).expect("the file")
}

fn known(world: &World) -> impl Fn(&str) -> bool {
    let personas = purlis_core::dispatchdormant::personas_of(world.root()).unwrap_or_default();
    move |name: &str| personas.iter().any(|one| one == name)
}

/// The kinds audited, with who and what, in order.
fn kinds(world: &World) -> Vec<(&'static str, String, String, &'static str)> {
    world
        .audited()
        .into_iter()
        .map(|one| (one.1, one.2.unwrap_or_default(), one.3, one.4))
        .collect()
}

fn audit(
    kind: &'static str,
    asking: &str,
    target: &str,
    level: &'static str,
) -> (&'static str, String, String, &'static str) {
    (kind, asking.to_owned(), target.to_owned(), level)
}

/// One listed grant: who asks, the target, the level, the chat, and whether it waits.
type Listed = (Option<String>, String, GrantLevel, Option<String>, bool);

const TEAMS: &str = "\n[dispatch.grants]\nsteward = [\"devops\", \"*\"]\nqa = [\"devops\"]\n";

// ---- every grant, with where it comes from ------------------------------------------------------

#[test]
fn the_table_is_told_every_grant_with_its_source_and_what_stands_beside_them() {
    let world = world_with(&["steward", "devops", "qa", "prod"], TEAMS);
    let (store, _) = store();
    // One pair, granted three ways: for a chat, for me, and by the project.
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    world.allow(&store, id, Level::Chat).expect("this chat");
    sandbox::local::grant_dispatch(world.root(), "steward", "devops").expect("me");
    world
        .on(|ground| accept(&store, ground, &known(&world), "steward", "devops"))
        .expect("the project's, accepted here");
    // Another chat is kept blocked for a pair, and one pair is refused for good.
    let kept = pending_of(&world.request(&store, chat(4, Some("qa")), "prod", BRIEF));
    assert!(store.keep_blocked(kept));
    dispatchgrant::never(world.root(), &Pair::new("devops", "prod").expect("a pair"))
        .expect("never");

    let listed: Vec<Listed> = world
        .listed(&store)
        .into_iter()
        .map(|one| (one.asking, one.target, one.level, one.chat, one.waiting))
        .collect();
    let steward = Some("steward".to_owned());
    assert_eq!(
        listed,
        [
            (
                steward.clone(),
                "devops".to_owned(),
                GrantLevel::Chat,
                Some("steward 3".to_owned()),
                false
            ),
            (
                steward.clone(),
                "devops".to_owned(),
                GrantLevel::You,
                None,
                false
            ),
            (
                steward,
                "devops".to_owned(),
                GrantLevel::Project,
                None,
                false
            ),
            (
                Some("qa".to_owned()),
                "devops".to_owned(),
                GrantLevel::Project,
                None,
                true
            ),
        ]
    );
    let standing = standing_read(world.root(), &store);
    assert_eq!(
        standing.personas,
        Some(vec![
            "devops".to_owned(),
            "prod".to_owned(),
            "qa".to_owned(),
            "steward".to_owned()
        ])
    );
    assert_eq!(
        standing.any,
        [DispatchAny {
            asking: "steward".to_owned(),
            level: GrantLevel::Project,
            waiting: true,
            declined: false,
        }]
    );
    assert_eq!(
        standing.nevers,
        [DispatchNever {
            asking: "devops".to_owned(),
            target: "prod".to_owned()
        }]
    );
    assert_eq!(
        standing.kept_blocked,
        [DispatchKeptBlocked {
            chat: "qa 4".to_owned(),
            asking: Some("qa".to_owned()),
            target: "prod".to_owned(),
        }]
    );
    assert_eq!(standing.dormant, Vec::<DispatchDormant>::new());
    assert_eq!(standing.nevers_unread, None);
    // Reading the table changed nothing that stands.
    assert_eq!(world.listed(&store).len(), 4);
}

#[test]
fn a_pair_kept_blocked_for_a_chat_is_listed_until_that_chat_closes() {
    let world = World::new();
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let other = pending_of(&world.request(&store, chat(3, Some("steward")), "qa", BRIEF));
    assert!(store.keep_blocked(id));
    assert!(store.keep_blocked(other));
    assert_eq!(
        store
            .kept_blocked()
            .into_iter()
            .map(|one| (one.chat, one.target))
            .collect::<Vec<_>>(),
        [
            ("steward 3".to_owned(), "devops".to_owned()),
            ("steward 3".to_owned(), "qa".to_owned())
        ]
    );

    store.chat_closed(3, Some("chat-3"));

    assert_eq!(store.kept_blocked(), Vec::<DispatchKeptBlocked>::new());
}

#[test]
fn the_table_says_the_list_of_nevers_does_not_read_and_names_no_never() {
    let world = world_with(&["steward", "devops"], "");
    let (store, _) = store();
    sandbox::local::grant_dispatch(world.root(), "steward", "devops").expect("me");
    nevers_by_hand(&world, "not json");

    let standing = standing_read(world.root(), &store);

    assert!(
        standing
            .nevers_unread
            .as_deref()
            .is_some_and(|said| said
                .starts_with("purlis could not read the list of pairs you said never to")),
        "{:?}",
        standing.nevers_unread
    );
    assert_eq!(standing.nevers, Vec::<DispatchNever>::new());
    // The grant is still listed, and still covers nothing while the list does not read.
    assert_eq!(world.listed(&store).len(), 1);
    pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
}

// ---- taking one back stops new dispatches only --------------------------------------------------

#[test]
fn taking_a_grant_back_stops_the_next_dispatch_and_reaches_no_task_that_is_running() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, answered) = store();
    let known = known(&world);
    // Three ways steward is covered: a pair of mine, the project's pair, and the project's
    // "any persona", each accepted here.
    sandbox::local::grant_dispatch(world.root(), "steward", "qa").expect("me");
    world
        .on(|ground| accept(&store, ground, &known, "steward", "devops"))
        .expect("accepted");
    world
        .on(|ground| accept(&store, ground, &known, "steward", "*"))
        .expect("accepted");
    // A task is started under them, and another dispatch of another chat waits on the person.
    assert!(matches!(
        world.request(&store, chat(3, Some("steward")), "devops", BRIEF),
        Requested::Covered(_)
    ));
    let waiting = pending_of(&world.request(&store, chat(4, Some("qa")), "steward", BRIEF));
    let told = answered.lock().unwrap().len();

    world
        .on(|ground| decline(ground.root, "steward", "*", ground.audit))
        .expect("not on my machine");
    world
        .on(|ground| decline(ground.root, "steward", "devops", ground.audit))
        .expect("not on my machine");
    let mine = world
        .listed(&store)
        .into_iter()
        .find(|one| one.level == GrantLevel::You)
        .expect("mine");
    world
        .on(|ground| revoke(ground.root, &store, &mine.id, ground.audit))
        .expect("revoked");

    // Nothing was said to any chat: no answer, no stop, and what waited still waits.
    assert_eq!(answered.lock().unwrap().len(), told);
    assert_eq!(store.waiting(4).len(), 1);
    assert_eq!(store.waiting(4)[0].id, waiting);
    // The next dispatch across each pair asks.
    for target in ["devops", "qa"] {
        pending_of(&world.request(&store, chat(3, Some("steward")), target, BRIEF));
    }
}

// ---- the project's grants, on this machine ------------------------------------------------------

#[test]
fn not_on_my_machine_is_audited_leaves_the_committed_file_and_is_undone_by_accept() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, _) = store();
    let known = known(&world);
    let before = file(&world);
    world
        .on(|ground| accept(&store, ground, &known, "steward", "devops"))
        .expect("accepted");
    assert!(matches!(
        world.request(&store, chat(3, Some("steward")), "devops", BRIEF),
        Requested::Covered(_)
    ));

    world
        .on(|ground| decline(ground.root, "steward", "devops", ground.audit))
        .expect("declined");

    assert_eq!(file(&world), before, "the team's file is not touched");
    let row = world
        .listed(&store)
        .into_iter()
        .find(|one| one.asking.as_deref() == Some("steward"))
        .expect("still listed");
    assert!(row.waiting && row.declined);
    assert_eq!(
        changed_of(world.root()).map(|one| one.added),
        Some(vec!["qa -> devops".to_owned()])
    );
    pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    assert_eq!(
        kinds(&world),
        [
            audit("trust.dispatch.grant", "steward", "devops", "project"),
            audit("trust.dispatch.decline", "steward", "devops", "project"),
        ]
    );

    world
        .on(|ground| accept(&store, ground, &known, "steward", "devops"))
        .expect("accepted again");
    let row = world
        .listed(&store)
        .into_iter()
        .find(|one| one.asking.as_deref() == Some("steward"))
        .expect("listed");
    assert!(!row.waiting && !row.declined);
    assert_eq!(kinds(&world).len(), 3);
}

#[test]
fn a_teammate_s_any_persona_is_shown_waiting_and_is_accepted_or_declined_in_settings() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, _) = store();
    let known = known(&world);
    let of_the_project = |world: &World| standing_of(world.root()).any;
    assert!(of_the_project(&world)[0].waiting && !of_the_project(&world)[0].declined);

    world
        .on(|ground| decline(ground.root, "steward", "*", ground.audit))
        .expect("declined");
    assert!(of_the_project(&world)[0].waiting && of_the_project(&world)[0].declined);
    pending_of(&world.request(&store, chat(3, Some("steward")), "qa", BRIEF));

    world
        .on(|ground| accept(&store, ground, &known, "steward", "*"))
        .expect("accepted");
    assert!(!of_the_project(&world)[0].waiting && !of_the_project(&world)[0].declined);
    // The dispatch that waited on the person is covered now, and a persona added later is too.
    assert!(store.waiting(3).is_empty());
    persona(&world, "added-later");
    assert!(matches!(
        world.request(&store, chat(5, Some("steward")), "added-later", BRIEF),
        Requested::Covered(_)
    ));
    assert_eq!(
        kinds(&world),
        [
            audit("trust.dispatch.decline", "steward", "*", "project"),
            audit("trust.dispatch.grant", "steward", "*", "project"),
        ]
    );
}

#[test]
fn a_row_that_is_no_longer_there_is_refused_with_a_sentence_and_nothing_is_audited() {
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, _) = store();
    let known = known(&world);
    // The table was drawn, and then the project's file changed underneath it.
    std::fs::write(purlis_core::names::manifest(world.root()), "schema = 1\n").expect("a pull");
    let gone = Err("purlis changed nothing: the project no longer has that grant.".to_owned());

    for target in ["devops", "*"] {
        assert_eq!(
            world.on(|ground| decline(ground.root, "steward", target, ground.audit)),
            gone
        );
        assert_eq!(
            world.on(|ground| accept(&store, ground, &known, "steward", target)),
            gone
        );
    }
    assert_eq!(
        world.on(|ground| revive(ground.root, "steward", "devops", ground.audit)),
        Err("purlis changed nothing: that grant is no longer there.".to_owned())
    );
    assert!(world.audited().is_empty());
    assert!(world.listed(&store).is_empty());
    assert!(
        !sandbox::local::path(world.root()).exists(),
        "nothing written"
    );
}

#[test]
fn nothing_is_accepted_for_a_name_that_is_no_persona_of_the_project_or_unrecorded() {
    let world = world_with(
        &["steward", "devops"],
        "\n[dispatch.grants]\nsteward = [\"devops\", \"ghost\"]\nghost = [\"*\", \"devops\"]\n",
    );
    let (store, _) = store();
    let known = known(&world);
    for (asking, target) in [
        ("steward", "ghost"),
        ("ghost", "*"),
        ("ghost", "devops"),
        ("*", "devops"),
        ("*", "*"),
        ("steward", "steward -> devops"),
    ] {
        let refused = world
            .on(|ground| accept(&store, ground, &known, asking, target))
            .expect_err("refused");
        assert!(
            refused.starts_with("This project has no persona named "),
            "{asking} to {target}: {refused}"
        );
    }
    assert!(world.audited().is_empty());
    assert!(InForce::read(world.root(), Vec::new()).project.is_empty());

    // And one nobody recorded puts nothing in force.
    let unheard = World {
        logging: false,
        ..world_with(&["steward", "devops"], TEAMS)
    };
    let known = self::known(&unheard);
    for target in ["devops", "*"] {
        assert!(
            unheard
                .on(|ground| accept(&store, ground, &known, "steward", target))
                .is_err()
        );
        assert!(
            unheard
                .on(|ground| decline(ground.root, "steward", target, ground.audit))
                .is_err()
        );
    }
    let now = InForce::read(unheard.root(), Vec::new());
    assert!(now.project.is_empty() && now.project_any.is_empty());
    assert!(dispatchgrant::declined(unheard.root()).is_empty());
}

// ---- a persona that is gone ---------------------------------------------------------------------

#[test]
fn a_removed_persona_s_grants_allow_nothing_and_wait_set_aside_for_the_person() {
    let world = world_with(&["steward", "devops", "qa"], "");
    let (store, _) = store();
    // devops holds a pair of mine, "any persona", and a grant made for one of its chats.
    let id = pending_of(&world.request(&store, chat(3, Some("devops")), "steward", BRIEF));
    world.allow(&store, id, Level::Chat).expect("this chat");
    sandbox::local::grant_dispatch(world.root(), "devops", "qa").expect("me");
    sandbox::local::grant_dispatch(world.root(), "steward", "devops").expect("me");
    sandbox::local::grant_dispatch_any(world.root(), "devops").expect("any");

    // Removed by hand, with its chat still open: the table is read, and nothing is left.
    std::fs::remove_dir_all(world.root().join("personas/devops")).expect("removed");
    let standing = standing_read(world.root(), &store);

    assert_eq!(
        standing
            .dormant
            .iter()
            .map(|one| (
                one.asking.as_str(),
                one.target.as_str(),
                one.any,
                one.revivable
            ))
            .collect::<Vec<_>>(),
        [
            ("devops", "qa", false, false),
            ("steward", "devops", false, false),
            ("devops", "*", true, false)
        ]
    );
    assert!(
        world.listed(&store).is_empty(),
        "the chat's grant ended too"
    );
    for target in ["qa", "steward"] {
        pending_of(&world.request(&store, chat(3, Some("devops")), target, BRIEF));
    }
    assert_eq!(
        world.audited().len(),
        1,
        "the chat's Allow, and nothing since"
    );

    // While the name is no persona's, nothing is given back.
    let refused = world
        .on(|ground| revive(ground.root, "devops", "*", ground.audit))
        .expect_err("refused");
    assert_eq!(
        refused,
        "This project has no persona named devops, so the grant stays set aside. Remove it, or \
         make the persona first."
    );

    // A persona made under the name inherits nothing, until the person gives each back.
    persona(&world, "devops");
    let standing = standing_read(world.root(), &store);
    assert!(standing.dormant.iter().all(|one| one.revivable));
    let now = InForce::read(world.root(), Vec::new());
    assert!(now.you.is_empty() && now.you_any.is_empty());

    world
        .on(|ground| revive(ground.root, "devops", "*", ground.audit))
        .expect("given back");
    assert_eq!(
        world.audited().last().cloned(),
        Some((
            None,
            "trust.dispatch.grant",
            Some("devops".to_owned()),
            "*".to_owned(),
            "you"
        ))
    );
    assert_eq!(dispatchgrant::any_yours(world.root()), ["devops"]);
    assert_eq!(dispatchgrant::yours(world.root()), Vec::<Pair>::new());
    assert_eq!(standing_of(world.root()).dormant.len(), 2);
}

#[test]
fn a_grant_given_back_that_nobody_recorded_stays_set_aside() {
    let mut world = world_with(&["steward", "devops"], "");
    sandbox::local::grant_dispatch(world.root(), "steward", "devops").expect("me");
    purlis_core::dispatchdormant::persona_gone(world.root(), "devops").expect("set aside");
    world.logging = false;

    assert!(
        world
            .on(|ground| revive(ground.root, "steward", "devops", ground.audit))
            .is_err()
    );
    assert_eq!(dispatchgrant::yours(world.root()), Vec::<Pair>::new());
    assert_eq!(standing_of(world.root()).dormant.len(), 1);
}

#[test]
fn a_grant_made_for_one_chat_ends_when_a_persona_it_names_is_removed_through_the_app() {
    let world = World::new();
    let (store, _) = store();
    for (session, asking, target) in [
        (3, "steward", "devops"),
        (4, "devops", "qa"),
        (5, "steward", "qa"),
    ] {
        let id = pending_of(&world.request(&store, chat(session, Some(asking)), target, BRIEF));
        world.allow(&store, id, Level::Chat).expect("this chat");
    }

    assert_eq!(store.persona_gone("devops"), 2);

    let left: Vec<(Option<String>, String)> = world
        .listed(&store)
        .into_iter()
        .map(|one| (one.asking, one.target))
        .collect();
    assert_eq!(left, [(Some("steward".to_owned()), "qa".to_owned())]);
    pending_of(&world.request(&store, chat(4, Some("devops")), "qa", BRIEF));
}
