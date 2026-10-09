//! The app's answer to a dispatch from an unattended chat (#1446): a standing grant starts it,
//! a missing one is refused in a sentence, and nothing of it is ever held for the person.

use super::*;
use crate::dispatchgrants::{Ground, Store, Uncovered};
use purlis_core::dispatchgrant::Audited;
use purlis_core::sandbox::grant::Level;

/// Chat `session`, running as `persona`, as the app records it.
fn chat(session: u32, persona: Option<&str>) -> Asking {
    Asking {
        session,
        id: Some(format!("chat-{session}")),
        name: format!("{} {session}", persona.unwrap_or("claude")),
        persona: persona.map(str::to_owned),
        held: false,
    }
}

/// Runs `with` on `root` as a project with no policy, every chat open and an audit that is
/// kept.
fn on<T>(root: &Path, with: impl FnOnce(&Ground<'_>) -> T) -> T {
    let audit = |_: Option<u32>, _: &Audited<'_>| Ok(());
    with(&Ground {
        root,
        locks: &Locks::none(),
        is_open: &|_| true,
        sandboxed: &|_| true,
        audit: &audit,
        at: 100,
    })
}

/// A chat this app started inside a sandbox, holding its own persona's grants.
const SANDBOXED: Runs = Runs {
    holds_anothers: false,
    sandboxed: true,
};

const MISSING: &str = "this chat runs with its harness's permission prompts off, so nobody is \
    here to answer for a dispatch, and no grant lets steward chats dispatch to devops: none \
    for the person on this machine, and none for this project. A grant made for one chat does \
    not count here. Only a person makes one: they dispatch to devops once from a steward chat \
    they are at and choose Allow for me on this machine or Allow for everyone in this project. \
    Settings › Project › Dispatch lists the grants that stand. Until then, do this work \
    without devops, or say in what you leave behind that it is waiting.";

#[test]
fn an_unattended_dispatch_with_no_grant_is_refused_naming_the_pair_it_lacks() {
    let project = tempfile::tempdir().expect("a project");

    let asked = unattended(
        project.path(),
        &Locks::none(),
        &chat(3, Some("steward")),
        SANDBOXED,
        "devops",
        None,
    );

    assert_eq!(asked, Requested::Refused(MISSING.to_owned()));
}

#[test]
fn a_grant_the_person_made_for_this_machine_starts_an_unattended_dispatch() {
    let project = tempfile::tempdir().expect("a project");
    purlis_core::sandbox::local::grant_dispatch(project.path(), "steward", "devops")
        .expect("granted");

    let asked = unattended(
        project.path(),
        &Locks::none(),
        &chat(3, Some("steward")),
        SANDBOXED,
        "devops",
        None,
    );

    assert_eq!(
        asked,
        Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat("devops"))
    );
    // The way back is another pair.
    assert!(matches!(
        unattended(
            project.path(),
            &Locks::none(),
            &chat(4, Some("devops")),
            SANDBOXED,
            "steward",
            None
        ),
        Requested::Refused(_)
    ));
}

#[test]
fn a_grant_in_the_project_s_file_starts_an_unattended_dispatch() {
    let project = tempfile::tempdir().expect("a project");
    std::fs::write(
        purlis_core::names::manifest(project.path()),
        "schema = 1\n[dispatch.grants]\nsteward = [\"devops\"]\n",
    )
    .expect("the project grants it");
    let ask = || {
        unattended(
            project.path(),
            &Locks::none(),
            &chat(3, Some("steward")),
            SANDBOXED,
            "devops",
            None,
        )
    };

    // A pair the file names counts on a machine only once someone there allowed it
    // (D-1437-R1). Until then the refusal says so, and where a person reviews it.
    assert_eq!(
        ask(),
        Requested::Refused(
            dispatchunattended::Missing {
                asking: Some("steward".to_owned()),
                target: "devops".to_owned(),
                unreviewed: true,
            }
            .say()
        )
    );
    dispatchgrant::acknowledge(project.path(), &["steward -> devops".to_owned()])
        .expect("a person read the project's grants on this machine");
    assert_eq!(
        ask(),
        Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat("devops"))
    );
}

