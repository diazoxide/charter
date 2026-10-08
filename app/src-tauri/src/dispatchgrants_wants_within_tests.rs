//! Where several pairs in one answer (#1502) meet a grant limited to one workspace (#1505):
//! every ticked pair is kept with the same workspace condition as the answer pressed, and a
//! box is offered only where a tick could be kept as a grant. Driven at [`Store`]. And where
//! a persona's `wants` meets Settings' table (#1504): the table is told it, and it grants
//! nothing there either.

use super::*;

/// Persona `name` of the project at `root`, finished, with `more` lines of frontmatter.
fn define(root: &Path, name: &str, more: &str) {
    let dir = root.join("personas").join(name);
    std::fs::create_dir_all(&dir).expect("made");
    std::fs::write(
        dir.join("persona.md"),
        format!("---\nname: {name}\nrole: {name}\nvault: none\n{more}---\n\n# {name}\n"),
    )
    .expect("written");
}

/// A project where steward wants devops, qa and docs, all four are personas, and runners and
/// web are workspaces.
fn wanting() -> World {
    let world = World::new();
    define(world.root(), "steward", "wants: [devops, qa, docs]\n");
    for name in ["devops", "qa", "docs"] {
        define(world.root(), name, "");
    }
    for name in ["runners", "web"] {
        std::fs::create_dir_all(world.root().join("workspaces").join(name)).expect("a workspace");
    }
    world
}

/// What `asking` is answered for `target`, for a task that works in `works_in`.
fn ask(
    world: &World,
    store: &Store,
    asking: Asking,
    target: &str,
    works_in: Option<&str>,
) -> Requested {
    world.on(|ground| {
        store
            .request_in(
                ground,
                asking,
                target,
                BRIEF,
                Uncovered::AskThePerson,
                works_in,
            )
            .0
    })
}

/// The held dispatch `id` of chat 3 as the window is told it.
fn shown(world: &World, store: &Store, id: u32) -> DispatchPending {
    let held = store
        .waiting(3)
        .into_iter()
        .find(|one| one.id == id)
        .expect("held");
    store.told(
        &PlaneId::for_tests(world.root()),
        world.root(),
        &world.locks,
        &held,
    )
}

fn boxes(told: &DispatchPending) -> Vec<&str> {
    told.also.iter().map(|one| one.persona.as_str()).collect()
}

fn ticked<'a>(also: &'a [String], shown: &'a str) -> Ticked<'a> {
    Ticked {
        also,
        shown: Some(shown),
    }
}

fn names(all: &[&str]) -> Vec<String> {
    all.iter().map(|one| (*one).to_owned()).collect()
}

/// Each grant audited, with the workspace it said: target, level, workspace.
fn granted(world: &World) -> Vec<(String, &'static str, Option<String>)> {
    world
        .audited()
        .into_iter()
        .zip(world.within.lock().unwrap().clone())
        .filter(|(one, _)| one.1 == "trust.dispatch.grant")
        .map(|(one, within)| (one.3, one.4, within))
        .collect()
}

fn is_held(asked: &Requested) -> bool {
    matches!(asked, Requested::NeedsGrant { .. })
}

#[test]
fn every_ticked_pair_is_kept_for_the_workspace_the_answer_holds_in_and_for_no_other() {
    let world = wanting();
    let (store, answered) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    let told = shown(&world, &store, id);
    assert_eq!(boxes(&told), ["docs", "qa"]);

    let also = names(&["docs", "qa"]);
    let said = world
        .on(|ground| store.allow_with(ground, id, Level::You, &ticked(&also, &told.shown)))
        .expect("allowed");

    assert_eq!(
        said,
        "Allowed for me on this machine, in runners. The dispatch starts now, and the next \
         one that works in runners starts without asking. Also allowed for me on this machine, \
         in runners: steward to docs and qa."
    );
    // Nothing of it holds everywhere: all three are kept for runners.
    assert!(sandbox::local::granted_dispatch(world.root()).is_empty());
    let kept: Vec<(String, String)> = sandbox::local::dispatch_mine_in(world.root())
        .into_iter()
        .map(|one| (one.target, one.workspace))
        .collect();
    assert_eq!(
        kept,
        [
            ("devops".to_owned(), "runners".to_owned()),
            ("docs".to_owned(), "runners".to_owned()),
            ("qa".to_owned(), "runners".to_owned()),
        ]
    );
    // Each audited as its own grant, each with the workspace.
    assert_eq!(
        granted(&world),
        [
            ("devops".to_owned(), "you", Some("runners".to_owned())),
            ("docs".to_owned(), "you", Some("runners".to_owned())),
            ("qa".to_owned(), "you", Some("runners".to_owned())),
        ]
    );
    assert_eq!(answered.lock().unwrap().len(), 1, "a tick starts no chat");
    // A ticked pair is covered for work in runners, and asked about anywhere else.
    assert!(matches!(
        ask(
            &world,
            &store,
            chat(4, Some("steward")),
            "qa",
            Some("runners")
        ),
        Requested::Covered(_)
    ));
    for elsewhere in [Some("web"), None] {
        assert!(
            is_held(&ask(
                &world,
                &store,
                chat(4, Some("steward")),
                "qa",
                elsewhere
            )),
            "{elsewhere:?}"
        );
    }
}

