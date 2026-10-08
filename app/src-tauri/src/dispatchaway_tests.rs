//! A dispatch refused while nobody was there, as the app keeps it and the person answers it
//! (#1507): which refusals reach the list, the grant "Allow from now on" writes and its audit,
//! that Dismiss holds, Never for this pair, an order a chat cannot move, what the sentence
//! says of the grant's reach, and what the list drops by the time the person looks.

use std::sync::Mutex;

use super::*;
use crate::dispatchunattended::{Runs, refusal_of, unattended};
use purlis_core::dispatchgrant::Audited;
use purlis_core::sandbox::policy::Locks;

/// One audit: the chat, the kind, the asking persona, the target and the level.
type Audit1 = (
    Option<u32>,
    &'static str,
    Option<String>,
    String,
    &'static str,
);

/// A project with three personas, its policy, and what was audited.
struct World {
    project: tempfile::TempDir,
    locks: Locks,
    audited: Mutex<Vec<Audit1>>,
    /// The project's personas; none where they could not be read.
    personas: Option<Vec<&'static str>>,
    /// Whether the event log takes an audit.
    logging: bool,
}

impl World {
    fn new() -> Self {
        Self {
            project: tempfile::tempdir().expect("a project"),
            locks: Locks::none(),
            audited: Mutex::new(Vec::new()),
            personas: Some(vec!["steward", "devops", "qa"]),
            logging: true,
        }
    }

    fn root(&self) -> &Path {
        self.project.path()
    }

