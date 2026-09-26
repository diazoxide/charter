use super::*;

/// A machine inside a temporary directory: Claude Code's, Codex's and opencode's folders exist, and the
/// bundle is the repository's own plugin.
fn machine() -> (tempfile::TempDir, Machine) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("claude")).unwrap();
    std::fs::create_dir_all(root.join("codex")).unwrap();
    std::fs::create_dir_all(root.join("opencode")).unwrap();
    let bundle = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../app/src-tauri/plugin")
        .canonicalize()
        .unwrap();
    let m = Machine {
        claude_config: root.join("claude"),
        codex_home: root.join("codex"),
        opencode_config: root.join("opencode"),
        charter_dir: root.join("config/charter"),
        binary: PathBuf::from("/opt/charter's app/charter"),
        bundle: Some(bundle),
    };
    (dir, m)
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn needed(outcomes: &[Outcome]) -> usize {
    outcomes
        .iter()
        .filter_map(|o| o.plan.as_ref().ok())
        .flat_map(|p| p.steps.iter())
        .filter(|s| s.needed)
        .count()
}

#[test]
fn claude_code_gets_a_copy_whose_hooks_run_this_charter_enabled_in_the_user_settings() {
    let (_d, m) = machine();
    let out = run(&m, Verb::Install, &["claude".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));

    let settings = json(&m.claude_config.join("settings.json"));
    assert_eq!(settings["enabledPlugins"][INSTALLED_AS], true);
    assert_eq!(
        settings["extraKnownMarketplaces"][MARKETPLACE]["source"],
        json!({"source": "directory", "path": plugin_dir(&m).display().to_string()})
    );

    let dir = plugin_dir(&m);
    let market = json(&dir.join(".claude-plugin/marketplace.json"));
    assert_eq!(market["name"], MARKETPLACE);
    assert_eq!(market["plugins"][0]["name"], "charter");
    assert_eq!(market["plugins"][0]["source"], "./");
    let hooks = std::fs::read_to_string(dir.join("hooks/hooks.json")).unwrap();
    // The app's variable is unset in a terminal, and a hook that cannot start lets the tool
    // run: the copy names the binary itself.
    assert!(!hooks.contains("CHARTER_HOOK_BINARY"), "{hooks}");
    let guard = &serde_json::from_str::<Value>(&hooks).unwrap()["hooks"]["PreToolUse"][0];
    assert_eq!(guard["matcher"], "Bash");
    assert_eq!(
        guard["hooks"][0]["command"],
        r"'/opt/charter'\''s app/charter' hook pretooluse"
    );
    assert!(dir.join("skills/handoff/SKILL.md").is_file());
}

#[test]
fn a_second_install_finds_everything_done_and_writes_nothing() {
    let (_d, m) = machine();
    let first = run(&m, Verb::Install, &[], false);
    assert!(needed(&first) > 0);
    let settings = m.claude_config.join("settings.json");
    let config = m.codex_home.join("config.toml");
    let before = (
        std::fs::read(&settings).unwrap(),
        std::fs::read(&config).unwrap(),
        std::fs::metadata(&settings).unwrap().modified().unwrap(),
    );
    let second = run(&m, Verb::Install, &[], false);
    assert_eq!(needed(&second), 0, "{}", render(&second, false));
    assert!(render(&second, false).contains("already"));
    let said = render(&first, false);
    assert!(!said.contains("not done"), "every write was made: {said}");
    assert_eq!(said.matches("\n  done ").count(), needed(&first), "{said}");
    assert_eq!(
        before,
        (
            std::fs::read(&settings).unwrap(),
            std::fs::read(&config).unwrap(),
            std::fs::metadata(&settings).unwrap().modified().unwrap(),
        )
    );
}

