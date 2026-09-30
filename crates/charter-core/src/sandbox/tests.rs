//! The sandbox policy (ADR 0067): what a plane may say, and nothing it may not. Every expected
//! answer is written out.

use super::*;

// -------------------------------------------------------------------------------------
// The schema: `[sandbox]` in `charter.toml`
// -------------------------------------------------------------------------------------

#[test]
fn a_plane_that_says_nothing_runs_its_chats_as_it_always_has() {
    let said = Said::of(Some("schema = 1\n"));
    assert_eq!(said.policy, None);
    assert!(said.refused.is_empty(), "{:?}", said.refused);
}

#[test]
fn a_plane_that_turns_the_sandbox_on_gets_the_default_egress() {
    let said = Said::of(Some("[sandbox]\nmode = \"on\"\n"));
    assert_eq!(
        said.policy,
        Some(Policy {
            egress: vec![Preset::ModelProviders, Preset::Forge, Preset::Toolchains]
        })
    );
    assert!(said.refused.is_empty(), "{:?}", said.refused);
}

#[test]
fn a_plane_can_never_carry_off() {
    let said = Said::of(Some("[sandbox]\nmode = \"off\"\n"));
    assert_eq!(said.policy, None);
    assert_eq!(
        said.refused,
        [
            "sandbox.mode in charter.toml cannot be \"off\": a committed file may turn the sandbox \
          on and never off — only a person turns it off, for one chat"
        ]
    );
}

#[test]
fn a_plane_names_its_egress_by_preset_and_an_unknown_preset_is_refused() {
    let said = Said::of(Some(
        "[sandbox]\nmode = \"on\"\negress = [\"forge\", \"everywhere\"]\n",
    ));
    assert_eq!(
        said.policy,
        Some(Policy {
            egress: vec![Preset::Forge]
        })
    );
    assert_eq!(
        said.refused,
        [
            "sandbox.egress in charter.toml names \"everywhere\", which is not a preset — one of: \
          model-providers, forge, toolchains"
        ]
    );
}

#[test]
fn an_empty_egress_list_is_the_strictest_answer_and_is_kept() {
    let said = Said::of(Some("[sandbox]\nmode = \"on\"\negress = []\n"));
    assert_eq!(said.policy, Some(Policy { egress: vec![] }));
}

#[test]
fn a_key_the_schema_does_not_have_is_refused() {
    let said = Said::of(Some("[sandbox]\nmode = \"on\"\nwritable = [\"/\"]\n"));
    assert_eq!(
        said.refused,
        [
            "sandbox.writable in charter.toml is not a key charter reads — [sandbox] holds mode \
          and egress"
        ]
    );
    assert!(said.policy.is_some());
}

#[test]
fn a_mode_that_is_not_a_word_charter_knows_leaves_the_plane_as_it_was() {
    let said = Said::of(Some("[sandbox]\nmode = true\n"));
    assert_eq!(said.policy, None);
    assert_eq!(
        said.refused,
        ["sandbox.mode in charter.toml is not \"on\" — the one value a plane may give it"]
    );
}

// -------------------------------------------------------------------------------------
// The denial classes (ADR 0067 §5): each one on its own, and none a plane can remove
// -------------------------------------------------------------------------------------

/// A plane at `/plane` on a machine whose home is `/home/op`, with `vaults` as its committed
/// registry.
fn denied_with(vaults: Option<&str>, os: Os) -> (tempfile::TempDir, Denied) {
    let plane = tempfile::tempdir().expect("a plane");
    if let Some(vaults) = vaults {
        std::fs::write(plane.path().join("vaults.json"), vaults).expect("the registry");
    }
    let machine = Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(std::path::PathBuf::from("/home/op")),
        os,
    };
    let denied = Denied::of(plane.path(), &machine);
    (plane, denied)
}

fn paths(denied: &Denied, class: Class, access: Access) -> Vec<std::path::PathBuf> {
    denied
        .paths
        .iter()
        .filter(|it| it.class == class && it.access == access)
        .map(|it| it.path.clone())
        .collect()
}

#[test]
fn the_four_classes_are_the_adrs_four() {
    assert_eq!(
        Class::ALL.map(Class::word),
        ["vaults", "integrity", "human-powers", "runner-internals"]
    );
}

#[test]
fn a_chat_never_reads_or_writes_a_vaults_storage() {
    let (plane, denied) = denied_with(
        Some(
            r#"{"vaults": {"dev": {"provider": "plain-file", "config": {"file": "secrets/dev.json"}}}}"#,
        ),
        Os::Linux,
    );
    let root = plane.path();
    assert_eq!(
        paths(&denied, Class::Vaults, Access::ReadWrite),
        [
            root.join(".charter/vaults"),
            root.join(".charter/fingerprint.key"),
            root.join("secrets/dev.json"),
            std::path::PathBuf::from("/home/op/.config/op"),
            std::path::PathBuf::from("/home/op/.op"),
            std::path::PathBuf::from("/home/op/.vault-token"),
        ]
    );
}

