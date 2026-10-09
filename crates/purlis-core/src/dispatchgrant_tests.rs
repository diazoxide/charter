//! A dispatch grant (#1437): who may dispatch to whom, at which level, under which policy, and
//! what a covered dispatch starts with. Every expected answer is written out.

use std::path::Path;

use super::*;
use crate::sandbox::policy::Locks;
use crate::sandbox::{Compiled, Machine, Os, Plane};

const FILE: &str = "/etc/purlis/policy.json";

fn locks(json: &str) -> Locks {
    Locks::parse(json, Path::new(FILE))
}

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

fn chat(asking: Option<&str>, target: &str) -> ChatPair {
    ChatPair {
        asking: asking.map(str::to_owned),
        target: target.to_owned(),
    }
}

// ---- the one question ---------------------------------------------------------------------------

#[test]
fn a_chat_dispatching_to_its_own_persona_is_covered_with_no_grant() {
    assert_eq!(
        covers(
            Some("devops"),
            "devops",
            &InForce::default(),
            &Locks::none()
        ),
        Covers::Covered
    );
}

#[test]
fn another_persona_needs_a_grant_until_one_is_in_force() {
    assert_eq!(
        covers(
            Some("steward"),
            "devops",
            &InForce::default(),
            &Locks::none()
        ),
        Covers::NeedsGrant
    );
}

#[test]
fn a_grant_at_each_level_covers_its_pair_and_no_other() {
    for grants in [
        InForce {
            chat: vec![chat(Some("steward"), "devops")],
            ..InForce::default()
        },
        InForce {
            you: vec![pair("steward", "devops")],
            ..InForce::default()
        },
        InForce {
            project: vec![pair("steward", "devops")],
            ..InForce::default()
        },
    ] {
        let none = Locks::none();
        assert_eq!(
            covers(Some("steward"), "devops", &grants, &none),
            Covers::Covered,
            "{grants:?}"
        );
        // Not the pair the other way round, another target, or another asking persona.
        for (asking, target) in [
            (Some("devops"), "steward"),
            (Some("steward"), "qa"),
            (Some("qa"), "devops"),
            (None, "devops"),
        ] {
            assert_eq!(
                covers(asking, target, &grants, &none),
                Covers::NeedsGrant,
                "{asking:?} to {target} under {grants:?}"
            );
        }
    }
}

#[test]
fn a_chat_on_no_persona_is_covered_only_by_a_grant_for_that_chat() {
    let grants = InForce {
        chat: vec![chat(None, "devops")],
        ..InForce::default()
    };
    assert_eq!(
        covers(None, "devops", &grants, &Locks::none()),
        Covers::Covered
    );
    // A grant made for a chat on no persona covers no persona's chats.
    assert_eq!(
        covers(Some("steward"), "devops", &grants, &Locks::none()),
        Covers::NeedsGrant
    );
}

#[test]
fn the_widest_level_a_pair_is_held_at_is_the_one_named() {
    let grants = InForce {
        chat: vec![chat(Some("steward"), "devops")],
        you: vec![pair("steward", "devops"), pair("steward", "qa")],
        project: vec![pair("steward", "devops")],
        ..InForce::default()
    };
    assert_eq!(
        grants.level_of(Some("steward"), "devops"),
        Some(Level::Project)
    );
    assert_eq!(grants.level_of(Some("steward"), "qa"), Some(Level::You));
    assert_eq!(grants.level_of(Some("qa"), "devops"), None);
}

// ---- policy -------------------------------------------------------------------------------------

