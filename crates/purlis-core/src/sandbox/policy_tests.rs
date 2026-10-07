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

// ---- a policy that forbids the opt-out requires the sandbox (D-1423-1) -------------------------

const REQUIRED: &str = r#"{"owner": "Platform team", "sandbox": {"opt-out": false}}"#;

/// What a chat in a project whose manifest says `text` reaches under `under`: `None` where no
/// sandbox is in force there.
fn reached(text: &str, under: &Locks) -> Option<Vec<String>> {
    let root = tempfile::tempdir().expect("a project");
    let plane = Plane::of(Some(text));
    let policy = plane.in_force(under)?;
    TEST_LOCKS.with(|locks| *locks.borrow_mut() = under.clone());
    let compiled = Compiled::granted(
        &policy,
        &plane,
        root.path(),
        &machine(),
        None,
        &Grants::default(),
    );
    TEST_LOCKS.with(|locks| *locks.borrow_mut() = Locks::none());
    Some(compiled.hosts)
}

#[test]
fn a_project_with_no_sandbox_runs_sandboxed_with_the_default_presets_where_policy_requires_it() {
    let none = "schema = 1\n";
    assert_eq!(reached(none, &Locks::none()), None, "its own choice");
    // A policy that locks something else requires nothing.
    assert_eq!(
        reached(none, &locks(r#"{"sandbox": {"write-grants": false}}"#)),
        None
    );
    let hosts = reached(none, &locks(REQUIRED)).expect("sandboxed");
    for preset in Preset::DEFAULT {
        for host in preset.own_hosts() {
            assert!(hosts.iter().any(|one| one == host), "{host}: {hosts:?}");
        }
    }
}

#[test]
fn a_project_whose_sandbox_is_off_runs_sandboxed_as_if_it_had_turned_it_on() {
    // `[sandbox]` with no mode has not turned it on: what it says applies once policy does.
    let off = "[sandbox]\negress = [\"model-providers\"]\nhosts = [\"api.example.com\"]\n";
    assert_eq!(reached(off, &Locks::none()), None);
    let hosts = reached(off, &locks(REQUIRED)).expect("sandboxed");
    assert!(
        hosts.iter().any(|one| one == "api.anthropic.com"),
        "{hosts:?}"
    );
    assert!(
        hosts.iter().any(|one| one == "api.example.com"),
        "{hosts:?}"
    );
    assert!(!hosts.iter().any(|one| one == "github.com"), "{hosts:?}");
}

#[test]
fn the_policy_s_own_presets_and_hosts_hold_the_sandbox_it_requires() {
    let under =
        locks(r#"{"sandbox": {"opt-out": false, "presets": ["model-providers"], "hosts": []}}"#);
    let hosts = reached("schema = 1\n", &under).expect("sandboxed");
    assert!(
        hosts.iter().any(|one| one == "api.anthropic.com"),
        "{hosts:?}"
    );
    assert!(!hosts.iter().any(|one| one == "github.com"), "{hosts:?}");
    assert!(!hosts.iter().any(|one| one == "crates.io"), "{hosts:?}");
    let off = "[sandbox]\nhosts = [\"api.example.com\"]\n";
    let hosts = reached(off, &under).expect("sandboxed");
    assert!(
        !hosts.iter().any(|one| one == "api.example.com"),
        "{hosts:?}"
    );
}

#[test]
fn a_refused_policy_file_requires_the_sandbox_too() {
    let hosts = reached("schema = 1\n", &locks("not json")).expect("sandboxed");
    assert!(
        hosts.iter().any(|one| one == "api.anthropic.com"),
        "{hosts:?}"
    );
}

#[test]
fn settings_is_told_the_sandbox_is_on_and_required_and_by_whom() {
    assert_eq!(
        locks(REQUIRED).required_by().as_deref(),
        Some("On, required by policy, set by Platform team in /etc/purlis/policy.json.")
    );
    assert_eq!(Locks::none().required_by(), None);
    assert_eq!(locks(r#"{"sandbox": {"hosts": []}}"#).required_by(), None);
    let refused = locks("not json").required_by().expect("required");
    assert!(refused.starts_with("On, required by policy: "), "{refused}");
    assert!(refused.contains(FILE), "{refused}");
}

/// What a chat of Claude Code starts under in the project at `root` on `os`, under `under`.
fn decided(
    root: &Path,
    os: Os,
    under: &Locks,
) -> Result<Option<super::super::Decided>, super::super::NotStarted> {
    TEST_LOCKS.with(|locks| *locks.borrow_mut() = under.clone());
    let decided = super::super::decide(
        crate::harness::Harness::ClaudeCode,
        root,
        &Machine { os, ..machine() },
        &|_| true,
        None,
        None,
    );
    TEST_LOCKS.with(|locks| *locks.borrow_mut() = Locks::none());
    decided
}

#[test]
fn a_system_with_no_backend_refuses_the_chat_where_policy_requires_the_sandbox() {
    use super::super::{By, Decided, Lifted, NotStarted};
    // A project whose manifest is gone reaches the decision (D-1410e): no file to write here.
    let project = tempfile::tempdir().expect("a project");
    // With no policy purlis starts it without the sandbox, as itself (ruling V21 3).
    assert_eq!(
        decided(project.path(), Os::Windows, &Locks::none()),
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        })))
    );
    // Under a policy that requires it, never unconfined.
    let under = locks(REQUIRED);
    let refused = decided(project.path(), Os::Windows, &under).expect_err("refused");
    assert_eq!(refused, NotStarted::RequiredWithoutBackend(Os::Windows));
    assert_eq!(
        refused.said(&under),
        "purlis has no sandbox backend on Windows yet, and policy requires the sandbox for \
         every chat on this machine, so nothing was started. Policy forbids starting a chat \
         without the sandbox. Locked by policy, set by Platform team in /etc/purlis/policy.json."
    );
}

#[test]
fn a_project_with_no_sandbox_starts_its_chats_sandboxed_where_policy_requires_it() {
    use super::super::Decided;
    let project = tempfile::tempdir().expect("a project");
    let project = project.path().canonicalize().expect("a real folder");
    std::fs::write(project.join(crate::plane::MANIFEST), "schema = 1\n").expect("a manifest");
    assert_eq!(decided(&project, Os::MacOs, &Locks::none()), Ok(None));
    assert!(matches!(
        decided(&project, Os::MacOs, &locks(REQUIRED)),
        Ok(Some(Decided::Sandboxed(_)))
    ));
    // And on a system with no backend it is refused, whatever its file says.
    assert_eq!(
        decided(&project, Os::Windows, &locks(REQUIRED)),
        Err(super::super::NotStarted::RequiredWithoutBackend(
            Os::Windows
        ))
    );
    assert_eq!(decided(&project, Os::Windows, &Locks::none()), Ok(None));
}

// ---- no refusal sends the person to an opt-out policy forbids (#1423) --------------------------

/// Every refusal whose way out names the opt-out.
fn refusals_that_name_the_opt_out() -> Vec<super::super::NotStarted> {
    use super::super::{Class, Named, NotStarted, Service, Uncompilable, Unheld};
    let named = Named {
        file: PathBuf::from("/p/.claude/settings.json"),
        word: "./run.sh".to_owned(),
    };
    vec![
        NotStarted::HeldBack(crate::harness::Harness::Codex, 1150),
        NotStarted::CoversItsGround {
            path: PathBuf::from("/p"),
            class: Class::LaterCode,
            named: Some(named.clone()),
            within: None,
        },
        NotStarted::CoversItsGround {
            path: PathBuf::from("/p"),
            class: Class::LaterCode,
            named: None,
            within: None,
        },
        NotStarted::Unread(named),
        NotStarted::Uncompilable(Uncompilable {
            harness: crate::harness::Harness::ClaudeCode,
            unheld: Unheld::Service(Service::CredentialStore, Os::Linux),
        }),
    ]
}

#[test]
fn a_refusal_names_the_policy_and_its_owner_instead_of_an_opt_out_policy_forbids() {
    let under = locks(REQUIRED);
    for refusal in refusals_that_name_the_opt_out() {
        let open = refusal.said(&Locks::none());
        assert_eq!(open, refusal.to_string());
        assert!(open.contains("without the sandbox"), "{open}");
        let locked = refusal.said(&under);
        assert!(
            !locked.contains("new-chat picker") && !locked.contains("tart this chat without"),
            "{locked}"
        );
        assert!(
            locked.ends_with(
                "Policy forbids starting a chat without the sandbox. Locked by policy, set by \
                 Platform team in /etc/purlis/policy.json."
            ),
            "{locked}"
        );
    }
    // The fix that is still the person's stays.
    let keyring = refusals_that_name_the_opt_out()
        .pop()
        .expect("the keyring refusal");
    assert!(
        keyring
            .said(&under)
            .contains("Move those secrets to a plain-file or 1Password vault"),
        "{}",
        keyring.said(&under)
    );
}

#[test]
fn the_picker_s_refusal_names_no_opt_out_policy_forbids() {
    use super::super::Ahead;
    // A project whose manifest is gone reaches the decision (D-1410e): no file to write here.
    let project = tempfile::tempdir().expect("a project");
    let ahead = |under: &Locks| {
        TEST_LOCKS.with(|locks| *locks.borrow_mut() = under.clone());
        let ahead = super::super::ahead(
            crate::harness::Harness::ClaudeCode,
            project.path(),
            &Machine {
                os: Os::Windows,
                ..machine()
            },
            &|_| true,
            "",
            &|_| Ok(()),
        );
        TEST_LOCKS.with(|locks| *locks.borrow_mut() = Locks::none());
        ahead
    };
    assert!(matches!(ahead(&Locks::none()), Ahead::Unsandboxed(_)));
    let Ahead::Refused { why, install } = ahead(&locks(REQUIRED)) else {
        panic!("a chat that cannot be sandboxed is refused under the policy");
    };
    assert_eq!(install, None);
    // The picker says what locks it on a line of its own, so the policy is not said twice.
    assert_eq!(
        why,
        "purlis has no sandbox backend on Windows yet, and policy requires the sandbox for \
         every chat on this machine, so nothing was started."
    );
}

#[test]
fn a_folder_refusal_names_the_policy_instead_of_an_opt_out_policy_forbids() {
    use super::super::FolderRefusal;
    let under = locks(REQUIRED);
    for why in [
        FolderRefusal::Missing,
        FolderRefusal::Linked,
        FolderRefusal::Outside,
    ] {
        let open = why.said(&Locks::none());
        assert!(
            open.ends_with("; Start without the sandbox is yours to pick when you start it."),
            "{open}"
        );
        let locked = why.said(&under);
        assert!(!locked.contains("yours to pick"), "{locked}");
        assert!(!locked.contains("without the sandbox is"), "{locked}");
        assert!(locked.contains("nothing was started"), "{locked}");
        assert!(
            locked.ends_with(&format!(". {POLICY_SENTENCE}")),
            "{locked}"
        );
    }
}

// ---- every refusal a chat's line gives is said under the lock (#1423, review M1) ---------------

/// How every refusal ends under [`REQUIRED`].
const POLICY_SENTENCE: &str = "Policy forbids starting a chat without the sandbox. Locked by \
                               policy, set by Platform team in /etc/purlis/policy.json.";

/// This thread's locks until it is dropped, a panic included.
struct Under;

impl Under {
    fn these(locks: &Locks) -> Self {
        TEST_LOCKS.with(|held| *held.borrow_mut() = locks.clone());
        Self
    }
}

impl Drop for Under {
    fn drop(&mut self) {
        TEST_LOCKS.with(|held| *held.borrow_mut() = Locks::none());
    }
}

/// Why a Claude Code chat's line is refused in `cwd` of the project at `root`, under `under`.
fn line_refused(root: &Path, cwd: &Path, under: &Locks) -> String {
    let _under = Under::these(under);
    let applied = super::super::applied_for(
        crate::harness::Harness::ClaudeCode,
        &policy(),
        &Plane::of(Some(PROJECT)),
        root,
        &machine(),
    )
    .expect("compiles");
    applied
        .line(
            super::super::Words {
                program: "claude".to_owned(),
                command: Vec::new(),
                armed: Vec::new(),
                charters: Vec::new(),
            },
            &super::super::At {
                cwd: Some(cwd),
                ..super::super::At::default()
            },
        )
        .expect_err("refused")
}

/// A refusal under [`REQUIRED`] names no opt-out and ends with the policy and who set it.
fn names_the_policy_and_no_opt_out(said: &str) {
    assert!(said.ends_with(POLICY_SENTENCE), "{said}");
    let before = said.strip_suffix(POLICY_SENTENCE).unwrap_or(said);
    assert!(!before.contains("without the sandbox"), "{said}");
    assert!(!said.contains("new-chat picker"), "{said}");
    assert!(!said.contains("yours to pick"), "{said}");
}

#[cfg(unix)]
#[test]
fn a_profile_chat_in_a_linked_folder_is_refused_naming_the_policy_and_no_opt_out() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path().canonicalize().expect("real");
    let elsewhere = tempfile::tempdir().expect("somewhere outside");
    let linked = root.join("swapped");
    std::os::unix::fs::symlink(elsewhere.path(), &linked).expect("a link");

    let open = line_refused(&root, &linked, &Locks::none());
    assert_eq!(
        open,
        super::super::FolderRefusal::Linked.said(&Locks::none())
    );
    assert!(open.contains("Start without the sandbox"), "{open}");

    let locked = line_refused(&root, &linked, &locks(REQUIRED));
    assert!(
        locked.contains("is a link or not a real folder"),
        "{locked}"
    );
    names_the_policy_and_no_opt_out(&locked);
}

