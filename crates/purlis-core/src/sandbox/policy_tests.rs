//! An administrator's policy (#1343): what each lock does to a compiled sandbox, the file it is
//! read from, and when that file is refused.

use std::path::{Path, PathBuf};

use super::super::grant::{self, Grants, What};
use super::super::hosts::Host;
use super::super::{Compiled, Machine, Os, Plane, Policy, Preset};
use super::{Locks, Seen, TEST_LOCKS, judge};

const FILE: &str = "/etc/purlis/policy.json";

fn locks(json: &str) -> Locks {
    Locks::parse(json, Path::new(FILE))
}

/// The project's sandbox, with its own hosts.
const PROJECT: &str = "[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"forge\", \
                       \"toolchains\"]\nhosts = [\"api.example.com\", \"build.corp.example\"]\n\n\
                       [sandbox.personas.devops]\nhosts = [\"10.0.0.5:6443\"]\n";

fn machine() -> Machine {
    Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(PathBuf::from("/home/op")),
        os: Os::MacOs,
    }
}

fn policy() -> Policy {
    Plane::of(Some(PROJECT)).said().policy.expect("on")
}

/// The project compiled for a chat running as `persona` with `chat`'s grants, under `under`.
fn compiled(under: &Locks, persona: Option<&str>, chat: &Grants) -> Compiled {
    let root = tempfile::tempdir().expect("a project");
    TEST_LOCKS.with(|locks| *locks.borrow_mut() = under.clone());
    let compiled = Compiled::granted(
        &policy(),
        &Plane::of(Some(PROJECT)),
        root.path(),
        &machine(),
        persona,
        chat,
    );
    TEST_LOCKS.with(|locks| *locks.borrow_mut() = Locks::none());
    compiled
}

fn has(compiled: &Compiled, host: &str) -> bool {
    compiled.hosts.iter().any(|one| one == host)
}

fn chat_host(host: &str) -> Grants {
    Grants {
        hosts: vec![Host::parse(host).expect("a host")],
        writes: Vec::new(),
    }
}

// ---- the acceptance: a policy that forbids a project host ---------------------------------------