    /// Runs `with` on this world at `at`.
    fn at<T>(&self, at: u64, with: impl FnOnce(&On<'_>) -> T) -> T {
        let known = |name: &str| {
            self.personas
                .as_ref()
                .is_some_and(|personas| personas.contains(&name))
        };
        let audit = |audited: &Audited<'_>| {
            if !self.logging {
                return Err("the event log is not open".to_owned());
            }
            self.audited.lock().unwrap().push((
                None,
                audited.kind(),
                audited.asking.map(str::to_owned),
                audited.target.to_owned(),
                audited.level.word(),
            ));
            Ok(())
        };
        with(&On {
            root: self.root(),
            locks: &self.locks,
            known: self.personas.is_some().then_some(&known as Known<'_>),
            audit: &audit,
            at,
        })
    }

    fn on<T>(&self, with: impl FnOnce(&On<'_>) -> T) -> T {
        self.at(NOW, with)
    }

    fn audited(&self) -> Vec<Audit1> {
        self.audited.lock().unwrap().clone()
    }

    /// A chat nobody is at, running as `persona` inside a sandbox, asks for `target`: what it
    /// is refused in, and whether that was kept for the person.
    fn asks(&self, persona: Option<&str>, target: &str, runs: Runs) -> (Option<String>, bool) {
        self.asks_at(NOW, persona, target, runs)
    }

    fn asks_at(
        &self,
        at: u64,
        persona: Option<&str>,
        target: &str,
        runs: Runs,
    ) -> (Option<String>, bool) {
        let asking = chat(persona);
        let said = match unattended(self.root(), &self.locks, &asking, runs, target) {
            crate::dispatchgrants::Requested::Refused(said) => said,
            _ => return (None, false),
        };
        let refusal = refusal_of(self.root(), &self.locks, &asking, runs, target);
        let kept = keep(self.root(), refusal.as_ref(), &said, Some("ide"), at);
        (Some(said), kept)
    }

    /// The pairs listed, each as asking, target, workspace and count.
    fn pairs(&self) -> Vec<(String, String, Option<String>, u32)> {
        self.on(listed)
            .into_iter()
            .map(|one| (one.asking, one.target, one.workspace, one.times))
            .collect()
    }
}

const NOW: u64 = 1_800_000_000;

const SANDBOXED: Runs = Runs {
    holds_anothers: false,
    sandboxed: true,
};

fn chat(persona: Option<&str>) -> Asking {
    Asking {
        session: 3,
        id: Some("chat-3".to_owned()),
        name: "steward 3".to_owned(),
        persona: persona.map(str::to_owned),
        held: false,
    }
}

fn one(asking: &str, target: &str, times: u32) -> (String, String, Option<String>, u32) {
    (
        asking.to_owned(),
        target.to_owned(),
        Some("ide".to_owned()),
        times,
    )
}

fn say_never(root: &Path, asking: &str, target: &str) {
    dispatchgrant::never(root, &Pair::new(asking, target).expect("a pair")).expect("said");
}

// ---- which refusals reach the list ---------------------------------------------------------------

#[test]
fn a_refusal_for_lack_of_a_grant_is_kept_once_a_pair_with_a_count() {
    let world = World::new();
    let (said, kept) = world.asks(Some("steward"), "devops", SANDBOXED);
    assert!(
        said.expect("refused")
            .contains("no grant lets steward chats dispatch to devops")
    );
    assert!(kept);
    assert!(world.asks(Some("steward"), "devops", SANDBOXED).1);
    assert!(world.asks(Some("steward"), "devops", SANDBOXED).1);

    assert_eq!(
        world.on(listed),
        vec![AwayRefusal {
            asking: "steward".to_owned(),
            target: "devops".to_owned(),
            workspace: Some("ide".to_owned()),
            latest: 1_800_000_000,
            times: 3,
            allows: "Allow from now on lets steward chats dispatch to devops without asking: \
                     this one pair, for you on this machine, in and into every workspace of \
                     this project. A chat nobody is at may use it too. No grant lets devops \
                     chats dispatch onward without asking. It starts nothing now. Revoke it in \
                     Settings › Project › Dispatch."
                .to_owned(),
        }]
    );
    // Keeping it granted nothing and audited nothing: the next ask is refused as the first.
    assert!(world.audited().is_empty());
    assert!(dispatchgrant::yours(world.root()).is_empty());
    assert!(world.asks(Some("steward"), "devops", SANDBOXED).0.is_some());
}

#[test]
fn a_refusal_for_any_other_reason_is_not_kept() {
    // A pair the person said never to.
    let world = World::new();
    say_never(world.root(), "steward", "devops");
    let (said, kept) = world.asks(Some("steward"), "devops", SANDBOXED);
    assert!(said.is_some() && !kept);

    // A record of nevers that does not read.
    let world = World::new();
    let nevers = purlis_core::dispatchnever::path(world.root());
    std::fs::create_dir_all(nevers.parent().expect("a folder")).expect("made");
    std::fs::write(&nevers, "not a record").expect("written");
    let (said, kept) = world.asks(Some("steward"), "devops", SANDBOXED);
    assert!(said.is_some() && !kept);

    // A policy lock is no refusal of the grants' at all.
    let world = World {
        locks: Locks::parse(
            r#"{"owner": "IT", "dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#,
            Path::new("/etc/purlis/policy.json"),
        ),
        ..World::new()
    };
    assert_eq!(
        refusal_of(
            world.root(),
            &world.locks,
            &chat(Some("steward")),
            SANDBOXED,
            "devops"
        ),
        None
    );

    // A chat with no sandbox, one that holds another chat's grants, one on no persona, and a
    // name that is no persona's.
    let world = World::new();
    for (persona, target, runs) in [
        (
            Some("steward"),
            "devops",
            Runs {
                sandboxed: false,
                ..SANDBOXED
            },
        ),
        (
            Some("steward"),
            "devops",
            Runs {
                holds_anothers: true,
                ..SANDBOXED
            },
        ),
        (None, "devops", SANDBOXED),
        (Some("steward"), "not a name", SANDBOXED),
    ] {
        let (said, kept) = world.asks(persona, target, runs);
        assert!(said.is_some(), "{persona:?} to {target}");
        assert!(!kept, "{persona:?} to {target}");
    }
    assert!(world.pairs().is_empty());
    assert!(!purlis_core::dispatchaway::path(world.root()).exists());
}

#[test]
fn a_refusal_said_in_another_sentence_than_the_grants_is_not_kept() {
    // The chat was refused for something the read here does not see (a persona the project
    // does not have, said before the grants are asked): what it was told is not the lack of a
    // grant, so nothing is kept whatever the grants would say.
    let world = World::new();
    let refusal = refusal_of(
        world.root(),
        &world.locks,
        &chat(Some("steward")),
        SANDBOXED,
        "ghost",
    );
    assert!(refusal.is_some());
    assert!(!keep(
        world.root(),
        refusal.as_ref(),
        "this project has no persona named ghost, so there is nothing to dispatch to.",
        Some("ide"),
        NOW,
    ));
    assert!(!keep(world.root(), None, "refused", None, NOW));
    assert!(world.pairs().is_empty());
}

#[test]
fn a_chat_asking_in_a_loop_raises_a_count_and_the_list_has_a_most() {
    let world = World::new();
    for _ in 0..30 {
        world.asks(Some("steward"), "devops", SANDBOXED);
    }
    assert_eq!(world.pairs(), vec![one("steward", "devops", 30)]);
    // Every persona's name a chat could ask for, and the list still ends.
    for n in 0..(purlis_core::dispatchaway::MOST + 5) {
        world.asks(Some("steward"), &format!("p{n}"), SANDBOXED);
    }
    assert_eq!(
        purlis_core::dispatchaway::list(world.root(), NOW).len(),
        purlis_core::dispatchaway::MOST
    );
}

// ---- Allow from now on ---------------------------------------------------------------------------

#[test]
fn allow_from_now_on_writes_the_person_s_grant_for_that_pair_and_audits_it_as_theirs() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.asks(Some("steward"), "qa", SANDBOXED);

    let said = world
        .on(|on| allow(on, "steward", "devops", Some("ide")))
        .expect("allowed");

    assert_eq!(
        said,
        "Allowed for me on this machine: steward chats dispatch to devops without asking from \
         now on, in and into every workspace of this project. Nothing was started. Revoke it \
         in Settings › Project › Dispatch."
    );
    // The person's, at their own level, under no chat.
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
    // One named pair in this machine's record, and nothing wider anywhere.
    assert_eq!(
        dispatchgrant::yours(world.root()),
        [Pair::new("steward", "devops").expect("a pair")]
    );
    assert!(dispatchgrant::any_yours(world.root()).is_empty());
    assert!(!purlis_core::names::manifest(world.root()).exists());
    let made = sandbox::local::made(world.root());
    assert_eq!(made.len(), 1);
    assert_eq!(
        (made[0].what.as_str(), made[0].level.as_str(), made[0].at),
        ("dispatch", "you", NOW)
    );
    assert_eq!(made[0].chat, None);
    // The next run works, for that pair and that direction alone.
    assert_eq!(world.asks(Some("steward"), "devops", SANDBOXED).0, None);
    assert!(world.asks(Some("devops"), "steward", SANDBOXED).0.is_some());
    // Its item is gone; the other pair's and the new one stay.
    assert_eq!(
        world.pairs(),
        vec![one("steward", "qa", 1), one("devops", "steward", 1)]
    );
}

#[test]
fn allow_takes_the_pair_s_items_from_every_workspace() {
    let world = World::new();
    for workspace in [Some("ide"), Some("web"), None] {
        purlis_core::dispatchaway::keep(world.root(), "steward", "devops", workspace, NOW)
            .expect("kept");
    }
    assert_eq!(world.pairs().len(), 3);
    world
        .on(|on| allow(on, "steward", "devops", Some("web")))
        .expect("allowed");
    assert!(world.pairs().is_empty());
    assert_eq!(world.audited().len(), 1);
}

#[test]
fn allow_grants_only_a_pair_that_is_listed_and_never_a_wider_one() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    for (asking, target, workspace) in [
        // Not listed: another pair, the other direction, another workspace.
        ("steward", "qa", Some("ide")),
        ("devops", "steward", Some("ide")),
        ("steward", "devops", Some("web")),
        ("steward", "devops", None),
        // Nothing that is not two personas' names.
        ("steward", "*", Some("ide")),
        ("*", "devops", Some("ide")),
        ("steward", "steward", Some("ide")),
    ] {
        assert!(
            world.on(|on| allow(on, asking, target, workspace)).is_err(),
            "{asking} to {target} in {workspace:?}"
        );
    }
    // A wildcard written into the record by hand is no entry, so it is no offer.
    let record = purlis_core::dispatchaway::path(world.root());
    std::fs::write(
        &record,
        format!(
            r#"{{"v":1,"refused":[{{"asking":"steward","target":"*","first":{NOW},"latest":{NOW},"times":1}}]}}"#
        ),
    )
    .expect("written");
    assert!(world.on(listed).is_empty());
    assert!(world.on(|on| allow(on, "steward", "*", None)).is_err());