#[cfg(unix)]
#[test]
fn a_manifest_covering_the_chat_s_ground_is_refused_naming_the_policy_and_no_opt_out() {
    // #1336: a manifest in the chat's folder that is a link to that folder would leave the
    // chat unable to write where it works.
    let project = tempfile::tempdir().expect("a project");
    let root = project.path().canonicalize().expect("real");
    let sub = root.join("sub");
    std::fs::create_dir_all(&sub).expect("a folder");
    std::os::unix::fs::symlink(&sub, sub.join("purlis.local.toml")).expect("a link");

    let open = line_refused(&root, &sub, &Locks::none());
    assert!(open.contains("purlis.local.toml"), "{open}");
    assert!(
        open.ends_with("Start this chat without the sandbox from the new-chat picker."),
        "{open}"
    );

    let locked = line_refused(&root, &sub, &locks(REQUIRED));
    assert!(
        locked.contains("purlis.local.toml") && locked.contains("nothing was started"),
        "{locked}"
    );
    names_the_policy_and_no_opt_out(&locked);
}

#[test]
fn a_wrap_that_cannot_be_written_names_no_opt_out_policy_forbids() {
    use super::super::seatbelt;
    let lead = "this project runs every chat sandboxed, and";
    let open = seatbelt::not_started(lead, seatbelt::TOO_LARGE, true);
    assert!(
        open.ends_with("or start this chat without the sandbox from the new-chat picker."),
        "{open}"
    );
    for why in [seatbelt::TOO_LARGE, seatbelt::CONTROL] {
        let locked = seatbelt::not_started(lead, why, false);
        assert!(locked.contains("so nothing was started."), "{locked}");
        assert!(!locked.contains("without the sandbox"), "{locked}");
        assert!(!locked.contains("new-chat picker"), "{locked}");
    }
    // What is still the person's to change stays.
    assert!(
        seatbelt::not_started(lead, seatbelt::TOO_LARGE, false)
            .ends_with("so nothing was started. Have the configs name fewer scripts."),
    );
}

