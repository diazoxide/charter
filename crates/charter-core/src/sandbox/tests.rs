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
fn the_five_classes_are_the_adrs_five() {
    assert_eq!(
        Class::ALL.map(Class::word),
        [
            "vaults",
            "integrity",
            "human-powers",
            "runner-internals",
            "later-code"
        ]
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

/// The second acceptance line of #667, as V22a words it: a chat cannot write another chat's
/// spool. A sandboxed chat neither reads nor writes any chat's, its own included, nor the keys
/// that check them: the hooks of the harnesses charter sandboxes run outside the sandbox their
/// tools run in (ADR 0068 §6, V63).
#[cfg(unix)]
#[test]
fn a_chat_never_reads_or_writes_any_chats_hook_spool_or_its_keys() {
    let (plane, denied) = denied_with(None, Os::Linux);
    let spool = crate::hookwire::spool::dir_for(&plane.path().join(".charter/app/hooks.sock"));

    let held = paths(&denied, Class::Integrity, Access::ReadWrite);
    for file in [
        spool.join("6.jsonl"),
        spool.join(crate::hookwire::spool::KEYS),
    ] {
        assert!(
            held.iter().any(|denied| file.starts_with(denied)),
            "{} is not under {held:?}",
            file.display()
        );
    }
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
    let wrap = codex::wrap(&compiled(denied, Os::MacOs)).expect("compiles");
    assert!(
        wrap.denied
            .iter()
            .any(|it| it.path == std::path::Path::new(etags) && it.access == Access::ReadWrite),
        "{:?}",
        wrap.denied
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
        homes: Homes::default(),
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
    assert_eq!(settings.deny[..2], ["WebFetch", "WebSearch"]);
}

/// `rules` without the later-code class's, which name a path at any depth (`**/`).
fn by_path(rules: &[String]) -> Vec<String> {
    rules
        .iter()
        .filter(|rule| !rule.contains("**/"))
        .cloned()
        .collect()
}

/// A Claude Code sandbox's `denyRead` and `denyWrite`, each without the later-code class's.
fn filesystem_by_path(settings: &claude::Settings) -> (Vec<String>, Vec<String>) {
    let list = |key: &str| -> Vec<String> {
        settings.sandbox["filesystem"][key]
            .as_array()
            .expect("a list")
            .iter()
            .map(|it| it.as_str().expect("a path").to_owned())
            .collect()
    };
    (by_path(&list("denyRead")), by_path(&list("denyWrite")))
}

#[test]
fn a_path_denied_to_read_is_denied_to_the_sandbox_and_to_claude_codes_own_tools() {
    let denied = Denied {
        paths: vec![one(Class::Vaults, "/p/.charter/vaults", Access::ReadWrite)],
        services: vec![],
    };
    let settings = claude::settings(&compiled(denied, Os::Linux)).expect("compiles");
    assert_eq!(
        filesystem_by_path(&settings),
        (
            vec!["/p/.charter/vaults".to_owned()],
            vec!["/p/.charter/vaults".to_owned()]
        )
    );
    assert_eq!(
        by_path(&settings.deny)[2..],
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
        filesystem_by_path(&settings),
        (Vec::new(), vec!["/p/.charter/app".to_owned()])
    );
    assert_eq!(
        by_path(&settings.deny)[2..],
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
        assert_eq!(refused.class(), Some(Class::Vaults), "{os:?}");
    }
}

// -------------------------------------------------------------------------------------
// Codex: what a chat's own words may not say to the Codex charter wraps
// -------------------------------------------------------------------------------------

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

/// The arguments `applied` gives a chat with these words, or why it may not start.
fn args_of(
    applied: &Applied,
    command: Vec<String>,
    armed: Vec<String>,
    charters: Vec<String>,
) -> Result<Vec<String>, String> {
    applied
        .line(
            Words {
                program: "codex".to_owned(),
                command,
                armed,
                charters,
            },
            &At {
                cwd: Some(applied.root()),
                ..At::default()
            },
        )
        .map(|line| line.args)
}

/// What a Codex chat starts under in `plane`, wrapped (#1123).
fn codex_sandbox_in(plane: &tempfile::TempDir) -> Applied {
    for_start(Harness::Codex, plane.path(), &machine(Os::MacOs), &|_| true)
        .expect("starts")
        .expect("sandboxed")
}

#[test]
fn a_sandbox_is_refused_a_line_that_would_drop_it_and_says_where_the_flag_came_from() {
    let plane = plane_saying(ON);
    let applied = codex_sandbox_in(&plane);
    assert_eq!(
        args_of(
            &applied,
            words("-s danger-full-access"),
            Vec::new(),
            Vec::new()
        ),
        Err(
            "this plane runs every chat sandboxed, and the profile's command names `-s`, which \
             would run Codex outside the sandbox charter compiled for it, so nothing was \
             started. Take it out of the profile's command."
                .to_owned()
        )
    );
    assert_eq!(
        args_of(
            &applied,
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
fn a_mistyped_mode_does_not_start_a_chat_unsandboxed() {
    let plane = plane_saying("[sandbox]\nmode = \"of\"\n");
    let refused = for_start(
        Harness::Opencode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    );
    assert_eq!(
        refused,
        Err(NotStarted::Uncompilable(Uncompilable {
            harness: Harness::Opencode,
            unheld: Unheld::Wrap(Os::Linux),
        }))
    );
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

#[test]
fn claude_code_is_denied_submodule_git_config_and_hook_managers_at_any_depth() {
    // Measured on 2.1.288: a `**` in the middle of a `denyWrite` glob holds at any depth.
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    let write = &settings.sandbox["filesystem"]["denyWrite"];
    for glob in [
        "**/.git/modules/**/config",
        "**/.git/modules/**/hooks",
        "**/.husky",
        "**/.githooks",
    ] {
        assert!(
            write
                .as_array()
                .expect("a list")
                .iter()
                .any(|it| it == glob),
            "{glob} in {write}"
        );
        assert!(settings.deny.contains(&format!("Edit({glob})")), "{glob}");
    }
}

#[test]
fn what_a_hooks_path_or_a_project_config_names_is_denied_to_every_harness_from_the_start() {
    // Ruling V73d: resolved when the chat starts, as path denials of the later-code class.
    let plane = plane_saying(ON);
    let clone = plane.path().join("workspaces/w/repo");
    std::fs::create_dir_all(clone.join(".git")).expect("a clone");
    std::fs::write(clone.join(".git/config"), "[core]\nhooksPath = hooks\n").expect("config");
    std::fs::create_dir_all(clone.join(".claude")).expect(".claude");
    std::fs::write(
        clone.join(".claude/settings.local.json"),
        r#"{"hooks": {"PreToolUse": [{"hooks": [{"command": "./guard.sh"}]}]}}"#,
    )
    .expect("settings");
    let denied = Denied::of(plane.path(), &machine(Os::MacOs));
    for want in [clone.join("hooks"), clone.join("guard.sh")] {
        assert!(
            denied.paths.iter().any(|it| it.path == want
                && it.class == Class::LaterCode
                && it.access == Access::Write),
            "{want:?} not denied: {:?}",
            denied.paths
        );
    }
}

#[test]
fn a_charter_toml_that_cannot_be_read_starts_no_chat_rather_than_one_unsandboxed() {
    // Absent `[sandbox]` is "not set", so a file charter cannot parse must not read as that.
    let plane = plane_saying("[sandbox\nmode = \"on\"\n");
    for harness in Harness::ALL {
        let refused =
            for_start(harness, plane.path(), &machine(Os::MacOs), &|_| true).expect_err("refused");
        assert_eq!(refused, NotStarted::PlaneUnreadable, "{harness:?}");
        assert_eq!(
            refused.to_string(),
            "charter.toml in this plane cannot be read as TOML, so charter cannot tell whether \
             it runs chats sandboxed, and nothing was started. Fix charter.toml and start the \
             chat again."
        );
    }
    // No file at all says nothing, as before.
    let none = tempfile::tempdir().expect("a directory");
    assert_eq!(
        for_start(Harness::Codex, none.path(), &machine(Os::MacOs), &|_| true),
        Ok(None)
    );
}

#[test]
fn the_directories_between_a_root_and_a_path_are_each_named_once() {
    let root = std::path::Path::new("/p");
    assert_eq!(
        ancestors_within(std::path::Path::new("/p/.charter/app/spool"), root),
        [
            std::path::PathBuf::from("/p/.charter"),
            std::path::PathBuf::from("/p/.charter/app")
        ]
    );
    assert!(ancestors_within(std::path::Path::new("/p/x"), root).is_empty());
    assert!(ancestors_within(std::path::Path::new("/q/x/y"), root).is_empty());
}

/// What `for_start` answers in a plane whose `charter.toml` is `make`'s.
#[cfg(unix)]
fn started_with(make: impl Fn(&std::path::Path)) -> Result<Option<Applied>, NotStarted> {
    let plane = tempfile::tempdir().expect("a plane");
    make(&plane.path().join("charter.toml"));
    for_start(Harness::Codex, plane.path(), &machine(Os::MacOs), &|_| true)
}

/// What puts something at a path, for a test of what is there.
#[cfg(unix)]
type Maker = dyn Fn(&std::path::Path) + Send;

#[cfg(unix)]
#[test]
fn a_charter_toml_that_is_not_a_regular_file_starts_no_chat() {
    let sandboxed = |dir: &std::path::Path| {
        let target = dir.join("real.toml");
        std::fs::write(&target, ON).expect("a real file");
        target
    };
    // A link to a file that turns the sandbox on, a dangling link, a directory, a FIFO, a socket.
    let cases: Vec<(&str, Box<Maker>)> = vec![
        (
            "a link",
            Box::new(move |at: &std::path::Path| {
                let target = sandboxed(at.parent().expect("a parent"));
                std::os::unix::fs::symlink(target, at).expect("a link");
            }),
        ),
        (
            "a dangling link",
            Box::new(|at: &std::path::Path| {
                std::os::unix::fs::symlink(at.with_extension("gone"), at).expect("a link");
            }),
        ),
        (
            "a directory",
            Box::new(|at: &std::path::Path| std::fs::create_dir(at).expect("a directory")),
        ),
        (
            "a FIFO",
            Box::new(|at: &std::path::Path| {
                let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(at))
                    .expect("mkfifo runs");
                assert!(made.success(), "a FIFO");
            }),
        ),
        (
            "a socket",
            Box::new(|at: &std::path::Path| {
                std::mem::forget(std::os::unix::net::UnixListener::bind(at).expect("a socket"));
            }),
        ),
    ];
    for (what, make) in cases {
        let (sender, answer) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(started_with(make).map(|it| it.is_some()));
        });
        // A FIFO with no writer must not hold the start up.
        let started = answer
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("{what}: the start blocked"));
        assert_eq!(started, Err(NotStarted::PlaneUnreadable), "{what}");
    }
}

#[cfg(unix)]
#[test]
fn a_device_is_not_read_as_a_plane_file() {
    // An endless device would otherwise be read until memory ran out.
    for device in ["/dev/zero", "/dev/null"] {
        assert_eq!(
            read_plane_file(std::path::Path::new(device)),
            Err(()),
            "{device}"
        );
    }
}

#[test]
fn a_charter_toml_over_the_cap_starts_no_chat() {
    let plane = tempfile::tempdir().expect("a plane");
    let mut text = String::from(ON);
    text.push_str(&"# padding\n".repeat(PLANE_FILE_MAX / 10 + 1));
    std::fs::write(plane.path().join("charter.toml"), &text).expect("charter.toml");
    assert_eq!(
        for_start(Harness::Codex, plane.path(), &machine(Os::MacOs), &|_| true),
        Err(NotStarted::PlaneUnreadable)
    );
    // Under the cap, the same words are read.
    std::fs::write(plane.path().join("charter.toml"), ON).expect("charter.toml");
    assert!(
        for_start(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true
        )
        .expect("starts")
        .is_some()
    );
}

#[cfg(unix)]
#[test]
fn no_sandboxed_chat_starts_in_a_folder_reached_through_a_link() {
    // Ruling of 2026-10-03, every harness: a plane-root chat could swap a workspace folder for
    // a link, and a chat started there would take that folder's rules to the link's target.
    let plane = plane_saying(ON);
    let workspaces = plane.path().join("workspaces");
    std::fs::create_dir_all(workspaces.join("real")).expect("a real workspace");
    let elsewhere = tempfile::tempdir().expect("somewhere outside");
    std::os::unix::fs::symlink(elsewhere.path(), workspaces.join("swapped")).expect("a link");
    std::fs::write(workspaces.join("a-file"), "").expect("a file");
    std::os::unix::fs::symlink(workspaces.join("real"), plane.path().join("ws-link"))
        .expect("a link to a real folder inside");
    let words = |program: &str| Words {
        program: program.to_owned(),
        command: Vec::new(),
        armed: Vec::new(),
        charters: Vec::new(),
    };
    for harness in Harness::ALL {
        let applied =
            compiled_anyway(harness, plane.path(), &machine(Os::MacOs)).expect("compiles");
        let confinement = applied.confine().expect("confined");
        let at = |cwd: std::path::PathBuf| (cwd, confinement.as_ref());
        for (cwd, want) in [
            (workspaces.join("swapped"), FOLDER_LINKED),
            (workspaces.join("swapped/below"), FOLDER_LINKED),
            (plane.path().join("ws-link"), FOLDER_LINKED),
            (workspaces.join("a-file"), FOLDER_LINKED),
            (workspaces.join("missing"), FOLDER_LINKED),
            (elsewhere.path().to_path_buf(), FOLDER_OUTSIDE),
        ] {
            let (cwd, confinement) = at(cwd);
            assert_eq!(
                applied.line(
                    words("harness"),
                    &At {
                        cwd: Some(&cwd),
                        confinement,
                        ..At::default()
                    }
                ),
                Err(want.to_owned()),
                "{harness:?} in {}",
                cwd.display()
            );
        }
        // A real folder inside the plane starts.
        assert!(
            applied
                .line(
                    words("harness"),
                    &At {
                        cwd: Some(&workspaces.join("real")),
                        confinement: confinement.as_ref(),
                        ..At::default()
                    }
                )
                .is_ok(),
            "{harness:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_charter_toml_above_a_folder_that_is_not_a_regular_file_is_seen() {
    // The plane's own walk takes only a regular file for a plane, so a start that asked it
    // alone would read "no plane" and start the chat unsandboxed.
    for kind in ["link", "dangling", "directory", "fifo"] {
        let plane = tempfile::tempdir().expect("a plane");
        let marker = plane.path().join("charter.toml");
        match kind {
            "link" => {
                std::fs::write(plane.path().join("real.toml"), ON).expect("a file");
                std::os::unix::fs::symlink(plane.path().join("real.toml"), &marker)
                    .expect("a link");
            }
            "dangling" => std::os::unix::fs::symlink("gone", &marker).expect("a link"),
            "directory" => std::fs::create_dir(&marker).expect("a directory"),
            _ => {
                let made =
                    crate::forklock::status(std::process::Command::new("mkfifo").arg(&marker))
                        .expect("mkfifo runs");
                assert!(made.success());
            }
        }
        let below = plane.path().join("workspaces/w");
        std::fs::create_dir_all(&below).expect("a workspace");
        assert!(marker_unreadable(&below), "{kind}");
    }
    let plain = plane_saying(ON);
    assert!(!marker_unreadable(plain.path()));
    let none = tempfile::tempdir().expect("no plane");
    assert!(!marker_unreadable(none.path()));
}

#[cfg(unix)]
#[test]
fn a_folder_is_found_inside_its_plane_whichever_side_names_it_through_a_link() {
    // The no-profile path takes the plane as the kernel names it and the folder as the chat
    // recorded it (`/tmp/…` against `/private/tmp/…` on macOS, or a linked parent anywhere).
    let base = tempfile::tempdir().expect("a base");
    let real_parent = base.path().join("real");
    std::fs::create_dir_all(real_parent.join("plane/workspaces/w")).expect("a plane");
    std::fs::write(real_parent.join("plane/charter.toml"), ON).expect("charter.toml");
    std::os::unix::fs::symlink(&real_parent, base.path().join("linked")).expect("a linked parent");
    let real = real_parent.join("plane").canonicalize().expect("real");
    let linked = base.path().join("linked/plane");
    for (root, cwd) in [
        (real.clone(), linked.join("workspaces/w")),
        (linked.clone(), real.join("workspaces/w")),
        (linked.clone(), linked.join("workspaces/w")),
        (real.clone(), real.join("workspaces/w")),
    ] {
        assert_eq!(
            folder_refusal(&root, &cwd),
            None,
            "{} in {}",
            cwd.display(),
            root.display()
        );
    }
    // A link inside the plane is still one, from either side.
    std::os::unix::fs::symlink(real.join("workspaces/w"), real.join("workspaces/again"))
        .expect("a link inside");
    for (root, cwd) in [
        (real.clone(), linked.join("workspaces/again")),
        (linked.clone(), real.join("workspaces/again")),
    ] {
        assert_eq!(
            folder_refusal(&root, &cwd),
            Some(FOLDER_LINKED),
            "{}",
            cwd.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn every_harness_is_compiled_against_the_plane_as_the_kernel_names_it() {
    // A plane reached through a link (`/tmp` on macOS, a linked parent anywhere): one spelling
    // for every harness, so no rule depends on how the caller wrote the root.
    let base = tempfile::tempdir().expect("a base");
    std::fs::create_dir_all(base.path().join("real/plane")).expect("a plane");
    std::fs::write(base.path().join("real/plane/charter.toml"), ON).expect("charter.toml");
    std::os::unix::fs::symlink(base.path().join("real"), base.path().join("tmp"))
        .expect("a linked parent");
    let linked = base.path().join("tmp/plane");
    let real = linked.canonicalize().expect("real");
    assert_ne!(linked, real);
    let applied = for_start(Harness::ClaudeCode, &linked, &machine(Os::MacOs), &|_| true)
        .expect("starts")
        .expect("sandboxed");
    assert_eq!(applied.root(), real.as_path());
    let Form::ClaudeCode(settings) = applied.form() else {
        panic!("compiled for Claude Code");
    };
    let write = settings.sandbox["filesystem"]["denyWrite"].to_string();
    assert!(
        write.contains(&real.join(".charter/app").display().to_string()),
        "{write}"
    );
    assert!(!write.contains(&linked.display().to_string()), "{write}");
}

#[test]
fn whether_a_harness_ever_starts_sandboxed_here_is_its_compiler_its_hold_and_its_wrap() {
    assert_eq!(never_on(Harness::ClaudeCode, Os::MacOs), None);
    assert_eq!(never_on(Harness::Opencode, Os::MacOs), None);
    assert_eq!(
        never_on(Harness::Opencode, Os::Linux).as_deref(),
        Some("charter can wrap it on macOS only, so far")
    );
    // #1123: charter wraps Codex as it wraps opencode.
    assert_eq!(never_on(Harness::Codex, Os::MacOs), None);
    assert_eq!(
        never_on(Harness::Codex, Os::Linux).as_deref(),
        Some("charter can wrap it on macOS only, so far")
    );
}

// -------------------------------------------------------------------------------------
// The per-chat opt-out (ADR 0067 §7) and Windows (ruling V21 3, V78 b)
// -------------------------------------------------------------------------------------

fn off(reason: Option<&str>) -> OptOut {
    OptOut {
        reason: reason.map(str::to_owned),
    }
}

#[test]
fn a_person_can_start_one_chat_without_the_sandbox_and_every_class_is_lifted_for_it() {
    let plane = plane_saying(ON);
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(Some("the build needs the network"))),
    );
    assert_eq!(
        decided,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: Some("the build needs the network".to_owned()),
        })))
    );
    assert_eq!(Lifted::CLASSES, Class::ALL);
}

#[test]
fn the_opt_out_is_what_lets_a_chat_start_where_the_sandbox_cannot_be_applied() {
    let plane = plane_saying(ON);
    let refused = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| false,
        None,
    );
    assert!(
        matches!(refused, Err(NotStarted::NoBackend(_))),
        "{refused:?}"
    );
    let started = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| false,
        Some(&off(None)),
    );
    assert_eq!(
        started,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

#[test]
fn a_project_that_has_not_turned_the_sandbox_on_has_nothing_to_opt_out_of() {
    let plane = plane_saying("schema = 1\n");
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(Some("why not"))),
    );
    assert_eq!(decided, Ok(None), "no lift, so nothing to audit");
}