#[test]
fn a_dry_run_says_what_it_would_change_and_writes_nothing() {
    let (_d, m) = machine();
    let out = run(&m, Verb::Install, &[], true);
    let said = render(&out, true);
    assert!(
        said.contains(&format!("would    enable {INSTALLED_AS}")),
        "{said}"
    );
    assert!(
        said.ends_with("Nothing was written (--dry-run).\n"),
        "{said}"
    );
    assert!(!m.claude_config.join("settings.json").exists());
    assert!(!m.codex_home.join("config.toml").exists());
    assert!(!plugin_dir(&m).exists());
}

#[test]
fn the_retired_plugin_is_never_enabled_and_is_turned_off_where_the_install_writes() {
    let (_d, m) = machine();
    std::fs::write(
        m.claude_config.join("settings.json"),
        r#"{"theme": "dark", "enabledPlugins": {"charter@charter": true, "other@x": true}}"#,
    )
    .unwrap();
    std::fs::write(
        m.codex_home.join("config.toml"),
        "model = \"o3\"\n\n[plugins.\"charter@charter\"]\nenabled = true\n",
    )
    .unwrap();
    let out = run(&m, Verb::Install, &[], false);
    assert!(!failed(&out), "{}", render(&out, false));
    let settings = json(&m.claude_config.join("settings.json"));
    assert_eq!(settings["enabledPlugins"]["charter@charter"], false);
    assert_eq!(settings["enabledPlugins"]["other@x"], true);
    assert_eq!(settings["theme"], "dark");
    let config = std::fs::read_to_string(m.codex_home.join("config.toml")).unwrap();
    assert!(
        config.contains("[plugins.\"charter@charter\"]\nenabled = false"),
        "{config}"
    );
    assert!(config.starts_with("model = \"o3\"\n"), "{config}");
    assert!(render(&out, false).contains("turn off charter@charter"));

    // And nothing it writes ever names it as on.
    let (_d, m) = machine();
    run(&m, Verb::Install, &[], false);
    let text = std::fs::read_to_string(m.claude_config.join("settings.json")).unwrap();
    assert!(!text.contains("charter@charter\""), "{text}");
}

