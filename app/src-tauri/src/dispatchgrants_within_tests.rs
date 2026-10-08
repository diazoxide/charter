//! A grant limited to one workspace, in the app (#1505): what each Allow keeps and where, that
//! the narrower one is what an Allow means unless the person chose the wider, what a chat is
//! answered for a task in another workspace, and what Settings' table shows and changes.
//! Driven at [`Store::request_in`] with the workspace the task works in.

use super::table::{known, persona, world_with};
use super::*;
use purlis_core::dispatchwithin::{Limited, Within};

/// Makes the workspace `name` in `world`'s project.
fn workspace(world: &World, name: &str) {
    std::fs::create_dir_all(world.root().join("workspaces").join(name)).expect("a workspace");
}

/// A world with the personas steward, devops and qa and the workspaces runners and web.
fn world() -> World {
    let world = World::new();
    for name in ["steward", "devops", "qa"] {
        persona(&world, name);
    }
    for name in ["runners", "web"] {
        workspace(&world, name);
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

fn covered() -> Requested {
    Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat("devops"))
}

fn held(asked: &Requested) -> bool {
    matches!(asked, Requested::NeedsGrant { .. })
}

/// Each audit with the workspace it said: kind, asking, target, level, workspace.
fn said(world: &World) -> Vec<(&'static str, String, String, &'static str, Option<String>)> {
    world
        .audited()
        .into_iter()
        .zip(world.within.lock().unwrap().clone())
        .map(|(one, within)| (one.1, one.2.unwrap_or_default(), one.3, one.4, within))
        .collect()
}

fn audit_in(
    kind: &'static str,
    asking: &str,
    target: &str,
    level: &'static str,
    workspace: Option<&str>,
) -> (&'static str, String, String, &'static str, Option<String>) {
    (
        kind,
        asking.to_owned(),
        target.to_owned(),
        level,
        workspace.map(str::to_owned),
    )
}

fn record(world: &World) -> serde_json::Value {
    let text = std::fs::read_to_string(sandbox::local::path(world.root())).expect("the record");
    serde_json::from_str(&text).expect("JSON")
}

fn limited(asking: &str, target: &str, workspace: &str) -> Limited {
    Limited::new(asking, target, workspace).expect("a limited grant")
}

// ---- the question, and what each Allow keeps ----------------------------------------------------

#[test]
fn the_question_says_where_the_task_works_and_an_allow_for_me_holds_there_only() {
    let world = world();
    let (store, answered) = store();
    let asked = ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    );
    let id = pending_of(&asked);
    let pending = store.waiting(3)[0].clone();
    assert_eq!(pending.works_in.as_deref(), Some("runners"));
    assert_eq!(
        told(&PlaneId::for_tests(world.root()), world.root(), &pending)
            .works_in
            .as_deref(),
        Some("runners")
    );

    assert_eq!(
        world.allow(&store, id, Level::You).as_deref(),
        Ok(
            "Allowed for me on this machine, in runners. The dispatch starts now, and the next \
            one that works in runners starts without asking."
        )
    );
    assert_eq!(
        answered.lock().unwrap().len(),
        1,
        "the held dispatch starts"
    );
    // Kept under the key of its own, and never among the grants that hold everywhere.
    let kept = record(&world);
    assert_eq!(kept.get("dispatch_mine"), None);
    assert_eq!(
        kept["dispatch_mine_in"],
        serde_json::json!([
            {"asking": "steward", "target": "devops", "workspace": "runners", "seen": 0}
        ])
    );
    assert_eq!(
        said(&world),
        [audit_in(
            "trust.dispatch.grant",
            "steward",
            "devops",
            "you",
            Some("runners")
        )]
    );

    // Any steward chat, for a task that works in runners: covered. Anywhere else: asked.
    assert_eq!(
        ask(
            &world,
            &store,
            chat(4, Some("steward")),
            "devops",
            Some("runners")
        ),
        covered()
    );
    assert!(held(&ask(
        &world,
        &store,
        chat(4, Some("steward")),
        "devops",
        Some("web")
    )));
    assert!(held(&ask(
        &world,
        &store,
        chat(5, Some("steward")),
        "devops",
        None
    )));
}