#[test]
fn a_locked_pair_is_locked_naming_who_set_it_and_a_grant_already_made_does_not_cover_it() {
    let under =
        locks(r#"{"owner": "IT", "dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#);
    let granted = InForce {
        project: vec![pair("steward", "devops")],
        ..InForce::default()
    };
    for grants in [InForce::default(), granted] {
        assert_eq!(
            covers(Some("steward"), "devops", &grants, &under),
            Covers::Locked(
                "Policy forbids steward chats dispatching to devops. Locked by policy, set by \
                 IT in /etc/purlis/policy.json."
                    .to_owned()
            )
        );
    }
    // Only that pair: the other way round is the person's to grant, and one's own persona
    // needs none.
    assert_eq!(
        covers(Some("devops"), "steward", &InForce::default(), &under),
        Covers::NeedsGrant
    );
    assert_eq!(
        covers(Some("steward"), "steward", &InForce::default(), &under),
        Covers::Covered
    );
}

#[test]
fn a_pair_lock_naming_one_persona_twice_locks_that_persona_s_dispatch_to_itself() {
    let under =
        locks(r#"{"owner": "IT", "dispatch": {"locked": [{"from": "devops", "to": "devops"}]}}"#);
    assert_eq!(
        covers(Some("devops"), "devops", &InForce::default(), &under),
        Covers::Locked(
            "Policy forbids devops chats dispatching to devops. Locked by policy, set by IT in \
             /etc/purlis/policy.json."
                .to_owned()
        )
    );
    // Another persona's own is untouched.
    assert_eq!(
        covers(Some("steward"), "steward", &InForce::default(), &under),
        Covers::Covered
    );
}

#[test]
fn a_lock_on_all_dispatch_locks_every_pair_and_a_chat_s_own_persona_too() {
    let under = locks(r#"{"owner": "IT", "dispatch": {"allow": false}}"#);
    let said = Covers::Locked(
        "Policy forbids one chat dispatching to another. Locked by policy, set by IT in \
         /etc/purlis/policy.json."
            .to_owned(),
    );
    let granted = InForce {
        you: vec![pair("steward", "devops")],
        ..InForce::default()
    };
    assert_eq!(covers(Some("steward"), "devops", &granted, &under), said);
    assert_eq!(
        covers(Some("devops"), "devops", &InForce::default(), &under),
        said
    );
    assert_eq!(covers(None, "devops", &InForce::default(), &under), said);
}

#[test]
fn a_policy_with_limits_and_locks_in_one_dispatch_object_is_read_for_both() {
    use crate::dispatchlimits::Limit;
    let under = locks(
        r#"{"owner": "IT", "dispatch": {"depth": 2, "may-run-at-once": 3, "allow": true,
            "locked": [{"from": "steward", "to": "devops"}]}}"#,
    );
    assert_eq!(under.refused_because(), None);
    assert_eq!(under.dispatch_ceiling().get(Limit::Depth), Some(2));
    assert_eq!(under.dispatch_ceiling().get(Limit::MayRunAtOnce), Some(3));
    assert_eq!(under.dispatch_ceiling().get(Limit::RunningPerChat), None);
    assert!(!under.forbids_dispatch());
    assert!(matches!(
        covers(Some("steward"), "devops", &InForce::default(), &under),
        Covers::Locked(_)
    ));
    // A key that is neither a lock nor a limit still refuses the file, which then locks all
    // dispatch and puts every limit at 0.
    let refused = locks(r#"{"dispatch": {"depth": 2, "allow": true, "sideways": 1}}"#);
    assert_eq!(
        refused.refused_because(),
        Some("its dispatch says \"sideways\", which purlis does not know")
    );
    assert!(refused.forbids_dispatch());
    assert_eq!(refused.dispatch_ceiling().get(Limit::Depth), Some(0));
}

#[test]
fn the_project_s_grants_sit_in_the_limits_table_and_neither_reader_refuses_the_other_s() {
    let text = "[dispatch]\ndepth = 2\n\n[dispatch.grants]\nsteward = [\"devops\"]\n\n\
                [dispatch.personas.devops]\nmay-run-at-once = 1\n";
    assert_eq!(committed(Some(text)).pairs, [pair("steward", "devops")]);
    assert_eq!(committed(Some(text)).refused, Vec::<String>::new());
    let file = crate::settings::Which::Shared.file();
    assert_eq!(
        crate::dispatchlimits::refusals(text, file),
        Vec::<String>::new()
    );
    // One refusal list: what `grants` holds that grants nothing is said with the limits'.
    let bad = "[dispatch]\nsideways = 1\n\n[dispatch.grants]\nsteward = \"devops\"\n";
    let refused = crate::dispatchlimits::refusals(bad, file);
    assert_eq!(refused.len(), 2, "{refused:?}");
    assert!(
        refused[0].starts_with("dispatch.sideways in "),
        "{refused:?}"
    );
    assert_eq!(
        refused[1],
        "dispatch.grants.steward is not a list of personas, so it grants nothing"
    );
    // This machine's own file holds limits that only lower, and never a grant.
    let mine = crate::dispatchlimits::refusals(text, crate::profiles::LOCAL_FILE);
    assert_eq!(mine.len(), 1, "{mine:?}");
    assert!(mine[0].starts_with("dispatch.grants in "), "{mine:?}");
    assert!(mine[0].contains("is not read"), "{mine:?}");
}

#[test]
fn a_policy_that_says_nothing_of_dispatch_locks_none_of_it() {
    for text in [
        r#"{"owner": "IT"}"#,
        r#"{"dispatch": {}}"#,
        r#"{"dispatch": {"allow": true, "locked": []}}"#,
    ] {
        let under = locks(text);
        assert!(under.refused_because().is_none(), "{text}");
        assert!(!under.forbids_dispatch(), "{text}");
        assert_eq!(under.dispatch_refused(Some("steward"), "devops"), None);
    }
}

#[test]
fn a_policy_file_purlis_refuses_locks_all_dispatch() {
    for text in [
        "not json",
        r#"{"dispatch": {"allow": "no"}}"#,
        r#"{"dispatch": {"locked": [{"from": "steward"}]}}"#,
        r#"{"dispatch": {"locked": [{"from": "Not A Name", "to": "devops"}]}}"#,
        r#"{"dispatch": {"sideways": true}}"#,
        r#"{"dispatch": false}"#,
    ] {
        let under = locks(text);
        assert!(under.refused_because().is_some(), "{text}");
        assert!(
            matches!(
                covers(Some("steward"), "devops", &InForce::default(), &under),
                Covers::Locked(why) if why.contains("is refused")
            ),
            "{text}"
        );
    }
}

// ---- the project's grants, as the committed file writes them ------------------------------------

#[test]
fn the_committed_file_s_grants_are_read_pair_by_pair_in_file_order() {
    let text = "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"qa\", \"devops\"]\n\
                qa = [\"devops\"]\n";
    assert_eq!(
        committed(Some(text)),
        Committed {
            pairs: vec![
                pair("steward", "devops"),
                pair("steward", "qa"),
                pair("qa", "devops")
            ],
            any: vec![],
            limited: vec![],
            refused: vec![],
        }
    );
}

#[test]
fn no_file_no_table_and_a_file_that_is_not_toml_grant_nothing() {
    for text in [
        None,
        Some("schema = 1\n"),
        Some("[dispatch]\n"),
        Some("not toml ["),
    ] {
        assert_eq!(committed(text), Committed::default(), "{text:?}");
    }
}

#[test]
fn what_is_not_a_persona_to_a_list_of_personas_grants_nothing_and_says_so() {
    let text = "[dispatch.grants]\nsteward = \"devops\"\nqa = [\"devops\", 3, \"Not A Name\", \
                \"qa\"]\n\"Bad Name\" = [\"devops\"]\n";
    let read = committed(Some(text));
    assert_eq!(read.pairs, [pair("qa", "devops")]);
    assert_eq!(
        read.refused,
        [
            "dispatch.grants.steward is not a list of personas, so it grants nothing",
            "dispatch.grants.qa holds something that is not a persona's name, which grants \
             nothing",
            "Not A Name is not a persona's name, so purlis keeps no dispatch grant for it.",
            "A qa chat dispatches to qa with no grant, so there is none to keep.",
            "Bad Name is not a persona's name, so purlis keeps no dispatch grant for it.",
        ]
    );
    assert_eq!(
        committed(Some("[dispatch]\ngrants = [\"steward\"]\n")).refused,
        [
            "dispatch.grants is not a table, so it grants nothing: write the asking persona, \
             then the personas it may dispatch to, as steward = [\"devops\"]"
        ]
    );
}

#[test]
fn a_pair_is_written_and_read_back_as_asking_arrow_target() {
    let one = pair("steward", "dev-ops.2");
    assert_eq!(one.to_string(), "steward -> dev-ops.2");
    assert_eq!(Pair::parse("steward -> dev-ops.2"), Some(one));
    assert_eq!(Pair::parse("steward"), None);
    assert_eq!(Pair::parse("steward -> steward"), None);
}

// ---- the teammate's one-time Notice -------------------------------------------------------------

#[test]
fn a_change_names_what_was_added_and_taken_away_and_order_is_no_change() {
    let seen = ["steward -> devops".to_owned(), "qa -> devops".to_owned()];
    assert_eq!(
        change_between(&seen, &[pair("qa", "devops"), pair("steward", "devops")]),
        None
    );
    assert_eq!(
        change_between(&seen, &[pair("steward", "devops"), pair("steward", "qa")]),
        Some(Change {
            added: vec!["steward -> qa".to_owned()],
            removed: vec!["qa -> devops".to_owned()],
            now: vec!["steward -> devops".to_owned(), "steward -> qa".to_owned()],
        })
    );
    // A project first seen on this machine with grants is a change; one with none is not.
    assert!(change_between(&[], &[pair("steward", "devops")]).is_some());
    assert_eq!(change_between(&[], &[]), None);
}

// ---- the audit ----------------------------------------------------------------------------------

#[test]
fn a_grant_and_a_revoke_are_trust_events_naming_the_pair_and_the_level() {
    let made = Audited {
        act: crate::dispatchgrant::Act::Grant,
        asking: Some("steward"),
        target: "devops",
        level: Level::Project,
        workspace: None,
    };
    assert_eq!(made.kind(), "trust.dispatch.grant");
    assert_eq!(
        made.body(),
        serde_json::json!({
            "actor_kind": "human",
            "actor": "operator",
            "scope": "local-ui",
            "level": "project",
            "asking": "steward",
            "target": "devops",
            "workspace": null,
        })
    );
    let taken = Audited {
        act: crate::dispatchgrant::Act::Revoke,
        asking: None,
        level: Level::Chat,
        ..made
    };
    assert_eq!(taken.kind(), "trust.dispatch.revoke");
    assert_eq!(taken.body()["asking"], serde_json::Value::Null);
    assert_eq!(taken.body()["level"], "chat");
}

// ---- the brief, as the Notice shows it ----------------------------------------------------------

#[test]
fn a_brief_is_shown_whole_with_its_lines_and_nothing_that_moves_or_hides_text() {
    let shown = shown_brief("Check prod.\r\n\tThen report.\u{1b}[2J\u{202e}evil\u{200b}\u{7}");
    assert_eq!(
        shown,
        ShownBrief {
            text: "Check prod.\n\tThen report.\\u001b[2J\\u202eevil\\u200b\\u0007".to_owned(),
            cut: false,
            lines: 2,
        }
    );
}

#[test]
fn a_brief_shows_every_character_with_no_glyph_as_its_escape_and_cannot_spell_one_itself() {
    // Whatever draws as nothing, or as a line break that is not one: a soft hyphen, the
    // Mongolian vowel separator, the line and paragraph separators, a tag character.
    let shown = shown_brief("a\u{ad}b\u{180e}c\u{2028}d\u{2029}e\u{e0041}f");
    assert_eq!(shown.text, "a\\u00adb\\u180ec\\u2028d\\u2029e\\U000e0041f");
    assert_eq!(
        shown.lines, 1,
        "a separator that was escaped breaks no line"
    );
    // A backslash the chat wrote is doubled, so its text never reads as one of purlis's
    // escapes: these two briefs are shown differently.
    assert_eq!(shown_brief("\\u202e").text, "\\\\u202e");
    assert_ne!(shown_brief("\\u202e").text, shown_brief("\u{202e}").text);
}

#[test]
fn a_brief_says_how_many_lines_it_is_blank_ones_counted() {
    assert_eq!(shown_brief("one").lines, 1);
    assert_eq!(shown_brief("").lines, 0);
    // A harmless opening, a run of blank lines, and the ask below the fold.
    let padded = format!("Say hello.{}Then delete the cluster.", "\n".repeat(40));
    assert_eq!(shown_brief(&padded).lines, 41);
}

#[test]
fn a_brief_longer_than_a_first_message_may_be_is_cut_there_and_says_so() {
    let long = "é".repeat(MOST_BRIEF_BYTES);
    let shown = shown_brief(&long);
    assert!(shown.cut);
    assert_eq!(shown.text.len(), MOST_BRIEF_BYTES);
    assert!(shown.text.chars().all(|ch| ch == 'é'));
    // Exactly at the bound is whole.
    let whole = shown_brief(&"a".repeat(MOST_BRIEF_BYTES));
    assert!(!whole.cut);
    assert_eq!(whole.text.len(), MOST_BRIEF_BYTES);
}

#[test]
fn a_text_is_written_out_inertly_whole_and_says_whether_it_held_anything_to_write_out() {
    // Plain text of several lines, markup and all, is what it was: only a backslash doubles.
    let plain = "# Title\n\t<b>bold</b> [a](b)\r\nnaïve 🚀 שלום";
    assert!(!holds_what_draws_as_nothing(plain));
    assert_eq!(inert(plain), "# Title\n\t<b>bold</b> [a](b)\nnaïve 🚀 שלום");
    // What turns the words around, hides between them or moves the cursor is written out.
    let odd = "safe\u{202e}evil\u{200b}\u{1b}[2J\r \\u202e";
    assert!(holds_what_draws_as_nothing(odd));
    assert_eq!(
        inert(odd),
        "safe\\u202eevil\\u200b\\u001b[2J\\u000d \\\\u202e"
    );
    // Nothing is cut, however long.
    let long = "é".repeat(MOST_BRIEF_BYTES);
    assert_eq!(inert(&long), long);
}

// ---- what a covered dispatch starts with --------------------------------------------------------

/// A project whose steward reaches one host and whose devops reaches the cluster.
const PROJECT: &str = "[sandbox]\nmode = \"on\"\negress = []\n\n[sandbox.personas.steward]\n\
                       hosts = [\"tracker.example\"]\n\n[sandbox.personas.devops]\n\
                       hosts = [\"10.100.39.145:6443\", \"*.internal.example\"]\n";

fn machine() -> Machine {
    Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(std::path::PathBuf::from("/home/op")),
        os: Os::MacOs,
    }
}

/// The hosts a chat started with `start` reaches, compiled from [`PROJECT`] as a start does:
/// the persona whose grants it runs with is [`crate::start::grants_persona`]'s answer.
fn reached(start: &Dispatched) -> Vec<String> {
    let root = tempfile::tempdir().expect("a project");
    let plane = Plane::of(Some(PROJECT));
    let with = crate::start::grants_persona(start.held.as_ref(), Some(&start.persona), || None);
    Compiled::granted(
        &plane.said().policy.expect("on"),
        &plane,
        root.path(),
        &machine(),
        with.as_deref(),
        &start.grants,
    )
    .hosts
}

#[test]
fn a_dispatched_chat_starts_with_its_own_persona_s_hosts_and_none_of_the_asking_chat_s() {
    let start = grants_for_a_dispatched_chat("devops");
    assert_eq!(
        start,
        Dispatched {
            persona: "devops".to_owned(),
            held: None,
            grants: crate::sandbox::grant::Grants::default(),
            without_sandbox: None,
        }
    );
    assert_eq!(
        reached(&start),
        ["10.100.39.145:6443", "*.internal.example"]
    );
}

#[test]
fn the_hold_a_handoff_to_the_same_persona_still_gets_is_not_a_dispatch_s() {
    // What `purlis handoff` from a steward chat to devops still does (D-1362-5; #1444 converts
    // it): the new chat holds steward's grants, so it reaches steward's host and not devops's.
    let policy = Plane::of(Some(PROJECT)).said().policy.expect("on");
    let held =
        crate::sandbox::persona::held_unless_within(Some(&policy), Some("steward"), Some("devops"));
    assert_eq!(
        held,
        Some(crate::reopen::HeldGrants {
            persona: Some("steward".to_owned())
        })
    );
    let handed_off = Dispatched {
        held,
        ..grants_for_a_dispatched_chat("devops")
    };
    assert_eq!(reached(&handed_off), ["tracker.example"]);
    // A dispatch a grant covers holds nothing.
    assert_eq!(grants_for_a_dispatched_chat("devops").held, None);
}

// ---- your own grants, on this machine -----------------------------------------------------------

#[test]
fn a_grant_for_me_on_this_machine_is_kept_beside_the_project_reads_back_and_is_revoked() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    assert_eq!(yours(root), []);
    crate::sandbox::local::grant_dispatch(root, "steward", "devops").expect("kept");
    crate::sandbox::local::grant_dispatch(root, "steward", "devops").expect("kept once");
    crate::sandbox::local::grant_dispatch(root, "steward", "qa").expect("kept");
    assert_eq!(
        yours(root),
        [pair("steward", "devops"), pair("steward", "qa")]
    );
    // It is in force for every chat of the project here, with nothing of the chat's own.
    let grants = InForce::read(root, Vec::new());
    assert_eq!(
        covers(Some("steward"), "devops", &grants, &Locks::none()),
        Covers::Covered
    );
    assert_eq!(grants.level_of(Some("steward"), "devops"), Some(Level::You));
    // In this machine's record, never the project file.
    let kept = std::fs::read_to_string(crate::sandbox::local::path(root)).expect("the record");
    assert!(kept.contains("\"dispatch_mine\""), "{kept}");
    assert!(!crate::names::manifest(root).exists());

    assert!(crate::sandbox::local::revoke_dispatch(root, "steward", "devops").expect("revoked"));
    assert!(!crate::sandbox::local::revoke_dispatch(root, "steward", "devops").expect("gone"));
    assert_eq!(yours(root), [pair("steward", "qa")]);
    assert_eq!(
        covers(
            Some("steward"),
            "devops",
            &InForce::read(root, Vec::new()),
            &Locks::none()
        ),
        Covers::NeedsGrant
    );
}