#[test]
fn without_an_opt_out_a_sandboxed_project_still_sandboxes_or_refuses() {
    let plane = plane_saying(ON);
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        None,
    );
    assert!(
        matches!(decided, Ok(Some(Decided::Sandboxed(ref applied))) if applied.harness() == Harness::ClaudeCode),
        "{decided:?}"
    );
}

#[test]
fn on_windows_a_chat_in_a_sandboxed_project_starts_at_the_opt_out_and_charter_is_the_actor() {
    let plane = plane_saying(ON);
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Windows),
        &|_| true,
        None,
    );
    assert_eq!(
        decided,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        })))
    );
}

#[test]
fn a_system_with_no_backend_that_is_not_windows_still_fails_closed() {
    let plane = plane_saying(ON);
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Other),
        &|_| true,
        None,
    );
    assert_eq!(
        decided,
        Err(NotStarted::NoBackend(backend::Missing::NoBackend(
            Os::Other
        )))
    );
}

#[test]
fn a_typed_reason_is_kept_to_one_short_line() {
    let long = format!("first line\nsecond {}", "x".repeat(600));
    let plane = plane_saying(ON);
    let Ok(Some(Decided::Unsandboxed(lifted))) = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(Some(&long))),
    ) else {
        panic!("unsandboxed");
    };
    let reason = lifted.reason.expect("a reason");
    assert!(!reason.contains('\n'), "{reason}");
    assert!(
        reason.chars().count() <= OptOut::MOST_REASON_CHARS,
        "{reason}"
    );
    let blank = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(Some("   "))),
    );
    assert_eq!(
        blank,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

#[test]
fn an_unsandboxed_chat_says_so_on_its_tab_and_why() {
    assert_eq!(
        Lifted {
            by: By::Person,
            reason: None,
        }
        .notice(),
        "This chat runs without the sandbox: you turned it off for this chat only. A new or \
         resumed chat does not inherit it."
    );
    assert_eq!(
        Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        }
        .notice(),
        "This chat runs without the sandbox: charter has no sandbox backend on Windows yet, so \
         every chat here starts without it until one exists."
    );
}