#[test]
fn in_any_workspace_is_an_answer_of_its_own_and_holds_everywhere() {
    let world = world();
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    assert_eq!(
        world
            .on(|ground| store.allow_anywhere(ground, id, Level::You))
            .as_deref(),
        Ok(
            "Allowed for me on this machine, in any workspace. The dispatch starts now, and the \
            next one starts without asking."
        )
    );
    assert_eq!(
        sandbox::local::granted_dispatch(world.root()),
        [("steward".to_owned(), "devops".to_owned())]
    );
    assert!(sandbox::local::dispatch_mine_in(world.root()).is_empty());
    assert_eq!(
        said(&world),
        [audit_in(
            "trust.dispatch.grant",
            "steward",
            "devops",
            "you",
            None
        )]
    );
    for place in [Some("runners"), Some("web"), None] {
        assert_eq!(
            ask(&world, &store, chat(4, Some("steward")), "devops", place),
            covered()
        );
    }
}

#[test]
fn a_task_at_the_project_s_root_has_no_workspace_to_limit_a_grant_to() {
    let world = world();
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        None,
    ));
    assert_eq!(
        world.allow(&store, id, Level::You).as_deref(),
        Ok(
            "Allowed for me on this machine, in any workspace. The dispatch starts now, and the \
            next one starts without asking."
        )
    );
    assert_eq!(
        said(&world),
        [audit_in(
            "trust.dispatch.grant",
            "steward",
            "devops",
            "you",
            None
        )]
    );
    assert!(sandbox::local::dispatch_mine_in(world.root()).is_empty());
}

#[test]
fn an_allow_for_this_chat_covers_that_chat_s_tasks_in_that_workspace_only() {
    let world = world();
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    world.allow(&store, id, Level::Chat).expect("this chat");
    assert_eq!(
        said(&world),
        [audit_in(
            "trust.dispatch.grant",
            "steward",
            "devops",
            "chat",
            Some("runners")
        )]
    );
    assert_eq!(
        ask(
            &world,
            &store,
            chat(3, Some("steward")),
            "devops",
            Some("runners")
        ),
        covered()
    );
    // The same chat, sending the task somewhere else, is asked again; so is another chat.
    assert!(held(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("web")
    )));
    assert!(held(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        None
    )));
    assert!(held(&ask(
        &world,
        &store,
        chat(4, Some("steward")),
        "devops",
        Some("runners")
    )));
    // The wider answer changes nothing for one chat: it is still that task's place.
    let other = pending_of(&ask(
        &world,
        &store,
        chat(5, Some("steward")),
        "devops",
        Some("web"),
    ));
    world
        .on(|ground| store.allow_anywhere(ground, other, Level::Chat))
        .expect("this chat");
    assert!(held(&ask(
        &world,
        &store,
        chat(5, Some("steward")),
        "devops",
        Some("runners")
    )));
    // The table says where each holds, and Revoke takes back the one pressed.
    let mut listed = world.listed(&store);
    listed.sort_by(|a, b| a.chat.cmp(&b.chat));
    let rows: Vec<(Option<&str>, Option<&str>)> = listed
        .iter()
        .map(|one| (one.chat.as_deref(), one.workspace.as_deref()))
        .collect();
    assert_eq!(
        rows,
        [
            (Some("steward 3"), Some("runners")),
            (Some("steward 5"), Some("web"))
        ]
    );
    revoke(world.root(), &store, &listed[0].id, &|_, _| Ok(())).expect("revoked");
    assert!(held(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners")
    )));
    assert_eq!(
        ask(
            &world,
            &store,
            chat(5, Some("steward")),
            "devops",
            Some("web")
        ),
        covered()
    );
}

#[test]
fn a_question_is_held_for_each_workspace_and_an_allow_for_one_starts_no_task_in_another() {
    let world = world();
    let (store, answered) = store();
    let first = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    let second = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("web"),
    ));
    assert_ne!(first, second);
    // Asked again for the same workspace, it is the question already held.
    assert_eq!(
        pending_of(&ask(
            &world,
            &store,
            chat(3, Some("steward")),
            "devops",
            Some("web")
        )),
        second
    );
    world.allow(&store, first, Level::You).expect("allowed");
    let started: Vec<u32> = answered
        .lock()
        .unwrap()
        .iter()
        .map(|one| one.pending.id)
        .collect();
    assert_eq!(started, [first]);
    assert_eq!(store.waiting(3).len(), 1);
    assert_eq!(store.waiting(3)[0].works_in.as_deref(), Some("web"));
}