    assert!(world.audited().is_empty());
    assert!(dispatchgrant::yours(world.root()).is_empty());
    assert!(dispatchgrant::any_yours(world.root()).is_empty());
}

#[test]
fn an_allow_the_event_log_does_not_take_grants_nothing_and_leaves_the_item() {
    let world = World {
        logging: false,
        ..World::new()
    };
    world.asks(Some("steward"), "devops", SANDBOXED);
    assert_eq!(
        world.on(|on| allow(on, "steward", "devops", Some("ide"))),
        Err("the event log is not open".to_owned())
    );
    assert!(dispatchgrant::yours(world.root()).is_empty());
    assert_eq!(world.pairs(), vec![one("steward", "devops", 1)]);
}

// ---- Dismiss ----------------------------------------------------------------------------------

#[test]
fn dismiss_puts_the_item_away_grants_nothing_and_holds_against_a_chat_that_asks_on() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.asks(Some("steward"), "qa", SANDBOXED);

    world
        .on(|on| dismiss(on, "steward", "devops", Some("ide")))
        .expect("dismissed");

    assert_eq!(world.pairs(), vec![one("steward", "qa", 1)]);
    assert!(world.audited().is_empty());
    assert!(dispatchgrant::yours(world.root()).is_empty());
    // The chat asks on. It is refused as before, in the sentence as it was with no clause,
    // and the item does not come back.
    for n in 1..=40 {
        let (said, kept) = world.asks_at(NOW + n * 60, Some("steward"), "devops", SANDBOXED);
        assert!(said.is_some());
        assert!(!kept, "listed again after {n}");
    }
    assert_eq!(world.pairs(), vec![one("steward", "qa", 1)]);
    // Counted all the same, where the person can be shown it later.
    let there = purlis_core::dispatchaway::kept(world.root(), NOW + 3_000);
    assert_eq!((there[0].target.as_str(), there[0].times), ("devops", 41));
}

