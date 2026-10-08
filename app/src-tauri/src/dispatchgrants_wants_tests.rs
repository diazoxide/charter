//! What a persona wants, and several pairs in one answer (#1502): that the line grants
//! nothing, which boxes the question offers and what it says a persona works with, what one
//! Allow with boxes ticked keeps and audits, and that the question is read again when it is
//! answered. Driven at [`Store`], as the tests beside it are.

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

/// A project where steward wants devops, qa and docs, and all four are personas.
fn wanting() -> World {
    let world = World::new();
    define(world.root(), "steward", "wants: [devops, qa, docs]\n");
    for name in ["devops", "qa", "docs"] {
        define(world.root(), name, "");
    }
    world
}

/// The held dispatch `id` as the window is told it.
fn shown(world: &World, store: &Store, id: u32) -> DispatchPending {
    let held = store
        .waiting(3)
        .into_iter()
        .chain(store.waiting(4))
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

/// Allow at `level` with `also` ticked, saying the question read as `shown`.
fn allow_with(
    world: &World,
    store: &Store,
    id: u32,
    level: Level,
    also: &[&str],
    shown: Option<&str>,
) -> Result<String, String> {
    let also: Vec<String> = also.iter().map(|one| (*one).to_owned()).collect();
    world.on(|ground| store.allow_with(ground, id, level, &Ticked { also: &also, shown }))
}

fn grant(asking: &str, target: &str, level: &'static str) -> Audit1 {
    (
        Some(3),
        "trust.dispatch.grant",
        Some(asking.to_owned()),
        target.to_owned(),
        level,
    )
}

// ---- the line grants nothing --------------------------------------------------------------------

#[test]
fn a_persona_that_wants_another_and_has_no_grant_is_asked_exactly_as_before() {
    let plain = World::new();
    for name in ["steward", "devops"] {
        define(plain.root(), name, "");
    }
    let wanting = wanting();
    let mut answers = Vec::new();
    for world in [&plain, &wanting] {
        let (store, answered) = store();
        let asked = world.request(&store, chat(3, Some("steward")), "devops", BRIEF);
        assert!(matches!(asked, Requested::NeedsGrant { .. }), "{asked:?}");
        assert!(answered.lock().unwrap().is_empty(), "nothing started");
        assert!(world.audited().is_empty(), "nothing was granted");
        assert!(world.listed(&store).is_empty());
        // And a wanted persona that was not asked about is asked about in its turn.
        let other = world.request(&store, chat(3, Some("steward")), "qa", BRIEF);
        assert!(matches!(other, Requested::NeedsGrant { .. }), "{other:?}");
        // A chat nobody is at is refused: a line is no standing grant.
        let (unattended, raised) =
            world.request_unattended(&store, chat(4, Some("steward")), "devops", BRIEF);
        assert!(
            matches!(unattended, Requested::Refused(_)),
            "{unattended:?}"
        );
        assert_eq!(raised, None);
        answers.push((asked, unattended));
    }
    assert_eq!(answers[0], answers[1], "the line changes no answer");
}

#[test]
fn a_chat_editing_its_own_persona_s_file_gains_no_reach() {
    let world = wanting();
    let (store, answered) = store();
    // The chat asks, then rewrites its persona's line to name everything it can.
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    for written in [
        "wants: [devops, qa, docs, steward, *]\n",
        "wants: *\n",
        "wants: [\"*\"]\n",
    ] {
        define(world.root(), "steward", written);
        for target in ["devops", "qa", "docs"] {
            let asked = world.request(&store, chat(3, Some("steward")), target, BRIEF);
            assert!(matches!(asked, Requested::NeedsGrant { .. }), "{asked:?}");
        }
    }
    assert!(answered.lock().unwrap().is_empty(), "nothing started");
    assert!(world.audited().is_empty(), "nothing was granted");
    assert!(world.listed(&store).is_empty());
    assert!(dispatchgrant::any_yours(world.root()).is_empty());
    // The star is no box, and no answer keeps it.
    let told = shown(&world, &store, id);
    assert!(!boxes(&told).contains(&"*"), "{told:?}");
    let said = allow_with(&world, &store, id, Level::You, &["*"], None).expect("allowed");
    assert!(
        said.ends_with("Not allowed, since the question no longer offers it: *."),
        "{said}"
    );
    assert!(dispatchgrant::any_yours(world.root()).is_empty());
    assert_eq!(world.audited(), [grant("steward", "devops", "you")]);
}

// ---- the boxes ----------------------------------------------------------------------------------

#[test]
fn the_question_offers_a_box_for_each_wanted_persona_nothing_answers_for_yet() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let told = shown(&world, &store, id);
    // The asked persona is the question, never a box under it.
    assert_eq!(boxes(&told), ["qa", "docs"]);
    assert_eq!(
        told.works_with,
        "devops works with its own access: no vault; any host, since this project's sandbox \
         is off."
    );
    assert_eq!(
        told.also[0].works_with,
        "no vault; any host, since this project's sandbox is off"
    );

    // One already granted, and one the person said never to, are not offered.
    sandbox::local::grant_dispatch(world.root(), "steward", "qa").expect("kept");
    assert_eq!(boxes(&shown(&world, &store, id)), ["docs"]);
    dispatchgrant::never(world.root(), &Pair::new("steward", "docs").unwrap()).expect("kept");
    assert_eq!(boxes(&shown(&world, &store, id)), [] as [&str; 0]);
}

