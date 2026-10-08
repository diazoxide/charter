//! Never for a pair, and any persona as an explicit grant (#1503): what each covers and
//! refuses, where each is kept, who can write it, and what a record edited by hand does. Every
//! expected answer is written out.

use std::path::Path;

use super::*;
use crate::sandbox::policy::Locks;

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

fn never(asking: &str, target: &str) -> Vec<(String, String)> {
    vec![(asking.to_owned(), target.to_owned())]
}

fn asked(asking: &str, target: &str, grants: &InForce) -> Covers {
    covers(Some(asking), target, grants, &Locks::none())
}

/// This machine's record for the project at `root`, written by hand.
fn by_hand(root: &Path, json: &str) {
    let path = crate::sandbox::local::path(root);
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    std::fs::write(path, json).expect("written");
}

// ---- a grant is one-way (V100-22) ---------------------------------------------------------------

#[test]
fn a_grant_from_one_persona_to_another_allows_nothing_the_other_way() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    crate::sandbox::local::grant_dispatch(root, "steward", "devops").expect("kept");
    let grants = InForce::read(root, Vec::new());
    assert_eq!(asked("steward", "devops", &grants), Covers::Covered);
    assert_eq!(asked("devops", "steward", &grants), Covers::NeedsGrant);
    assert_eq!(grants.level_of(Some("devops"), "steward"), None);
}

#[test]
fn any_persona_for_one_persona_allows_nothing_back_to_it() {
    let grants = InForce {
        you_any: vec!["steward".to_owned()],
        ..InForce::default()
    };
    assert_eq!(asked("steward", "devops", &grants), Covers::Covered);
    assert_eq!(asked("devops", "steward", &grants), Covers::NeedsGrant);
    assert_eq!(asked("devops", "qa", &grants), Covers::NeedsGrant);
}

// ---- never for a pair (V100-25) -----------------------------------------------------------------

#[test]
fn a_never_refuses_the_pair_whatever_grants_it_at_any_level() {
    let every = InForce {
        chat: vec![ChatPair {
            asking: Some("steward".to_owned()),
            target: "devops".to_owned(),
        }],
        you: vec![pair("steward", "devops")],
        project: vec![pair("steward", "devops")],
        you_any: vec!["steward".to_owned()],
        project_any: vec!["steward".to_owned()],
        never: never("steward", "devops"),
        never_unread: false,
        ..InForce::default()
    };
    assert_eq!(asked("steward", "devops", &every), Covers::Never);
    // Only that pair: the same persona to another target, and the pair the other way round.
    assert_eq!(asked("steward", "qa", &every), Covers::Covered);
    assert_eq!(asked("devops", "steward", &every), Covers::NeedsGrant);
    // With no grant at all it is still a never, and not a question.
    let alone = InForce {
        never: never("steward", "devops"),
        ..InForce::default()
    };
    assert_eq!(asked("steward", "devops", &alone), Covers::Never);
}

#[test]
fn a_never_is_kept_in_this_machine_s_record_and_never_in_the_project_s_file() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    super::never(root, &pair("steward", "devops")).expect("kept");
    super::never(root, &pair("steward", "devops")).expect("kept once");
    assert_eq!(nevers(root), never("steward", "devops"));
    // In a record of its own: not beside the grants, and not in the project's file.
    assert!(crate::dispatchnever::path(root).exists());
    assert!(!crate::sandbox::local::path(root).exists());
    assert!(!crate::names::manifest(root).exists());
    assert_eq!(
        asked("steward", "devops", &InForce::read(root, Vec::new())),
        Covers::Never
    );
}

#[test]
fn a_never_stands_until_it_is_lifted_and_then_the_pair_asks_again() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    super::never(root, &pair("steward", "devops")).expect("kept");
    assert!(lift_never(root, "steward", "devops").expect("lifted"));
    assert!(!lift_never(root, "steward", "devops").expect("gone"));
    assert_eq!(nevers(root), []);
    assert_eq!(
        asked("steward", "devops", &InForce::read(root, Vec::new())),
        Covers::NeedsGrant
    );
}