#[test]
fn a_dismissed_item_is_shown_again_by_a_refusal_a_week_later_where_it_was() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.asks(Some("steward"), "qa", SANDBOXED);
    world
        .on(|on| dismiss(on, "steward", "devops", Some("ide")))
        .expect("dismissed");
    let week = purlis_core::dispatchaway::QUIET_SECS;

    assert!(
        !world
            .asks_at(NOW + week - 1, Some("steward"), "devops", SANDBOXED)
            .1
    );
    assert!(
        world
            .asks_at(NOW + week, Some("steward"), "devops", SANDBOXED)
            .1
    );
    // First in the list, as it was first refused, with every refusal counted.
    let shown: Vec<(String, u32)> = world
        .at(NOW + week, listed)
        .into_iter()
        .map(|one| (one.target, one.times))
        .collect();
    assert_eq!(shown, vec![("devops".to_owned(), 3), ("qa".to_owned(), 1)]);
}

// ---- Never for this pair --------------------------------------------------------------------------

#[test]
fn never_for_this_pair_is_the_store_s_never_audited_as_the_person_s_and_ends_the_item() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.asks(Some("steward"), "qa", SANDBOXED);

    let said = world
        .on(|on| never(on, "steward", "devops", Some("ide")))
        .expect("kept");

    assert_eq!(
        said,
        "No steward chat dispatches to devops on this machine from now on, and nothing more is \
         listed for the pair. Lift it in Settings › Project › Dispatch."
    );
    assert_eq!(
        world.audited(),
        [(
            None,
            "trust.dispatch.never",
            Some("steward".to_owned()),
            "devops".to_owned(),
            "you"
        )]
    );
    assert_eq!(
        dispatchgrant::nevers(world.root()),
        [("steward".to_owned(), "devops".to_owned())]
    );
    assert!(dispatchgrant::yours(world.root()).is_empty());
    assert_eq!(world.pairs(), vec![one("steward", "qa", 1)]);
    // The chat asking on is refused for the never, which is not kept and adds no clause.
    let (said, kept) = world.asks(Some("steward"), "devops", SANDBOXED);
    assert_eq!(said, Some(dispatchgrant::never_said("steward", "devops")));
    assert!(!kept);
    assert!(
        purlis_core::dispatchaway::kept(world.root(), NOW)
            .iter()
            .all(|one| one.target != "devops")
    );
}

#[test]
fn never_is_said_only_for_a_pair_that_is_listed_and_only_where_it_is_recorded() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    for (asking, target, workspace) in [
        ("steward", "qa", Some("ide")),
        ("devops", "steward", Some("ide")),
        ("steward", "devops", Some("web")),
        ("steward", "*", Some("ide")),
    ] {
        assert!(
            world.on(|on| never(on, asking, target, workspace)).is_err(),
            "{asking} to {target} in {workspace:?}"
        );
    }
    let quiet = World {
        logging: false,
        ..World::new()
    };
    quiet.asks(Some("steward"), "devops", SANDBOXED);
    assert_eq!(
        quiet.on(|on| never(on, "steward", "devops", Some("ide"))),
        Err("the event log is not open".to_owned())
    );
    for world in [&world, &quiet] {
        assert!(dispatchgrant::nevers(world.root()).is_empty());
        assert_eq!(world.pairs(), vec![one("steward", "devops", 1)]);
    }
}