#[test]
fn an_allow_for_everyone_writes_the_limited_spelling_and_covers_that_workspace_only() {
    let world = world_with(&["steward", "devops"], "");
    workspace(&world, "runners");
    workspace(&world, "web");
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    assert_eq!(
        world.allow(&store, id, Level::Project).as_deref(),
        Ok(
            "Allowed for everyone in this project, in runners. The dispatch starts now, and the \
            next one that works in runners starts without asking."
        )
    );
    let file =
        std::fs::read_to_string(purlis_core::names::manifest(world.root())).expect("the file");
    assert_eq!(
        file,
        "schema = 1\n\n[dispatch.grants]\nsteward = [{ to = \"devops\", in = \"runners\" }]\n"
    );
    assert_eq!(
        said(&world),
        [audit_in(
            "trust.dispatch.grant",
            "steward",
            "devops",
            "project",
            Some("runners")
        )]
    );
    assert_eq!(
        ask(
            &world,
            &store,
            chat(4, Some("steward")),
            "devops",
            Some("runners")
        ),
        covered()
    );
    assert!(held(&ask(
        &world,
        &store,
        chat(4, Some("steward")),
        "devops",
        Some("web")
    )));
    // No news on this machine, and nothing accepted for every workspace.
    assert_eq!(dispatchgrant::changed(world.root()), None);
    assert_eq!(record(&world).get("dispatch_seen"), None);
}

#[test]
fn an_allow_into_a_workspace_that_is_not_there_yet_starts_that_one_dispatch_and_keeps_nothing() {
    // A handoff that makes its workspace is judged before the workspace is there. No grant
    // is kept for a name that is no workspace, at any level.
    for level in [Level::Chat, Level::You] {
        let world = world();
        let (store, answered) = store();
        let id = pending_of(&ask(
            &world,
            &store,
            chat(3, Some("steward")),
            "devops",
            Some("fresh"),
        ));
        // The question says so, and offers one answer.
        let shown = told(
            &PlaneId::for_tests(world.root()),
            world.root(),
            &store.waiting(3)[0],
        );
        assert!(shown.works_in_missing);
        assert_eq!(shown.levels, [GrantLevel::Chat]);

        assert_eq!(
            world.allow(&store, id, level).as_deref(),
            Ok(
                "fresh is not a workspace of this project yet, so this starts this one dispatch \
                and keeps no grant. The next one asks you."
            )
        );
        assert_eq!(
            answered.lock().unwrap().len(),
            1,
            "the one they read starts"
        );
        assert_eq!(
            said(&world),
            [audit_in(
                "trust.dispatch.once",
                "steward",
                "devops",
                level.word(),
                Some("fresh")
            )]
        );
        // Nothing is kept: not for me, not for the chat.
        assert!(!sandbox::local::path(world.root()).exists());
        assert!(world.listed(&store).is_empty());
        // Asked again as it was first asked, it is let through, once.
        assert_eq!(
            ask(
                &world,
                &store,
                chat(3, Some("steward")),
                "devops",
                Some("fresh")
            ),
            covered()
        );
        // The workspace is made: nothing was waiting for it, and the next dispatch asks.
        workspace(&world, "fresh");
        assert!(held(&ask(
            &world,
            &store,
            chat(3, Some("steward")),
            "devops",
            Some("fresh")
        )));
        assert!(held(&ask(
            &world,
            &store,
            chat(4, Some("steward")),
            "devops",
            Some("fresh")
        )));
    }
    // The wider answer keeps nothing there either.
    let world = world();
    let (store, _) = store();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("fresh"),
    ));
    world
        .on(|ground| store.allow_anywhere(ground, id, Level::Project))
        .expect("started once");
    assert!(sandbox::local::granted_dispatch(world.root()).is_empty());
    assert!(!sandbox::local::path(world.root()).exists());
}

// ---- the one start -------------------------------------------------------------------------------

/// What `asking` is answered for `target` in `works_in`, with `brief`.
fn ask_with(
    world: &World,
    store: &Store,
    asking: Asking,
    works_in: Option<&str>,
    brief: &str,
) -> Requested {
    world.on(|ground| {
        store
            .request_in(
                ground,
                asking,
                "devops",
                brief,
                Uncovered::AskThePerson,
                works_in,
            )
            .0
    })
}