#[test]
fn lifting_a_never_leaves_a_grant_made_before_it_as_it_was() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    crate::sandbox::local::grant_dispatch(root, "steward", "devops").expect("kept");
    super::never(root, &pair("steward", "devops")).expect("kept");
    assert_eq!(
        asked("steward", "devops", &InForce::read(root, Vec::new())),
        Covers::Never
    );
    lift_never(root, "steward", "devops").expect("lifted");
    assert_eq!(
        asked("steward", "devops", &InForce::read(root, Vec::new())),
        Covers::Covered
    );
}

#[test]
fn a_policy_lock_is_said_before_a_never() {
    let under = Locks::parse(
        r#"{"owner": "IT", "dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    let grants = InForce {
        never: never("steward", "devops"),
        ..InForce::default()
    };
    assert!(matches!(
        covers(Some("steward"), "devops", &grants, &under),
        Covers::Locked(_)
    ));
}

#[test]
fn a_chat_reads_the_person_s_never_whole_and_where_it_is_lifted() {
    assert_eq!(
        never_said("steward", "devops"),
        "the person said never to steward chats dispatching to devops on this machine, so \
         nothing was started and they were not asked. Do not dispatch to devops again. Do this \
         work without devops, or tell the person it is waiting: only they lift it, in Settings \
         › Project › Dispatch."
    );
}

#[test]
fn a_chat_nobody_is_at_is_refused_a_pair_the_person_said_never_to_with_that_sentence() {
    use crate::dispatchunattended::{Answer, Refusal, covers as unattended};
    let grants = InForce {
        you: vec![pair("steward", "devops")],
        project_any: vec!["steward".to_owned()],
        never: never("steward", "devops"),
        ..InForce::default()
    };
    let answer = unattended(Some("steward"), "devops", &grants, &Locks::none(), true);
    assert_eq!(
        answer,
        Answer::Refused(Refusal::Never("steward".to_owned(), "devops".to_owned()))
    );
    let Answer::Refused(why) = answer else {
        unreachable!()
    };
    assert_eq!(why.say(), never_said("steward", "devops"));
}

// ---- any persona (V100-23) ----------------------------------------------------------------------

#[test]
fn any_persona_for_me_covers_every_target_and_one_added_later() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    allow_any(root, "steward", Level::You).expect("kept");
    allow_any(root, "steward", Level::You).expect("kept once");
    assert_eq!(any_yours(root), ["steward"]);
    let grants = InForce::read(root, Vec::new());
    // Nothing reads the project's personas: a name nobody had defined when it was granted is
    // covered like any other.
    for target in ["devops", "qa", "a-persona-added-next-year"] {
        assert_eq!(
            asked("steward", target, &grants),
            Covers::Covered,
            "{target}"
        );
        assert_eq!(grants.level_of(Some("steward"), target), Some(Level::You));
    }
    // For that asking persona alone, and never for a chat on no persona.
    assert_eq!(asked("qa", "devops", &grants), Covers::NeedsGrant);
    assert_eq!(
        covers(None, "devops", &grants, &Locks::none()),
        Covers::NeedsGrant
    );
    // In this machine's record, in a key of its own.
    let kept = std::fs::read_to_string(crate::sandbox::local::path(root)).expect("the record");
    assert!(kept.contains("\"dispatch_any\""), "{kept}");
    assert!(!kept.contains("\"dispatch_mine\""), "{kept}");

    assert_eq!(revoke_any(root, "steward", Level::You), Ok(true));
    assert_eq!(revoke_any(root, "steward", Level::You), Ok(false));
    assert_eq!(
        asked("steward", "devops", &InForce::read(root, Vec::new())),
        Covers::NeedsGrant
    );
}

#[test]
fn any_persona_is_never_granted_for_one_chat_or_for_what_is_no_persona() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    assert_eq!(
        allow_any(root, "steward", Level::Chat),
        Err(
            "Any persona is granted for you on this machine or for everyone in this project, \
             never for one chat."
                .to_owned()
        )
    );
    for asking in ["*", "", "Not A Name"] {
        assert!(
            allow_any(root, asking, Level::You)
                .expect_err("refused")
                .ends_with("is not a persona's name, so purlis keeps no dispatch grant for it."),
            "{asking}"
        );
    }
    assert_eq!(any_yours(root), Vec::<String>::new());
    assert!(!crate::sandbox::local::path(root).exists());
}