// ---- an order a chat cannot move ------------------------------------------------------------------

#[test]
fn the_list_is_in_the_order_pairs_were_first_refused_and_a_chat_asking_again_moves_no_row() {
    let world = World::new();
    world.asks_at(NOW, Some("qa"), "devops", SANDBOXED);
    world.asks_at(NOW + 10, Some("steward"), "devops", SANDBOXED);
    world.asks_at(NOW + 20, Some("steward"), "qa", SANDBOXED);
    let order = |at: u64| -> Vec<(String, String)> {
        world
            .at(at, listed)
            .into_iter()
            .map(|one| (one.asking, one.target))
            .collect()
    };
    let first = order(NOW + 20);
    assert_eq!(
        first,
        vec![
            ("qa".to_owned(), "devops".to_owned()),
            ("steward".to_owned(), "devops".to_owned()),
            ("steward".to_owned(), "qa".to_owned()),
        ]
    );
    for n in 0..12 {
        let target = if n % 2 == 0 { "qa" } else { "devops" };
        world.asks_at(NOW + 30 + n, Some("steward"), target, SANDBOXED);
        assert_eq!(order(NOW + 30 + n), first, "after {n}");
    }
}

// ---- what the sentence says of the grant's reach --------------------------------------------------

#[test]
fn the_sentence_before_the_press_names_whom_the_target_may_dispatch_to_onward() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    let allows = || world.on(listed)[0].allows.clone();
    assert!(
        allows().contains(" No grant lets devops chats dispatch onward without asking. "),
        "{}",
        allows()
    );

    // The target's own named grants, the person's and the project's reviewed ones.
    sandbox::local::grant_dispatch(world.root(), "devops", "qa").expect("granted");
    assert!(
        allows().contains(" devops chats may themselves dispatch to qa without asking. "),
        "{}",
        allows()
    );
    for onward in ["prod", "db", "audit"] {
        sandbox::local::grant_dispatch(world.root(), "devops", onward).expect("granted");
    }
    assert!(
        allows().contains(
            " devops chats may themselves dispatch to audit, db, prod and 1 more without \
             asking. "
        ),
        "{}",
        allows()
    );
    // One the person said never to is not reach.
    say_never(world.root(), "devops", "qa");
    assert!(
        allows().contains(" devops chats may themselves dispatch to audit, db and prod without"),
        "{}",
        allows()
    );
    // Any persona is said as that.
    sandbox::local::grant_dispatch_any(world.root(), "devops").expect("granted");
    assert!(
        allows().contains(" devops chats may themselves dispatch to any persona without asking. "),
        "{}",
        allows()
    );
    // And every form says the reach across workspaces and for a chat nobody is at.
    assert!(allows().contains("in and into every workspace of this project"));
    assert!(allows().contains("A chat nobody is at may use it too."));
    assert!(!allows().contains("allows no other persona"));
}

// ---- by the time the person looks ----------------------------------------------------------------

#[test]
fn a_pair_the_person_said_never_to_since_is_dropped_without_a_word() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.asks(Some("steward"), "qa", SANDBOXED);
    say_never(world.root(), "steward", "devops");

    assert_eq!(world.pairs(), vec![one("steward", "qa", 1)]);
    // Gone from the record, not only from the list: lifting the never brings no offer back.
    dispatchgrant::lift_never(world.root(), "steward", "devops").expect("lifted");
    assert_eq!(world.pairs(), vec![one("steward", "qa", 1)]);
    assert!(world.audited().is_empty());
}

#[test]
fn an_allow_pressed_across_a_never_said_since_the_list_was_drawn_grants_nothing() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    assert_eq!(world.pairs().len(), 1);
    say_never(world.root(), "steward", "devops");

    assert_eq!(
        world.on(|on| allow(on, "steward", "devops", Some("ide"))),
        Err(
            "You said never to steward chats dispatching to devops on this machine, so nothing \
             was allowed. Lift it in Settings › Project › Dispatch first."
                .to_owned()
        )
    );
    assert!(world.audited().is_empty());
    assert!(dispatchgrant::yours(world.root()).is_empty());
    assert!(purlis_core::dispatchaway::list(world.root(), NOW).is_empty());
}