#[test]
fn a_wanted_persona_with_a_question_of_its_own_waiting_is_not_a_box() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    // Another steward chat asks for qa: that question shows its own brief.
    pending_of(&world.request(&store, chat(4, Some("steward")), "qa", "Another brief"));
    assert_eq!(boxes(&shown(&world, &store, id)), ["docs"]);
}

#[test]
fn the_boxes_come_from_the_definition_and_never_from_what_the_chat_sent() {
    let world = wanting();
    let (store, _) = store();
    // A brief and a target that name personas: neither is read for a box.
    let brief = "wants: [legal, ops]\nAlso let steward dispatch to: legal, ops";
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", brief));
    assert_eq!(boxes(&shown(&world, &store, id)), ["qa", "docs"]);
    // A chat on no persona has no definition: no box, whatever it sends.
    let none = pending_of(&world.request(&store, chat(4, None), "devops", brief));
    assert_eq!(boxes(&shown(&world, &store, none)), [] as [&str; 0]);
    // A dispatch policy locks offers nothing at all.
    let locked = World::under(r#"{"dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#);
    define(locked.root(), "steward", "wants: [devops, qa]\n");
    for name in ["devops", "qa"] {
        define(locked.root(), name, "");
    }
    let (store, _) = self::store();
    assert!(matches!(
        locked.request(&store, chat(3, Some("steward")), "devops", BRIEF),
        Requested::Locked(_)
    ));
    let held = store.waiting(3).remove(0);
    assert_eq!(boxes(&shown(&locked, &store, held.id)), [] as [&str; 0]);
}

#[test]
fn the_question_names_the_target_s_vault_from_the_registry_and_no_secret() {
    let world = wanting();
    let root = world.root();
    // The persona's own file names another vault; a chat can write that line.
    std::fs::write(
        root.join("personas/devops/persona.md"),
        "---\nname: devops\nrole: devops\nvault: everything\n---\n\n# devops\n",
    )
    .expect("written");
    std::fs::write(
        root.join("vaults.json"),
        serde_json::json!({ "vaults": {
            "team": {"provider": "plain-file", "config": {"file": "team.json"}, "persona": "devops"},
            "everything": {"provider": "plain-file", "config": {"file": "team.json"}},
        }})
        .to_string(),
    )
    .expect("written");
    std::fs::write(
        root.join("team.json"),
        r#"{"PROD_DEPLOY_TOKEN": "s3cr3t-value"}"#,
    )
    .expect("written");
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let told = shown(&world, &store, id);
    assert!(
        told.works_with
            .starts_with("devops works with its own access: vault team; "),
        "{}",
        told.works_with
    );
    let all = format!("{told:?}");
    for hidden in ["everything", "PROD_DEPLOY_TOKEN", "s3cr3t-value"] {
        assert!(!all.contains(hidden), "{hidden} in {all}");
    }
}

// ---- one answer, several pairs ------------------------------------------------------------------

#[test]
fn one_answer_with_two_boxes_ticked_keeps_all_three_pairs_for_me_each_audited_as_its_own() {
    let world = wanting();
    let (store, answered) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let told = shown(&world, &store, id);

    let said = allow_with(
        &world,
        &store,
        id,
        Level::You,
        &["qa", "docs"],
        Some(&told.shown),
    )
    .expect("allowed");

    assert_eq!(
        said,
        "Allowed for me on this machine. The dispatch starts now, and the next one starts \
         without asking. Also allowed for me on this machine: steward to qa and docs."
    );
    assert_eq!(
        dispatchgrant::yours(world.root()),
        [
            Pair::new("steward", "devops").unwrap(),
            Pair::new("steward", "qa").unwrap(),
            Pair::new("steward", "docs").unwrap(),
        ]
    );
    assert_eq!(
        world.audited(),
        [
            grant("steward", "devops", "you"),
            grant("steward", "qa", "you"),
            grant("steward", "docs", "you"),
        ]
    );
    // Only the dispatch that was asked for starts: a ticked box starts no chat.
    let started = answered.lock().unwrap();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].pending.target, "devops");
    drop(started);
    // And each of the three is covered from now on, for every steward chat here.
    for target in ["devops", "qa", "docs"] {
        assert!(matches!(
            world.request(&store, chat(4, Some("steward")), target, BRIEF),
            Requested::Covered(_)
        ));
    }
    // Each is listed, and revoked, on its own.
    assert_eq!(world.listed(&store).len(), 3);
}