#[test]
fn a_policy_lock_holds_under_any_persona() {
    let under = Locks::parse(
        r#"{"owner": "IT", "dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    let grants = InForce {
        you_any: vec!["steward".to_owned()],
        project_any: vec!["steward".to_owned()],
        ..InForce::default()
    };
    assert!(matches!(
        covers(Some("steward"), "devops", &grants, &under),
        Covers::Locked(_)
    ));
    assert_eq!(
        covers(Some("steward"), "qa", &grants, &under),
        Covers::Covered
    );
}

#[test]
fn any_persona_leaves_the_rule_for_a_chat_nobody_is_at_as_it_was() {
    use crate::dispatchunattended::{Answer, Refusal, covers as unattended};
    let grants = InForce {
        you_any: vec!["steward".to_owned()],
        ..InForce::default()
    };
    let none = Locks::none();
    // A standing grant, so it counts for a sandboxed chat nobody is at.
    assert_eq!(
        unattended(Some("steward"), "devops", &grants, &none, true),
        Answer::Covered
    );
    // With no sandbox it still dispatches to no other persona.
    assert_eq!(
        unattended(Some("steward"), "devops", &grants, &none, false),
        Answer::Refused(Refusal::Unsandboxed("devops".to_owned()))
    );
    // And a persona without it is refused, never asked.
    assert!(matches!(
        unattended(Some("qa"), "devops", &grants, &none, true),
        Answer::Refused(Refusal::Missing(_))
    ));
}

#[test]
fn any_persona_does_not_lift_the_loop_rule() {
    use crate::dispatchdecision::{Asker, AskingChat, Decision, Mode, Persona, Refused, Request};
    use crate::dispatchlimits::{self, Lineage};
    // devops, dispatched by steward, asks to dispatch back to steward, and devops chats may
    // dispatch to any persona. The grant covers the pair; the loop rule refuses it all the
    // same, since it reads who is above the asking chat and never a grant.
    let grants = InForce {
        you_any: vec!["devops".to_owned()],
        project_any: vec!["devops".to_owned()],
        ..InForce::default()
    };
    let grant = asked("devops", "steward", &grants);
    assert_eq!(grant, Covers::Covered);
    let limits = dispatchlimits::in_force(
        &dispatchlimits::Table::default(),
        None,
        Some("devops"),
        Some("steward"),
        &dispatchlimits::Table::default(),
        &dispatchlimits::Level::unset(),
    );
    let lineage = Lineage {
        depth: 1,
        chain: vec![Some("steward".to_owned())],
        lineage: 2,
        ..Lineage::default()
    };
    let decision = crate::dispatchdecision::decide(&Request {
        asker: Asker::Chat(AskingChat {
            persona: Some("devops"),
            held: false,
        }),
        to: Persona::Defined("steward"),
        mode: Mode::Task,
        grant: &grant,
        profile: None,
        limits: &limits,
        lineage: &lineage,
    });
    assert_eq!(
        decision,
        Decision::Refused(Refused::Limit(dispatchlimits::Refused::Loop(
            "steward".to_owned()
        )))
    );
}

// ---- any persona, in the project's file ---------------------------------------------------------

#[test]
fn the_project_s_file_grants_any_persona_as_a_star_in_a_persona_s_list() {
    let text = "[dispatch.grants]\nsteward = [\"devops\", \"*\", \"*\"]\nqa = [\"*\"]\n";
    let read = committed(Some(text));
    assert_eq!(read.pairs, [pair("steward", "devops")]);
    assert_eq!(read.any, ["steward", "qa"]);
    assert_eq!(read.refused, Vec::<String>::new());
}

#[test]
fn a_star_where_the_asking_persona_goes_grants_nothing() {
    let read = committed(Some(
        "[dispatch.grants]\n\"*\" = [\"devops\"]\n\"Bad Name\" = [\"*\"]\n",
    ));
    assert_eq!(read.pairs, []);
    assert_eq!(read.any, Vec::<String>::new());
    assert_eq!(
        read.refused,
        [
            "* is not a persona's name, so purlis keeps no dispatch grant for it.",
            "Bad Name is not a persona's name, so purlis keeps no dispatch grant for it.",
        ]
    );
    // And a pattern is not a star: only the one spelling means any persona.
    let read = committed(Some("[dispatch.grants]\nsteward = [\"dev*\", \"**\"]\n"));
    assert_eq!((read.pairs, read.any), (vec![], vec![]));
    assert_eq!(read.refused.len(), 2);
}

