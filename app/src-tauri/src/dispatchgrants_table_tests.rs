//! Settings' table of dispatch grants (#1504): every grant with where it comes from, what each
//! action takes back and what it leaves alone, a project grant accepted and declined on this
//! machine, the pairs chats are kept blocked for, and what becomes of a removed persona's
//! grants. Driven at the functions the window's commands call.

use super::*;

/// Writes the persona `name` into `world`'s project, as a hand or a pull would.
pub(super) fn persona(world: &World, name: &str) {
    let dir = world.root().join("personas").join(name);
    std::fs::create_dir_all(&dir).expect("its folder");
    std::fs::write(
        dir.join("persona.md"),
        format!("---\nname: {name}\ndelegate-when: {name} work\n---\n\n# {name}\n"),
    )
    .expect("its definition");
}

/// A world whose project has a project file holding `grants` and these personas.
pub(super) fn world_with(personas: &[&str], grants: &str) -> World {
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

pub(super) fn file(world: &World) -> String {
    std::fs::read_to_string(purlis_core::names::manifest(world.root())).expect("the file")
}

pub(super) fn known(world: &World) -> impl Fn(&str) -> bool {
    let personas = purlis_core::dispatchdormant::personas_of(world.root()).unwrap_or_default();
    move |name: &str| personas.iter().any(|one| one == name)
}

/// The kinds audited, with who and what, in order.
pub(super) fn kinds(world: &World) -> Vec<(&'static str, String, String, &'static str)> {
    world
        .audited()
        .into_iter()
        .map(|one| (one.1, one.2.unwrap_or_default(), one.3, one.4))
        .collect()
}

pub(super) fn audit(
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
            workspace: None,
            nowhere: None,
            id: None,
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
    // Declined here is not told as arrived again; what nobody answered still is.
    assert_eq!(
        arrival_of(world.root())
            .waiting
            .iter()
            .map(|one| format!("{} -> {}", one.asking, one.target))
            .collect::<Vec<_>>(),
        ["steward -> *", "qa -> devops"]
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
        world.on(|ground| give_back(ground.root, "steward", ground.audit)),
        Err(
            "purlis changed nothing: nothing is waiting to be given back to that persona."
                .to_owned()
        )
    );
    assert_eq!(
        std::fs::read_to_string(purlis_core::names::manifest(world.root())).expect("the file"),
        "schema = 1\n",
        "no Accept writes the team's file"
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

// ---- a persona that is away, and another under its name ----------------------------------------

fn gone(world: &World, name: &str) {
    std::fs::remove_dir_all(world.root().join("personas").join(name)).expect("removed by hand");
}

/// Another persona under `name`: a definition that is not the one `persona` writes.
fn another(world: &World, name: &str) {
    persona(world, name);
    let file = world.root().join("personas").join(name).join("persona.md");
    let text = std::fs::read_to_string(&file).expect("its definition");
    std::fs::write(&file, format!("{text}\nAnother persona altogether.\n")).expect("rewritten");
}

fn judge(world: &World, store: &Store) {
    world.on(|ground| bring_up_to_date(ground.root, store, ground.audit));
}

/// devops with a pair of mine, "any persona", an accepted project pair and a grant for one of
/// its chats; a dispatch has been judged with it there.
fn devops_with_everything() -> (World, Store) {
    let world = world_with(
        &["steward", "devops", "qa"],
        "\n[dispatch.grants]\nqa = [\"devops\"]\n",
    );
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("devops")), "steward", BRIEF));
    world.allow(&store, id, Level::Chat).expect("this chat");
    let id = pending_of(&world.request(&store, chat(4, Some("steward")), "devops", BRIEF));
    world.allow(&store, id, Level::You).expect("me");
    sandbox::local::grant_dispatch_any(world.root(), "devops").expect("any");
    world
        .on(|ground| accept(&store, ground, &known(&world), "qa", "devops"))
        .expect("the project's, accepted here");
    judge(&world, &store);
    (world, store)
}

#[test]
fn reading_the_table_while_a_persona_is_away_moves_nothing_and_drops_nothing() {
    let (world, store) = devops_with_everything();
    let listed = world.listed(&store);
    assert_eq!(listed.len(), 3);
    let audited = world.audited().len();
    gone(&world, "devops");

    // Settings is opened on a branch without it, twice, and a dispatch is judged there.
    let standing = standing_read(world.root(), &store);
    judge(&world, &store);
    let again = standing_read(world.root(), &store);

    assert_eq!(standing, again);
    assert_eq!(standing.dormant, Vec::<DispatchDormant>::new());
    assert_eq!(standing.returned, Vec::<String>::new());
    assert_eq!(standing.back, Vec::<String>::new());
    assert_eq!(
        standing.personas,
        Some(vec!["qa".to_owned(), "steward".to_owned()])
    );
    // Every grant is where it was, with when and from which chat, and the chat's own too.
    assert_eq!(world.listed(&store), listed);
    assert_eq!(dispatchgrant::any_yours(world.root()), ["devops"]);
    assert_eq!(
        arrival_of(world.root()).waiting,
        [],
        "nothing accepted here was forgotten"
    );
    assert_eq!(
        world.audited().len(),
        audited,
        "and nothing was recorded, for nothing changed"
    );

    // Back on the branch that has it: the same persona, nothing to answer, all in force.
    persona(&world, "devops");
    let back = standing_read(world.root(), &store);
    assert_eq!(back.returned, Vec::<String>::new());
    judge(&world, &store);
    assert_eq!(world.listed(&store), listed);
    assert!(matches!(
        world.request(&store, chat(4, Some("steward")), "devops", BRIEF),
        Requested::Covered(_)
    ));
    assert_eq!(world.audited().len(), audited);
}