/// A dispatch allowed once, by each of the two ways there is one: the list of nevers does
/// not read (`unread`), or the task's workspace is not there yet. Answers the store, the
/// world, the held dispatch's number and the workspace.
fn allowed_once(unread: bool) -> (World, Store, u32, Option<&'static str>) {
    let world = world();
    let (store, _) = store();
    let works_in = if unread {
        Some("runners")
    } else {
        Some("fresh")
    };
    if unread {
        let path = purlis_core::dispatchnever::path(world.root());
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
        std::fs::write(path, "not json").expect("written");
    }
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        works_in,
    ));
    world.allow(&store, id, Level::You).expect("allowed");
    (world, store, id, works_in)
}

#[test]
fn the_one_start_is_for_the_brief_the_person_read_and_never_for_another() {
    for unread in [true, false] {
        let (world, store, _, works_in) = allowed_once(unread);
        // Another brief, from the same chat, to the same persona, for the same workspace: it
        // is another dispatch, and the person is asked.
        assert!(
            held(&ask_with(
                &world,
                &store,
                chat(3, Some("steward")),
                works_in,
                "Drop prod."
            )),
            "unread: {unread}"
        );
        // The one they read is still let through, once.
        assert_eq!(
            ask_with(&world, &store, chat(3, Some("steward")), works_in, BRIEF),
            covered(),
            "unread: {unread}"
        );
        assert!(held(&ask_with(
            &world,
            &store,
            chat(3, Some("steward")),
            works_in,
            BRIEF
        )));
    }
}

#[test]
fn a_replay_that_never_reached_the_grants_leaves_no_start_behind() {
    // The answered dispatch was refused by a limit before it asked for its grant again, or
    // was held again: either way it has returned, and what was left of its one start ends.
    for unread in [true, false] {
        let (world, store, id, works_in) = allowed_once(unread);
        store.end_once(id);
        assert!(
            held(&ask_with(
                &world,
                &store,
                chat(3, Some("steward")),
                works_in,
                BRIEF
            )),
            "unread: {unread}"
        );
    }
}

#[test]
fn a_revoke_a_never_and_a_change_of_workspace_each_take_an_unspent_start_with_them() {
    let pair = Pair::new("steward", "devops").expect("a pair");
    // Revoked: the grant the Allow kept while the list of nevers did not read.
    let (world, store, _, works_in) = allowed_once(true);
    let listed = world.listed(&store);
    world
        .on(|ground| revoke(ground.root, &store, &listed[0].id, ground.audit))
        .expect("revoked");
    assert!(held(&ask_with(
        &world,
        &store,
        chat(3, Some("steward")),
        works_in,
        BRIEF
    )));

    // Its workspace changed.
    let (world, store, _, works_in) = allowed_once(true);
    world
        .on(|ground| {
            set_workspace(
                &store,
                ground,
                &known(&world),
                "steward",
                "devops",
                Level::You,
                (
                    &Within::Workspace("runners".to_owned()),
                    &Within::Workspace("web".to_owned()),
                ),
            )
        })
        .expect("changed");
    assert!(held(&ask_with(
        &world,
        &store,
        chat(3, Some("steward")),
        works_in,
        BRIEF
    )));

    // Said never to, from another chat's question, with the workspace not there yet.
    let (world, store, _, works_in) = allowed_once(false);
    let other = pending_of(&ask(
        &world,
        &store,
        chat(4, Some("steward")),
        "devops",
        Some("web"),
    ));
    world
        .on(|ground| store.never(ground, other))
        .expect("never");
    assert!(matches!(
        ask_with(&world, &store, chat(3, Some("steward")), works_in, BRIEF),
        Requested::Refused(_)
    ));
    purlis_core::dispatchgrant::lift_never(world.root(), &pair.asking, &pair.target)
        .expect("lifted");
    assert!(held(&ask_with(
        &world,
        &store,
        chat(3, Some("steward")),
        works_in,
        BRIEF
    )));
}