#[test]
fn any_persona_is_written_into_the_project_s_text_and_taken_out_with_every_other_line_kept() {
    use crate::settings::dispatch::{with, with_any, without, without_any};
    let before = "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\"]\n";
    let after = with_any(before, "steward").expect("written");
    assert_eq!(
        after,
        "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"*\"]\n"
    );
    assert_eq!(with_any(&after, "steward").as_deref(), Ok(after.as_str()));
    assert_eq!(without_any(&after, "steward").as_deref(), Ok(before));
    // Taking the named pair out leaves any persona, and the other way round.
    assert_eq!(
        without(&after, &pair("steward", "devops")).as_deref(),
        Ok("schema = 1\n\n[dispatch.grants]\nsteward = [\"*\"]\n")
    );
    // A pair is still written by its name, and never as a star.
    assert!(
        with("schema = 1\n", &pair("steward", "devops"))
            .expect("written")
            .ends_with("steward = [\"devops\"]\n")
    );
    assert!(with_any("schema = 1\n", "*").is_err());
}

// ---- a record edited by hand fails closed -------------------------------------------------------

#[test]
fn a_star_written_by_hand_among_your_named_grants_grants_nothing() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    by_hand(
        root,
        r#"{"dispatch_mine": [{"asking": "steward", "target": "*"}, {"asking": "*", "target": "devops"}]}"#,
    );
    let grants = InForce::read(root, Vec::new());
    assert_eq!((grants.you.len(), grants.you_any.len()), (0, 0));
    assert_eq!(asked("steward", "devops", &grants), Covers::NeedsGrant);
    assert_eq!(asked("qa", "devops", &grants), Covers::NeedsGrant);
}

#[test]
fn a_star_written_by_hand_as_who_may_dispatch_to_anyone_grants_nothing() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    by_hand(root, r#"{"dispatch_any": ["*", "", "Not A Name"]}"#);
    let grants = InForce::read(root, Vec::new());
    assert_eq!(grants.you_any, Vec::<String>::new());
    assert_eq!(asked("steward", "devops", &grants), Covers::NeedsGrant);
}

#[test]
fn an_accepted_star_with_nothing_in_the_project_s_file_grants_nothing() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    // Both records a Notice's answer or a hand could leave, and no file that grants it.
    by_hand(
        root,
        r#"{"dispatch_seen": ["steward -> *", "steward -> devops"], "dispatch_any_seen": ["steward"]}"#,
    );
    let grants = InForce::read(root, Vec::new());
    assert_eq!(grants.project_any, Vec::<String>::new());
    assert_eq!(asked("steward", "devops", &grants), Covers::NeedsGrant);
    // With no project file to read, nothing stored is changed either.
    assert_eq!(crate::sandbox::local::dispatch_any_seen(root), ["steward"]);
}

#[test]
fn a_never_written_by_hand_is_held_as_written() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    by_hand(
        root,
        r#"{"dispatch_mine": [{"asking": "steward", "target": "devops"}],
            "dispatch_any": ["steward"]}"#,
    );
    std::fs::write(
        crate::dispatchnever::path(root),
        r#"{"never": [{"asking": "steward", "target": "devops"},
                      {"asking": "steward", "target": "steward"},
                      {"asking": "steward", "target": "*"}]}"#,
    )
    .expect("written by hand");
    let grants = InForce::read(root, Vec::new());
    assert_eq!(asked("steward", "devops", &grants), Covers::Never);
    // One naming a persona twice is held too: nothing reads it as a mistake to step over.
    assert_eq!(asked("steward", "steward", &grants), Covers::Never);
    // A star in a never is no persona's name: it refuses nothing, and widens nothing.
    assert_eq!(asked("steward", "qa", &grants), Covers::Covered);
    // Each can be lifted as it is spelled.
    assert!(lift_never(root, "steward", "steward").expect("lifted"));
    assert!(lift_never(root, "steward", "*").expect("lifted"));
    assert_eq!(nevers(root), never("steward", "devops"));
}

