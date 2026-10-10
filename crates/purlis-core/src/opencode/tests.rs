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
    let config = session_config(
        Path::new("/Apps/my charter#1/plugin/opencode/charter.ts"),
        None,
        None,
        false,
    );
    let doc: serde_json::Value = serde_json::from_str(&config).expect("JSON");
    assert_eq!(
        doc,
        serde_json::json!({
            "plugin": ["file:///Apps/my%20charter%231/plugin/opencode/charter.ts"]
        })
    );
}

#[test]
fn a_session_config_hands_the_shim_the_skills_directory_as_its_option() {
    // opencode's `[spec, options]` form: the options are the plugin factory's second argument
    // (measured on 1.18.32). Not a top-level `skills` key: opencode merges configs with arrays
    // replaced, so `skills.paths` here would drop every path the operator's own config names.
    let config = session_config(
        Path::new("/Apps/plugin/opencode/charter.ts"),
        Some(Path::new("/Apps/my charter/plugin/skills")),
        None,
        false,
    );
    let doc: serde_json::Value = serde_json::from_str(&config).expect("JSON");
    assert_eq!(
        doc,
        serde_json::json!({
            "plugin": [[
                "file:///Apps/plugin/opencode/charter.ts",
                { "skills": "/Apps/my charter/plugin/skills" }
            ]]
        })
    );
}

#[test]
fn the_app_chats_shim_adds_charters_skills_beside_the_operators_own() {
    let text = shim(Arming::Session);
    assert!(text.contains("config: async (cfg) =>"), "{text}");
    // Appended to what is there, never in its place, and never twice.
    assert!(text.contains("if (!paths.includes(skills)) cfg.skills.paths = [...paths, skills]"));
    assert!(text.contains("async (plugin, options) =>"), "{text}");
    // The installed guard is for opencode outside the app: it hands no skills.
    let guard = shim(Arming::GuardOnly(Path::new("/bin/charter")));
    assert!(!guard.contains("cfg.skills"), "{guard}");
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
    let bundle = Path::new("/app/plugin");
    assert_eq!(
        shim_in(bundle).file_name(),
        Some(std::ffi::OsStr::new(FILE_NAME))
    );
    // #1266: the shim is `purlis.ts` now, in the bundle and in opencode's folder alike.
    assert_eq!(FILE_NAME, "purlis.ts");
}

#[test]
fn a_line_the_app_did_not_take_is_said_in_the_pane_and_never_dropped_in_silence() {
    // Ruling V73c: a wrapped opencode chat's hook cannot spool, so a line the app did not take
    // is lost; the shim shows the hook's own sentence in opencode's window instead.
    let text = shim(Arming::Session);
    assert!(
        text.contains(&format!(
            "const NOT_TAKEN = {}",
            serde_json::Value::from(crate::hookwire::NOT_TAKEN)
        )),
        "{text}"
    );
    assert!(text.contains("showToast"), "{text}");
    assert!(
        text.contains("said = await Promise.race([answered, late])"),
        "{text}"
    );
    assert!(text.contains("notTaken(said)"), "{text}");
}

#[test]
fn a_sandboxed_chat_makes_no_snapshot() {
    // Ruling V73a: opencode's snapshot repositories are directories git is later run in, so a
    // wrapped chat writes none. Measured on 1.18.33: this wins over a global and a project
    // config that turn snapshots on, and a turn runs without them.
    let config = session_config(
        Path::new("/Apps/plugin/opencode/charter.ts"),
        None,
        None,
        true,
    );
    let doc: serde_json::Value = serde_json::from_str(&config).expect("JSON");
    assert_eq!(doc["snapshot"], serde_json::Value::Bool(false));
    let unsandboxed = session_config(
        Path::new("/Apps/plugin/opencode/charter.ts"),
        None,
        None,
        false,
    );
    let doc: serde_json::Value = serde_json::from_str(&unsandboxed).expect("JSON");
    assert_eq!(doc.get("snapshot"), None);
}

#[test]
fn a_shell_commands_result_goes_to_the_block_hook_as_claude_codes_does() {
    // #1353: a sandbox block in an opencode chat reaches the same Notice as Claude Code's. The
    // block detector is harness-neutral; the shim only routes `bash` to the block word.
    let bash = TOOLS.iter().find(|t| t.id == "bash").expect("bash");
    assert_eq!(bash.post, Some("posttooluse-blocked"));
    let text = shim(Arming::Session);
    assert!(text.contains(r#""post": "posttooluse-blocked""#), "{text}");
    // Only the session shim reports results: the installed copy is the guards alone.
    let installed = shim(Arming::GuardOnly(Path::new("/bin/purlis")));
    assert!(!installed.contains("tool.execute.after"), "{installed}");
}

#[test]
fn a_failed_shell_commands_one_stream_is_read_as_its_standard_error() {
    // opencode hands back a shell command's standard output and error as one text, with its
    // exit status in `metadata.exit` (read off opencode 1.18.33's bash tool). A command that
    // failed is read as standard error is, marked mixed so no host or path it printed is named
    // for a grant (`sandboxblock`); one that did not is one mixed text, of which the detector
    // reads only a sandbox's own violation lines, so a `cat` that prints a refusal's words
    // raises nothing.
    let text = shim(Arming::Session);
    assert!(
        text.contains(
            r#"typeof exit === "number" && exit !== 0 ? { stderr: said, mixed: true } : said"#
        ),
        "{text}"
    );
    assert!(
        text.contains("tool_response: response(tool, output)"),
        "{text}"
    );
    // A relative path in a block is read against the folder the command ran in, as before it,
    // and a relative `workdir` against the instance's folder.
    assert!(text.contains("cwd: where(tool, input?.args)"), "{text}");
    assert!(
        text.contains(r#"workdir.startsWith("/") ? workdir"#),
        "{text}"
    );
}

#[test]
fn opencode_s_permission_prompt_is_also_an_ask_answered_on_opencode_s_own_client() {
    // #1691: the shim hands opencode's ask to purlis's permission hook, which the app holds
    // until the person answers in the window; the hook's reply word goes to opencode's own
    // client. Answered in opencode's pane first, the hook is stopped and its ask withdrawn.
    let text = shim(Arming::Session);
    assert!(
        text.contains(&format!(
            "spawn([BINARY, \"hook\", \"{}\"]",
            crate::harness::hooked::WORD
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "{} * 1000",
            crate::harness::hooked::HOOK_TIMEOUT.as_secs()
        )),
        "{text}"
    );
    assert!(text.contains("REPLIES.includes(word)"), "{text}");
    assert!(
        text.contains("postSessionIdPermissionsPermissionId"),
        "{text}"
    );
    assert!(text.contains("permission?.reply"), "{text}");
    assert!(text.contains("case \"permission.replied\""), "{text}");
    assert!(
        text.contains("settled(props?.requestID ?? props?.permissionID)"),
        "{text}"
    );
    assert!(text.contains("asked(props)"), "{text}");
    // The installed guard-only copy asks nothing of an app.
    let installed = shim(Arming::GuardOnly(Path::new("/bin/charter")));
    assert!(!installed.contains("REPLIES"), "{installed}");
}

#[test]
fn a_question_opencode_asks_is_a_prompt_waiting_in_its_terminal() {
    // #1691: opencode's `question.asked` is named as a question, as its permission is.
    let text = shim(Arming::Session);
    assert!(text.contains("case \"question.asked\""), "{text}");
    assert!(text.contains("notification_type: \"question\""), "{text}");
}
