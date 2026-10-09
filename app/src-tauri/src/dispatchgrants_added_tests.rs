//! A standing dispatch grant made in Settings (#1465, V100-24): for you on this machine or for
//! everyone in the project, in one workspace or in any, with no dispatch waiting. Held to every
//! rule an Allow is, audited as the person's under no chat, and refused with nothing audited
//! where any rule says no. Driven at [`add_grant`], which the window's command calls.

use super::table::{known, persona, world_with};
use super::*;
use purlis_core::dispatchwithin::Limited;

/// Makes the workspace `name` in `world`'s project.
fn workspace(world: &World, name: &str) {
    std::fs::create_dir_all(world.root().join("workspaces").join(name)).expect("a workspace");
}

/// A world with the personas steward and devops and the workspace runners, and no project file.
fn world() -> World {
    let world = World::new();
    for name in ["steward", "devops"] {
        persona(&world, name);
    }
    workspace(&world, "runners");
    world
}

/// Adds `asking` to `target` at `level` in `workspace`, as Settings' Add does.
fn add(
    world: &World,
    store: &Store,
    (asking, target): (&str, &str),
    level: Level,
    workspace: Option<&str>,
) -> Result<String, String> {
    let known = known(world);
    world.on(|ground| {
        add_grant(
            store,
            ground,
            Some(&known),
            (asking, target),
            level,
            workspace,
        )
    })
}

/// What an unattended, sandboxed chat running as `asking` is answered for `target`.
fn unattended(world: &World, asking: &str, target: &str, works_in: Option<&str>) -> Requested {
    crate::dispatchunattended::unattended(
        world.root(),
        &world.locks,
        &chat(3, Some(asking)),
        crate::dispatchunattended::Runs {
            holds_anothers: false,
            sandboxed: true,
        },
        target,
        works_in,
    )
}

#[test]
fn a_grant_for_you_in_any_workspace_is_audited_as_yours_and_a_chat_nobody_is_at_runs_under_it() {
    let world = world();
    let (store, _) = store();
    // Before: a chat nobody is at is refused, and told where a person grants it.
    assert!(matches!(
        unattended(&world, "steward", "devops", None),
        Requested::Refused(said) if said.contains("they grant it under Settings › Project › Dispatch")
    ));

    let said = add(&world, &store, ("steward", "devops"), Level::You, None).expect("added");

    assert_eq!(
        said,
        "Allowed for me on this machine, in any workspace: steward chats dispatch to devops \
         without asking you, a chat nobody is at included."
    );
    assert_eq!(
        world.audited(),
        [(
            None,
            "trust.dispatch.grant",
            Some("steward".to_owned()),
            "devops".to_owned(),
            "you"
        )]
    );
    assert_eq!(*world.within.lock().unwrap(), [None]);
    assert_eq!(
        dispatchgrant::yours(world.root()),
        [Pair::new("steward", "devops").unwrap()]
    );
    // The table lists it as yours, made from no chat.
    let listed = world.listed(&store);
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (listed[0].level, listed[0].chat.as_deref(), listed[0].at),
        (GrantLevel::You, None, Some(100))
    );
    assert_eq!(
        unattended(&world, "steward", "devops", None),
        Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat("devops"))
    );

    // Made again, it is refused, and nothing more is audited.
    assert_eq!(
        add(&world, &store, ("steward", "devops"), Level::You, None),
        Err(
            "You have that grant already, so nothing changed. steward chats dispatch to devops \
             without asking you, a chat nobody is at included."
                .to_owned()
        )
    );
    assert_eq!(world.audited().len(), 1);
}

#[test]
fn a_grant_limited_to_one_workspace_holds_there_and_nowhere_else() {
    let world = world();
    let (store, _) = store();

    let said = add(
        &world,
        &store,
        ("steward", "devops"),
        Level::You,
        Some("runners"),
    )
    .expect("added");

    assert_eq!(
        said,
        "Allowed for me on this machine, in runners: steward chats dispatch to devops for work \
         in runners without asking you, a chat nobody is at included."
    );
    assert_eq!(*world.within.lock().unwrap(), [Some("runners".to_owned())]);
    let one = Limited::new("steward", "devops", "runners").unwrap();
    assert!(
        purlis_core::dispatchwithin::yours(world.root())
            .iter()
            .any(|(held, _)| *held == one)
    );
    assert!(dispatchgrant::yours(world.root()).is_empty());
    assert!(matches!(
        unattended(&world, "steward", "devops", Some("runners")),
        Requested::Covered(_)
    ));
    assert!(matches!(
        unattended(&world, "steward", "devops", None),
        Requested::Refused(_)
    ));
}

