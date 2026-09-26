use super::*;

#[test]
fn every_word_a_tool_is_routed_to_is_one_charter_answers_for_that_tool() {
    // One routing: `hooks/hooks.json` wires each word to Claude Code's tool names, and an
    // opencode tool reaches a word only under the name that word is wired to there.
    for tool in TOOLS {
        for word in [tool.pre, tool.post].into_iter().flatten() {
            let handler = crate::hookreg::find(word).unwrap_or_else(|| {
                panic!(
                    "{} routes to {word}, which charter does not answer",
                    tool.id
                )
            });
            let matcher = handler.matcher.expect("a tool hook has a matcher");
            assert!(
                matcher.split('|').any(|name| name == tool.name),
                "{} is {}, and {word} is wired to {matcher}",
                tool.id,
                tool.name
            );
        }
    }
    assert!(crate::hookreg::find(DEFAULT_PRE).is_some());
}

#[test]
fn the_bash_tool_goes_to_the_bash_guard_and_a_read_to_the_vault_read_guard() {
    let of = |id: &str| TOOLS.iter().find(|t| t.id == id).expect(id).pre;
    assert_eq!(of("bash"), Some("pretooluse"));
    assert_eq!(of("read"), Some("pretooluse-read"));
    assert_eq!(of("grep"), Some("pretooluse-read"));
    assert_eq!(of("write"), Some("pretooluse-edit"));
    assert_eq!(of("edit"), Some("pretooluse-edit"));
}

#[test]
fn the_session_shim_names_the_binary_by_the_variable_the_app_sets() {
    let text = shim(Arming::Session);
    assert!(text.starts_with(MARK), "{text}");
    assert!(text.contains("const BINARY = process.env.CHARTER_HOOK_BINARY || \"\"\n"));
    assert_eq!(
        binary_in(&text),
        None,
        "no path is written into the bundled file"
    );
    for hook in [
        "\"tool.execute.before\"",
        "\"tool.execute.after\"",
        "\"chat.message\"",
        "\"shell.env\"",
        "event:",
    ] {
        assert!(text.contains(hook), "{hook}");
    }
}

#[test]
fn the_installed_shim_carries_the_guard_alone_and_names_the_binary_by_its_path() {
    // ADR 0057's Codex reason: an app chat loads this beside the bundled shim, and a doubled
    // guard only refuses twice.
    let text = shim(Arming::GuardOnly(Path::new("/home/o'brien/\"charter\"")));
    assert!(text.starts_with(MARK));
    assert!(text.contains("\"tool.execute.before\": before"));
    for hook in [
        "\"tool.execute.after\"",
        "\"chat.message\"",
        "\"shell.env\"",
        "event:",
    ] {
        assert!(
            !text.contains(hook),
            "{hook} is armed by the installed guard"
        );
    }
    assert_eq!(
        binary_in(&text),
        Some(PathBuf::from("/home/o'brien/\"charter\""))
    );
}

#[test]
fn every_placeholder_is_filled() {
    for text in [
        shim(Arming::Session),
        shim(Arming::GuardOnly(Path::new("/bin/charter"))),
    ] {
        assert!(!text.contains("{{"), "{text}");
    }
}

#[test]
fn the_shim_routes_by_its_own_keys_and_refuses_when_the_guard_cannot_answer() {
    let text = shim(Arming::Session);
    assert!(text.contains("hasOwn(ROUTES, tool)"));
    assert!(text.contains("does not allow"));
    assert!(text.contains("throw new Error(why)"));
    // The routing table is the Rust one, as JSON.
    assert!(text.contains("\"pre\": \"pretooluse-read\""));
}

#[test]
fn a_session_config_names_the_shim_as_a_url_no_character_can_end() {
    let config = session_config(Path::new("/Apps/my charter#1/plugin/opencode/charter.ts"));
    let doc: serde_json::Value = serde_json::from_str(&config).expect("JSON");
    assert_eq!(
        doc,
        serde_json::json!({
            "plugin": ["file:///Apps/my%20charter%231/plugin/opencode/charter.ts"]
        })
    );
}

#[test]
fn a_profile_that_turns_plugins_off_is_named() {
    let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(
        disarmed_by(&words(&["opencode", "--pure"]), &[]).is_some_and(|why| why.contains("--pure"))
    );
    assert!(disarmed_by(&words(&["opencode"]), &[(PURE_ENV.into(), "1".into())]).is_some());
    assert!(disarmed_by(&words(&["opencode"]), &[(CONFIG_ENV.into(), "{}".into())]).is_some());
    assert_eq!(
        disarmed_by(
            &words(&["opencode", "--model", "x/y"]),
            &[("XDG_DATA_HOME".into(), "/d".into())]
        ),
        None
    );
}

#[test]
fn a_missing_charter_refuses_in_an_app_chat_and_allows_through_the_installed_copy() {
    // The app's chat runs the app's own binary, so its absence is a fault. The installed copy
    // names a path that can move, and refusing then would stop every opencode on the machine.
    assert!(shim(Arming::Session).contains("const MISSING = \"refuses\""));
    assert!(
        shim(Arming::GuardOnly(Path::new("/bin/charter"))).contains("const MISSING = \"allows\"")
    );
}

#[test]
fn the_deadline_races_the_answer_rather_than_waiting_on_the_kill() {
    let text = shim(Arming::Session);
    assert!(text.contains("Promise.race([answered, late])"), "{text}");
    assert!(
        text.contains(": 10\n"),
        "the default deadline is the Bash guard's"
    );
}

#[test]
fn the_pure_flag_is_seen_with_a_value_attached() {
    let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(disarmed_by(&words(&["opencode", "--pure=true"]), &[]).is_some());
    assert!(disarmed_by(&words(&["opencode", "--pure=1"]), &[]).is_some());
    assert_eq!(disarmed_by(&words(&["opencode", "--purely"]), &[]), None);
}

#[test]
fn the_shim_file_is_named_the_same_in_the_bundle_and_in_opencodes_directory() {
    assert!(SHIM_IN_BUNDLE.ends_with(&format!("/{FILE_NAME}")));
}