#[test]
fn a_pair_a_policy_locks_a_grant_covers_or_a_persona_that_is_gone_is_dropped_too() {
    // Locked by policy since.
    let mut world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.locks = Locks::parse(
        r#"{"owner": "IT", "dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    assert!(world.pairs().is_empty());
    assert!(purlis_core::dispatchaway::list(world.root(), NOW).is_empty());

    // Granted from a chat's Notice since.
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    sandbox::local::grant_dispatch(world.root(), "steward", "devops").expect("granted");
    assert!(world.pairs().is_empty());

    // The persona was removed since: nothing is offered, and an Allow sent anyway is refused.
    let mut world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.personas = Some(vec!["steward", "qa"]);
    assert_eq!(
        world.on(|on| allow(on, "steward", "devops", Some("ide"))),
        Err("This project has no persona named devops, so nothing was allowed.".to_owned())
    );
    assert!(world.pairs().is_empty());
    assert!(world.audited().is_empty());
}

#[test]
fn while_the_record_of_nevers_does_not_read_nothing_is_offered_allowed_or_dropped() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    let nevers = purlis_core::dispatchnever::path(world.root());
    std::fs::write(&nevers, "not a record").expect("written");

    assert!(world.on(listed).is_empty());
    assert!(
        world
            .on(|on| allow(on, "steward", "devops", Some("ide")))
            .is_err()
    );
    assert!(world.audited().is_empty());
    assert!(dispatchgrant::yours(world.root()).is_empty());
    // Mended, the item is there as it was.
    std::fs::remove_file(&nevers).expect("deleted");
    assert_eq!(world.pairs(), vec![one("steward", "devops", 1)]);
}

#[test]
fn an_item_is_gone_thirty_days_after_its_last_refusal_and_cannot_be_allowed_then() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    let later = NOW + purlis_core::dispatchaway::KEPT_SECS;
    assert_eq!(world.at(later - 1, listed).len(), 1);
    assert!(world.at(later, listed).is_empty());
    assert!(
        world
            .at(later, |on| allow(on, "steward", "devops", Some("ide")))
            .is_err()
    );
    assert!(world.audited().is_empty());
}

#[test]
fn where_the_project_s_personas_could_not_be_read_nothing_is_offered_and_nothing_is_lost() {
    let mut world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.asks(Some("steward"), "qa", SANDBOXED);
    let record = purlis_core::dispatchaway::path(world.root());
    let before = std::fs::read_to_string(&record).expect("the record");
    world.personas = None;

    assert!(world.on(listed).is_empty());
    assert_eq!(world.on(shown), Shown::default());
    assert_eq!(
        world.on(|on| allow(on, "steward", "devops", Some("ide"))),
        Err(
            "purlis could not read this project's personas just now, so nothing was changed. \
             Try again."
                .to_owned()
        )
    );
    assert!(world.audited().is_empty());
    assert_eq!(
        std::fs::read_to_string(&record).expect("the record"),
        before
    );
    // Read again, both are there as they were.
    world.personas = Some(vec!["steward", "devops", "qa"]);
    assert_eq!(
        world.pairs(),
        vec![one("steward", "devops", 1), one("steward", "qa", 1)]
    );
}

#[test]
fn the_read_the_window_is_told_from_writes_nothing_and_the_tidying_is_the_list_s_own() {
    let world = World::new();
    world.asks(Some("steward"), "devops", SANDBOXED);
    world.asks(Some("steward"), "qa", SANDBOXED);
    say_never(world.root(), "steward", "devops");
    let record = purlis_core::dispatchaway::path(world.root());
    let before = std::fs::read_to_string(&record).expect("the record");

    let read = world.on(shown);

    assert_eq!(
        read.settled,
        vec![("steward".to_owned(), "devops".to_owned())]
    );
    assert_eq!(read.listed.len(), 1);
    assert_eq!(read.listed[0].target, "qa");
    assert_eq!(
        std::fs::read_to_string(&record).expect("the record"),
        before,
        "a read leaves the record as it is"
    );
    // The window's own look is what takes the settled pair out.
    world.on(listed);
    assert!(
        !std::fs::read_to_string(&record)
            .expect("the record")
            .contains("devops")
    );
}

// ---- what the chat is told -----------------------------------------------------------------------

#[test]
fn the_chat_s_sentence_is_the_refusal_as_it_was_with_one_clause_added() {
    let world = World::new();
    let (said, kept) = world.asks(Some("steward"), "devops", SANDBOXED);
    let said = said.expect("refused");
    assert!(kept);
    assert_eq!(
        purlis_core::dispatchaway::told(&said),
        format!(
            "{said} purlis kept that this was refused, and the person will see it when they \
             are back."
        )
    );
}