#[test]
fn a_name_in_this_machine_s_record_that_is_no_persona_s_grants_nothing() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    crate::sandbox::local::grant_dispatch(root, "Not A Name", "devops").expect("kept");
    crate::sandbox::local::grant_dispatch(root, "qa", "qa").expect("kept");
    assert_eq!(yours(root), []);
}

#[test]
fn what_the_person_was_told_of_is_kept_as_it_was_shown() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    assert_eq!(crate::sandbox::local::dispatch_seen(root), None);
    acknowledge(root, &["steward -> devops".to_owned()]).expect("kept");
    assert_eq!(
        crate::sandbox::local::dispatch_seen(root),
        Some(vec!["steward -> devops".to_owned()])
    );
    // No project file here grants nothing, so what was seen has been taken away.
    assert_eq!(
        changed(root),
        Some(Change {
            added: vec![],
            removed: vec!["steward -> devops".to_owned()],
            now: vec![],
        })
    );
}

// ---- its persona's vault, through brokered `secret exec` ----------------------------------------

#[test]
fn a_dispatched_chat_is_handed_its_own_persona_s_vault_and_the_asking_chat_s_persona_is_not() {
    use crate::secrets::{Ctx, Env, brokered};
    let project = tempfile::tempdir().expect("a project");
    std::fs::write(
        project.path().join("vaults.json"),
        serde_json::json!({ "vaults": {
            "prod": {"provider": "plain-file", "config": {"file": "prod.json"}, "persona": "devops"},
        }})
        .to_string(),
    )
    .expect("the registry");
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    // The asking chat, running as steward, is refused devops's vault: why it dispatches.
    let why = brokered::authorise(&ctx, Some("steward"), "prod").expect_err("refused");
    assert!(
        why.starts_with("vault 'prod' is not one persona 'steward' may use"),
        "{why}"
    );
    // The app asks with the persona the chat was started as, which is the dispatch's target:
    // from its first command, with no restart and nothing to allow on its tab.
    let start = grants_for_a_dispatched_chat("devops");
    assert_eq!(
        brokered::authorise(&ctx, Some(&start.persona), "prod"),
        Ok(())
    );
}