#[test]
fn every_rule_an_allow_is_held_to_refuses_it_with_nothing_audited() {
    let world = world();
    let (store, _) = store();
    let refused = |asked: Result<String, String>| asked.expect_err("refused");

    // Never for one chat: that is the chat's tab.
    assert!(
        refused(add(
            &world,
            &store,
            ("steward", "devops"),
            Level::Chat,
            None
        ))
        .contains("chat's tab")
    );
    // A persona's dispatch to itself needs no grant, so none is kept.
    assert_eq!(
        refused(add(
            &world,
            &store,
            ("steward", "steward"),
            Level::You,
            None
        )),
        "A steward chat dispatches to steward with no grant, so there is none to keep."
    );
    // A name that is no persona of the project, and "any persona", which is a press of its own.
    assert_eq!(
        refused(add(&world, &store, ("steward", "qa"), Level::You, None)),
        "This project has no persona named qa, so nothing was granted."
    );
    assert!(
        refused(add(&world, &store, ("steward", "*"), Level::You, None))
            .contains("not a persona's name")
    );
    // A workspace that is not there.
    assert_eq!(
        refused(add(
            &world,
            &store,
            ("steward", "devops"),
            Level::You,
            Some("web")
        )),
        purlis_core::dispatchwithin::not_there_said("web")
    );
    // The personas that could not be listed.
    assert_eq!(
        world.on(|ground| add_grant(
            &store,
            ground,
            None,
            ("steward", "devops"),
            Level::You,
            None
        )),
        Err(PERSONAS_UNREAD.to_owned())
    );
    // The person's never, until they lift it.
    dispatchgrant::never(world.root(), &Pair::new("steward", "devops").unwrap()).expect("never");
    assert!(
        refused(add(&world, &store, ("steward", "devops"), Level::You, None))
            .starts_with("You said never to steward chats dispatching to devops")
    );
    // A list of nevers that does not read grants nothing.
    std::fs::write(purlis_core::dispatchnever::path(world.root()), "not json").expect("broken");
    assert_eq!(
        refused(add(&world, &store, ("devops", "steward"), Level::You, None)),
        purlis_core::dispatchnever::unread_said(world.root())
    );
    // A grant for everyone where the project has no file of its own.
    std::fs::remove_file(purlis_core::dispatchnever::path(world.root())).expect("mended");
    assert!(
        refused(add(
            &world,
            &store,
            ("devops", "steward"),
            Level::Project,
            None
        ))
        .starts_with("This project has no file of its own yet")
    );

    assert!(world.audited().is_empty());
    assert!(dispatchgrant::yours(world.root()).is_empty());
}

#[test]
fn a_pair_policy_locks_is_refused_with_the_policy_s_sentence() {
    let world = World::under(
        r#"{"owner": "IT", "dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#,
    );
    for name in ["steward", "devops"] {
        persona(&world, name);
    }
    let (store, _) = store();

    assert_eq!(
        add(&world, &store, ("steward", "devops"), Level::You, None),
        Err(
            "Policy forbids steward chats dispatching to devops. Locked by policy, set by IT \
             in /etc/purlis/policy.json."
                .to_owned()
        )
    );
    assert!(world.audited().is_empty());
}

#[test]
fn a_dispatch_waiting_on_the_person_that_it_covers_starts() {
    let world = world();
    let (store, answered) = store();
    let id = pending_of(&world.request(&store, chat(3, Some("steward")), "devops", BRIEF));

    add(&world, &store, ("steward", "devops"), Level::You, None).expect("added");

    let answered = answered.lock().unwrap();
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0].pending.id, id);
    assert!(answered[0].allowed.is_some());
    assert!(store.waiting(3).is_empty());
}

#[test]
fn a_grant_for_everyone_is_written_to_the_project_s_file_and_followed_on_this_machine() {
    let world = world_with(&["steward", "devops"], "");
    workspace(&world, "runners");
    let (store, _) = store();

    add(&world, &store, ("steward", "devops"), Level::Project, None).expect("added");
    add(
        &world,
        &store,
        ("devops", "steward"),
        Level::Project,
        Some("runners"),
    )
    .expect("added");

    let file = super::table::file(&world);
    assert!(file.contains("steward = [\"devops\"]"), "{file}");
    assert!(
        file.contains("devops = [{ to = \"steward\", in = \"runners\" }]"),
        "{file}"
    );
    assert_eq!(
        super::table::kinds(&world),
        [
            super::table::audit("trust.dispatch.grant", "steward", "devops", "project"),
            super::table::audit("trust.dispatch.grant", "devops", "steward", "project"),
        ]
    );
    // Followed here as it is written: nothing of it waits for a yes on this machine.
    assert!(dispatchgrant::unacknowledged(world.root()).is_empty());
    assert!(purlis_core::dispatchwithin::unaccepted(world.root()).is_empty());
    // And it is in the file already: Accept is where one that waits is followed.
    assert!(
        add(&world, &store, ("steward", "devops"), Level::Project, None)
            .expect_err("in the file already")
            .starts_with("The project's file has that grant already")
    );
}

#[test]
fn a_pair_declined_on_this_machine_gets_no_grant_for_everyone_in_any_workspace() {
    // #1465 review: Not on my machine is not undone by another name, as the Allow holds it.
    let world = world_with(
        &["steward", "devops"],
        "[dispatch.grants]\nsteward = [\"devops\"]\n",
    );
    workspace(&world, "runners");
    let (store, _) = store();
    world
        .on(|ground| decline(ground.root, "steward", "devops", ground.audit))
        .expect("declined here");
    let before = world.audited().len();

    let refused = add(
        &world,
        &store,
        ("steward", "devops"),
        Level::Project,
        Some("runners"),
    )
    .expect_err("declined here");

    assert!(
        refused.starts_with("You said Not on my machine"),
        "{refused}"
    );
    assert_eq!(world.audited().len(), before, "nothing audited");
    assert!(!super::table::file(&world).contains("runners"));
    // For you on this machine it is still yours to make.
    add(
        &world,
        &store,
        ("steward", "devops"),
        Level::You,
        Some("runners"),
    )
    .expect("mine");
}