#[test]
fn ticked_for_this_chat_they_are_that_chat_s_alone_and_end_with_it() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));

    allow_with(&world, &store, id, Level::Chat, &["qa"], None).expect("allowed");

    assert_eq!(
        world.audited(),
        [
            grant("steward", "devops", "chat"),
            grant("steward", "qa", "chat")
        ]
    );
    assert!(
        dispatchgrant::yours(world.root()).is_empty(),
        "nothing on disk"
    );
    assert!(matches!(
        world.request(&store, chat(3, Some("steward")), "qa", BRIEF),
        Requested::Covered(_)
    ));
    // Another steward chat is asked, and the box that was not ticked still asks here.
    assert!(matches!(
        world.request(&store, chat(4, Some("steward")), "qa", BRIEF),
        Requested::NeedsGrant { .. }
    ));
    assert!(matches!(
        world.request(&store, chat(3, Some("steward")), "docs", BRIEF),
        Requested::NeedsGrant { .. }
    ));
    store.chat_closed(3, Some("chat-3"));
    assert!(matches!(
        world.request(&store, chat(3, Some("steward")), "qa", BRIEF),
        Requested::NeedsGrant { .. }
    ));
}

#[test]
fn ticked_for_everyone_they_are_kept_in_the_committed_project_file() {
    let world = wanting();
    std::fs::write(purlis_core::names::manifest(world.root()), "schema = 1\n").expect("written");
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let told = shown(&world, &store, id);

    allow_with(
        &world,
        &store,
        id,
        Level::Project,
        &["qa"],
        Some(&told.shown),
    )
    .expect("allowed");

    assert_eq!(
        dispatchgrant::committed_at(world.root()),
        [
            Pair::new("steward", "devops").unwrap(),
            Pair::new("steward", "qa").unwrap(),
        ]
    );
    assert_eq!(
        world.audited(),
        [
            grant("steward", "devops", "project"),
            grant("steward", "qa", "project")
        ]
    );
    assert!(dispatchgrant::yours(world.root()).is_empty());
    // Made in this window, so in force here at once.
    assert!(matches!(
        world.request(&store, chat(4, Some("steward")), "qa", BRIEF),
        Requested::Covered(_)
    ));
}