// -------------------------------------------------------------------------------------
// SD-30's install action (ruling V78 c): the distribution's own command, typed and never run
// -------------------------------------------------------------------------------------

#[test]
fn the_install_action_is_the_distributions_own_package_manager_command() {
    let missing = backend::missing(Os::Linux, &|_| false).expect("missing");
    let cases = [
        (
            "ID=ubuntu\nID_LIKE=debian\n",
            "sudo apt install bubblewrap socat",
        ),
        ("ID=debian\n", "sudo apt install bubblewrap socat"),
        ("ID=\"fedora\"\n", "sudo dnf install bubblewrap socat"),
        (
            "ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n",
            "sudo dnf install bubblewrap socat",
        ),
        ("ID=arch\n", "sudo pacman -S bubblewrap socat"),
        (
            "ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n",
            "sudo zypper install bubblewrap socat",
        ),
        ("ID=alpine\n", "sudo apk add bubblewrap socat"),
    ];
    for (os_release, command) in cases {
        assert_eq!(
            backend::install_command(&missing, os_release).as_deref(),
            Some(command),
            "{os_release}"
        );
    }
}

#[test]
fn only_what_is_missing_is_installed() {
    let missing = backend::missing(Os::Linux, &|program| program == "bwrap").expect("missing");
    assert_eq!(
        backend::install_command(&missing, "ID=debian\n").as_deref(),
        Some("sudo apt install socat")
    );
}