#[test]
fn codex_gets_the_guard_in_its_user_config_once_and_nothing_else() {
    let (_d, m) = machine();
    std::fs::write(
        m.codex_home.join("config.toml"),
        "# mine\n[[hooks.PreToolUse]]\nmatcher = \"Bash\"\n[[hooks.PreToolUse.hooks]]\ntype = \"command\"\ncommand = \"my-own-check\"\n",
    )
    .unwrap();
    run(&m, Verb::Install, &["codex".to_owned()], false);
    run(&m, Verb::Install, &["codex".to_owned()], false);
    let config = std::fs::read_to_string(m.codex_home.join("config.toml")).unwrap();
    let doc: toml::Table = toml::from_str(&config).unwrap();
    let groups = doc["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(groups.len(), 2, "{config}");
    assert_eq!(
        groups[0]["hooks"][0]["command"].as_str(),
        Some("my-own-check")
    );
    assert_eq!(groups[1]["matcher"].as_str(), Some("Bash"));
    assert_eq!(
        groups[1]["hooks"][0]["command"].as_str(),
        Some(r"'/opt/charter'\''s app/charter' hook pretooluse")
    );
    assert_eq!(groups[1]["hooks"][0]["timeout"].as_integer(), Some(10));
    assert!(config.starts_with("# mine\n"), "{config}");
    // Only the guard: the app's own flags carry the state hooks into an app chat, and Codex
    // would run both.
    assert!(!config.contains("hook sessionstart"), "{config}");
    assert!(!m.claude_config.join("settings.json").exists());
}

#[test]
fn a_moved_binary_replaces_the_guard_rather_than_adding_a_second() {
    let (_d, mut m) = machine();
    run(&m, Verb::Install, &["codex".to_owned()], false);
    m.binary = PathBuf::from("/elsewhere/charter");
    let out = run(&m, Verb::Install, &["codex".to_owned()], false);
    assert_eq!(needed(&out), 1);
    let config = std::fs::read_to_string(m.codex_home.join("config.toml")).unwrap();
    assert_eq!(config.matches("hook pretooluse").count(), 1, "{config}");
    assert!(
        config.contains("'/elsewhere/charter' hook pretooluse"),
        "{config}"
    );
}

#[test]
fn uninstall_takes_back_exactly_what_install_wrote() {
    let (_d, m) = machine();
    std::fs::write(
        m.claude_config.join("settings.json"),
        "{\n    \"theme\": \"dark\"\n}\n",
    )
    .unwrap();
    std::fs::write(m.codex_home.join("config.toml"), "model = \"o3\"\n").unwrap();
    std::fs::create_dir_all(m.claude_config.join("plugins")).unwrap();
    std::fs::write(
        m.claude_config.join("plugins/known_marketplaces.json"),
        r#"{"charter-app": {"source": {}}, "theirs": {"source": {}}}"#,
    )
    .unwrap();
    run(&m, Verb::Install, &[], false);
    let out = run(&m, Verb::Uninstall, &[], false);
    assert!(!failed(&out), "{}", render(&out, false));

    let settings = json(&m.claude_config.join("settings.json"));
    assert_eq!(settings["theme"], "dark");
    assert_eq!(settings["enabledPlugins"], json!({}));
    assert_eq!(settings["extraKnownMarketplaces"], json!({}));
    assert!(
        std::fs::read_to_string(m.claude_config.join("settings.json"))
            .unwrap()
            .starts_with("{\n    \"theme\""),
        "the layout the operator wrote is kept"
    );
    assert_eq!(
        json(&m.claude_config.join("plugins/known_marketplaces.json")),
        json!({"theirs": {"source": {}}})
    );
    assert!(!plugin_dir(&m).exists());
    assert_eq!(
        std::fs::read_to_string(m.codex_home.join("config.toml")).unwrap(),
        "model = \"o3\"\n"
    );
    // And a second uninstall has nothing left to do.
    assert_eq!(needed(&run(&m, Verb::Uninstall, &[], false)), 0);
}

#[test]
fn a_harness_never_set_up_here_is_skipped_unless_named() {
    let (_d, m) = machine();
    std::fs::remove_dir(&m.codex_home).unwrap();
    let out = run(&m, Verb::Install, &[], false);
    let said = render(&out, false);
    assert!(said.contains("codex:\n  skipped: "), "{said}");
    assert!(!m.codex_home.exists());
    run(&m, Verb::Install, &["codex".to_owned()], false);
    assert!(m.codex_home.join("config.toml").is_file());
}

#[test]
fn a_file_charter_cannot_read_is_left_alone_and_the_run_fails() {
    let (_d, m) = machine();
    std::fs::write(m.claude_config.join("settings.json"), "{not json").unwrap();
    std::fs::write(m.codex_home.join("config.toml"), "[[[").unwrap();
    let out = run(&m, Verb::Install, &[], false);
    assert!(failed(&out));
    assert_eq!(
        std::fs::read_to_string(m.claude_config.join("settings.json")).unwrap(),
        "{not json"
    );
    assert_eq!(
        std::fs::read_to_string(m.codex_home.join("config.toml")).unwrap(),
        "[[["
    );
    assert!(!plugin_dir(&m).exists(), "nothing is half-installed");
}

#[test]
fn a_bundle_that_is_not_charters_plugin_is_refused() {
    let (d, mut m) = machine();
    let other = d.path().join("other");
    std::fs::create_dir_all(other.join(".claude-plugin")).unwrap();
    std::fs::write(
        other.join(".claude-plugin/plugin.json"),
        r#"{"name": "evil"}"#,
    )
    .unwrap();
    m.bundle = Some(other);
    let out = run(&m, Verb::Install, &["claude".to_owned()], false);
    assert!(failed(&out));
    assert!(!m.claude_config.join("settings.json").exists());
}

#[test]
fn codex_hooks_written_inline_are_read_and_kept() {
    let (_d, m) = machine();
    let config = m.codex_home.join("config.toml");
    std::fs::write(
        &config,
        "hooks = { PreToolUse = [{ matcher = \"Bash\", hooks = [{ type = \"command\", command = \"mine\" }] }] }\n",
    )
    .unwrap();
    let out = run(&m, Verb::Install, &["codex".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    let doc: toml::Table = toml::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    let groups = doc["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["hooks"][0]["command"].as_str(), Some("mine"));
    let out = run(&m, Verb::Uninstall, &["codex".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    let doc: toml::Table = toml::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(doc["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);

    std::fs::write(&config, "hooks = {}\n").unwrap();
    let out = run(&m, Verb::Install, &["codex".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .contains("hook pretooluse")
    );
}

#[test]
fn a_settings_file_that_is_a_link_stays_a_link_and_keeps_its_non_ascii() {
    let (d, m) = machine();
    let real = d.path().join("dotfiles-claude.json");
    std::fs::write(
        &real,
        "{\n  \"statusLine\": {\"command\": \"echo \u{2192}\"}\n}\n",
    )
    .unwrap();
    let link = m.claude_config.join("settings.json");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let out = run(&m, Verb::Install, &["claude".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let text = std::fs::read_to_string(&real).unwrap();
    assert!(text.contains("echo \u{2192}"), "{text}");
    assert!(text.contains(INSTALLED_AS), "{text}");
}

#[test]
fn a_write_that_fails_is_reported_as_not_done() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, m) = machine();
    std::fs::set_permissions(&m.claude_config, std::fs::Permissions::from_mode(0o500)).unwrap();
    if std::fs::write(m.claude_config.join("probe"), "").is_ok() {
        // Root writes through a mode of 500, and the question does not arise.
        return;
    }
    let out = run(&m, Verb::Install, &["claude".to_owned()], false);
    std::fs::set_permissions(&m.claude_config, std::fs::Permissions::from_mode(0o755)).unwrap();
    let said = render(&out, false);
    assert!(failed(&out));
    assert!(
        said.contains("done     a copy of charter's plugin"),
        "{said}"
    );
    assert!(
        said.contains(&format!("not done enable {INSTALLED_AS}")),
        "{said}"
    );
    assert!(said.contains("FAILED: "), "{said}");
}

#[test]
fn what_is_installed_is_read_back_by_harness() {
    let (_d, m) = machine();
    for a in adapters() {
        assert_eq!(a.installed(&m), Ok(false), "{}", a.harness());
        assert_eq!(a.runs(&m), None, "{}", a.harness());
    }
    run(&m, Verb::Install, &[], false);
    for a in adapters() {
        assert_eq!(a.installed(&m), Ok(true), "{}", a.harness());
        assert_eq!(a.runs(&m), Some(m.binary.clone()), "{}", a.harness());
    }
}

#[test]
fn the_retired_plugin_is_found_in_each_file_that_enables_it() {
    let (d, m) = machine();
    std::fs::write(
        m.claude_config.join("settings.json"),
        r#"{"enabledPlugins": {"charter@charter": true}}"#,
    )
    .unwrap();
    std::fs::write(
        m.codex_home.join("config.toml"),
        "[plugins.\"charter@charter\"]\nenabled = true\n",
    )
    .unwrap();
    let plane = d.path().join("plane");
    std::fs::create_dir_all(plane.join(".claude")).unwrap();
    std::fs::write(
        plane.join(".claude/settings.local.json"),
        r#"{"enabledPlugins": {"charter@charter": true}}"#,
    )
    .unwrap();
    std::fs::write(
        plane.join(".claude/settings.json"),
        r#"{"enabledPlugins": {"charter@charter": false}}"#,
    )
    .unwrap();
    assert_eq!(
        ClaudeCode.superseded(&m),
        vec![m.claude_config.join("settings.json")]
    );
    assert_eq!(Codex.superseded(&m), vec![m.codex_home.join("config.toml")]);
    assert_eq!(
        superseded_in_plane(&plane),
        vec![plane.join(".claude/settings.local.json")]
    );
}

// ---- opencode (#371) --------------------------------------------------------------------

fn opencode_shim(m: &Machine) -> PathBuf {
    m.opencode_config.join("plugin/charter.ts")
}

#[test]
fn opencode_gets_the_guard_as_a_plugin_that_runs_this_charter() {
    let (_d, m) = machine();
    let out = run(&m, Verb::Install, &["opencode".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));

    let text = std::fs::read_to_string(opencode_shim(&m)).unwrap();
    assert!(text.starts_with(crate::opencode::MARK), "{text}");
    assert_eq!(crate::opencode::binary_in(&text), Some(m.binary.clone()));
    assert!(!text.contains("\"chat.message\""), "only the guard: {text}");
    assert!(Opencode.installed(&m).unwrap());
    assert_eq!(Opencode.runs(&m), Some(m.binary.clone()));

    let again = run(&m, Verb::Install, &["opencode".to_owned()], false);
    assert_eq!(needed(&again), 0, "{}", render(&again, false));
}

#[test]
fn opencode_is_left_alone_when_its_config_folder_does_not_exist_and_it_was_not_named() {
    let (_d, m) = machine();
    std::fs::remove_dir(&m.opencode_config).unwrap();
    let out = run(&m, Verb::Install, &[], false);
    let opencode = out
        .iter()
        .find(|o| o.harness == "opencode")
        .expect("opencode");
    assert!(opencode.skipped.is_some());
    assert!(!m.opencode_config.exists());
}

#[test]
fn the_python_charters_opencode_shim_is_replaced_and_named_until_it_is() {
    let (_d, m) = machine();
    std::fs::create_dir_all(opencode_shim(&m).parent().unwrap()).unwrap();
    std::fs::write(
        opencode_shim(&m),
        "// charter-version: 0.62.1\n// Generated by `charter init`.\n",
    )
    .unwrap();
    assert_eq!(Opencode.superseded(&m), vec![opencode_shim(&m)]);
    assert!(!Opencode.installed(&m).unwrap());

    let out = run(&m, Verb::Install, &["opencode".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    assert!(render(&out, false).contains("replace the retired Python charter's opencode shim"));
    assert!(Opencode.installed(&m).unwrap());
    assert!(Opencode.superseded(&m).is_empty());
}

#[test]
fn a_plugin_charter_did_not_write_is_never_replaced_or_removed() {
    let (_d, m) = machine();
    std::fs::create_dir_all(opencode_shim(&m).parent().unwrap()).unwrap();
    std::fs::write(opencode_shim(&m), "export const Mine = async () => ({})\n").unwrap();

    let out = run(&m, Verb::Install, &["opencode".to_owned()], false);
    assert!(failed(&out));
    assert!(render(&out, false).contains("charter did not write it"));
    let gone = run(&m, Verb::Uninstall, &["opencode".to_owned()], false);
    assert!(!failed(&gone));
    assert_eq!(
        std::fs::read_to_string(opencode_shim(&m)).unwrap(),
        "export const Mine = async () => ({})\n"
    );
}

#[test]
fn uninstall_takes_the_opencode_guard_back() {
    let (_d, m) = machine();
    run(&m, Verb::Install, &["opencode".to_owned()], false);
    let dry = run(&m, Verb::Uninstall, &["opencode".to_owned()], true);
    assert!(opencode_shim(&m).is_file(), "{}", render(&dry, true));
    let out = run(&m, Verb::Uninstall, &["opencode".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    assert!(!opencode_shim(&m).exists());
    assert!(
        m.opencode_config.join("plugin").is_dir(),
        "only the file goes"
    );
}

#[test]
fn a_stale_opencode_guard_is_rewritten_for_this_charter() {
    let (_d, m) = machine();
    let mut older = m.clone();
    older.binary = PathBuf::from("/old/charter");
    run(&older, Verb::Install, &["opencode".to_owned()], false);
    assert_eq!(Opencode.runs(&m), Some(PathBuf::from("/old/charter")));

    let out = run(&m, Verb::Install, &["opencode".to_owned()], false);
    assert_eq!(needed(&out), 1);
    assert_eq!(Opencode.runs(&m), Some(m.binary.clone()));
}

/// Codex's trust record for the hook in `config` at `group`, as Codex 0.147.0 writes it.
fn trust(config: &Path, group: usize, hash: &str) -> String {
    format!(
        "\n[hooks.state.\"{}:pre_tool_use:{group}:0\"]\ntrusted_hash = \"sha256:{hash}\"\n",
        config.display()
    )
}

#[test]
fn uninstall_takes_codexs_trust_record_for_the_guard_and_renumbers_the_ones_after_it() {
    let (_d, m) = machine();
    let config = m.codex_home.join("config.toml");
    run(&m, Verb::Install, &["codex".to_owned()], false);
    // The operator adds a hook of their own after the guard, and Codex records trusting both,
    // keyed by position: the guard is group 0, theirs group 1.
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(
        "\n[[hooks.PreToolUse]]\nmatcher = \"Bash\"\n[[hooks.PreToolUse.hooks]]\ntype = \
         \"command\"\ncommand = \"my-own-check\"\n",
    );
    text.push_str(&trust(&config, 0, "ours"));
    text.push_str(&trust(&config, 1, "mine"));
    text.push_str(&format!(
        "\n[hooks.state.\"{}:session_start:0:0\"]\ntrusted_hash = \"sha256:start\"\n\
         \n[hooks.state.\"other@x:hooks/hooks.json:pre_tool_use:0:0\"]\ntrusted_hash = \
         \"sha256:plugin\"\n",
        config.display()
    ));
    std::fs::write(&config, text).unwrap();

    let out = run(&m, Verb::Uninstall, &["codex".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    let after = std::fs::read_to_string(&config).unwrap();
    let doc: toml::Table = toml::from_str(&after).unwrap();
    let groups = doc["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "{after}");
    assert_eq!(
        groups[0]["hooks"][0]["command"].as_str(),
        Some("my-own-check")
    );
    let state = doc["hooks"]["state"].as_table().unwrap();
    let key = |event: &str, group: usize| format!("{}:{event}:{group}:0", config.display());
    // Their hook is group 0 now, and keeps the trust Codex gave it.
    assert_eq!(
        state[&key("pre_tool_use", 0)]["trusted_hash"].as_str(),
        Some("sha256:mine"),
        "{after}"
    );
    assert!(!state.contains_key(&key("pre_tool_use", 1)), "{after}");
    assert!(!after.contains("sha256:ours"), "{after}");
    // Nothing else Codex trusts is touched.
    assert_eq!(
        state[&key("session_start", 0)]["trusted_hash"].as_str(),
        Some("sha256:start")
    );
    assert_eq!(
        state["other@x:hooks/hooks.json:pre_tool_use:0:0"]["trusted_hash"].as_str(),
        Some("sha256:plugin")
    );
}

#[test]
fn uninstall_leaves_no_empty_trust_table_behind() {
    let (_d, m) = machine();
    let config = m.codex_home.join("config.toml");
    std::fs::write(&config, "model = \"o3\"\n").unwrap();
    run(&m, Verb::Install, &["codex".to_owned()], false);
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&trust(&config, 0, "ours"));
    std::fs::write(&config, text).unwrap();
    let out = run(&m, Verb::Uninstall, &["codex".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "model = \"o3\"\n"
    );
}

#[test]
fn a_replaced_guard_takes_its_trust_record_with_it() {
    let (_d, mut m) = machine();
    let config = m.codex_home.join("config.toml");
    run(&m, Verb::Install, &["codex".to_owned()], false);
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&trust(&config, 0, "ours"));
    std::fs::write(&config, text).unwrap();
    m.binary = PathBuf::from("/elsewhere/charter");
    let out = run(&m, Verb::Install, &["codex".to_owned()], false);
    assert!(!failed(&out), "{}", render(&out, false));
    let after = std::fs::read_to_string(&config).unwrap();
    assert!(!after.contains("sha256:ours"), "{after}");
    assert!(
        after.contains("'/elsewhere/charter' hook pretooluse"),
        "{after}"
    );
}

// ---- #449: the app brings an installed copy up to date at launch -----------------------------

/// Make every harness's installed copy stale: an older copy lacks a skill, an older Codex guard
/// a timeout, an older opencode shim a line.
fn age(m: &Machine) {
    std::fs::remove_dir_all(plugin_dir(m).join("skills/handoff")).unwrap();
    let config = m.codex_home.join("config.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, text.replace("timeout = 10", "timeout = 3")).unwrap();
    let shim = opencode_shim(m);
    let mut text = std::fs::read_to_string(&shim).unwrap();
    text.push_str("// an older charter's line\n");
    std::fs::write(&shim, text).unwrap();
}

#[test]
fn a_stale_copy_that_runs_this_charter_is_brought_up_to_date() {
    let (_d, m) = machine();
    assert!(!failed(&run(&m, Verb::Install, &[], false)));
    age(&m);
    let out = refresh(&m);
    let harnesses: Vec<&str> = out.iter().map(|o| o.harness).collect();
    assert_eq!(harnesses, ["claude", "codex", "opencode"]);
    assert!(!failed(&out), "{}", render(&out, false));
    assert!(plugin_dir(&m).join("skills/handoff/SKILL.md").is_file());
    assert_eq!(needed(&run(&m, Verb::Install, &[], true)), 0);
    // And a copy already current is left alone, with nothing to say.
    assert!(refresh(&m).is_empty());
}

#[test]
fn nothing_is_installed_by_a_refresh_where_nothing_was() {
    let (_d, m) = machine();
    assert!(refresh(&m).is_empty());
    assert!(!m.claude_config.join("settings.json").exists());
    assert!(!m.codex_home.join("config.toml").exists());
    assert!(!opencode_shim(&m).exists());
    assert!(!plugin_dir(&m).exists());
}

#[test]
fn a_copy_that_runs_another_charter_still_there_is_that_charters_to_refresh() {
    let (d, mut m) = machine();
    let other = d.path().canonicalize().unwrap().join("other-charter");
    std::fs::write(&other, "").unwrap();
    let app = m.binary.clone();
    m.binary = other.clone();
    assert!(!failed(&run(&m, Verb::Install, &[], false)));
    age(&m);
    m.binary = app;
    assert!(refresh(&m).is_empty());
    assert!(!plugin_dir(&m).join("skills/handoff").exists());

    // Once that charter is gone, its hooks cannot start, and the app's is the one to run.
    std::fs::remove_file(&other).unwrap();
    let out = refresh(&m);
    assert_eq!(out.len(), 3, "{}", render(&out, false));
    assert_eq!(ClaudeCode.runs(&m), Some(m.binary.clone()));
    assert_eq!(Codex.runs(&m), Some(m.binary.clone()));
    assert_eq!(Opencode.runs(&m), Some(m.binary.clone()));
}

#[test]
fn only_a_release_build_refreshes_on_its_own() {
    assert!(refreshes_on_its_own(false, false));
    assert!(!refreshes_on_its_own(true, false));
    assert!(!refreshes_on_its_own(false, true));
    assert!(!refreshes_on_its_own(true, true));
}