#[test]
fn a_project_host_the_policy_does_not_allow_is_refused_when_the_sandbox_compiles() {
    let under = locks(r#"{"sandbox": {"hosts": ["*.corp.example"]}}"#);
    let compiled = compiled(&under, None, &Grants::default());
    assert!(has(&compiled, "build.corp.example"), "{:?}", compiled.hosts);
    assert!(!has(&compiled, "api.example.com"), "{:?}", compiled.hosts);
    // The presets' own hosts are the presets' lock, not the hosts'.
    assert!(has(&compiled, "github.com"));
}

#[test]
fn without_a_policy_the_project_s_hosts_compile_as_before() {
    let compiled = compiled(&Locks::none(), None, &Grants::default());
    assert!(has(&compiled, "api.example.com"));
    assert!(has(&compiled, "build.corp.example"));
}

// ---- one compiled test per lock ---------------------------------------------------------------

#[test]
fn fixed_presets_turn_every_other_preset_off_whatever_the_project_says() {
    let under = locks(r#"{"sandbox": {"presets": ["model-providers"]}}"#);
    let compiled = compiled(&under, None, &Grants::default());
    assert!(has(&compiled, "api.anthropic.com"));
    assert!(!has(&compiled, "github.com"), "{:?}", compiled.hosts);
    assert!(!has(&compiled, "registry.npmjs.org"));
    // Package registries off: no package caches widen the sandbox either.
    assert_eq!(compiled.widened.caches, None);
}

#[test]
fn a_fixed_preset_list_never_turns_on_a_preset_the_project_left_off() {
    let under = locks(r#"{"sandbox": {"presets": ["model-providers", "forge"]}}"#);
    assert_eq!(
        under.presets(&[Preset::Forge]),
        [Preset::Forge],
        "the strictest wins: the project's choice, within the policy's"
    );
}

#[test]
fn forbidden_personal_hosts_reach_no_chat_and_the_project_s_still_do() {
    let under = locks(r#"{"sandbox": {"personal-hosts": false}}"#);
    let compiled = compiled(&under, None, &chat_host("tools.example.org"));
    assert!(!has(&compiled, "tools.example.org"), "{:?}", compiled.hosts);
    assert!(has(&compiled, "api.example.com"));
    // …and without the lock, the same grant reaches the chat.
    assert!(has(
        &self::compiled(&Locks::none(), None, &chat_host("tools.example.org")),
        "tools.example.org"
    ));
}

#[test]
fn forbidden_persona_hosts_reach_no_chat_of_the_persona() {
    let under = locks(r#"{"sandbox": {"persona-hosts": false}}"#);
    assert!(!has(
        &compiled(&under, Some("devops"), &Grants::default()),
        "10.0.0.5:6443"
    ));
    assert!(has(
        &compiled(&Locks::none(), Some("devops"), &Grants::default()),
        "10.0.0.5:6443"
    ));
}

#[test]
fn forbidden_write_grants_compile_no_granted_folder() {
    let root = tempfile::tempdir().expect("a project");
    let folder = std::fs::canonicalize(root.path())
        .expect("real")
        .join("out");
    std::fs::create_dir(&folder).expect("a folder");
    let grants = Grants {
        hosts: Vec::new(),
        writes: vec![folder.clone()],
    };
    let compile = |under: &Locks| {
        TEST_LOCKS.with(|locks| *locks.borrow_mut() = under.clone());
        let compiled = Compiled::granted(
            &policy(),
            &Plane::of(Some(PROJECT)),
            root.path(),
            &machine(),
            None,
            &grants,
        );
        TEST_LOCKS.with(|locks| *locks.borrow_mut() = Locks::none());
        compiled
    };
    grant::TEST_HARNESS_TEMP.with(|temp| temp.set(&[]));
    assert_eq!(
        compile(&Locks::none()).writable,
        std::slice::from_ref(&folder)
    );
    assert!(
        compile(&locks(r#"{"sandbox": {"write-grants": false}}"#))
            .writable
            .is_empty()
    );
    grant::TEST_HARNESS_TEMP.with(|temp| temp.set(&grant::HARNESS_TEMP));
}

#[test]
fn a_forbidden_opt_out_is_refused_with_the_policy_and_its_owner() {
    let under = locks(r#"{"owner": "Platform team", "sandbox": {"opt-out": false}}"#);
    let why = under.opt_out_refused().expect("forbidden");
    assert!(why.contains("Locked by policy"), "{why}");
    assert!(why.contains("Platform team"), "{why}");
    assert!(why.contains(FILE), "{why}");
    assert_eq!(Locks::none().opt_out_refused(), None);
}

#[test]
fn a_grant_policy_forbids_is_refused_with_why_at_every_level_it_forbids() {
    let under = locks(
        r#"{"owner": "IT", "sandbox": {"personal-hosts": false, "write-grants": false,
            "hosts": ["*.corp.example"]}}"#,
    );
    let host = What::Host(Host::parse("a.corp.example").expect("a host"));
    assert!(under.refuses_grant(&host, grant::Level::Chat).is_some());
    assert!(under.refuses_grant(&host, grant::Level::You).is_some());
    assert_eq!(under.refuses_grant(&host, grant::Level::Project), None);
    let elsewhere = What::Host(Host::parse("pastebin.example").expect("a host"));
    let why = under
        .refuses_grant(&elsewhere, grant::Level::Project)
        .expect("not allowed");
    assert!(why.contains("not a host policy allows"), "{why}");
    assert!(why.contains("set by IT"), "{why}");
    let write = What::Write(PathBuf::from("/opt/cache"));
    assert!(under.refuses_grant(&write, grant::Level::Chat).is_some());
}

// ---- reading the file -------------------------------------------------------------------------

#[test]
fn an_allowed_host_covers_names_under_a_wildcard_and_any_port_where_it_names_none() {
    let one = |typed: &str| Host::parse(typed).expect("a host");
    assert!(one("*.corp.example").covers(&one("a.corp.example")));
    assert!(one("*.corp.example").covers(&one("a.b.corp.example:8443")));
    assert!(one("*.corp.example").covers(&one("*.b.corp.example")));
    assert!(one("*.corp.example").covers(&one("*.corp.example")));
    assert!(!one("*.corp.example").covers(&one("corp.example")));
    assert!(!one("*.corp.example").covers(&one("evilcorp.example")));
    assert!(one("api.example.com").covers(&one("api.example.com:8443")));
    assert!(!one("api.example.com:443").covers(&one("api.example.com:8443")));
    assert!(!one("api.example.com").covers(&one("*.api.example.com")));
    assert!(one("10.0.0.5").covers(&one("10.0.0.5:6443")));
}

#[test]
fn a_policy_naming_no_owner_is_set_by_this_machine_s_administrator() {
    let under = locks(r#"{"sandbox": {"opt-out": false}}"#);
    assert_eq!(
        under.locked_by(),
        "Locked by policy, set by this machine's administrator in /etc/purlis/policy.json."
    );
}

#[test]
fn true_and_absent_lock_nothing() {
    let under = locks(
        r#"{"sandbox": {"personal-hosts": true, "persona-hosts": true, "opt-out": true,
            "write-grants": true}}"#,
    );
    assert!(under.any());
    assert!(!under.forbids_personal_hosts());
    assert!(!under.forbids_persona_hosts());
    assert!(!under.forbids_opt_out());
    assert!(!under.forbids_write_grants());
    assert!(!under.forbids_vault_grants());
    assert_eq!(under.vault_grants_refused(), None);
    assert!(!under.fixes_presets());
    assert_eq!(under.allowed_hosts(), None);
}

#[test]
fn a_policy_this_version_cannot_read_is_refused_and_locks_everything() {
    for text in [
        "not json",
        "[]",
        r#"{"sandbox": {"networks": []}}"#,
        r#"{"sandbox": {"presets": ["browser"]}}"#,
        r#"{"sandbox": {"hosts": ["https://a.example/"]}}"#,
        r#"{"sandbox": {"opt-out": "no"}}"#,
        r#"{"owner": 7}"#,
        r#"{"admins": []}"#,
    ] {
        let under = locks(text);
        assert!(under.refused_because().is_some(), "{text}");
        assert!(under.forbids_opt_out(), "{text}");
        assert!(under.forbids_write_grants(), "{text}");
        assert!(under.forbids_vault_grants(), "{text}");
        assert!(under.forbids_personal_hosts(), "{text}");
        assert!(under.forbids_persona_hosts(), "{text}");
        assert_eq!(under.allowed_hosts(), Some(&[][..]), "{text}");
        assert!(under.locked_by().contains("is refused"), "{text}");
    }
}

fn seen(uid: u32, mode: u32) -> Seen {
    Seen {
        link: false,
        expected_type: true,
        uid,
        mode,
    }
}

#[test]
fn only_a_file_and_folder_root_owns_and_no_one_else_writes_is_trusted() {
    assert_eq!(judge(seen(0, 0o100_644), seen(0, 0o40_755)), Ok(()));
    // Owned by the person, or writable by the group or anyone: a chat running as the person
    // could have written it.
    assert!(judge(seen(501, 0o100_644), seen(0, 0o40_755)).is_err());
    assert!(judge(seen(0, 0o100_664), seen(0, 0o40_755)).is_err());
    assert!(judge(seen(0, 0o100_646), seen(0, 0o40_755)).is_err());
    assert!(judge(seen(0, 0o100_644), seen(501, 0o40_755)).is_err());
    assert!(judge(seen(0, 0o100_644), seen(0, 0o40_777)).is_err());
    // A link, the file or its folder: it could be pointed anywhere.
    let link = Seen {
        link: true,
        ..seen(0, 0o120_755)
    };
    assert_eq!(
        judge(link, seen(0, 0o40_755)),
        Err("it is a link".to_owned())
    );
    assert!(judge(seen(0, 0o100_644), link).is_err());
}

#[cfg(unix)]
#[test]
fn a_policy_file_the_person_owns_is_refused_and_locks_everything() {
    let dir = tempfile::tempdir().expect("a folder");
    let file = dir.path().join("policy.json");
    std::fs::write(&file, r#"{"sandbox": {}}"#).expect("written");
    if rustix::process::geteuid().is_root() {
        return;
    }
    let under = Locks::read(&file);
    assert!(
        under
            .refused_because()
            .is_some_and(|why| why.contains("not owned")),
        "{under:?}"
    );
    assert!(under.forbids_opt_out());
}

#[cfg(unix)]
#[test]
fn a_policy_file_that_is_a_link_is_refused() {
    let dir = tempfile::tempdir().expect("a folder");
    let real = dir.path().join("real.json");
    std::fs::write(&real, "{}").expect("written");
    let link = dir.path().join("policy.json");
    std::os::unix::fs::symlink(&real, &link).expect("linked");
    let under = Locks::read(&link);
    assert!(under.refused_because().is_some(), "{under:?}");
}

#[test]
fn no_policy_folder_is_no_policy() {
    let dir = tempfile::tempdir().expect("a folder");
    assert_eq!(
        Locks::read(&dir.path().join("absent").join("policy.json")),
        Locks::none()
    );
}

#[cfg(unix)]
#[test]
fn a_policy_folder_the_person_owns_is_refused_even_with_no_file_in_it() {
    // Whoever owns the folder could have taken the file out of it: never read as no policy.
    let dir = tempfile::tempdir().expect("a folder");
    if rustix::process::geteuid().is_root() {
        return;
    }
    let under = Locks::read(&dir.path().join("policy.json"));
    assert!(
        under
            .refused_because()
            .is_some_and(|why| why.contains("its folder is not owned")),
        "{under:?}"
    );
    assert!(under.forbids_opt_out());
    assert_eq!(under.allowed_hosts(), Some(&[][..]));
}

#[test]
fn every_chat_is_denied_writing_the_policy_folder() {
    let root = tempfile::tempdir().expect("a project");
    let denied = super::super::Denied::of(root.path(), &machine());
    let folder = super::super::real(&super::machine_folder());
    assert!(
        denied.paths.iter().any(|denial| denial.path == folder
            && denial.class == super::super::Class::HumanPowers),
        "{:?}",
        denied.paths
    );
}

#[test]
fn a_person_s_opt_out_is_refused_where_policy_forbids_it_and_the_refusal_names_who() {
    // A project whose manifest is gone reaches the opt-out (D-1410e): no file to write here.
    let gone = tempfile::tempdir().expect("a project");
    let start = |under: &Locks| {
        TEST_LOCKS.with(|locks| *locks.borrow_mut() = under.clone());
        let decided = super::super::decide(
            crate::harness::Harness::ClaudeCode,
            gone.path(),
            &machine(),
            &|_| true,
            Some(&super::super::OptOut { reason: None }),
            None,
        );
        TEST_LOCKS.with(|locks| *locks.borrow_mut() = Locks::none());
        decided
    };
    assert!(matches!(
        start(&Locks::none()),
        Ok(Some(super::super::Decided::Unsandboxed(_)))
    ));
    let under = locks(r#"{"owner": "Platform team", "sandbox": {"opt-out": false}}"#);
    let refused = start(&under).expect_err("refused");
    assert_eq!(
        refused,
        super::super::NotStarted::OptOutLocked(under.opt_out_refused().expect("locked"))
    );
    let said = refused.to_string();
    assert!(said.contains("set by Platform team"), "{said}");
    assert!(said.ends_with("Nothing was started."), "{said}");
}

// ---- a forge's host is the project's choice, and the hosts lock holds it (D-1343-10) ----------

const FORGES: &str = "[sandbox]\nmode = \"on\"\negress = [\"forge\"]\n\n[[forge]]\nkind = \"gitlab\"\n\
                      host = \"pastebin.example\"\n\n[[forge]]\nkind = \"gitlab\"\n\
                      host = \"git.corp.example\"\n";

/// What a chat in a project saying [`FORGES`] reaches, under `under`.
fn forged(under: &Locks) -> Vec<String> {
    let root = tempfile::tempdir().expect("a project");
    let plane = Plane::of(Some(FORGES));
    TEST_LOCKS.with(|locks| *locks.borrow_mut() = under.clone());
    let compiled = Compiled::granted(
        &plane.said().policy.expect("on"),
        &plane,
        root.path(),
        &machine(),
        None,
        &Grants::default(),
    );
    TEST_LOCKS.with(|locks| *locks.borrow_mut() = Locks::none());
    compiled.hosts
}

#[test]
fn a_forge_host_policy_does_not_allow_is_not_reached() {
    let reached = forged(&locks(r#"{"sandbox": {"hosts": []}}"#));
    assert!(
        !reached.iter().any(|one| one == "pastebin.example"),
        "{reached:?}"
    );
    assert!(
        !reached.iter().any(|one| one == "git.corp.example"),
        "{reached:?}"
    );
    // The preset's own fixed list is the presets lock's, not the hosts'.
    assert!(reached.iter().any(|one| one == "github.com"), "{reached:?}");

    let reached = forged(&locks(r#"{"sandbox": {"hosts": ["*.corp.example"]}}"#));
    assert!(
        reached.iter().any(|one| one == "git.corp.example"),
        "{reached:?}"
    );
    assert!(
        !reached.iter().any(|one| one == "pastebin.example"),
        "{reached:?}"
    );

    let reached = forged(&Locks::none());
    assert!(
        reached.iter().any(|one| one == "pastebin.example"),
        "{reached:?}"
    );
}

#[test]
fn under_a_refused_policy_file_the_presets_stay_and_no_forge_host_is_reached() {
    let refused = locks("not json");
    assert!(refused.refused_because().is_some());
    let reached = forged(&refused);
    assert!(reached.iter().any(|one| one == "github.com"), "{reached:?}");
    assert!(
        !reached.iter().any(|one| one == "pastebin.example"),
        "{reached:?}"
    );
    assert!(
        !reached.iter().any(|one| one == "git.corp.example"),
        "{reached:?}"
    );
}

#[test]
fn vault_grants_is_a_lock_of_its_own() {
    let under = locks(r#"{"owner": "IT", "sandbox": {"vault-grants": false}}"#);
    assert!(under.forbids_vault_grants());
    assert!(!under.forbids_write_grants(), "a folder is another lock's");
    assert!(!under.forbids_persona_hosts(), "and so is a persona's host");
    assert_eq!(
        under.vault_grants_refused().as_deref(),
        Some(
            "Policy forbids allowing a persona a vault it is not tagged for. Locked by policy, \
             set by IT in /etc/purlis/policy.json."
        )
    );
    assert!(!locks(r#"{"sandbox": {"write-grants": false}}"#).forbids_vault_grants());
}
