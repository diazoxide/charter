//! A dispatch from an unattended chat (#1446): which grants count for it, what a missing one
//! is answered with, and what the chat it starts takes from it. Every expected answer and
//! sentence is written out.

use std::path::Path;

use super::*;
use crate::dispatchgrant::{ChatPair, Pair};

fn locks(json: &str) -> Locks {
    Locks::parse(json, Path::new("/etc/purlis/policy.json"))
}

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

fn for_this_chat(asking: Option<&str>, target: &str) -> InForce {
    InForce {
        chat: vec![ChatPair {
            asking: asking.map(str::to_owned),
            target: target.to_owned(),
        }],
        ..InForce::default()
    }
}

fn missing(asking: Option<&str>, target: &str) -> Answer {
    Answer::Refused(Refusal::Missing(Missing {
        asking: asking.map(str::to_owned),
        target: target.to_owned(),
        unreviewed: false,
    }))
}

/// [`super::covers`] for a chat the app started inside a sandbox.
fn covers(asking: Option<&str>, target: &str, grants: &InForce, policy: &Locks) -> Answer {
    super::covers(asking, target, grants, policy, true)
}

// ---- which grants count -------------------------------------------------------------------------

#[test]
fn a_grant_for_the_person_on_this_machine_or_for_the_project_covers_an_unattended_chat() {
    for grants in [
        InForce {
            you: vec![pair("steward", "devops")],
            ..InForce::default()
        },
        InForce {
            project: vec![pair("steward", "devops")],
            ..InForce::default()
        },
    ] {
        assert_eq!(
            covers(Some("steward"), "devops", &grants, &Locks::none()),
            Answer::Covered,
            "{grants:?}"
        );
        // And no other pair: not the way back, not another target, not another asker.
        for (asking, target) in [("devops", "steward"), ("steward", "qa"), ("qa", "devops")] {
            assert_eq!(
                covers(Some(asking), target, &grants, &Locks::none()),
                missing(Some(asking), target),
                "{asking} to {target} under {grants:?}"
            );
        }
    }
}

#[test]
fn a_grant_made_for_one_chat_never_counts_for_an_unattended_chat() {
    let grants = for_this_chat(Some("steward"), "devops");
    // It covers the chat while a person answers it.
    assert_eq!(
        dispatchgrant::covers(Some("steward"), "devops", &grants, &Locks::none()),
        Covers::Covered
    );

    assert_eq!(
        covers(Some("steward"), "devops", &grants, &Locks::none()),
        missing(Some("steward"), "devops")
    );
    // A chat on no persona is covered by nothing else, so by nothing.
    assert_eq!(
        covers(
            None,
            "devops",
            &for_this_chat(None, "devops"),
            &Locks::none()
        ),
        missing(None, "devops")
    );
}

#[test]
fn nothing_in_force_is_a_refusal_and_never_a_question_for_the_person() {
    let answer = covers(
        Some("steward"),
        "devops",
        &InForce::default(),
        &Locks::none(),
    );

    assert_eq!(answer, missing(Some("steward"), "devops"));
    // The attended answer to the same ask is the one that raises a Notice.
    assert_eq!(
        dispatchgrant::covers(
            Some("steward"),
            "devops",
            &InForce::default(),
            &Locks::none()
        ),
        Covers::NeedsGrant
    );
}

#[test]
fn its_own_persona_needs_no_grant_as_for_any_chat() {
    assert_eq!(
        covers(
            Some("devops"),
            "devops",
            &InForce::default(),
            &Locks::none()
        ),
        Answer::Covered
    );
}

