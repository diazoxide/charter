//! The sandbox policy (ADR 0067): what a plane may say, and nothing it may not. Every expected
//! answer is written out.

use super::*;

fn said(text: &str) -> Said {
    Plane::of(Some(text)).said()
}

fn default_policy() -> Option<Policy> {
    Some(Policy {
        egress: vec![Preset::ModelProviders, Preset::Forge, Preset::Toolchains],
    })
}

// -------------------------------------------------------------------------------------
// The schema: `[sandbox]` in `charter.toml`
// -------------------------------------------------------------------------------------

#[test]
fn a_plane_that_says_nothing_runs_its_chats_as_it_always_has() {
    let said = said("schema = 1\n");
    assert_eq!(said.policy, None);
    assert!(said.refused.is_empty(), "{:?}", said.refused);
}

#[test]
fn a_plane_that_turns_the_sandbox_on_gets_the_default_egress() {
    let said = said("[sandbox]\nmode = \"on\"\n");
    assert_eq!(said.policy, default_policy());
    assert!(said.refused.is_empty(), "{:?}", said.refused);
}

#[test]
fn a_plane_can_never_carry_off_and_saying_it_leaves_the_sandbox_on() {
    let said = said("[sandbox]\nmode = \"off\"\n");
    assert_eq!(said.policy, default_policy());
    assert_eq!(said.refused, [Refusal::ModeOff]);
    assert_eq!(
        said.refused[0].to_string(),
        "sandbox.mode in charter.toml cannot be \"off\": a committed file may turn the sandbox \
         on and never off — only a person turns it off, for one chat; so the sandbox is on"
    );
}

#[test]
fn a_mistyped_mode_turns_the_sandbox_on_rather_than_leaving_it_off() {
    for mode in ["\"onn\"", "\"On\"", "true", "0"] {
        let said = said(&format!("[sandbox]\nmode = {mode}\n"));
        assert_eq!(said.policy, default_policy(), "{mode}");
        assert_eq!(said.refused, [Refusal::ModeUnknown], "{mode}");
    }
}

#[test]
fn a_sandbox_that_is_not_a_table_turns_it_on() {
    let said = said("sandbox = \"off\"\n");
    assert_eq!(said.policy, default_policy());
    assert_eq!(said.refused, [Refusal::NotATable]);
}

#[test]
fn a_plane_names_its_egress_by_preset_and_an_unknown_preset_is_refused() {
    let said = said("[sandbox]\nmode = \"on\"\negress = [\"forge\", \"everywhere\"]\n");
    assert_eq!(
        said.policy,
        Some(Policy {
            egress: vec![Preset::Forge]
        })
    );
    assert_eq!(
        said.refused
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "sandbox.egress in charter.toml names \"everywhere\", which is not a preset — one of: \
          model-providers, forge, toolchains"
        ]
    );
}

#[test]
fn an_empty_egress_list_is_the_strictest_answer_and_is_kept() {
    let said = said("[sandbox]\nmode = \"on\"\negress = []\n");
    assert_eq!(said.policy, Some(Policy { egress: vec![] }));
}

#[test]
fn a_key_the_schema_does_not_have_is_refused() {
    let said = said("[sandbox]\nmode = \"on\"\nwritable = [\"/\"]\n");
    assert_eq!(said.refused, [Refusal::UnknownKey("writable".to_owned())]);
    assert!(said.policy.is_some());
}

#[test]
fn only_the_committed_file_is_asked_for_sandbox_refusals() {
    let off = "[sandbox]\nmode = \"off\"\n";
    assert_eq!(refusals(off, "charter.toml").len(), 1);
    assert_eq!(refusals(off, "charter.local.toml"), Vec::<String>::new());
}

// -------------------------------------------------------------------------------------
// The denial classes (ADR 0067 §5): each one on its own, and none a plane can remove
// -------------------------------------------------------------------------------------