#[test]
fn ticked_with_the_wider_answer_every_pair_holds_in_any_workspace() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    let told = shown(&world, &store, id);

    let also = names(&["qa"]);
    let said = world
        .on(|ground| store.allow_anywhere_with(ground, id, Level::You, &ticked(&also, &told.shown)))
        .expect("allowed");

    assert_eq!(
        said,
        "Allowed for me on this machine, in any workspace. The dispatch starts now, and the \
         next one starts without asking. Also allowed for me on this machine, in any \
         workspace: steward to qa."
    );
    assert!(sandbox::local::dispatch_mine_in(world.root()).is_empty());
    assert_eq!(
        sandbox::local::granted_dispatch(world.root()),
        [
            ("steward".to_owned(), "devops".to_owned()),
            ("steward".to_owned(), "qa".to_owned()),
        ]
    );
    assert_eq!(
        granted(&world),
        [
            ("devops".to_owned(), "you", None),
            ("qa".to_owned(), "you", None),
        ]
    );
    assert!(matches!(
        ask(&world, &store, chat(4, Some("steward")), "qa", Some("web")),
        Requested::Covered(_)
    ));
}

#[test]
fn ticked_for_this_chat_each_pair_is_that_chat_s_for_the_task_s_workspace() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    let told = shown(&world, &store, id);

    let also = names(&["qa"]);
    let said = world
        .on(|ground| store.allow_with(ground, id, Level::Chat, &ticked(&also, &told.shown)))
        .expect("allowed");

    assert_eq!(
        said,
        "Allowed for this chat. The dispatch starts now, and the next one from this chat that \
         works in runners starts without asking. Also allowed for this chat: steward to qa."
    );
    assert_eq!(
        granted(&world),
        [
            ("devops".to_owned(), "chat", Some("runners".to_owned())),
            ("qa".to_owned(), "chat", Some("runners".to_owned())),
        ]
    );
    // That chat, in that workspace; not that chat elsewhere, and not another chat.
    assert!(matches!(
        ask(
            &world,
            &store,
            chat(3, Some("steward")),
            "qa",
            Some("runners")
        ),
        Requested::Covered(_)
    ));
    assert!(is_held(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "qa",
        Some("web")
    )));
    assert!(is_held(&ask(
        &world,
        &store,
        chat(4, Some("steward")),
        "qa",
        Some("runners")
    )));
}

#[test]
fn a_box_is_offered_where_the_pair_is_granted_for_another_workspace_and_not_for_this_one() {
    let world = wanting();
    let (store, _) = store();
    // qa is mine for web only; docs is mine everywhere.
    purlis_core::dispatchwithin::grant_yours(
        world.root(),
        &Pair::new("steward", "qa").unwrap(),
        &purlis_core::dispatchwithin::Within::of(Some("web")),
    )
    .expect("a grant for web");
    sandbox::local::grant_dispatch(world.root(), "steward", "docs").expect("a grant everywhere");

    let in_runners = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    assert_eq!(boxes(&shown(&world, &store, in_runners)), ["qa"]);
    let in_web = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("web"),
    ));
    assert!(boxes(&shown(&world, &store, in_web)).is_empty());
}

#[test]
fn no_box_is_offered_and_no_tick_is_kept_where_the_task_s_workspace_is_not_there_yet() {
    let world = wanting();
    let (store, answered) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("fresh"),
    ));
    let told = shown(&world, &store, id);
    assert!(told.works_in_missing);
    assert!(boxes(&told).is_empty(), "nothing a tick could be kept as");

    // A tick sent all the same is kept as nothing, at either answer, and said.
    let also = names(&["qa"]);
    let said = world
        .on(|ground| store.allow_anywhere_with(ground, id, Level::You, &ticked(&also, &told.shown)))
        .expect("the one dispatch starts");
    assert_eq!(
        said,
        "fresh is not a workspace of this project yet, so this starts this one dispatch and \
         keeps no grant. The next one asks you. Not allowed, since the question no longer \
         offers it: qa."
    );
    assert_eq!(answered.lock().unwrap().len(), 1);
    assert!(granted(&world).is_empty(), "no grant was audited");
    assert!(sandbox::local::granted_dispatch(world.root()).is_empty());
    assert!(sandbox::local::dispatch_mine_in(world.root()).is_empty());
}

