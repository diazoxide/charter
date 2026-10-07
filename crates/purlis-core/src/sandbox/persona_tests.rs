//! A persona's own sandbox grants (#1362): the hosts its chats reach, and only its chats.

use super::hosts::{Granted, Host, Level, Locks, in_force};
use super::{Compiled, Machine, Os, Plane, Policy, Refusal};

const DEVOPS: &str = "[sandbox]\nmode = \"on\"\negress = []\n\n[sandbox.personas.devops]\n\
                      hosts = [\"10.100.39.145:6443\", \"*.internal.example\"]\n";

fn policy(text: &str) -> Policy {
    Plane::of(Some(text)).said().policy.expect("on")
}

fn machine() -> Machine {
    Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(std::path::PathBuf::from("/home/op")),
        os: Os::MacOs,
    }
}

/// What a chat at `root` running as `persona` reaches, compiled from `text`.
fn reached(text: &str, persona: Option<&str>) -> Vec<String> {
    let root = tempfile::tempdir().expect("a project");
    Compiled::of(
        &policy(text),
        &Plane::of(Some(text)),
        root.path(),
        &machine(),
        persona,
    )
    .hosts
}

#[test]
fn a_chat_running_as_the_persona_reaches_its_hosts_private_address_and_port_included() {
    assert_eq!(
        reached(DEVOPS, Some("devops")),
        ["10.100.39.145:6443", "*.internal.example"]
    );
}

#[test]
fn a_chat_on_another_persona_or_on_none_does_not() {
    assert_eq!(reached(DEVOPS, Some("qa")), Vec::<String>::new());
    assert_eq!(reached(DEVOPS, None), Vec::<String>::new());
}

#[test]
fn a_persona_host_goes_through_the_same_parser_and_a_bad_one_grants_nothing() {
    let said = Plane::of(Some(
        "[sandbox]\nmode = \"on\"\n[sandbox.personas.devops]\nhosts = [\"127.0.0.1\", \
         \"10.0.0.5:6443\"]\ntools = 3\n[sandbox.personas.Bad]\nhosts = [\"a.example\"]\n",
    ))
    .said();
    let policy = said.policy.expect("on");
    assert_eq!(
        policy.personas["devops"].hosts,
        [Host::parse("10.0.0.5:6443").unwrap()]
    );
    assert!(!policy.personas.contains_key("Bad"));
    let refused: Vec<String> = said.refused.iter().map(ToString::to_string).collect();
    assert_eq!(refused.len(), 3, "{refused:?}");
    assert!(
        refused[2].starts_with("sandbox.personas.Bad in charter.toml is not a persona's name"),
        "{refused:?}"
    );
    assert!(
        refused[0]
            .starts_with("sandbox.personas.devops.tools in charter.toml is not a key purlis reads"),
        "{refused:?}"
    );
    assert!(
        refused[1].starts_with(
            "sandbox.personas.devops.hosts in charter.toml names \"127.0.0.1\", which no chat \
             is let reach: 127.0.0.1 is this machine."
        ),
        "{refused:?}"
    );
    assert!(
        said.refused
            .iter()
            .all(|one| matches!(one, Refusal::Persona(_)))
    );
}

#[test]
fn personas_that_are_not_a_table_grant_nothing() {
    let said = Plane::of(Some("[sandbox]\nmode = \"on\"\npersonas = [\"devops\"]\n")).said();
    assert!(said.policy.expect("on").personas.is_empty());
    assert_eq!(said.refused.len(), 1);
}

#[test]
fn a_persona_host_the_project_already_grants_is_the_project_s() {
    let one = |typed: &str| Host::parse(typed).unwrap();
    assert_eq!(
        in_force(
            &[one("a.example")],
            &[],
            &[one("a.example"), one("10.0.0.5:6443")],
            &Locks::none()
        ),
        [
            Granted {
                host: one("a.example"),
                level: Level::Project
            },
            Granted {
                host: one("10.0.0.5:6443"),
                level: Level::Persona
            },
        ]
    );
}

#[test]
fn policy_can_forbid_persona_hosts_and_leaves_the_project_s() {
    let one = |typed: &str| Host::parse(typed).unwrap();
    let locks = Locks::parse(
        r#"{"sandbox": {"persona-hosts": false}}"#,
        std::path::Path::new("/etc/purlis/policy.json"),
    );
    assert_eq!(
        in_force(&[one("a.example")], &[], &[one("10.0.0.5:6443")], &locks),
        [Granted {
            host: one("a.example"),
            level: Level::Project
        }]
    );
    let why = locks
        .refuses(&Granted {
            host: one("10.0.0.5:6443"),
            level: Level::Persona,
        })
        .expect("locked");
    assert!(why.contains("Locked by policy"), "{why}");
}

#[test]
fn a_change_to_a_persona_s_hosts_is_told_naming_whose_chats_reach_it() {
    assert_eq!(
        Plane::of(Some(
            "[sandbox]\nmode = \"on\"\nhosts = [\"a.example\"]\n\
             [sandbox.personas.devops]\nhosts = [\"10.100.39.145:6443\"]\n"
        ))
        .granted_hosts(&crate::sandbox::policy::Locks::none()),
        ["a.example", "10.100.39.145:6443 for devops chats"]
    );
}