#[test]
fn a_chat_may_read_the_registry_that_names_vaults_but_never_rewrite_it() {
    let (plane, denied) = denied_with(None, Os::Linux);
    let root = plane.path();
    assert_eq!(
        paths(&denied, Class::Vaults, Access::Write),
        [root.join("vaults.json"), root.join(".charter/vaults.json")]
    );
}

#[test]
fn a_keyring_vault_is_denied_as_the_operating_systems_credential_service() {
    let (_plane, denied) = denied_with(
        Some(r#"{"vaults": {"dev": {"provider": "keyring"}}}"#),
        Os::MacOs,
    );
    assert_eq!(denied.services, [Service::CredentialStore]);
}

#[test]
fn a_plane_with_no_keyring_vault_asks_nothing_of_the_credential_service() {
    let (_plane, denied) = denied_with(
        Some(r#"{"vaults": {"dev": {"provider": "plain-file", "config": {"file": "d.json"}}}}"#),
        Os::MacOs,
    );
    assert_eq!(denied.services, []);
}

#[test]
fn a_registry_charter_cannot_read_is_taken_to_hold_a_keyring_vault() {
    // Fail closed: a corrupt registry could be hiding the default provider.
    let (_plane, denied) = denied_with(Some("not json"), Os::MacOs);
    assert_eq!(denied.services, [Service::CredentialStore]);
}

#[test]
fn a_chat_never_writes_charters_integrity_state() {
    let (plane, denied) = denied_with(None, Os::Linux);
    assert_eq!(
        paths(&denied, Class::Integrity, Access::Write),
        [plane.path().join(".charter/app")]
    );
}

#[test]
fn a_chat_never_writes_the_approvals_a_person_gave() {
    let (_plane, denied) = denied_with(None, Os::Linux);
    assert_eq!(
        paths(&denied, Class::HumanPowers, Access::Write),
        [std::path::PathBuf::from("/home/op/.config/charter")]
    );
}

#[test]
fn off_a_runner_there_are_no_runner_internals_to_deny() {
    let (_plane, denied) = denied_with(None, Os::Linux);
    assert!(
        denied
            .paths
            .iter()
            .all(|it| it.class != Class::RunnerInternals)
    );
}

// -------------------------------------------------------------------------------------
// Egress: named presets, and the plane's own forge hosts
// -------------------------------------------------------------------------------------

#[test]
fn the_forge_preset_adds_the_self_managed_hosts_the_plane_tracks() {
    let hosts = hosts(
        &[Preset::Forge],
        Some("[[forge]]\nkind = \"gitlab\"\nhost = \"git.example.org:8443\"\n"),
    );
    assert!(hosts.contains(&"github.com".to_owned()), "{hosts:?}");
    assert!(hosts.contains(&"gitlab.com".to_owned()), "{hosts:?}");
    assert!(hosts.contains(&"git.example.org".to_owned()), "{hosts:?}");
    assert!(
        !hosts.contains(&"api.anthropic.com".to_owned()),
        "{hosts:?}"
    );
}

#[test]
fn a_forge_host_that_is_not_a_host_is_never_let_through() {
    let hosts = hosts(&[Preset::Forge], Some("[[forge]]\nhost = \"*\"\n"));
    assert!(!hosts.contains(&"*".to_owned()), "{hosts:?}");
}

#[test]
fn no_preset_is_no_host() {
    assert_eq!(hosts(&[], None), Vec::<String>::new());
}

#[test]
fn the_model_provider_preset_reaches_the_three_harnesses_providers() {
    let hosts = hosts(&[Preset::ModelProviders], None);
    for host in ["api.anthropic.com", "api.openai.com", "opencode.ai"] {
        assert!(hosts.contains(&host.to_owned()), "{host}: {hosts:?}");
    }
}

// -------------------------------------------------------------------------------------
// Claude Code: the `sandbox` object in the `--settings` charter already passes
// -------------------------------------------------------------------------------------

fn compiled(denied: Denied, os: Os) -> Compiled {
    Compiled {
        denied,
        hosts: vec!["github.com".to_owned()],
        os,
    }
}

fn one(class: Class, path: &str, access: Access) -> Denial {
    Denial {
        class,
        path: std::path::PathBuf::from(path),
        access,
    }
}

#[test]
fn a_claude_code_chat_is_sandboxed_with_no_way_out_and_no_silent_fallback() {
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    assert_eq!(settings.sandbox["enabled"], true);
    assert_eq!(settings.sandbox["allowUnsandboxedCommands"], false);
    assert_eq!(settings.sandbox["failIfUnavailable"], true);
}

#[test]
fn a_claude_code_chat_reaches_only_the_presets_hosts_and_is_never_asked_to_widen_them() {
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    assert_eq!(
        settings.sandbox["network"],
        serde_json::json!({"allowedDomains": ["github.com"], "strictAllowlist": true})
    );
}

#[test]
fn a_path_denied_to_read_is_denied_to_the_sandbox_and_to_claude_codes_own_tools() {
    let denied = Denied {
        paths: vec![one(Class::Vaults, "/p/.charter/vaults", Access::ReadWrite)],
        services: vec![],
    };
    let settings = claude::settings(&compiled(denied, Os::Linux)).expect("compiles");
    assert_eq!(
        settings.sandbox["filesystem"],
        serde_json::json!({
            "denyRead": ["/p/.charter/vaults"],
            "denyWrite": ["/p/.charter/vaults"],
        })
    );
    assert_eq!(
        settings.deny,
        [
            "Read(//p/.charter/vaults)",
            "Read(//p/.charter/vaults/**)",
            "Edit(//p/.charter/vaults)",
            "Edit(//p/.charter/vaults/**)",
        ]
    );
}

#[test]
fn a_path_denied_to_write_stays_readable() {
    let denied = Denied {
        paths: vec![one(Class::Integrity, "/p/.charter/app", Access::Write)],
        services: vec![],
    };
    let settings = claude::settings(&compiled(denied, Os::Linux)).expect("compiles");
    assert_eq!(
        settings.sandbox["filesystem"],
        serde_json::json!({"denyRead": [], "denyWrite": ["/p/.charter/app"]})
    );
    assert_eq!(
        settings.deny,
        ["Edit(//p/.charter/app)", "Edit(//p/.charter/app/**)"]
    );
}

#[test]
fn on_macos_claude_code_cannot_deny_the_credential_store_so_the_chat_does_not_start() {
    // No key in its settings denies it there (its schema, 2.1.285).
    let denied = Denied {
        paths: vec![],
        services: vec![Service::CredentialStore],
    };
    let refused = claude::settings(&compiled(denied, Os::MacOs)).expect_err("refused");
    assert_eq!(refused.class, Class::Vaults);
}

#[test]
fn on_linux_claude_codes_sandbox_already_cuts_the_credential_store_off() {
    // Its sandbox there already keeps a command away from the credential store.
    let denied = Denied {
        paths: vec![],
        services: vec![Service::CredentialStore],
    };
    assert!(claude::settings(&compiled(denied, Os::Linux)).is_ok());
}

// -------------------------------------------------------------------------------------
// The start: sandboxed, or not started (fail closed)
// -------------------------------------------------------------------------------------

use crate::harness::Harness;

fn plane_saying(toml: &str) -> tempfile::TempDir {
    let plane = tempfile::tempdir().expect("a plane");
    std::fs::write(plane.path().join("charter.toml"), toml).expect("charter.toml");
    plane
}

fn machine(os: Os) -> Machine {
    Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(std::path::PathBuf::from("/home/op")),
        os,
    }
}

const ON: &str = "[sandbox]\nmode = \"on\"\n";

#[test]
fn a_chat_in_a_plane_that_says_nothing_starts_as_it_always_has() {
    let plane = plane_saying("schema = 1\n");
    let started = for_start(Harness::Codex, plane.path(), &machine(Os::Windows), &|_| {
        false
    });
    assert_eq!(started, Ok(None));
}

#[test]
fn a_claude_code_chat_in_a_sandboxed_plane_starts_sandboxed() {
    let plane = plane_saying(ON);
    let started = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    )
    .expect("starts");
    let Some(Applied::ClaudeCode(settings)) = started else {
        panic!("not sandboxed: {started:?}");
    };
    assert_eq!(settings.sandbox["enabled"], true);
}