#[test]
fn a_question_asked_again_for_another_workspace_says_where_the_pair_is_already_allowed() {
    let world = world();
    workspace(&world, "alpha");
    let (store, _) = store();
    // For this chat in runners, and for me in web.
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    world.allow(&store, id, Level::Chat).expect("this chat");
    purlis_core::dispatchwithin::grant_yours(
        world.root(),
        &Pair::new("steward", "devops").unwrap(),
        &Within::Workspace("web".to_owned()),
    )
    .expect("kept");
    pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("alpha"),
    ));
    let shown = told_by(
        &store,
        &PlaneId::for_tests(world.root()),
        world.root(),
        &store.waiting(3)[0],
    );
    assert_eq!(shown.allowed_in, ["runners", "web"]);
    assert!(!shown.works_in_missing);
    // Another chat has no grant of its own: only mine is said.
    pending_of(&ask(
        &world,
        &store,
        chat(4, Some("steward")),
        "devops",
        Some("alpha"),
    ));
    let shown = told_by(
        &store,
        &PlaneId::for_tests(world.root()),
        world.root(),
        &store.waiting(4)[0],
    );
    assert_eq!(shown.allowed_in, ["web"]);
}

#[test]
fn count_it_again_on_a_project_grant_is_this_machine_s_act_and_leaves_the_file_as_it_is() {
    let text = "schema = 1\n\n[dispatch.grants]\nsteward = [\"qa\", { to = \"devops\", in = \"runners\" }]\n";
    let world = world_with(&["steward", "devops", "qa"], &text["schema = 1\n".len()..]);
    workspace(&world, "runners");
    let (store, _) = store();
    let root = world.root();
    let one = limited("steward", "devops", "runners");
    world
        .on(|ground| accept_in(&store, ground, &known(&world), &one))
        .expect("accepted");
    // The workspace is made again; the acceptance is for the one that was removed.
    std::fs::remove_dir_all(root.join("workspaces/runners")).expect("removed");
    world.listed(&store);
    workspace(&world, "runners");
    assert!(held(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners")
    )));

    let runners = Within::Workspace("runners".to_owned());
    world
        .on(|ground| {
            set_workspace(
                &store,
                ground,
                &known(&world),
                "steward",
                "devops",
                Level::Project,
                (&runners, &runners),
            )
        })
        .expect("counted again");
    // The committed file is byte for byte what it was.
    assert_eq!(
        std::fs::read_to_string(purlis_core::names::manifest(root)).expect("the file"),
        text
    );
    assert_eq!(
        ask(
            &world,
            &store,
            chat(4, Some("steward")),
            "devops",
            Some("runners")
        ),
        covered()
    );
    // Recorded as what it is: this machine following the project's grant, for that workspace.
    assert_eq!(
        said(&world).last(),
        Some(&audit_in(
            "trust.dispatch.grant",
            "steward",
            "devops",
            "project",
            Some("runners")
        ))
    );
    // And nothing is accepted for a workspace that is not there.
    std::fs::remove_dir_all(root.join("workspaces/runners")).expect("removed");
    assert_eq!(
        world.on(|ground| accept_in(&store, ground, &known(&world), &one)),
        Err(
            "runners is not a workspace of this project now, so purlis keeps no grant for it."
                .to_owned()
        )
    );
}

#[test]
fn a_project_change_that_is_in_the_file_is_not_recorded_as_put_back_when_this_machine_cannot_follow_it()
 {
    let world = world_with(
        &["steward", "devops"],
        "\n[dispatch.grants]\nsteward = [{ to = \"devops\", in = \"runners\" }]\n",
    );
    workspace(&world, "runners");
    let (store, _) = store();
    let root = world.root();
    // This machine's own record cannot be written: a folder stands where the file goes.
    let record = sandbox::local::path(root);
    std::fs::create_dir_all(&record).expect("a folder in its place");

    let refused = world
        .on(|ground| {
            set_workspace(
                &store,
                ground,
                &known(&world),
                "steward",
                "devops",
                Level::Project,
                (&Within::Workspace("runners".to_owned()), &Within::Any),
            )
        })
        .expect_err("not followed here");
    assert!(
        refused.contains("and it covers nothing on this machine yet"),
        "{refused}"
    );
    // The file holds the wider grant, for the team, and the log's last word is that grant.
    assert_eq!(
        std::fs::read_to_string(purlis_core::names::manifest(root)).expect("the file"),
        "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\"]\n"
    );
    assert_eq!(
        said(&world),
        [
            audit_in(
                "trust.dispatch.revoke",
                "steward",
                "devops",
                "project",
                Some("runners")
            ),
            audit_in("trust.dispatch.grant", "steward", "devops", "project", None),
        ]
    );
}

// ---- a chat nobody is at -------------------------------------------------------------------------