#[test]
fn a_grant_made_for_this_chat_while_a_person_answered_it_does_not_count_once_nobody_does() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    let store = Store::default();
    // Attended: asked, and allowed for this chat.
    let (asked, _) = on(root, |ground| {
        store.request(
            ground,
            chat(3, Some("steward")),
            "devops",
            "Check the queue.",
            Uncovered::AskThePerson,
        )
    });
    let Requested::NeedsGrant { pending } = asked else {
        panic!("held: {asked:?}");
    };
    on(root, |ground| store.allow(ground, pending, Level::Chat)).expect("allowed");
    let (again, _) = on(root, |ground| {
        store.request(
            ground,
            chat(3, Some("steward")),
            "devops",
            "And again.",
            Uncovered::AskThePerson,
        )
    });
    assert!(matches!(again, Requested::Covered(_)), "{again:?}");

    let asked = unattended(
        root,
        &Locks::none(),
        &chat(3, Some("steward")),
        SANDBOXED,
        "devops",
        None,
    );

    assert_eq!(asked, Requested::Refused(MISSING.to_owned()));
    assert!(store.waiting(3).is_empty(), "nothing is held to allow");
}

#[test]
fn its_own_persona_starts_and_a_policy_lock_refuses_in_the_policy_s_words() {
    let project = tempfile::tempdir().expect("a project");
    assert_eq!(
        unattended(
            project.path(),
            &Locks::none(),
            &chat(3, Some("devops")),
            SANDBOXED,
            "devops",
            None
        ),
        Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat("devops"))
    );

    purlis_core::sandbox::local::grant_dispatch(project.path(), "steward", "devops")
        .expect("granted");
    let locked = Locks::parse(
        r#"{"owner": "IT", "dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    assert_eq!(
        unattended(
            project.path(),
            &locked,
            &chat(3, Some("steward")),
            SANDBOXED,
            "devops",
            None
        ),
        Requested::Locked(
            "Policy forbids steward chats dispatching to devops. Locked by policy, set by IT in \
             /etc/purlis/policy.json."
                .to_owned()
        )
    );
}

#[test]
fn a_chat_still_holding_another_s_grants_dispatches_to_nobody_unattended() {
    let project = tempfile::tempdir().expect("a project");
    purlis_core::sandbox::local::grant_dispatch(project.path(), "steward", "devops")
        .expect("granted");

    for target in ["devops", "steward"] {
        assert_eq!(
            unattended(
                project.path(),
                &Locks::none(),
                &chat(3, Some("steward")),
                Runs {
                    holds_anothers: true,
                    ..SANDBOXED
                },
                target,
                None
            ),
            Requested::Refused(
                "this chat still runs on the grants of the chat that opened it, and it runs \
                 with its harness's permission prompts off, so nobody is here to allow it its \
                 own. It dispatches to nobody until a person allows them on its tab."
                    .to_owned()
            ),
            "{target}"
        );
    }
}

#[test]
fn an_unattended_chat_started_with_no_sandbox_dispatches_to_no_other_persona() {
    let project = tempfile::tempdir().expect("a project");
    purlis_core::sandbox::local::grant_dispatch(project.path(), "steward", "devops")
        .expect("granted");
    let unsandboxed = Runs {
        sandboxed: false,
        ..SANDBOXED
    };

    assert_eq!(
        unattended(
            project.path(),
            &Locks::none(),
            &chat(3, Some("steward")),
            unsandboxed,
            "devops",
            None
        ),
        Requested::Refused(
            "this chat runs with its harness's permission prompts off and with no sandbox. A \
             chat with neither can change who it may dispatch to, so purlis starts nobody for \
             it but a chat of its own persona, and devops is another. Run this work in a \
             sandboxed chat, or in one a person answers."
                .to_owned()
        )
    );
    // Its own persona still starts.
    assert_eq!(
        unattended(
            project.path(),
            &Locks::none(),
            &chat(3, Some("steward")),
            unsandboxed,
            "steward",
            None
        ),
        Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat("steward"))
    );
}

#[test]
fn a_target_that_is_no_persona_s_name_is_refused_before_anything_is_read() {
    let project = tempfile::tempdir().expect("a project");
    assert_eq!(
        unattended(
            project.path(),
            &Locks::none(),
            &chat(3, Some("steward")),
            SANDBOXED,
            "../devops",
            None
        ),
        Requested::Refused(
            "../devops is not a persona's name, so there is nothing to dispatch to.".to_owned()
        )
    );
}