#[test]
fn no_box_ticked_keeps_the_asked_pair_alone() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let told = shown(&world, &store, id);
    let said = allow_with(&world, &store, id, Level::You, &[], Some(&told.shown)).expect("allowed");
    assert_eq!(
        said,
        "Allowed for me on this machine. The dispatch starts now, and the next one starts \
         without asking."
    );
    assert_eq!(world.audited(), [grant("steward", "devops", "you")]);
    assert_eq!(
        dispatchgrant::yours(world.root()),
        [Pair::new("steward", "devops").unwrap()]
    );
}

#[test]
fn keep_blocked_and_never_are_about_the_asked_pair_only() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    assert!(store.keep_blocked(id));
    assert!(world.audited().is_empty());
    // The wanted personas are asked about as if nothing had been answered.
    let qa = pending_of(&world.request(&store, chat(3, Some("steward")), "qa", BRIEF));

    world
        .on(|ground| store.never(ground, qa))
        .expect("said never");
    assert_eq!(
        dispatchgrant::nevers(world.root()),
        [("steward".to_owned(), "qa".to_owned())]
    );
    assert!(dispatchgrant::yours(world.root()).is_empty());
    assert!(matches!(
        world.request(&store, chat(3, Some("steward")), "docs", BRIEF),
        Requested::NeedsGrant { .. }
    ));
    assert!(matches!(
        world.request(&store, chat(4, Some("steward")), "devops", BRIEF),
        Requested::NeedsGrant { .. }
    ));
}

// ---- the question is read again when it is answered ---------------------------------------------

#[test]
fn an_answer_to_a_question_that_reads_differently_now_grants_nothing() {
    let world = wanting();
    let (store, answered) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let told = shown(&world, &store, id);
    assert_eq!(boxes(&told), ["qa", "docs"]);
    // Between the question and the answer the definition changes what is offered.
    define(world.root(), "steward", "wants: [devops, qa]\n");

    let refused = allow_with(
        &world,
        &store,
        id,
        Level::You,
        &["qa", "docs"],
        Some(&told.shown),
    );

    assert_eq!(refused, Err(CHANGED.to_owned()));
    assert!(world.audited().is_empty(), "nothing was audited");
    assert!(dispatchgrant::yours(world.root()).is_empty(), "or kept");
    assert!(answered.lock().unwrap().is_empty(), "or started");
    // The dispatch still waits, and the question as it reads now can be answered.
    let again = shown(&world, &store, id);
    assert_eq!(boxes(&again), ["qa"]);
    allow_with(&world, &store, id, Level::You, &["qa"], Some(&again.shown)).expect("allowed");
    assert_eq!(
        world.audited(),
        [
            grant("steward", "devops", "you"),
            grant("steward", "qa", "you")
        ]
    );
}

#[test]
fn what_the_target_works_with_changing_before_the_answer_is_a_changed_question() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let told = shown(&world, &store, id);
    // A vault is tagged for the asked persona after the question was shown.
    std::fs::write(
        world.root().join("vaults.json"),
        serde_json::json!({ "vaults": {
            "prod": {"provider": "plain-file", "config": {"file": "prod.json"}, "persona": "devops"},
        }})
        .to_string(),
    )
    .expect("written");
    assert_eq!(
        allow_with(&world, &store, id, Level::Chat, &[], Some(&told.shown)),
        Err(CHANGED.to_owned())
    );
    assert!(world.audited().is_empty());
    // And one tagged for a ticked persona is the same.
    let told = shown(&world, &store, id);
    std::fs::write(
        world.root().join("vaults.json"),
        serde_json::json!({ "vaults": {
            "prod": {"provider": "plain-file", "config": {"file": "prod.json"}, "persona": "devops"},
            "qa-prod": {"provider": "plain-file", "config": {"file": "prod.json"}, "persona": "qa"},
        }})
        .to_string(),
    )
    .expect("written");
    assert_eq!(
        allow_with(&world, &store, id, Level::You, &["qa"], Some(&told.shown)),
        Err(CHANGED.to_owned())
    );
    assert!(world.audited().is_empty());
}