#[test]
fn a_chat_nobody_is_at_is_covered_by_a_limited_grant_in_its_workspace_and_refused_elsewhere() {
    let world = world();
    let (store, _) = store();
    purlis_core::dispatchwithin::grant_yours(
        world.root(),
        &Pair::new("steward", "devops").unwrap(),
        &Within::Workspace("runners".to_owned()),
    )
    .expect("kept");
    let unattended = |works_in: Option<&str>| {
        world.on(|ground| {
            store
                .request_in(
                    ground,
                    chat(3, Some("steward")),
                    "devops",
                    BRIEF,
                    Uncovered::Refuse,
                    works_in,
                )
                .0
        })
    };
    assert_eq!(unattended(Some("runners")), covered());
    assert!(matches!(unattended(Some("web")), Requested::Refused(_)));
    assert!(matches!(unattended(None), Requested::Refused(_)));
    assert!(store.waiting(3).is_empty(), "nothing is ever held for it");
}

// ---- Settings' table ----------------------------------------------------------------------------

#[test]
fn the_table_says_which_workspace_each_grant_holds_in_and_why_one_covers_nothing() {
    let world = world();
    let (store, _) = store();
    let root = world.root();
    sandbox::local::grant_dispatch(root, "steward", "qa").expect("me, anywhere");
    for workspace in ["runners", "gone"] {
        sandbox::local::keep_dispatch_in(
            root,
            sandbox::local::Limited::Mine,
            &sandbox::local::DispatchIn {
                asking: "steward".to_owned(),
                target: "devops".to_owned(),
                any: false,
                workspace: workspace.to_owned(),
                seen: 0,
            },
        )
        .expect("kept");
    }
    sandbox::local::keep_dispatch_in(
        root,
        sandbox::local::Limited::Mine,
        &sandbox::local::DispatchIn {
            asking: "qa".to_owned(),
            target: "*".to_owned(),
            any: true,
            workspace: "web".to_owned(),
            seen: 0,
        },
    )
    .expect("kept");

    let rows: Vec<(String, Option<String>, Option<String>)> = world
        .listed(&store)
        .into_iter()
        .map(|one| (one.target, one.workspace, one.nowhere))
        .collect();
    assert_eq!(
        rows,
        [
            ("qa".to_owned(), None, None),
            ("devops".to_owned(), Some("runners".to_owned()), None),
            (
                "devops".to_owned(),
                Some("gone".to_owned()),
                Some(
                    "gone is not a workspace of this project now, so this grant covers nothing. \
                     A grant does not follow a workspace that was renamed: set its workspace \
                     again, or remove it."
                        .to_owned()
                )
            ),
        ]
    );
    let standing = standing_read(root, &store);
    assert_eq!(standing.workspaces, ["runners", "web"]);
    assert_eq!(
        standing.any,
        [DispatchAny {
            asking: "qa".to_owned(),
            level: GrantLevel::You,
            waiting: false,
            declined: false,
            workspace: Some("web".to_owned()),
            nowhere: None,
            id: Some(limited_id(Level::You, &limited("qa", "*", "web"))),
        }]
    );
    // Remove takes the one that covers nothing out, by its id, and says where it held.
    let gone = world
        .listed(&store)
        .into_iter()
        .find(|one| one.workspace.as_deref() == Some("gone"))
        .expect("listed");
    world
        .on(|ground| revoke(root, &store, &gone.id, ground.audit))
        .expect("removed");
    assert_eq!(
        said(&world),
        [audit_in(
            "trust.dispatch.revoke",
            "steward",
            "devops",
            "you",
            Some("gone")
        )]
    );
    assert_eq!(world.listed(&store).len(), 2);
    // And "any persona" in web is cleared the same way.
    let any = standing.any[0].id.clone().expect("an id");
    world
        .on(|ground| revoke(root, &store, &any, ground.audit))
        .expect("cleared");
    assert!(standing_read(root, &store).any.is_empty());
    assert_eq!(
        said(&world).last(),
        Some(&audit_in(
            "trust.dispatch.revoke",
            "qa",
            "*",
            "you",
            Some("web")
        ))
    );
}