#[test]
fn a_record_that_does_not_read_grants_nothing_at_all() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    for broken in [
        "not json",
        r#"{"dispatch_any": "steward"}"#,
        r#"{"dispatch_mine": [{"asking": "steward", "target": "devops"}], "hosts_mine": 3}"#,
    ] {
        by_hand(root, broken);
        let grants = InForce::read(root, Vec::new());
        assert_eq!(grants, InForce::default(), "{broken}");
        assert_eq!(asked("steward", "devops", &grants), Covers::NeedsGrant);
    }
}

// ---- the audit ----------------------------------------------------------------------------------

#[test]
fn a_never_its_lifting_and_any_persona_are_trust_events_of_their_own() {
    let said = Audited {
        act: Act::Never,
        asking: Some("steward"),
        target: "devops",
        level: Level::You,
        workspace: None,
    };
    assert_eq!(said.kind(), "trust.dispatch.never");
    assert_eq!(
        said.body(),
        serde_json::json!({
            "actor_kind": "human",
            "actor": "operator",
            "scope": "local-ui",
            "level": "you",
            "asking": "steward",
            "target": "devops",
            "workspace": null,
        })
    );
    assert_eq!(
        Audited {
            act: Act::LiftNever,
            ..said
        }
        .kind(),
        "trust.dispatch.never.lift"
    );
    let any = Audited {
        act: Act::Grant,
        asking: Some("steward"),
        target: ANY,
        level: Level::Project,
        workspace: None,
    };
    assert_eq!(any.kind(), "trust.dispatch.grant");
    assert_eq!(any.body()["target"], "*");
}

// ---- any persona for everyone in the project, on disk -------------------------------------------

/// A project whose committed file is `shared`.
fn project(shared: &str) -> tempfile::TempDir {
    let project = tempfile::tempdir().expect("a project");
    std::fs::write(crate::names::manifest(project.path()), shared).expect("the file");
    project
}

#[test]
fn any_persona_for_everyone_is_written_as_a_star_and_is_in_force_for_whoever_set_it() {
    let project = project("schema = 1\n");
    let root = project.path();
    allow_any(root, "steward", Level::Project).expect("kept");
    let text = std::fs::read_to_string(crate::names::manifest(root)).expect("the file");
    assert_eq!(text, "schema = 1\n\n[dispatch.grants]\nsteward = [\"*\"]\n");
    let grants = InForce::read(root, Vec::new());
    assert_eq!(grants.project_any, ["steward"]);
    assert_eq!(
        grants.level_of(Some("steward"), "someone-new"),
        Some(Level::Project)
    );
    // No pair was made of it, and whoever set it is told of nothing arriving.
    assert_eq!(committed_at(root), []);
    assert_eq!(
        crate::dispatcharrival::arrival(root),
        crate::dispatcharrival::Arrival::default()
    );

    assert_eq!(revoke_any(root, "steward", Level::Project), Ok(true));
    assert_eq!(revoke_any(root, "steward", Level::Project), Ok(false));
    assert_eq!(
        std::fs::read_to_string(crate::names::manifest(root)).expect("the file"),
        "schema = 1\n"
    );
    assert_eq!(InForce::read(root, Vec::new()), InForce::default());
}