#[test]
fn a_distribution_charter_does_not_know_gets_no_action_rather_than_a_guess() {
    let missing = backend::missing(Os::Linux, &|_| false).expect("missing");
    assert_eq!(backend::install_command(&missing, "ID=nixos\n"), None);
    assert_eq!(backend::install_command(&missing, ""), None);
}

#[test]
fn a_system_with_no_backend_has_nothing_to_install() {
    let missing = backend::missing(Os::Windows, &|_| false).expect("missing");
    assert_eq!(backend::install_command(&missing, "ID=debian\n"), None);
    let missing = backend::missing(Os::MacOs, &|_| false).expect("missing");
    assert_eq!(backend::install_command(&missing, "ID=debian\n"), None);
}

// -------------------------------------------------------------------------------------
// What the new-chat picker says before anything starts, and what each start records
// -------------------------------------------------------------------------------------

#[test]
fn the_picker_says_nothing_of_a_sandbox_a_project_has_not_turned_on() {
    let plane = plane_saying("schema = 1\n");
    assert_eq!(
        ahead(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::Linux),
            &|_| false,
            "",
            &|_| Ok(()),
        ),
        Ahead::Off
    );
}

#[test]
fn the_picker_says_a_chat_will_be_sandboxed_where_it_can_be() {
    let plane = plane_saying(ON);
    assert_eq!(
        ahead(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true,
            "",
            &|_| Ok(()),
        ),
        Ahead::Sandboxed
    );
}