#[test]
fn my_grant_s_workspace_is_narrowed_and_widened_each_audited_before_one_write() {
    let world = world();
    let (store, answered) = store();
    let root = world.root();
    sandbox::local::grant_dispatch(root, "steward", "devops").expect("me, anywhere");
    let set = |from: Option<&str>, to: Option<&str>| {
        world.on(|ground| {
            set_workspace(
                &store,
                ground,
                &known(&world),
                "steward",
                "devops",
                Level::You,
                (&Within::of(from), &Within::of(to)),
            )
        })
    };

    set(None, Some("runners")).expect("narrowed");
    assert_eq!(
        said(&world),
        [
            audit_in("trust.dispatch.revoke", "steward", "devops", "you", None),
            audit_in(
                "trust.dispatch.grant",
                "steward",
                "devops",
                "you",
                Some("runners")
            ),
        ]
    );
    assert!(held(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("web")
    )));
    assert_eq!(
        ask(
            &world,
            &store,
            chat(4, Some("steward")),
            "devops",
            Some("runners")
        ),
        covered()
    );

    // Widened: the dispatch that waited for web starts.
    set(Some("runners"), None).expect("widened");
    assert_eq!(answered.lock().unwrap().len(), 1);
    assert_eq!(
        said(&world)[2..],
        [
            audit_in(
                "trust.dispatch.revoke",
                "steward",
                "devops",
                "you",
                Some("runners")
            ),
            audit_in("trust.dispatch.grant", "steward", "devops", "you", None),
        ]
    );
    assert_eq!(
        sandbox::local::granted_dispatch(root),
        [("steward".to_owned(), "devops".to_owned())]
    );
    assert!(sandbox::local::dispatch_mine_in(root).is_empty());
}

#[test]
fn a_change_the_table_could_not_mean_writes_nothing_and_audits_nothing() {
    let world = world();
    let (store, _) = store();
    let root = world.root();
    sandbox::local::grant_dispatch(root, "steward", "devops").expect("me, anywhere");
    let set = |target: &str, level: Level, from: Option<&str>, to: Option<&str>| {
        world.on(|ground| {
            set_workspace(
                &store,
                ground,
                &known(&world),
                "steward",
                target,
                level,
                (&Within::of(from), &Within::of(to)),
            )
        })
    };
    // A row that is no longer there as the table drew it.
    assert_eq!(
        set("devops", Level::You, Some("runners"), None),
        Err("purlis changed nothing: that grant is no longer there.".to_owned())
    );
    assert!(set("qa", Level::You, None, Some("runners")).is_err());
    // A workspace the project does not have, and a name that can be none.
    assert_eq!(
        set("devops", Level::You, None, Some("nowhere")),
        Err("nowhere is not a workspace of this project now, so nothing was changed.".to_owned())
    );
    assert!(set("devops", Level::You, None, Some("../runners")).is_err());
    // Already as asked, and a grant made for one chat.
    assert_eq!(
        set("devops", Level::You, None, None),
        Err("That grant holds in any workspace already.".to_owned())
    );
    assert!(set("devops", Level::Chat, None, Some("runners")).is_err());
    assert!(said(&world).is_empty(), "nothing was audited");
    assert_eq!(
        sandbox::local::granted_dispatch(root),
        [("steward".to_owned(), "devops".to_owned())]
    );
    assert!(sandbox::local::dispatch_mine_in(root).is_empty());
    // Where the event log does not take it, nothing is written either.
    let deaf = World {
        logging: false,
        ..world
    };
    let refused = deaf.on(|ground| {
        set_workspace(
            &store,
            ground,
            &known(&deaf),
            "steward",
            "devops",
            Level::You,
            (&Within::Any, &Within::Workspace("runners".to_owned())),
        )
    });
    assert_eq!(refused, Err("the event log is not open".to_owned()));
    assert!(sandbox::local::dispatch_mine_in(deaf.root()).is_empty());
}