/// A plane on a machine whose home is `/home/op`, with `vaults` as its committed registry.
fn denied_with(vaults: Option<&str>, os: Os) -> (tempfile::TempDir, Denied) {
    let plane = tempfile::tempdir().expect("a plane");
    if let Some(vaults) = vaults {
        std::fs::write(plane.path().join("vaults.json"), vaults).expect("the registry");
    }
    let denied = Denied::of(plane.path(), &machine(os));
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
    // A placeholder for class 4, kept because ADR 0067 fixes the classes: charter has no
    // runner yet (RR-5), so there is nothing of one to deny. The runner slice replaces this
    // with the paths it installs, denied on a runner.
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

fn hosts_of(presets: &[Preset], plane: Option<&str>) -> Vec<String> {
    hosts(presets, &Plane::of(plane))
}

#[test]
fn the_forge_preset_adds_the_self_managed_hosts_the_plane_tracks() {
    let hosts = hosts_of(
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
    let hosts = hosts_of(&[Preset::Forge], Some("[[forge]]\nhost = \"*\"\n"));
    assert!(!hosts.contains(&"*".to_owned()), "{hosts:?}");
}

#[test]
fn no_preset_is_no_host() {
    assert_eq!(hosts_of(&[], None), Vec::<String>::new());
}

#[test]
fn the_model_provider_preset_reaches_the_three_harnesses_providers() {
    let hosts = hosts_of(&[Preset::ModelProviders], None);
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
fn claude_codes_web_tools_are_denied_because_the_allowed_hosts_do_not_hold_them() {
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    assert_eq!(settings.deny, ["WebFetch", "WebSearch"]);
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
        settings.deny[2..],
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
        settings.deny[2..],
        ["Edit(//p/.charter/app)", "Edit(//p/.charter/app/**)"]
    );
}

#[test]
fn claude_code_cannot_hold_the_credential_store_on_any_system_so_the_chat_does_not_start() {
    // No key in its settings denies it (its schema, 2.1.285), and no system's sandbox has been
    // measured keeping a command away from it without one.
    for os in [Os::MacOs, Os::Linux] {
        let denied = Denied {
            paths: vec![],
            services: vec![Service::CredentialStore],
        };
        let refused = claude::settings(&compiled(denied, os)).expect_err("refused");
        assert_eq!(refused.class(), Class::Vaults, "{os:?}");
    }
}

// -------------------------------------------------------------------------------------
// The start: sandboxed, or not started (fail closed)
// -------------------------------------------------------------------------------------

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
fn a_claude_code_chat_in_a_sandboxed_plane_starts_sandboxed_for_claude_code() {
    let plane = plane_saying(ON);
    let applied = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    assert_eq!(applied.harness(), Harness::ClaudeCode);
    let Form::ClaudeCode(settings) = applied.form();
    assert_eq!(settings.sandbox["enabled"], true);
}

#[test]
fn a_harness_charter_has_no_compiler_for_does_not_start_in_a_sandboxed_plane() {
    let plane = plane_saying(ON);
    for harness in [Harness::Codex, Harness::Opencode] {
        let refused = for_start(harness, plane.path(), &machine(Os::Linux), &|_| true);
        assert_eq!(refused, Err(NotStarted::NoCompiler(harness)));
    }
}

#[test]
fn a_mistyped_mode_does_not_start_a_chat_unsandboxed() {
    let plane = plane_saying("[sandbox]\nmode = \"of\"\n");
    let refused = for_start(Harness::Codex, plane.path(), &machine(Os::Linux), &|_| true);
    assert_eq!(refused, Err(NotStarted::NoCompiler(Harness::Codex)));
}

#[test]
fn a_linux_machine_without_socat_is_told_which_program_and_how_to_install_it() {
    let plane = plane_saying(ON);
    let refused = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|program| program == "bwrap",
    )
    .expect_err("refused");
    assert_eq!(
        refused.to_string(),
        "this plane runs every chat sandboxed, and this machine cannot apply the sandbox: socat \
         is not installed — install it with `sudo apt install socat` or `sudo dnf install \
         socat`. Nothing was started."
    );
}

#[test]
fn bwrap_is_installed_as_the_bubblewrap_package() {
    let missing = backend::missing(Os::Linux, &|_| false).expect("missing");
    assert_eq!(
        missing.to_string(),
        "bwrap and socat are not installed — install them with `sudo apt install bubblewrap socat` \
         or `sudo dnf install bubblewrap socat`"
    );
}

#[test]
fn on_windows_a_sandboxed_plane_starts_no_chat_until_a_backend_exists() {
    let plane = plane_saying(ON);
    let refused = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Windows),
        &|_| true,
    );
    assert_eq!(
        refused,
        Err(NotStarted::NoBackend(backend::Missing::NoBackend(
            Os::Windows
        )))
    );
}

#[test]
fn a_keyring_vault_in_a_sandboxed_plane_is_named_in_the_refusal() {
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
    assert_eq!(
        refused.to_string(),
        "this plane runs every chat sandboxed, and Claude Code cannot keep a chat away from the \
         operating system's credential store, where this plane's keyring vaults are kept, so \
         the vaults class cannot be held. Until charter can wrap the harness or resolve secrets \
         for it, a sandboxed plane with a keyring vault starts no Claude Code chat. Nothing was \
         started."
    );
}