/// A wrapped harness's own refusal goes through the line, which tells the adapter the opt-out
/// is locked and ends what it says with the policy: here a path the profile cannot state
/// (`seatbelt::not_started`), for Codex and opencode both.
#[test]
fn a_seatbelt_refusal_at_the_line_ends_with_the_policy_and_names_no_opt_out() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path().canonicalize().expect("real");
    // A folder whose name holds a control character: no rule can state it exactly.
    let cwd = root.join("a\nb");
    std::fs::create_dir_all(&cwd).expect("a folder");
    let under = locks(REQUIRED);
    for harness in [
        crate::harness::Harness::Codex,
        crate::harness::Harness::Opencode,
    ] {
        let refused = |locks: &Locks| {
            let _under = Under::these(locks);
            let applied = super::super::applied_for(
                harness,
                &policy(),
                &Plane::of(Some(PROJECT)),
                &root,
                &machine(),
            )
            .expect("compiles");
            let confinement = applied.confine().expect("confined");
            applied
                .line(
                    super::super::Words {
                        program: "harness".to_owned(),
                        command: Vec::new(),
                        armed: Vec::new(),
                        charters: Vec::new(),
                    },
                    &super::super::At {
                        cwd: Some(&cwd),
                        confinement: confinement.as_ref(),
                        ..super::super::At::default()
                    },
                )
                .expect_err("refused")
        };
        let open = refused(&Locks::none());
        assert!(open.contains(super::super::seatbelt::CONTROL), "{open}");
        let locked = refused(&under);
        assert!(
            locked.contains(super::super::seatbelt::CONTROL),
            "{harness:?}: {locked}"
        );
        names_the_policy_and_no_opt_out(&locked);
    }
}