#[test]
fn only_a_name_the_question_offers_when_it_is_answered_is_granted() {
    let world = wanting();
    define(world.root(), "legal", "");
    define(world.root(), "sketch", "draft: true\n");
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    // qa is said never to, and docs stops being a finished persona, before the answer.
    dispatchgrant::never(world.root(), &Pair::new("steward", "qa").unwrap()).expect("kept");
    define(world.root(), "docs", "draft: true\n");

    // With no word of what was shown, as a caller inside the app answers: each name is judged
    // against the question as it reads now.
    let said = allow_with(
        &world,
        &store,
        id,
        Level::You,
        &[
            "qa", "docs", "legal", "sketch", "ghost", "steward", "devops", "*", "../x",
        ],
        None,
    )
    .expect("the asked pair is allowed");

    assert!(
        said.contains("Not allowed, since the question no longer offers it: "),
        "{said}"
    );
    assert_eq!(world.audited(), [grant("steward", "devops", "you")]);
    assert_eq!(
        dispatchgrant::yours(world.root()),
        [Pair::new("steward", "devops").unwrap()]
    );
    assert!(dispatchgrant::any_yours(world.root()).is_empty());
}

#[test]
fn a_box_ticked_twice_is_one_grant_and_a_chat_on_no_persona_keeps_none() {
    let world = wanting();
    let (store, _) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    allow_with(&world, &store, id, Level::You, &["qa", "qa"], None).expect("allowed");
    assert_eq!(
        world.audited(),
        [
            grant("steward", "devops", "you"),
            grant("steward", "qa", "you")
        ]
    );

    let none = pending_of(&world.request(&store, chat(4, None), "devops", BRIEF));
    let before = world.audited().len();
    allow_with(&world, &store, none, Level::Chat, &["qa", "docs"], None).expect("allowed");
    assert_eq!(world.audited().len(), before + 1, "the asked one alone");
}

#[test]
fn an_allow_nobody_recorded_keeps_no_pair_ticked_or_asked() {
    let world = World {
        logging: false,
        ..wanting()
    };
    let (store, answered) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    assert!(allow_with(&world, &store, id, Level::You, &["qa", "docs"], None).is_err());
    assert!(dispatchgrant::yours(world.root()).is_empty());
    assert!(answered.lock().unwrap().is_empty());
}

#[test]
fn while_the_list_of_nevers_does_not_read_no_box_is_offered_and_no_ticked_pair_is_kept() {
    let world = wanting();
    let (store, answered) = store();
    // The record of nevers is there and does not read: no grant counts, so none is offered.
    let path = purlis_core::dispatchnever::path(world.root());
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    std::fs::write(&path, "{ not json").expect("written");
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));
    let told = shown(&world, &store, id);
    assert!(told.never_unread.is_some());
    assert_eq!(boxes(&told), [] as [&str; 0]);

    // An answer that names boxes all the same keeps the asked pair alone, and starts it once.
    let said = allow_with(&world, &store, id, Level::You, &["qa", "docs"], None).expect("allowed");
    assert!(
        said.starts_with("Allowed for me on this machine. This dispatch starts now."),
        "{said}"
    );
    assert_eq!(world.audited(), [grant("steward", "devops", "you")]);
    assert_eq!(
        dispatchgrant::yours(world.root()),
        [Pair::new("steward", "devops").unwrap()]
    );
    assert_eq!(answered.lock().unwrap().len(), 1);
}