#[test]
fn no_box_is_offered_for_a_task_in_a_workspace_while_the_list_of_nevers_does_not_read() {
    let world = wanting();
    let (store, _) = store();
    let path = purlis_core::dispatchnever::path(world.root());
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    std::fs::write(&path, "{ not json").expect("written");
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    let told = shown(&world, &store, id);
    assert!(told.never_unread.is_some());
    assert!(boxes(&told).is_empty());
    // A forced tick keeps no pair: only the asked one is audited, and it starts once.
    let also = names(&["qa"]);
    world
        .on(|ground| store.allow_with(ground, id, Level::You, &ticked(&also, &told.shown)))
        .expect("the one dispatch starts");
    assert_eq!(
        granted(&world),
        [("devops".to_owned(), "you", Some("runners".to_owned()))]
    );
}

#[test]
fn a_pair_kept_blocked_in_the_chat_is_no_box_there_whichever_workspace_the_next_task_works_in() {
    let world = wanting();
    let (store, _) = store();
    let qa = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "qa",
        Some("runners"),
    ));
    assert!(store.keep_blocked(qa));
    // Keep blocked is per chat and persona, not per workspace: no box in web either.
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("web"),
    ));
    assert_eq!(boxes(&shown(&world, &store, id)), ["docs"]);
}

#[test]
fn settings_table_is_told_what_each_persona_wants_and_nothing_is_in_force_by_it() {
    let world = wanting();
    // A line that offers nothing is not told: itself, a name that is no persona, the star.
    define(world.root(), "qa", "wants: [qa, ghost, *]\n");
    let (store, _) = store();

    let standing = standing_read(world.root(), &store);

    assert_eq!(
        standing.wants,
        [DispatchWants {
            persona: "steward".to_owned(),
            wants: vec!["devops".to_owned(), "qa".to_owned(), "docs".to_owned()],
        }]
    );
    // Told, and no more: nothing is granted, listed as a grant, or in force by it.
    assert!(world.listed(&store).is_empty());
    assert!(standing.any.is_empty());
    assert!(world.audited().is_empty());
    assert!(is_held(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "qa",
        Some("runners")
    )));
}

// ---- the train's review: a box never undoes a decline, and an answer is held to all it said --

/// [`wanting`], with the project's file granting steward the pairs `targets`.
fn wanting_with_project(targets: &[&str]) -> World {
    let world = wanting();
    let list: Vec<String> = targets.iter().map(|one| format!("\"{one}\"")).collect();
    std::fs::write(
        purlis_core::names::manifest(world.root()),
        format!(
            "schema = 1\n\n[dispatch.grants]\nsteward = [{}]\n",
            list.join(", ")
        ),
    )
    .expect("the project's file");
    world
}

#[test]
fn a_pair_declined_on_this_machine_is_no_box_and_a_forced_tick_does_not_accept_it() {
    let world = wanting_with_project(&["qa"]);
    dispatchgrant::decline(world.root(), "steward", "qa").expect("Not on my machine");
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    let told = shown(&world, &store, id);
    assert_eq!(boxes(&told), ["docs"], "qa was declined here");

    // A tick sent for it all the same, at the project's level, keeps nothing of it.
    let also = names(&["qa"]);
    let said = world
        .on(|ground| store.allow_with(ground, id, Level::Project, &ticked(&also, &told.shown)))
        .expect("the asked pair is allowed");
    assert!(
        said.ends_with("Not allowed, since the question no longer offers it: qa."),
        "{said}"
    );
    assert!(
        dispatchgrant::declined(world.root()).contains(&"steward -> qa".to_owned()),
        "still declined"
    );
    assert!(
        !granted(&world).iter().any(|(target, ..)| target == "qa"),
        "nothing was audited for it"
    );
}

#[test]
fn an_answer_is_held_to_whether_the_workspace_was_there_and_to_the_answers_offered() {
    // Drawn while the workspace was not there: one answer, which keeps nothing.
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("fresh"),
    ));
    let told = shown(&world, &store, id);
    assert!(told.works_in_missing);
    // It is made before the press: the question reads differently, so nothing is kept.
    std::fs::create_dir_all(world.root().join("workspaces/fresh")).expect("made");
    assert_eq!(
        world.on(|ground| store.allow_with(ground, id, Level::Chat, &ticked(&[], &told.shown))),
        Err(CHANGED.to_owned())
    );
    assert!(granted(&world).is_empty());

    // An answer the question does not offer is refused whatever the window sends: here the
    // project's level for a pair this machine declined.
    let world = wanting_with_project(&["devops"]);
    dispatchgrant::decline(world.root(), "steward", "devops").expect("Not on my machine");
    let (store, _) = super::store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    let told = shown(&world, &store, id);
    assert!(!told.levels.contains(&GrantLevel::Project));
    assert_eq!(
        world.on(|ground| {
            store.allow_with(ground, id, Level::Project, &ticked(&[], &told.shown))
        }),
        Err(
            "That answer is not one this question offers, so nothing was allowed. Read it \
             again, then answer."
                .to_owned()
        )
    );
    assert!(granted(&world).is_empty());
    assert!(dispatchgrant::declined(world.root()).contains(&"steward -> devops".to_owned()));
}