#[test]
fn a_change_under_a_persona_s_grants_is_a_sandbox_key_no_brokered_write_makes() {
    assert!(super::changes_a_sandbox_key(
        Some("[sandbox]\nmode = \"on\"\n"),
        "[sandbox]\nmode = \"on\"\n[sandbox.personas.devops]\nhosts = [\"10.0.0.5\"]\n"
    ));
}

// ---- a handoff never launders a persona's hosts (D-1362-5) ----------------------------------

const TWO: &str = "[sandbox]\nmode = \"on\"\negress = []\n[sandbox.personas.devops]\nhosts = [\"10.100.39.145:6443\", \
                   \"a.example\"]\n[sandbox.personas.qa]\nhosts = [\"a.example\"]\n";

#[test]
fn a_handoff_from_a_chat_to_a_wider_persona_holds_the_asking_chat_s_grants() {
    let policy = policy(TWO);
    let held = super::persona::held_unless_within(Some(&policy), Some("qa"), Some("devops"))
        .expect("held");
    assert_eq!(held.persona.as_deref(), Some("qa"));
    // Compiled as the asking chat's persona, the new devops chat reaches only qa's hosts.
    let as_held = crate::start::grants_persona(Some(&held), Some("devops"), || None);
    assert_eq!(as_held.as_deref(), Some("qa"));
    assert_eq!(reached(TWO, as_held.as_deref()), ["a.example"]);
    // From a chat on no persona, it holds none at all.
    let none =
        super::persona::held_unless_within(Some(&policy), None, Some("devops")).expect("held");
    assert_eq!(none.persona, None);
    assert_eq!(
        reached(
            TWO,
            crate::start::grants_persona(Some(&none), Some("devops"), || None).as_deref()
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_handoff_to_a_persona_whose_hosts_the_asker_already_reaches_holds_its_own() {
    let policy = policy(TWO);
    assert_eq!(
        super::persona::held_unless_within(Some(&policy), Some("devops"), Some("qa")),
        None
    );
    assert_eq!(
        super::persona::held_unless_within(Some(&policy), Some("devops"), Some("devops")),
        None
    );
    // A persona with no hosts of its own widens nothing either.
    assert_eq!(
        super::persona::held_unless_within(Some(&policy), None, Some("writer")),
        None
    );
    // A project that does not sandbox its chats has nothing to hold.
    assert_eq!(
        super::persona::held_unless_within(None, None, Some("devops")),
        None
    );
}

#[test]
fn a_chat_the_person_starts_holds_its_own_persona_s_grants_and_one_on_none_the_default_s() {
    assert_eq!(
        crate::start::grants_persona(None, Some("devops"), || Some("qa".to_owned())).as_deref(),
        Some("devops")
    );
    assert_eq!(
        reached(
            TWO,
            crate::start::grants_persona(None, Some("devops"), || None).as_deref()
        ),
        ["10.100.39.145:6443", "a.example"]
    );
    // No persona named: the one a new chat adopts by default, whose grants its briefing takes.
    assert_eq!(
        crate::start::grants_persona(None, None, || Some("devops".to_owned())).as_deref(),
        Some("devops")
    );
}

#[test]
fn a_chat_that_holds_another_persona_s_grants_runs_with_them_and_hands_off_holding_them() {
    // qa hands off to devops (B, held at qa); B hands off to devops again (C).
    let policy = policy(TWO);
    let b = crate::reopen::Chat {
        persona: Some("devops".to_owned()),
        held: Some(crate::reopen::HeldGrants {
            persona: Some("qa".to_owned()),
        }),
        ..Default::default()
    };
    let root = tempfile::tempdir().expect("a project");
    let b_runs_with = crate::start::runs_with(&b, root.path());
    assert_eq!(b_runs_with.as_deref(), Some("qa"));
    let c =
        super::persona::held_unless_within(Some(&policy), b_runs_with.as_deref(), Some("devops"))
            .expect("C stays held");
    assert_eq!(c.persona.as_deref(), Some("qa"));
    // Once allowed, B runs with its own, and a handoff to devops widens nothing.
    let allowed = crate::reopen::Chat { held: None, ..b };
    let runs = crate::start::runs_with(&allowed, root.path());
    assert_eq!(runs.as_deref(), Some("devops"));
    assert_eq!(
        super::persona::held_unless_within(Some(&policy), runs.as_deref(), Some("devops")),
        None
    );
}

#[test]
fn a_resume_as_a_persona_wider_than_the_default_holds_the_default_s_grants() {
    // D-1362-6: the record says devops; the default persona is qa.
    let policy = policy(TWO);
    let held = super::persona::held_unless_within(Some(&policy), Some("qa"), Some("devops"))
        .expect("held");
    assert_eq!(held.persona.as_deref(), Some("qa"));
    // A record as the default persona, or one within it, holds its own.
    assert_eq!(
        super::persona::held_unless_within(Some(&policy), Some("devops"), Some("qa")),
        None
    );
}