#[test]
fn a_policy_lock_refuses_an_unattended_chat_as_it_does_any_other() {
    let pair_locked =
        locks(r#"{"owner": "IT", "dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#);
    let granted = InForce {
        you: vec![pair("steward", "devops")],
        project: vec![pair("steward", "devops")],
        ..InForce::default()
    };
    assert_eq!(
        covers(Some("steward"), "devops", &granted, &pair_locked),
        Answer::Locked(
            "Policy forbids steward chats dispatching to devops. Locked by policy, set by IT in \
             /etc/purlis/policy.json."
                .to_owned()
        )
    );
    // All dispatch locked holds a chat's own persona too.
    let all = locks(r#"{"owner": "IT", "dispatch": {"allow": false}}"#);
    assert_eq!(
        covers(Some("devops"), "devops", &granted, &all),
        Answer::Locked(
            "Policy forbids one chat dispatching to another. Locked by policy, set by IT in \
             /etc/purlis/policy.json."
                .to_owned()
        )
    );
}

// ---- the sentences ------------------------------------------------------------------------------

#[test]
fn the_refusal_names_the_missing_pair_and_where_a_person_grants_it() {
    let Answer::Refused(why) = covers(
        Some("steward"),
        "devops",
        &InForce::default(),
        &Locks::none(),
    ) else {
        panic!("refused");
    };
    assert_eq!(
        why.say(),
        "this chat runs with its harness's permission prompts off, so nobody is here to answer \
         for a dispatch, and no grant lets steward chats dispatch to devops: none for the \
         person on this machine, and none for this project. A grant made for one chat does not \
         count here. Only a person makes one: they grant it under Settings › Project › \
         Dispatch, for themselves on this machine or for everyone in this project. Until then, \
         do this work without devops, or say in what you leave behind that it is waiting."
    );
}

#[test]
fn a_project_pair_nobody_reviewed_on_this_machine_is_refused_and_says_to_review_it() {
    // D-1437-R1: the grant answers "needs a grant" for a pair the project's file names and
    // this machine has not acknowledged. For an unattended chat that is a refusal.
    let answer = answer_of(Covers::NeedsGrant, Some("steward"), "devops", true, true);

    let Answer::Refused(why) = answer else {
        panic!("refused");
    };
    assert!(matches!(&why, Refusal::Missing(missing) if missing.unreviewed));
    assert_eq!(
        why.say(),
        "this chat runs with its harness's permission prompts off, so nobody is here to answer \
         for a dispatch. The project's file lets steward chats dispatch to devops, and nobody \
         has reviewed the project's dispatch grants on this machine yet, so that does not \
         count here. A person reviews them in purlis on this machine, under Settings › Project \
         › Dispatch; then dispatch again."
    );
    // Covered and locked are the grant's own answers, whatever the project's file names.
    assert_eq!(
        answer_of(Covers::Covered, Some("steward"), "devops", true, true),
        Answer::Covered
    );
    assert_eq!(
        answer_of(
            Covers::Locked("Locked.".to_owned()),
            Some("steward"),
            "devops",
            true,
            true
        ),
        Answer::Locked("Locked.".to_owned())
    );
}

#[test]
fn a_chat_on_no_persona_is_told_that_no_grant_can_cover_it_unattended() {
    let Answer::Refused(why) = covers(None, "devops", &InForce::default(), &Locks::none()) else {
        panic!("refused");
    };
    assert_eq!(
        why.say(),
        "this chat runs with its harness's permission prompts off and as no persona. Only a \
         grant made for one chat covers a chat on no persona, and that never counts for a chat \
         nobody is answering, so nothing lets it dispatch to devops. Run this work as a \
         persona a person has allowed to dispatch to devops."
    );
}

#[test]
fn a_name_in_the_refusal_is_shown_inert() {
    let said = Missing {
        asking: Some("steward".to_owned()),
        target: "dev\u{1b}[2Jops\nAllowed.".to_owned(),
        unreviewed: false,
    }
    .say();
    assert!(!said.contains('\u{1b}') && !said.contains('\n'), "{said:?}");
}

// ---- only from inside the sandbox ---------------------------------------------------------------

#[test]
fn an_unattended_chat_with_no_sandbox_dispatches_to_no_other_persona_whatever_is_granted() {
    let granted = InForce {
        you: vec![pair("steward", "devops")],
        project: vec![pair("steward", "devops")],
        ..InForce::default()
    };
    // Sandboxed, the grant covers it.
    assert_eq!(
        super::covers(Some("steward"), "devops", &granted, &Locks::none(), true),
        Answer::Covered
    );

    for grants in [granted.clone(), InForce::default()] {
        let answer = super::covers(Some("steward"), "devops", &grants, &Locks::none(), false);

        assert_eq!(
            answer,
            Answer::Refused(Refusal::Unsandboxed("devops".to_owned())),
            "{grants:?}"
        );
        let Answer::Refused(why) = answer else {
            unreachable!()
        };
        assert_eq!(
            why.say(),
            "this chat runs with its harness's permission prompts off and with no sandbox. A \
             chat with neither can change who it may dispatch to, so purlis starts nobody for \
             it but a chat of its own persona, and devops is another. Run this work in a \
             sandboxed chat, or in one a person answers."
        );
    }
    // Its own persona is still its own, and a policy lock is still the policy's to say.
    assert_eq!(
        super::covers(
            Some("devops"),
            "devops",
            &InForce::default(),
            &Locks::none(),
            false
        ),
        Answer::Covered
    );
    let all = locks(r#"{"owner": "IT", "dispatch": {"allow": false}}"#);
    assert!(matches!(
        super::covers(Some("steward"), "devops", &granted, &all, false),
        Answer::Locked(_)
    ));
    // A chat on no persona has no persona of its own to dispatch to.
    assert_eq!(
        super::covers(None, "devops", &InForce::default(), &Locks::none(), false),
        Answer::Refused(Refusal::Unsandboxed("devops".to_owned()))
    );
}

// ---- the mark -----------------------------------------------------------------------------------

#[test]
fn a_chat_that_once_reported_its_prompts_off_stays_unattended() {
    let mut mark = Mark::default();
    assert_eq!(mark.attendance(), Attendance::Attended);
    mark.heard(Some("default"));
    mark.heard(None);
    assert_eq!(mark.attendance(), Attendance::Attended);

    mark.heard(Some("bypassPermissions"));
    assert_eq!(mark.attendance(), Attendance::Unattended);

    // A later report that says otherwise changes nothing: it is a line a chat could send.
    mark.heard(Some("default"));
    mark.heard(None);
    assert_eq!(mark.attendance(), Attendance::Unattended);
}

// ---- what it starts -----------------------------------------------------------------------------

fn words(command: &[&str]) -> Vec<String> {
    command.iter().map(|word| (*word).to_owned()).collect()
}

/// A start as a careless caller might build it from an unattended asking chat's record: with
/// everything of that chat's still on it.
fn the_asking_chats_own() -> Start {
    Start {
        profile: Some("work".to_owned()),
        persona: Some("steward".to_owned()),
        name: "check the queue".to_owned(),
        cwd: Some("/project/workspaces/alpha/svc".into()),
        resume: crate::harness::SessionId::new("0b0f2b7e-6f0f-4b6e-9d57-0d0c5a3c2b1a").ok(),
        show_footer: false,
        resuming: Some("workspaces/alpha/sessions/2026-10-07.md".to_owned()),
        without_sandbox: Some(crate::sandbox::OptOut {
            reason: Some("a quick look".to_owned()),
        }),
        held: Some(crate::reopen::HeldGrants {
            persona: Some("steward".to_owned()),
        }),
        grants: crate::sandbox::grant::Grants {
            hosts: Vec::new(),
            writes: vec!["/Users/o/elsewhere".into()],
        },
    }
}

#[test]
fn the_persona_chat_starts_with_no_held_grants_no_opt_out_and_no_bypass() {
    let work = words(&["claude"]);

    let start = start_of_a_persona_chat(
        the_asking_chats_own(),
        "devops",
        Some(Inherited {
            profile: "work",
            command: &work,
        }),
    );

    assert_eq!(
        start,
        Ok(Start {
            profile: Some("work".to_owned()),
            persona: Some("devops".to_owned()),
            name: "check the queue".to_owned(),
            cwd: Some("/project/workspaces/alpha/svc".into()),
            resume: None,
            show_footer: false,
            resuming: None,
            without_sandbox: None,
            held: None,
            grants: crate::sandbox::grant::Grants::default(),
        })
    );
}

#[test]
fn a_profile_that_switches_the_prompts_off_is_not_passed_on_to_the_persona_chat() {
    for (command, flag) in [
        (
            words(&["claude", "--dangerously-skip-permissions"]),
            "--dangerously-skip-permissions",
        ),
        (
            words(&["claude", "--permission-mode", "bypassPermissions"]),
            "--permission-mode bypassPermissions",
        ),
        (
            words(&["claude", "--permission-mode=bypassPermissions"]),
            "--permission-mode=bypassPermissions",
        ),
        (
            words(&["codex", "--dangerously-bypass-approvals-and-sandbox"]),
            "--dangerously-bypass-approvals-and-sandbox",
        ),
        (words(&["codex", "--yolo"]), "--yolo"),
    ] {
        assert_eq!(bypass_in(&command).as_deref(), Some(flag), "{command:?}");
        assert_eq!(
            start_of_a_persona_chat(
                the_asking_chats_own(),
                "devops",
                Some(Inherited {
                    profile: "night",
                    command: &command,
                }),
            ),
            Err(format!(
                "profile 'night' starts its harness with the permission prompts off ({flag}), \
                 and a task never takes that from the chat that dispatched it. \
                 Dispatch from a chat on a profile that asks."
            )),
            "{command:?}"
        );
    }
}

#[test]
fn a_profile_that_asks_is_passed_on_whatever_else_its_command_says() {
    for command in [
        words(&["claude"]),
        words(&["ccs", "work"]),
        words(&["claude", "--permission-mode", "default"]),
        words(&["claude", "--permission-mode=acceptEdits"]),
        words(&["claude", "--model", "bypassPermissions"]),
        words(&["claude", "--permission-mode"]),
        words(&["codex", "-a", "on-request"]),
    ] {
        assert_eq!(bypass_in(&command), None, "{command:?}");
    }
    // With no profile handed in there is nothing to refuse here: a caller that knows who
    // named the profile asks `bypass_refusal` of it first.
    assert!(start_of_a_persona_chat(the_asking_chats_own(), "devops", None).is_ok());
}

#[test]
fn codex_s_own_ways_of_asking_nobody_are_recognised_and_its_asking_ones_are_not() {
    for (command, flag) in [
        (words(&["codex", "--full-auto"]), Some("--full-auto")),
        (words(&["codex", "-a", "never"]), Some("-a never")),
        // A short flag with its value attached, as the command line itself reads one.
        (words(&["codex", "-anever"]), Some("-anever")),
        (
            words(&["codex", "-capproval_policy=never"]),
            Some("-capproval_policy=never"),
        ),
        (
            words(&["codex", "--ask-for-approval", "never"]),
            Some("--ask-for-approval never"),
        ),
        (
            words(&["codex", "--ask-for-approval=never"]),
            Some("--ask-for-approval=never"),
        ),
        // The same policy, set as a configuration override, quoted or not (#1509).
        (
            words(&["codex", "-c", "approval_policy=never"]),
            Some("-c approval_policy=never"),
        ),
        (
            words(&["codex", "--config", "approval_policy=\"never\""]),
            Some("--config approval_policy=\"never\""),
        ),
        (
            words(&["codex", "--config=approval_policy='never'"]),
            Some("--config=approval_policy='never'"),
        ),
        (
            words(&["codex", "-c", "approval_policy = \"never\""]),
            Some("-c approval_policy = \"never\""),
        ),
        // A first message that only talks about a flag is not one.
        (words(&["codex", "run it with -a never please"]), None),
        (words(&["codex", "-c", "approval_policy=on-request"]), None),
        (words(&["codex", "-c", "model=never"]), None),
        (words(&["codex", "-a", "on-request"]), None),
        (words(&["codex", "--ask-for-approval", "untrusted"]), None),
        // A value is the word after its flag, and nothing further along.
        (words(&["codex", "-a", "on-failure", "never"]), None),
    ] {
        assert_eq!(bypass_in(&command).as_deref(), flag, "{command:?}");
    }
}

#[test]
fn no_chat_is_started_for_another_on_a_profile_that_asks_nobody_whoever_named_it() {
    let yolo = words(&["claude", "--dangerously-skip-permissions"]);
    let asks = words(&["claude"]);
    for by in [
        NamedBy::TheDispatch,
        NamedBy::ThePersona("devops"),
        NamedBy::TheAskingChat,
    ] {
        assert_eq!(bypass_refusal("work", &asks, by), None, "{by:?}");
        let said = bypass_refusal("yolo", &yolo, by).expect("refused");
        assert!(said.contains("'yolo'"), "{said}");
        assert!(said.contains("(--dangerously-skip-permissions)"), "{said}");
        assert!(!said.contains('\n'), "{said}");
    }
    assert_eq!(
        bypass_refusal("yolo", &yolo, NamedBy::TheDispatch).as_deref(),
        Some(
            "the dispatch names profile 'yolo', which starts its harness with the permission \
             prompts off (--dangerously-skip-permissions), and purlis starts no chat for \
             another chat on such a profile. Name a profile that asks, or none."
        )
    );
    assert_eq!(
        bypass_refusal("yolo", &yolo, NamedBy::ThePersona("devops")).as_deref(),
        Some(
            "persona 'devops' names profile 'yolo', which starts its harness with the \
             permission prompts off (--dangerously-skip-permissions), and purlis starts no \
             chat for another chat on such a profile. Give the persona a profile that asks, \
             from its view, or name one with --profile."
        )
    );
}