// ---- the hosts a teammate is told of are the hosts a chat reaches (#1423) -----------------------

#[test]
fn the_hosts_changed_notice_names_no_host_policy_locks_out() {
    let plane = Plane::of(Some(
        "[sandbox]\nmode = \"on\"\nhosts = [\"api.example.com\", \"build.corp.example\"]\n\n\
         [sandbox.personas.devops]\nhosts = [\"10.0.0.5:6443\"]\n\n\
         [[forge]]\nkind = \"gitlab\"\nhost = \"pastebin.example\"\n\n\
         [[forge]]\nkind = \"gitlab\"\nhost = \"git.corp.example\"\n",
    ));
    assert_eq!(
        plane.granted_hosts(&Locks::none()),
        [
            "api.example.com",
            "build.corp.example",
            "pastebin.example",
            "git.corp.example",
            "10.0.0.5:6443 for devops chats"
        ]
    );
    // A hosts list holds the project's own and its forges'; a persona's are forbidden apart.
    assert_eq!(
        plane.granted_hosts(&locks(
            r#"{"sandbox": {"hosts": ["*.corp.example"], "persona-hosts": false}}"#
        )),
        ["build.corp.example", "git.corp.example"]
    );
    // A forge's host reaches nothing while policy turns the forge preset off.
    assert_eq!(
        plane.granted_hosts(&locks(r#"{"sandbox": {"presets": ["model-providers"]}}"#)),
        [
            "api.example.com",
            "build.corp.example",
            "10.0.0.5:6443 for devops chats"
        ]
    );
    // In a project that has not turned the sandbox on, nothing is told until policy requires it.
    let off = Plane::of(Some("[sandbox]\nhosts = [\"api.example.com\"]\n"));
    assert_eq!(off.granted_hosts(&Locks::none()), Vec::<String>::new());
    assert_eq!(off.granted_hosts(&locks(REQUIRED)), ["api.example.com"]);
}
