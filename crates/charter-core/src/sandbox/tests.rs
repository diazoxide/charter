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
fn a_sandboxed_chat_may_read_and_run_charters_git_hooks_and_never_rewrite_them() {
    // SQ-16, ADR 0074: git treats a hook it cannot read or run as absent and commits unscanned,
    // so nothing a chat is denied may cover the directory. The directory is the app's data
    // directory on macOS (Tauri's, under the bundle's identifier) — outside the chat's own
    // directory, which is all Claude Code's sandbox lets a command write — so a chat reads and
    // runs it and cannot rewrite it.
    let (_plane, denied) = denied_with(None, Os::MacOs);
    let hooks =
        std::path::PathBuf::from("/home/op/Library/Application Support/dev.charter.app/git-hooks");
    for denial in &denied.paths {
        assert!(
            denial.access == Access::Write || !hooks.starts_with(&denial.path),
            "{} is denied to a chat's reads and holds charter's git hooks",
            denial.path.display()
        );
    }
    let settings = claude::settings(&compiled(denied, Os::MacOs)).expect("compiles");
    let deny_read = settings.sandbox["filesystem"]["denyRead"].to_string();
    assert!(!deny_read.contains("Library"), "{deny_read}");
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
fn a_chat_never_reads_or_writes_the_forge_answers_the_humans_token_fetched() {
    // The native forge transport's ETag store holds raw answers the human's sign-in token
    // fetched (ADR 0070 §3, FW-2a); a chat reads neither it nor the bodies in it.
    let (_plane, denied) = denied_with(None, Os::Linux);
    assert_eq!(
        paths(&denied, Class::HumanPowers, Access::ReadWrite),
        [std::path::PathBuf::from(
            "/home/op/.config/charter/forge-etags"
        )]
    );
}

#[test]
fn claude_code_and_codex_both_deny_a_chat_the_forge_etag_store() {
    let etags = "/home/op/.config/charter/forge-etags";
    let (_plane, denied) = denied_with(None, Os::Linux);
    let settings = claude::settings(&compiled(denied.clone(), Os::Linux)).expect("compiles");
    let read_denied = settings.sandbox["filesystem"]["denyRead"].clone();
    assert!(
        read_denied
            .as_array()
            .is_some_and(|all| all.iter().any(|p| p == etags)),
        "{read_denied}"
    );
    assert!(
        settings.deny.contains(&format!("Read(/{etags}/**)")),
        "{:?}",
        settings.deny
    );
    let flags =
        codex::flags_named(&compiled(denied, Os::Linux), "charter-sandbox-1").expect("compiles");
    let profile = codex_value(&flags, "permissions.charter-sandbox-1");
    assert_eq!(
        profile["filesystem"][etags].as_str(),
        Some("deny"),
        "{profile}"
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
// Codex: `-c` flags that select a permissions profile charter names for this start alone
// -------------------------------------------------------------------------------------

/// Each `-c key=value` a Codex chat is handed, the value read back as the TOML Codex parses.
fn codex_config(flags: &codex::Flags) -> Vec<(String, toml::Value)> {
    let mut pairs = flags.args.chunks(2);
    let mut out = Vec::new();
    for pair in &mut pairs {
        if pair[0] != "-c" {
            continue;
        }
        let (key, value) = pair[1].split_once('=').expect("key=value");
        let value: toml::Table = toml::from_str(&format!("v = {value}")).expect("TOML");
        out.push((key.to_owned(), value["v"].clone()));
    }
    out
}

fn codex_value(flags: &codex::Flags, key: &str) -> toml::Value {
    codex_config(flags)
        .into_iter()
        .find(|(it, _)| it == key)
        .unwrap_or_else(|| panic!("{key} is not handed: {:?}", flags.args))
        .1
}

#[test]
fn a_codex_chat_runs_under_a_workspace_write_profile_that_charter_selects_explicitly() {
    let flags = codex::flags_named(&compiled(Denied::default(), Os::MacOs), "charter-sandbox-1")
        .expect("compiles");
    assert_eq!(
        codex_value(&flags, "default_permissions"),
        toml::Value::from("charter-sandbox-1")
    );
    assert_eq!(
        codex_value(&flags, "permissions.charter-sandbox-1")["extends"],
        toml::Value::from(":workspace")
    );
}

#[test]
fn a_codex_chat_reaches_the_presets_hosts_through_codexs_own_proxy_and_nothing_else() {
    let flags = codex::flags_named(&compiled(Denied::default(), Os::MacOs), "charter-sandbox-1")
        .expect("compiles");
    let profile = codex_value(&flags, "permissions.charter-sandbox-1");
    assert_eq!(
        profile["network"],
        toml::toml! { enabled = true
        [domains]
        "github.com" = "allow" }
        .into()
    );
    // Without the proxy, a profile's `network.enabled` opens the network whole (measured).
    // `--enable` rather than `-c`: Codex refuses to start on a feature it does not have, so a
    // Codex without the proxy never runs this profile (measured).
    assert!(
        flags
            .args
            .windows(2)
            .any(|pair| pair == ["--enable", "network_proxy"]),
        "{:?}",
        flags.args
    );
}

#[test]
fn a_codex_chat_has_the_features_that_may_reach_past_the_proxy_turned_off() {
    // Stable in 0.147.0 and unmeasured against the sandbox: off until measured. A Codex that
    // does not know one of them refuses to start (measured), so this fails closed too.
    let flags = codex::flags_named(&compiled(Denied::default(), Os::MacOs), "charter-sandbox-1")
        .expect("compiles");
    let disabled: Vec<&str> = flags
        .args
        .windows(2)
        .filter(|pair| pair[0] == "--disable")
        .map(|pair| pair[1].as_str())
        .collect();
    assert_eq!(disabled, ["browser_use", "computer_use", "in_app_browser"]);
}

#[test]
fn codex_denies_a_read_denied_path_outright_and_leaves_a_write_denied_one_readable() {
    let denied = Denied {
        paths: vec![
            one(Class::Vaults, "/p/.charter/vaults", Access::ReadWrite),
            one(Class::Integrity, "/p/.charter/app", Access::Write),
        ],
        services: vec![],
    };
    let flags =
        codex::flags_named(&compiled(denied, Os::Linux), "charter-sandbox-1").expect("compiles");
    let profile = codex_value(&flags, "permissions.charter-sandbox-1");
    assert_eq!(
        profile["filesystem"],
        toml::toml! {
            "/p/.charter/vaults" = "deny"
            "/p/.charter/app" = "read"
        }
        .into()
    );
}

#[test]
fn a_codex_chat_cannot_be_escalated_out_of_its_sandbox_or_search_the_web() {
    let flags = codex::flags_named(&compiled(Denied::default(), Os::MacOs), "charter-sandbox-1")
        .expect("compiles");
    // Every request to run a command outside the sandbox, or with more than it allows, is
    // rejected rather than shown; the prompts that do not widen it stay.
    assert_eq!(
        codex_value(&flags, "approval_policy"),
        toml::toml! { [granular]
        sandbox_approval = false
        request_permissions = false
        skill_approval = false
        rules = true
        mcp_elicitations = true }
        .into()
    );
    // A person decides what is still asked, never a reviewing model.
    assert_eq!(
        codex_value(&flags, "approvals_reviewer"),
        toml::Value::from("user")
    );
    assert_eq!(
        codex_value(&flags, "web_search"),
        toml::Value::from("disabled")
    );
}

#[test]
fn codex_cannot_hold_the_credential_store_on_any_system_so_the_chat_does_not_start() {
    // Measured on macOS, codex-cli 0.147.0: with the network on, a command in its sandbox
    // still reached the keychain. Unmeasured on Linux.
    for os in [Os::MacOs, Os::Linux] {
        let denied = Denied {
            paths: vec![],
            services: vec![Service::CredentialStore],
        };
        let refused = codex::flags_named(&compiled(denied, os), "n").expect_err("refused");
        assert_eq!(refused.harness, Harness::Codex, "{os:?}");
        assert_eq!(refused.class(), Class::Vaults, "{os:?}");
    }
}

#[test]
fn each_codex_start_names_a_profile_no_config_file_can_have_named_before_it() {
    // A config layer that names the profile merges hosts into it (measured), so the name is
    // new at every start.
    let compiled = compiled(Denied::default(), Os::MacOs);
    let one = codex::flags(&compiled).expect("compiles");
    let two = codex::flags(&compiled).expect("compiles");
    let name = |flags: &codex::Flags| {
        codex_value(flags, "default_permissions")
            .as_str()
            .expect("a name")
            .to_owned()
    };
    assert!(name(&one).starts_with("charter-sandbox-"), "{}", name(&one));
    assert_ne!(name(&one), name(&two));
}

fn words(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

#[test]
fn a_codex_command_that_would_drop_or_widen_the_sandbox_is_named() {
    // Each measured on codex-cli 0.147.0 or read from its source: `-s` of any value and the
    // bypass drop the profile whole, `--add-dir` and `--cd` move what is writable, `-a` and
    // `--approve-for-me` outrank the approval policy, `--search` turns the web search back on,
    // and a feature flag's effect on the sandbox is unmeasured, one feature at a time.
    for (line, flag) in [
        ("codex -s danger-full-access", "-s"),
        ("codex -s read-only", "-s"),
        ("codex --sandbox=workspace-write", "--sandbox"),
        ("codex -sdanger-full-access", "-s"),
        (
            "codex --dangerously-bypass-approvals-and-sandbox",
            "--dangerously-bypass-approvals-and-sandbox",
        ),
        ("codex --yolo", "--yolo"),
        ("codex --add-dir /home/op", "--add-dir"),
        ("codex -C /home/op", "-C"),
        ("codex --cd=/home/op", "--cd"),
        ("codex -a on-request", "-a"),
        ("codex --ask-for-approval untrusted", "--ask-for-approval"),
        ("codex --approve-for-me", "--approve-for-me"),
        ("codex --not-so-yolo", "--not-so-yolo"),
        ("codex --search", "--search"),
        ("codex --disable network_proxy", "--disable"),
        ("codex --disable=network_proxy", "--disable"),
        ("codex --enable browser_use", "--enable"),
        ("codex --enable=respect_system_proxy", "--enable"),
    ] {
        let why = codex::loosened_by(&words(line)).unwrap_or_else(|| panic!("{line}"));
        assert!(why.contains(&format!("`{flag}`")), "{line}: {why}");
    }
}

#[test]
fn a_codex_config_override_outside_the_few_that_cannot_touch_the_sandbox_is_named() {
    // Each class a `-c` could reach the sandbox through, in each spelling Codex takes.
    for key in [
        "sandbox_mode=\"danger-full-access\"",
        "sandbox_workspace_write.network_access=true",
        "profile=\"loose\"",
        "default_permissions=\":danger-full-access\"",
        "permissions.mine.network.enabled=true",
        "features.network_proxy=false",
        "tools.web_search=true",
        "approval_policy=\"on-request\"",
        "approvals_reviewer=\"auto_review\"",
        "web_search=\"live\"",
        "network.enabled=true",
        "hooks.Stop=[]",
        "mcp_servers.x.command=\"sh\"",
        "shell_environment_policy.inherit=\"all\"",
    ] {
        let name = key.split('=').next().expect("a key");
        for line in [
            format!("codex -c {key}"),
            format!("codex --config {key}"),
            format!("codex --config={key}"),
            format!("codex -c{key}"),
        ] {
            let why = codex::loosened_by(&words(&line)).unwrap_or_else(|| panic!("{line}"));
            assert!(why.contains(&format!("`-c {name}`")), "{line}: {why}");
        }
    }
}

#[test]
fn a_codex_command_that_only_tightens_or_does_not_touch_the_sandbox_is_not_named() {
    for line in [
        "codex",
        "codex -a never",
        "codex --ask-for-approval=never",
        // Measured: a `-p` profile's `sandbox_mode`, `default_permissions`,
        // `sandbox_workspace_write`, approval policy, web search and proxy feature all lose to
        // charter's flags.
        "codex -m gpt-5 -p work",
        "codex resume 0199",
        "codex -c model=\"o3\"",
        "codex --config model_reasoning_effort=\"high\"",
        "codex -c model_providers.local.base_url=\"http://localhost:1234/v1\"",
    ] {
        assert_eq!(codex::loosened_by(&words(line)), None, "{line}");
    }
    // A first message is not a flag, even one that starts with a dash.
    assert_eq!(codex::loosened_by(&["-a quick fix".to_owned()]), None);
}

fn codex_sandbox_in(plane: &tempfile::TempDir) -> Applied {
    for_start(Harness::Codex, plane.path(), &machine(Os::MacOs), &|_| true)
        .expect("starts")
        .expect("sandboxed")
}

#[test]
fn a_codex_line_puts_the_sandbox_last_among_the_flags_and_before_a_first_message() {
    let plane = plane_saying(ON);
    let applied = codex_sandbox_in(&plane);
    let Form::Codex(flags) = applied.form() else {
        panic!("compiled for Codex");
    };
    let sandbox = flags.args.clone();

    let line = applied
        .line(
            words("work"),
            words("-c hooks.Stop=[]"),
            vec!["-m".to_owned(), "o3".to_owned(), "fix the bug".to_owned()],
        )
        .expect("starts");
    let want: Vec<String> = [words("work -c hooks.Stop=[] -m o3"), sandbox.clone()]
        .concat()
        .into_iter()
        .chain(["fix the bug".to_owned()])
        .collect();
    assert_eq!(line, want);

    // A resume is a subcommand and its id: the flags go in front of both, where the hooks'
    // own `-c` flags already stand.
    let line = applied
        .line(Vec::new(), Vec::new(), words("resume 0199"))
        .expect("starts");
    assert_eq!(line, [sandbox, words("resume 0199")].concat());
}

#[test]
fn a_sandbox_is_refused_a_line_that_would_drop_it_and_says_where_the_flag_came_from() {
    let plane = plane_saying(ON);
    let applied = codex_sandbox_in(&plane);
    assert_eq!(
        applied.line(words("-s danger-full-access"), Vec::new(), Vec::new()),
        Err(
            "this plane runs every chat sandboxed, and the profile's command names `-s`, which \
             would run Codex outside the sandbox charter compiled for it, so nothing was \
             started. Take it out of the profile's command."
                .to_owned()
        )
    );
    assert_eq!(
        applied.line(
            Vec::new(),
            Vec::new(),
            words("-c sandbox_mode=\"danger-full-access\"")
        ),
        Err(
            "this plane runs every chat sandboxed, and the chat's own arguments name \
             `-c sandbox_mode`, which would run Codex outside the sandbox charter compiled for \
             it, so nothing was started. Start it without that argument."
                .to_owned()
        )
    );
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
    let Form::ClaudeCode(settings) = applied.form() else {
        panic!("compiled for Claude Code: {:?}", applied.form());
    };
    assert_eq!(settings.sandbox["enabled"], true);
}

#[test]
fn a_codex_chat_in_a_sandboxed_plane_starts_sandboxed_for_codex() {
    let plane = plane_saying(ON);
    let applied = for_start(Harness::Codex, plane.path(), &machine(Os::MacOs), &|_| true)
        .expect("starts")
        .expect("sandboxed");
    assert_eq!(applied.harness(), Harness::Codex);
    let Form::Codex(flags) = applied.form() else {
        panic!("compiled for Codex: {:?}", applied.form());
    };
    assert!(
        codex_value(flags, "default_permissions")
            .as_str()
            .is_some_and(|name| name.starts_with(codex::PROFILE_PREFIX)),
        "{flags:?}"
    );
}

#[test]
fn a_harness_charter_has_no_compiler_for_does_not_start_in_a_sandboxed_plane() {
    let plane = plane_saying(ON);
    let refused = for_start(
        Harness::Opencode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    );
    assert_eq!(refused, Err(NotStarted::NoCompiler(Harness::Opencode)));
    assert_eq!(
        refused.expect_err("refused").to_string(),
        "this plane runs every chat sandboxed, and charter cannot sandbox an opencode chat yet, \
         so it was not started. Start this chat on a Claude Code or Codex profile."
    );
}

#[test]
fn a_mistyped_mode_does_not_start_a_chat_unsandboxed() {
    let plane = plane_saying("[sandbox]\nmode = \"of\"\n");
    let refused = for_start(
        Harness::Opencode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    );
    assert_eq!(refused, Err(NotStarted::NoCompiler(Harness::Opencode)));
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