#[test]
fn a_grant_whose_workspace_was_made_again_covers_nothing_until_the_person_counts_it_again() {
    let world = world();
    let (store, _) = store();
    let root = world.root();
    let id = pending_of(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("runners"),
    ));
    world.allow(&store, id, Level::You).expect("allowed");
    // The workspace goes, the table is read, and another is made under its name.
    std::fs::remove_dir_all(root.join("workspaces/runners")).expect("removed");
    let gone = world.listed(&store);
    assert_eq!(
        gone[0].nowhere.as_deref(),
        Some(
            "runners is not a workspace of this project now, so this grant covers nothing. A \
             grant does not follow a workspace that was renamed: set its workspace again, or \
             remove it."
        )
    );
    workspace(&world, "runners");
    assert!(held(&ask(
        &world,
        &store,
        chat(4, Some("steward")),
        "devops",
        Some("runners")
    )));
    assert_eq!(
        world.listed(&store)[0].nowhere.as_deref(),
        Some(
            "A workspace named runners was removed or renamed after this grant was made, so it \
             covers nothing in the one that is there now."
        )
    );
    // Count it again: the same workspace, set by the person, audited.
    world
        .on(|ground| {
            set_workspace(
                &store,
                ground,
                &known(&world),
                "steward",
                "devops",
                Level::You,
                (
                    &Within::Workspace("runners".to_owned()),
                    &Within::Workspace("runners".to_owned()),
                ),
            )
        })
        .expect("counted again");
    assert_eq!(world.listed(&store)[0].nowhere, None);
    assert_eq!(
        ask(
            &world,
            &store,
            chat(5, Some("steward")),
            "devops",
            Some("runners")
        ),
        covered()
    );
    // The record of when it was made, and from which chat, stayed with it.
    assert_eq!(world.listed(&store)[0].chat.as_deref(), Some("steward 3"));
}

#[test]
fn a_project_grant_s_workspace_is_changed_for_everyone_and_a_teammate_s_waits_for_accept() {
    let world = world_with(
        &["steward", "devops", "qa"],
        "\n[dispatch.grants]\nsteward = [\"devops\"]\nqa = [{ to = \"devops\", in = \"web\" }]\n",
    );
    workspace(&world, "runners");
    workspace(&world, "web");
    let (store, _) = store();
    let root = world.root();
    world
        .on(|ground| accept(&store, ground, &known(&world), "steward", "devops"))
        .expect("accepted");
    world
        .on(|ground| {
            set_workspace(
                &store,
                ground,
                &known(&world),
                "steward",
                "devops",
                Level::Project,
                (&Within::Any, &Within::Workspace("runners".to_owned())),
            )
        })
        .expect("narrowed for everyone");
    let file = std::fs::read_to_string(purlis_core::names::manifest(root)).expect("the file");
    assert_eq!(
        file,
        "schema = 1\n\n[dispatch.grants]\nsteward = [{ to = \"devops\", in = \"runners\" }]\n\
         qa = [{ to = \"devops\", in = \"web\" }]\n"
    );
    assert_eq!(
        ask(
            &world,
            &store,
            chat(3, Some("steward")),
            "devops",
            Some("runners")
        ),
        covered()
    );
    assert!(held(&ask(
        &world,
        &store,
        chat(3, Some("steward")),
        "devops",
        Some("web")
    )));

    // A teammate's limited grant waits here, is accepted for its workspace, and declined.
    let waiting: Vec<(Option<String>, Option<String>, bool)> = world
        .listed(&store)
        .into_iter()
        .map(|one| (one.asking, one.workspace, one.waiting))
        .collect();
    assert_eq!(
        waiting,
        [
            (
                Some("steward".to_owned()),
                Some("runners".to_owned()),
                false
            ),
            (Some("qa".to_owned()), Some("web".to_owned()), true),
        ]
    );
    assert!(held(&ask(
        &world,
        &store,
        chat(4, Some("qa")),
        "devops",
        Some("web")
    )));
    let theirs = limited("qa", "devops", "web");
    world
        .on(|ground| accept_in(&store, ground, &known(&world), &theirs))
        .expect("accepted");
    assert_eq!(
        ask(&world, &store, chat(5, Some("qa")), "devops", Some("web")),
        covered()
    );
    world
        .on(|ground| decline_in(root, &theirs, ground.audit))
        .expect("declined");
    assert!(held(&ask(
        &world,
        &store,
        chat(5, Some("qa")),
        "devops",
        Some("web")
    )));
    let last: Vec<_> = said(&world).into_iter().rev().take(2).rev().collect();
    assert_eq!(
        last,
        [
            audit_in(
                "trust.dispatch.grant",
                "qa",
                "devops",
                "project",
                Some("web")
            ),
            audit_in(
                "trust.dispatch.decline",
                "qa",
                "devops",
                "project",
                Some("web")
            ),
        ]
    );
    // Nothing is accepted that the file does not hold.
    assert!(
        world
            .on(|ground| accept_in(
                &store,
                ground,
                &known(&world),
                &limited("qa", "steward", "web")
            ))
            .is_err()
    );
}