#[test]
fn another_persona_under_a_name_seen_gone_is_set_aside_as_a_dispatch_is_judged_and_recorded() {
    let (world, store) = devops_with_everything();
    gone(&world, "devops");
    judge(&world, &store);
    another(&world, "devops");
    let audited = world.audited().len();

    // Read first: it is said to be back and waiting, and the read moves nothing.
    let waiting = standing_read(world.root(), &store);
    assert_eq!(waiting.returned, ["devops"]);
    assert_eq!(waiting.back, ["devops"]);
    assert_eq!(waiting.dormant, Vec::<DispatchDormant>::new());
    assert_eq!(world.listed(&store).len(), 3);
    assert_eq!(world.audited().len(), audited);

    judge(&world, &store);

    // Nothing an earlier devops was allowed is in force, the grant for one chat included.
    assert!(
        world
            .listed(&store)
            .iter()
            .all(|one| one.level == GrantLevel::Project && one.waiting),
        "{:?}",
        world.listed(&store)
    );
    let now = InForce::read(world.root(), Vec::new());
    assert!(now.you.is_empty() && now.you_any.is_empty() && now.project.is_empty());
    pending_of(&world.request(&store, chat(4, Some("steward")), "devops", BRIEF));
    // Each is in the log as taken back, at the level it was kept.
    assert_eq!(
        kinds(&world)[audited..],
        [
            audit("trust.dispatch.revoke", "steward", "devops", "you"),
            audit("trust.dispatch.revoke", "devops", "*", "you"),
            audit("trust.dispatch.revoke", "qa", "devops", "project"),
        ]
    );
    let standing = standing_read(world.root(), &store);
    assert_eq!(standing.returned, Vec::<String>::new());
    assert_eq!(standing.back, ["devops"]);
    assert_eq!(standing.dormant.len(), 2);

    // One acknowledgement for the name: recorded first, then each grant that came back.
    let before = world.audited().len();
    world
        .on(|ground| give_back(ground.root, "devops", ground.audit))
        .expect("given back");
    assert_eq!(
        kinds(&world)[before..],
        [
            audit("trust.dispatch.give_back", "devops", "devops", "you"),
            audit("trust.dispatch.grant", "steward", "devops", "you"),
            audit("trust.dispatch.grant", "devops", "*", "you"),
            audit("trust.dispatch.grant", "qa", "devops", "project"),
        ]
    );
    let standing = standing_read(world.root(), &store);
    assert_eq!(standing.back, Vec::<String>::new());
    assert_eq!(standing.dormant, Vec::<DispatchDormant>::new());
    let mine = world
        .listed(&store)
        .into_iter()
        .find(|one| one.level == GrantLevel::You)
        .expect("mine is back");
    assert_eq!(
        mine.chat.as_deref(),
        Some("steward 4"),
        "with the chat it came from"
    );
    assert_eq!(arrival_of(world.root()).waiting, []);
}

#[test]
fn nothing_is_given_back_that_nobody_recorded_or_to_a_name_that_is_no_persona() {
    let (mut world, store) = devops_with_everything();
    gone(&world, "devops");
    judge(&world, &store);
    another(&world, "devops");
    judge(&world, &store);

    world.logging = false;
    assert!(
        world
            .on(|ground| give_back(ground.root, "devops", ground.audit))
            .is_err()
    );
    assert_eq!(standing_of(world.root()).dormant.len(), 2);
    assert!(dispatchgrant::yours(world.root()).is_empty());

    world.logging = true;
    gone(&world, "devops");
    assert_eq!(
        world.on(|ground| give_back(ground.root, "devops", ground.audit)),
        Err(
            "This project has no persona named devops, so its grants stay as they are. Make \
             the persona first, or remove them."
                .to_owned()
        )
    );
    assert_eq!(standing_of(world.root()).dormant.len(), 2);
}

#[cfg(unix)]
#[test]
fn a_decline_that_could_not_be_kept_is_recorded_as_followed_again() {
    use std::os::unix::fs::PermissionsExt;
    let world = world_with(&["steward", "devops", "qa"], TEAMS);
    let (store, _) = store();
    world
        .on(|ground| accept(&store, ground, &known(&world), "steward", "devops"))
        .expect("accepted");
    let folder = sandbox::local::path(world.root())
        .parent()
        .expect("a folder")
        .to_path_buf();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).expect("read-only");
    if std::fs::write(folder.join("probe"), "").is_ok() {
        // Run as a user no permission stops: there is no failed write to see here.
        return;
    }

    let refused = world.on(|ground| decline(ground.root, "steward", "devops", ground.audit));

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).expect("restored");
    assert!(refused.is_err());
    assert_eq!(
        kinds(&world)[1..],
        [
            audit("trust.dispatch.decline", "steward", "devops", "project"),
            audit("trust.dispatch.grant", "steward", "devops", "project"),
        ]
    );
    assert_eq!(
        InForce::read(world.root(), Vec::new()).project.len(),
        1,
        "still followed, as the log now says"
    );
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