#[test]
fn a_star_a_pull_brought_in_covers_nothing_until_it_is_accepted_in_settings() {
    let project = project("schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"*\"]\n");
    let root = project.path();
    assert_eq!(any_committed_at(root), ["steward"]);
    assert_eq!(any_unaccepted(root), ["steward"]);
    assert_eq!(InForce::read(root, Vec::new()), InForce::default());

    // No answer to a Notice accepts it: not the project's Notice, whatever it sends back,
    // and not an Allow for everyone on a chat's tab, which names its one pair.
    acknowledge(
        root,
        &[
            "steward -> devops".to_owned(),
            "steward -> *".to_owned(),
            "steward".to_owned(),
            "*".to_owned(),
        ],
    )
    .expect("kept");
    acknowledge_pair(root, &pair("steward", "devops")).expect("kept");
    crate::settings::dispatch::grant(root, &pair("steward", "qa")).expect("kept");
    let grants = InForce::read(root, Vec::new());
    assert_eq!(grants.project_any, Vec::<String>::new());
    assert_eq!(
        grants.project,
        [pair("steward", "devops"), pair("steward", "qa")]
    );
    assert_eq!(asked("steward", "prod", &grants), Covers::NeedsGrant);
    assert_eq!(any_unaccepted(root), ["steward"]);

    // Settings' explicit grant does, and writes nothing the file holds already.
    let before = std::fs::read_to_string(crate::names::manifest(root)).expect("the file");
    allow_any(root, "steward", Level::Project).expect("accepted");
    assert_eq!(
        std::fs::read_to_string(crate::names::manifest(root)).expect("the file"),
        before
    );
    assert_eq!(any_unaccepted(root), Vec::<String>::new());
    assert_eq!(
        asked("steward", "prod", &InForce::read(root, Vec::new())),
        Covers::Covered
    );
}

#[test]
fn the_project_s_file_cannot_lift_your_never() {
    let project = project("schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"*\"]\n");
    let root = project.path();
    super::never(root, &pair("steward", "devops")).expect("kept");
    // Everything the file can say for the pair, accepted here in every way there is.
    acknowledge_pair(root, &pair("steward", "devops")).expect("kept");
    allow_any(root, "steward", Level::Project).expect("accepted");
    let grants = InForce::read(root, Vec::new());
    assert_eq!(asked("steward", "devops", &grants), Covers::Never);
    assert_eq!(asked("steward", "qa", &grants), Covers::Covered);
    // And no key of the project's file is read as a never, or as its lifting.
    std::fs::write(
        crate::names::manifest(root),
        "schema = 1\n\n[dispatch]\nnever = []\n\n[dispatch.grants]\nsteward = [\"devops\"]\n\n\
         [dispatch.never]\nsteward = []\n",
    )
    .expect("another push");
    assert_eq!(
        asked("steward", "devops", &InForce::read(root, Vec::new())),
        Covers::Never
    );
}

#[test]
fn any_persona_for_everyone_needs_a_project_file_to_be_written_into() {
    let project = tempfile::tempdir().expect("no file");
    let root = project.path();
    assert!(can_allow_any(root, "steward", Level::Project).is_err());
    assert!(allow_any(root, "steward", Level::Project).is_err());
    assert_eq!(InForce::read(root, Vec::new()), InForce::default());
    assert_eq!(can_allow_any(root, "steward", Level::You), Ok(()));
}

#[test]
fn a_star_the_file_on_disk_does_not_hold_covers_nothing_and_its_acceptance_is_not_dropped() {
    let with_it = "schema = 1\n\n[dispatch.grants]\nsteward = [\"*\"]\n";
    let project = project(with_it);
    let root = project.path();
    allow_any(root, "steward", Level::Project).expect("accepted");
    assert_eq!(InForce::read(root, Vec::new()).project_any, ["steward"]);

    // The file on disk is without it (a branch switched, a hand's edit): it covers nothing.
    // Nothing is dropped for that alone: only a commit that took it out drops an acceptance
    // (#1506, `dispatcharrival`), and this project has no history to read.
    std::fs::write(crate::names::manifest(root), "schema = 1\n").expect("another file");
    assert_eq!(InForce::read(root, Vec::new()), InForce::default());
    assert_eq!(crate::sandbox::local::dispatch_any_seen(root), ["steward"]);

    // And back: in force again, with nothing asked.
    std::fs::write(crate::names::manifest(root), with_it).expect("the file as it was");
    assert_eq!(InForce::read(root, Vec::new()).project_any, ["steward"]);
    assert_eq!(any_unaccepted(root), Vec::<String>::new());
}