#[test]
fn the_picker_shows_the_refusal_and_the_distributions_install_command_before_the_start() {
    let plane = plane_saying(ON);
    let Ahead::Refused { why, install } = ahead(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|program| program == "bwrap",
        "ID=debian\n",
        &|_| Ok(()),
    ) else {
        panic!("refused");
    };
    assert!(why.contains("socat is not installed"), "{why}");
    assert_eq!(install.as_deref(), Some("sudo apt install socat"));
}

#[test]
fn a_refusal_nothing_can_be_installed_for_offers_no_install() {
    let plane = plane_saying(ON);
    std::fs::write(
        plane.path().join("vaults.json"),
        r#"{"vaults": {"dev": {"provider": "keyring"}}}"#,
    )
    .expect("the registry");
    let Ahead::Refused { install, .. } = ahead(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        "ID=debian\n",
        &|_| Ok(()),
    ) else {
        panic!("refused");
    };
    assert_eq!(install, None);
}

#[test]
fn on_windows_the_picker_says_the_chat_starts_without_the_sandbox_and_why() {
    let plane = plane_saying(ON);
    assert_eq!(
        ahead(
            Harness::Codex,
            plane.path(),
            &machine(Os::Windows),
            &|_| true,
            "",
            &|_| Ok(()),
        ),
        Ahead::Unsandboxed(Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        })
    );
}