// ---- the entry point the dispatch core calls, on a project the app holds ------------------------

/// Starts a chat as `persona` in a project the app holds. It opens a terminal and the
/// project's hook socket and writes the project's file, which a sandboxed chat may not: CI
/// runs it first.
fn a_held_chat(persona: &str) -> (tempfile::TempDir, std::sync::Arc<crate::planes::Held>, u32) {
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path().join("project");
    for name in ["steward", "devops"] {
        let persona = root.join("personas").join(name);
        std::fs::create_dir_all(&persona).expect("a persona");
        std::fs::write(
            persona.join("persona.md"),
            format!("---\nname: {name}\n---\n# {name}\n"),
        )
        .expect("its file");
    }
    std::fs::write(purlis_core::names::manifest(&root), "schema = 1\n").expect("the project");
    let planes = crate::planes::Planes::telling(
        std::sync::Arc::new(|_| {}),
        crate::Shipped::default(),
        None,
    );
    let id = planes.open(&root);
    let held = planes.held(&id).expect("held");
    let chat = purlis_core::reopen::Chat {
        program: "/bin/sh".to_owned(),
        args: vec!["-c".to_owned(), "sleep 5".to_owned()],
        name: "sh".to_owned(),
        persona: Some(persona.to_owned()),
        ..Default::default()
    };
    let session = held
        .chats()
        .start(
            &chat,
            purlis_core::engine::Size {
                columns: 80,
                rows: 24,
            },
        )
        .expect("it runs");
    (dir, held, session)
}

const NO_SANDBOX: &str = "this chat runs with its harness's permission prompts off and with no \
    sandbox. A chat with neither can change who it may dispatch to, so purlis starts nobody for \
    it but a chat of its own persona, and devops is another. Run this work in a sandboxed chat, \
    or in one a person answers.";

#[test]
fn an_unattended_chat_s_ask_raises_no_notice_and_its_sandbox_is_the_app_s_own_record() {
    // The project has no sandbox, so the chat the app started has none: its record says so.
    let (_dir, held, session) = a_held_chat("steward");
    assert!(held.chats().confines_of(session).is_none());

    let asked = request_dispatch(
        &held,
        session,
        Attendance::Unattended,
        "devops",
        "Check it.",
        None,
    );

    assert_eq!(asked, Requested::Refused(NO_SANDBOX.to_owned()));
    assert!(
        held.dispatch_grants().waiting(session).is_empty(),
        "nothing is held, so there is no Notice and nothing to allow for this chat"
    );
    // The same ask from a chat a person answers is the one that is held.
    let attended = request_dispatch(
        &held,
        session,
        Attendance::Attended,
        "devops",
        "Check it.",
        None,
    );
    let Requested::NeedsGrant { pending } = attended else {
        panic!("held: {attended:?}");
    };
    // A person allows it for this chat, and the project grants it too. Unattended and with
    // no sandbox, the chat is still refused: nothing it could have written counts.
    on(held.root(), |ground| {
        held.dispatch_grants().allow(ground, pending, Level::Chat)
    })
    .expect("allowed");
    std::fs::write(
        purlis_core::names::manifest(held.root()),
        "schema = 1\n[dispatch.grants]\nsteward = [\"devops\"]\n",
    )
    .expect("the project grants it");
    assert_eq!(
        request_dispatch(
            &held,
            session,
            Attendance::Unattended,
            "devops",
            "Check it.",
            None
        ),
        Requested::Refused(NO_SANDBOX.to_owned())
    );
    // Its own persona needs no grant and no sandbox.
    assert_eq!(
        request_dispatch(
            &held,
            session,
            Attendance::Unattended,
            "steward",
            "Check it.",
            None
        ),
        Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat("steward"))
    );
    // A persona the project does not have, and a chat this app does not hold.
    assert_eq!(
        request_dispatch(
            &held,
            session,
            Attendance::Unattended,
            "nobody",
            "Check it.",
            None
        ),
        Requested::Refused(
            "this project has no persona named nobody, so there is nothing to dispatch to."
                .to_owned()
        )
    );
    assert_eq!(
        request_dispatch(
            &held,
            session + 100,
            Attendance::Unattended,
            "devops",
            "Check it.",
            None
        ),
        Requested::Refused(format!(
            "chat {} is not one this app has open",
            session + 100
        ))
    );
}