#[test]
fn a_project_file_that_does_not_read_for_a_moment_changes_nothing_that_is_stored() {
    let with_it = "schema = 1\n\n[dispatch.grants]\nsteward = [\"*\"]\n";
    let project = project(with_it);
    let root = project.path();
    allow_any(root, "steward", Level::Project).expect("accepted");

    // A merge left half done, then no file at all: nothing is granted meanwhile, and the
    // acceptance is still there when the file reads again.
    for broken in [
        Some("<<<<<<< HEAD\nschema = 1\n=======\nschema = 2\n>>>>>>> theirs\n"),
        None,
    ] {
        match broken {
            Some(text) => std::fs::write(crate::names::manifest(root), text).expect("a conflict"),
            None => std::fs::remove_file(crate::names::manifest(root)).expect("gone"),
        }
        assert_eq!(
            InForce::read(root, Vec::new()).project_any,
            Vec::<String>::new()
        );
        assert_eq!(crate::sandbox::local::dispatch_any_seen(root), ["steward"]);
    }
    std::fs::write(crate::names::manifest(root), with_it).expect("resolved");
    assert_eq!(InForce::read(root, Vec::new()).project_any, ["steward"]);
}

// ---- any persona covers personas, and nothing that is not one -----------------------------------

#[test]
fn any_persona_covers_no_target_that_is_not_a_persona_s_name() {
    let grants = InForce {
        you_any: vec!["steward".to_owned()],
        project_any: vec!["steward".to_owned()],
        ..InForce::default()
    };
    for target in ["*", "Devops", "dev ops", "../devops", ""] {
        assert_eq!(
            asked("steward", target, &grants),
            Covers::NeedsGrant,
            "{target:?}"
        );
        assert_eq!(grants.level_of(Some("steward"), target), None, "{target:?}");
    }
    assert_eq!(asked("steward", "devops", &grants), Covers::Covered);
}

#[test]
fn only_a_grant_that_names_the_pair_is_a_named_one() {
    let grants = InForce {
        chat: vec![ChatPair {
            asking: Some("steward".to_owned()),
            target: "prod".to_owned(),
        }],
        you: vec![pair("steward", "qa")],
        project: vec![pair("steward", "devops")],
        you_any: vec!["steward".to_owned()],
        project_any: vec!["steward".to_owned()],
        ..InForce::default()
    };
    assert_eq!(
        grants.named_level_of(Some("steward"), "devops"),
        Some(Level::Project)
    );
    assert_eq!(
        grants.named_level_of(Some("steward"), "qa"),
        Some(Level::You)
    );
    // Not one chat's, and not "any persona".
    assert_eq!(grants.named_level_of(Some("steward"), "prod"), None);
    assert_eq!(grants.named_level_of(Some("steward"), "someone-new"), None);
    assert_eq!(
        grants.level_of(Some("steward"), "someone-new"),
        Some(Level::Project)
    );
}

// ---- a never holds down the chain ---------------------------------------------------------------

fn above(personas: &[Option<&str>]) -> Vec<Option<String>> {
    personas.iter().map(|one| one.map(str::to_owned)).collect()
}

#[test]
fn a_never_for_a_persona_refuses_its_target_to_every_chat_below_a_chat_of_that_persona() {
    let none = Locks::none();
    // steward never dispatches to devops; qa may, by name, and steward may dispatch to qa.
    let grants = InForce {
        you: vec![pair("steward", "qa"), pair("qa", "devops")],
        you_any: vec!["qa".to_owned()],
        never: never("steward", "devops"),
        ..InForce::default()
    };
    // steward to qa to devops: refused, naming the pair the person refused.
    for chain in [
        above(&[Some("steward")]),
        // However far above, and past a chat on no persona.
        above(&[Some("ops"), None, Some("steward")]),
    ] {
        assert_eq!(
            covers_in_chain(Some("qa"), "devops", &grants, &none, &chain),
            Covers::NeverAbove("steward".to_owned()),
            "{chain:?}"
        );
    }
    // qa to devops with no steward chat above it is as it was.
    for chain in [above(&[]), above(&[Some("ops"), None])] {
        assert_eq!(
            covers_in_chain(Some("qa"), "devops", &grants, &none, &chain),
            Covers::Covered,
            "{chain:?}"
        );
    }
    // Below steward, qa still reaches every other target.
    assert_eq!(
        covers_in_chain(
            Some("qa"),
            "prod",
            &grants,
            &none,
            &above(&[Some("steward")])
        ),
        Covers::Covered
    );
    // It is refused where it would have been asked, too, and qa's own persona below it.
    let bare = InForce {
        never: never("steward", "devops"),
        ..InForce::default()
    };
    assert_eq!(
        covers_in_chain(
            Some("qa"),
            "devops",
            &bare,
            &none,
            &above(&[Some("steward")])
        ),
        Covers::NeverAbove("steward".to_owned())
    );
    assert_eq!(
        covers_in_chain(
            Some("devops"),
            "devops",
            &bare,
            &none,
            &above(&[Some("steward")])
        ),
        Covers::NeverAbove("steward".to_owned())
    );
}