fn person() -> Lifted {
    Lifted {
        by: By::Person,
        reason: None,
    }
}

#[test]
fn a_new_chat_a_person_starts_unsandboxed_is_audited_off_and_counted_as_an_opt_out() {
    assert_eq!(
        at_start(Some(&person()), false, false, true),
        (Some(Change::Off(person())), Some(local::Started::OptedOut))
    );
}

#[test]
fn a_windows_start_is_audited_and_counted_apart_from_a_choice() {
    let windows = Lifted {
        by: By::NoBackend(Os::Windows),
        reason: None,
    };
    assert_eq!(
        at_start(Some(&windows), false, false, true),
        (
            Some(Change::Off(windows.clone())),
            Some(local::Started::NoBackend)
        )
    );
}

#[test]
fn a_chat_that_ran_unsandboxed_and_starts_sandboxed_again_is_audited_back_on_and_not_counted() {
    assert_eq!(at_start(None, true, true, false), (Some(Change::On), None));
}

#[test]
fn a_sandboxed_new_chat_is_counted_and_has_nothing_to_audit() {
    assert_eq!(
        at_start(None, true, false, true),
        (None, Some(local::Started::Sandboxed))
    );
}

#[test]
fn a_relaunch_is_audited_but_never_counted_again() {
    assert_eq!(
        at_start(Some(&person()), false, false, false),
        (Some(Change::Off(person())), None)
    );
}