// ---- a pulled grant waits for this machine's yes (D-1437-R1) -----------------------------------

#[test]
fn a_committed_pair_this_machine_has_not_acknowledged_covers_nothing_until_it_is() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    std::fs::write(
        crate::names::manifest(root),
        "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"qa\"]\n",
    )
    .expect("a teammate's push");
    // In the file, and not yet in force here: nobody on this machine has seen it.
    assert_eq!(
        committed_at(root),
        [pair("steward", "devops"), pair("steward", "qa")]
    );
    let grants = InForce::read(root, Vec::new());
    assert_eq!(grants.project, []);
    assert_eq!(
        covers(Some("steward"), "devops", &grants, &Locks::none()),
        Covers::NeedsGrant
    );
    assert_eq!(
        unacknowledged(root),
        [pair("steward", "devops"), pair("steward", "qa")]
    );

    // Allowed pair by pair: the one allowed is in force, the other still waits and is still told.
    acknowledge_pair(root, &pair("steward", "devops")).expect("kept");
    let grants = InForce::read(root, Vec::new());
    assert_eq!(grants.project, [pair("steward", "devops")]);
    assert_eq!(
        covers(Some("steward"), "qa", &grants, &Locks::none()),
        Covers::NeedsGrant
    );
    assert_eq!(unacknowledged(root), [pair("steward", "qa")]);
    assert_eq!(changed(root).expect("still told").added, ["steward -> qa"]);

    // An acknowledgement of a pair the file does not hold puts nothing in force.
    acknowledge(
        root,
        &["qa -> prod".to_owned(), "steward -> devops".to_owned()],
    )
    .expect("kept");
    assert_eq!(
        InForce::read(root, Vec::new()).project,
        [pair("steward", "devops")]
    );

    // Taken out of the file, it is in force nowhere, acknowledged or not.
    std::fs::write(crate::names::manifest(root), "schema = 1\n").expect("another push");
    assert_eq!(InForce::read(root, Vec::new()).project, []);
}