#[test]
fn a_policy_lock_and_the_asking_chat_s_own_never_are_said_before_one_from_above() {
    let under = Locks::parse(
        r#"{"owner": "IT", "dispatch": {"locked": [{"from": "qa", "to": "devops"}]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    let grants = InForce {
        never: vec![
            ("steward".to_owned(), "devops".to_owned()),
            ("qa".to_owned(), "prod".to_owned()),
            ("steward".to_owned(), "prod".to_owned()),
        ],
        ..InForce::default()
    };
    let chain = above(&[Some("steward")]);
    assert!(matches!(
        covers_in_chain(Some("qa"), "devops", &grants, &under, &chain),
        Covers::Locked(_)
    ));
    assert_eq!(
        covers_in_chain(Some("qa"), "prod", &grants, &Locks::none(), &chain),
        Covers::Never
    );
}

#[test]
fn a_chat_reads_who_above_it_the_person_refused_and_what_to_do() {
    assert_eq!(
        never_above_said("steward", "devops"),
        "the person said never to steward chats dispatching to devops on this machine, and \
         this chat works for a steward chat: one is above it in its chain. So nothing was \
         started and they were not asked. Do not dispatch to devops for this work. Do it \
         without devops, or say in your report that it is waiting: only the person lifts it, \
         in Settings › Project › Dispatch."
    );
}

// ---- the one decision reads a never itself ------------------------------------------------------

/// What the decision says of a task from a `qa` chat to `devops` under `grant`, asked by a
/// chat or by the person from its tab.
fn decided(grant: &Covers, by_the_person: bool) -> crate::dispatchdecision::Decision {
    use crate::dispatchdecision::{Asker, AskingChat, Mode, Persona, Request};
    use crate::dispatchlimits::{self, Lineage};
    let limits = dispatchlimits::in_force(
        &dispatchlimits::Table::default(),
        None,
        Some("qa"),
        Some("devops"),
        &dispatchlimits::Table::default(),
        &dispatchlimits::Level::unset(),
    );
    let chat = AskingChat {
        persona: Some("qa"),
        held: false,
    };
    crate::dispatchdecision::decide(&Request {
        asker: if by_the_person {
            Asker::Person(chat)
        } else {
            Asker::Chat(chat)
        },
        to: Persona::Defined("devops"),
        mode: Mode::Task,
        grant,
        profile: None,
        limits: &limits,
        lineage: &Lineage {
            lineage: 1,
            ..Lineage::default()
        },
    })
}

#[test]
fn the_decision_refuses_a_never_and_asks_nobody() {
    use crate::dispatchdecision::{Decision, Refused};
    assert_eq!(
        decided(&Covers::Never, false),
        Decision::Refused(Refused::Never(never_said("qa", "devops")))
    );
    assert_eq!(
        decided(&Covers::NeverAbove("steward".to_owned()), false),
        Decision::Refused(Refused::Never(never_above_said("steward", "devops")))
    );
    assert_eq!(
        Refused::Never(never_said("qa", "devops")).say(),
        never_said("qa", "devops")
    );
}

#[test]
fn the_decision_asks_the_person_while_the_record_of_nevers_does_not_read() {
    use crate::dispatchdecision::Decision;
    assert_eq!(
        decided(&Covers::Unread, false),
        Decision::NeedsGrant {
            from: Some("qa".to_owned()),
            to: "devops".to_owned(),
        }
    );
}

#[test]
fn the_person_s_own_dispatch_from_a_tab_is_not_held_to_their_never() {
    use crate::dispatchdecision::Decision;
    for grant in [
        Covers::Never,
        Covers::NeverAbove("steward".to_owned()),
        Covers::Unread,
        Covers::NeedsGrant,
    ] {
        assert_eq!(decided(&grant, true), Decision::Start, "{grant:?}");
    }
}