#[test]
fn a_harness_charter_does_not_compile_for_yet_does_not_start_in_a_sandboxed_plane() {
    let plane = plane_saying(ON);
    for (harness, title) in [(Harness::Codex, "Codex"), (Harness::Opencode, "opencode")] {
        let refused =
            for_start(harness, plane.path(), &machine(Os::Linux), &|_| true).expect_err("refused");
        assert!(
            refused.starts_with(&format!(
                "this plane runs every chat sandboxed, and charter cannot sandbox a {title} \
                 chat yet"
            )),
            "{refused}"
        );
    }
}

#[test]
fn a_machine_without_the_sandboxs_programs_does_not_start_the_chat_and_names_them() {
    let plane = plane_saying(ON);
    let refused = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|program| program == "bwrap",
    )
    .expect_err("refused");
    assert!(refused.contains("socat"), "{refused}");
    assert!(!refused.contains("bwrap,"), "{refused}");
}

#[test]
fn on_windows_a_sandboxed_plane_starts_no_chat_until_a_backend_exists() {
    let plane = plane_saying(ON);
    let refused = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Windows),
        &|_| true,
    )
    .expect_err("refused");
    assert!(refused.contains("Windows"), "{refused}");
}

#[test]
fn a_class_the_harness_cannot_hold_here_is_named_in_the_refusal() {
    let plane = plane_saying(ON);
    std::fs::write(
        plane.path().join("vaults.json"),
        r#"{"vaults": {"dev": {"provider": "keyring"}}}"#,
    )
    .expect("the registry");
    let refused = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
    )
    .expect_err("refused");
    assert!(refused.contains("vaults"), "{refused}");
    assert!(refused.contains("credential store"), "{refused}");
}