#[test]
fn a_chat_in_a_project_without_the_sandbox_is_neither_audited_nor_counted() {
    assert_eq!(at_start(None, false, false, true), (None, None));
    assert_eq!(at_start(None, false, true, true), (None, None));
}

#[test]
fn an_unreadable_charter_toml_still_refuses_rather_than_reading_as_off() {
    let plane = plane_saying("[sandbox\nmode = \"on\"\n");
    assert_eq!(
        decide(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true,
            None,
        ),
        Err(NotStarted::PlaneUnreadable)
    );
    assert!(
        matches!(
            ahead(
                Harness::ClaudeCode,
                plane.path(),
                &machine(Os::MacOs),
                &|_| true,
                "",
                &|_| Ok(()),
            ),
            Ahead::Refused { .. }
        ),
        "the picker shows the refusal"
    );
    // The opt-out sits inside that refusal, and is audited as every other one is.
    assert_eq!(
        decide(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true,
            Some(&off(None)),
        ),
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

/// #1123 lifts ruling V87f's hold-back where charter wraps Codex. Where it cannot (Linux, #1040),
/// the picker shows that refusal before anything starts, and the opt-out inside it still starts
/// the chat, audited.
#[test]
fn the_picker_shows_codex_sandboxed_where_charter_wraps_it_and_the_opt_out_elsewhere() {
    let plane = plane_saying(ON);
    assert_eq!(
        ahead(
            Harness::Codex,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true,
            "",
            &|_| Ok(()),
        ),
        Ahead::Sandboxed
    );
    let Ahead::Refused { why, install } = ahead(
        Harness::Codex,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
        "ID=debian\n",
        &|_| Ok(()),
    ) else {
        panic!("refused on Linux");
    };
    assert!(why.contains("(#1040)"), "{why}");
    assert_eq!(install, None, "nothing to install fixes it");
    assert_eq!(
        decide(
            Harness::Codex,
            plane.path(),
            &machine(Os::Linux),
            &|_| true,
            Some(&off(None)),
        ),
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

/// Ruling V87g, before the start: whatever the start's program check refuses — a relative
/// program, one wherever the chat can write, one not answering as the harness — is shown as a
/// refusal with "Start without the sandbox", and asked only where the chat would be sandboxed.
#[test]
fn the_picker_shows_every_refusal_of_the_program_check() {
    let plane = plane_saying(ON);
    for refusal in [
        NotStarted::ProgramRelative,
        NotStarted::ProgramWritable(plane.path().join("bin/claude")),
        NotStarted::WordWritable(plane.path().join("bin/run.sh").display().to_string()),
        NotStarted::WordTooLong,
        NotStarted::NotTheHarness(Harness::ClaudeCode),
    ] {
        let said = refusal.to_string();
        let check = |_: &Applied| Err(refusal.clone());
        assert_eq!(
            ahead(
                Harness::ClaudeCode,
                plane.path(),
                &machine(Os::MacOs),
                &|_| true,
                "",
                &check,
            ),
            Ahead::Refused {
                why: said,
                install: None,
            }
        );
    }
    let asked = std::sync::atomic::AtomicBool::new(false);
    let check = |_: &Applied| {
        asked.store(true, std::sync::atomic::Ordering::SeqCst);
        Err(NotStarted::ProgramRelative)
    };
    assert!(matches!(
        ahead(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::Windows),
            &|_| true,
            "",
            &check
        ),
        Ahead::Unsandboxed(_)
    ));
    assert!(
        !asked.load(std::sync::atomic::Ordering::SeqCst),
        "not asked where the chat is not sandboxed"
    );
}
