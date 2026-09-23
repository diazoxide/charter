//! The Claude Code plugin the app ships inside its own bundle, and the hook registry both
//! harnesses are armed from.
//!
//! **The app depends on no charter it did not ship** (operator ruling, 2026-09-23: "charter-app
//! should be standalone application without any dependency from old charter"). Until this, a
//! Claude chat was refused unless the Python charter's `charter@charter` plugin was installed
//! from the `diazoxide/charter` marketplace — a git clone over the network at the first launch,
//! and a refusal of every chat on a machine that was offline. Now the bundle carries its own
//! plugin, named [`NAME`], and every chat the app starts loads it for that session alone with
//! `claude --plugin-dir`. Nothing is installed, nothing is written into a config folder, and a
//! `claude` the operator runs in a terminal is untouched.
//!
//! # Measured on claude 2.1.280, with a throwaway `CLAUDE_CONFIG_DIR` and a stand-in API
//!
//! - `--plugin-dir <dir>` loads the plugin as `charter-app@inline` for that session: its skills
//!   reach the model as `charter-app:<skill>`, and its `SessionStart`, `UserPromptSubmit`,
//!   `PreToolUse` (matcher `Bash`), `Stop` and `SessionEnd` hooks each fired once in a one-turn
//!   `-p` run that called Bash. `${CLAUDE_PLUGIN_ROOT}` is the directory as given.
//! - A plane whose `.claude/settings.json` enables `charter@charter`, with that plugin
//!   installed at project scope, loads BOTH plugins under `--plugin-dir`: two sets of hooks on
//!   every event and two `handoff` skills. `--settings '{"enabledPlugins":{"charter@charter":
//!   false}}'` on the same command line turns the installed one off for that session — none of
//!   its hooks fired and its skills were not offered — while the project file still enables it
//!   for every other session. That is the operator's ruling ("App chats disable it"), and
//!   [`SUPERSEDED`] is what it names.
//!
//! # Why the hook command names `$CHARTER_HOOK_BINARY` and not a path
//!
//! The command has to reach the `charter` the app shipped, by an absolute path, and the
//! plugin directory cannot spell one. `${CLAUDE_PLUGIN_ROOT}` is fixed, but where the binary
//! sits relative to it is not: `Contents/Resources/plugin` beside `Contents/MacOS/charter` in
//! a macOS bundle, `/usr/lib/charter/plugin` beside `/usr/bin/charter` in a `.deb` and an
//! AppImage, `target/debug/plugin` beside `target/debug/charter` in a development build — and
//! a scenario test points the app at the binary it just built with `CHARTER_BINARY`, which no
//! relative path can follow. So the app, which already knows the one path it arms every hook
//! with, puts it in the chat's environment as [`BINARY_ENV`], and the shell a hook runs in
//! expands it (measured: the value arrived in every hook above). Where the variable is unset
//! the plugin is not loaded at all, because the app loads it only when it has a binary.

/// The plugin's name, which is also how its skills are namespaced (`charter-app:handoff`).
///
/// Not `charter`, so a skill of this plugin never collides with one of the Python charter's in
/// a session that somehow has both.
pub const NAME: &str = "charter-app";

/// The plugin a chat the app starts turns off for itself — the Python charter's, installed
/// from its marketplace. Only for that session: `--settings` is the whole of how.
pub const SUPERSEDED: &str = "charter@charter";

/// The variable a hook command reads the app's own `charter` from.
pub const BINARY_ENV: &str = "CHARTER_HOOK_BINARY";

/// Where the plugin's hooks file sits inside the plugin directory.
pub const HOOKS_FILE: &str = "hooks/hooks.json";

/// One hook charter arms: the harness event, the tool it is about, and the word `charter hook`
/// answers it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hook {
    /// The harness's own event name, as both Claude Code and Codex spell it.
    pub event: &'static str,
    /// The tools a tool hook is about, as a matcher. `None` for an event that is not about a
    /// tool.
    pub matcher: Option<&'static str>,
    /// The word `charter hook` takes.
    pub word: &'static str,
    /// Seconds. Far more than the milliseconds each was measured at, and far less than a
    /// turn: a hook that somehow hung must not hold the turn open behind it.
    pub timeout: i64,
    /// Whether Codex is armed with it too. Codex has no `Notification`, and a hook armed on
    /// Codex is one more the operator is asked to trust, so one that would change nothing
    /// they can see is left off.
    pub codex: bool,
}

/// **Every hook charter arms, and only handlers that answer.**
///
/// A word goes in this list only once `charter hook <word>` answers an ordinary call with exit
/// 0 — `charter-cli/tests/hook.rs` runs each one and fails otherwise. The binary blocks every
/// tool-hook word it has not ported (`is_a_tool_hook`), so wiring `pretooluse-read` here before
/// its handler lands would refuse every `Read` in every chat.
///
/// The six state events are the ones the app used to arm through `--settings`; they moved here
/// so the plugin's `hooks.json` is the one place a Claude chat's hooks are declared.
pub const HOOKS: &[Hook] = &[
    Hook {
        event: "SessionStart",
        matcher: None,
        word: "sessionstart",
        timeout: 5,
        codex: true,
    },
    Hook {
        event: "UserPromptSubmit",
        matcher: None,
        word: "userpromptsubmit",
        timeout: 5,
        codex: true,
    },
    // The Bash guard (M3.1). Ten seconds, as the Python charter's plugin gives it: it is the
    // one hook here that reads the plane.
    Hook {
        event: "PreToolUse",
        matcher: Some("Bash"),
        word: "pretooluse",
        timeout: 10,
        codex: true,
    },
    Hook {
        event: "Notification",
        matcher: None,
        word: "notification",
        timeout: 5,
        codex: false,
    },
    Hook {
        event: "SubagentStop",
        matcher: None,
        word: "subagentstop",
        timeout: 5,
        codex: false,
    },
    Hook {
        event: "Stop",
        matcher: None,
        word: "stop",
        timeout: 5,
        codex: true,
    },
    Hook {
        event: "SessionEnd",
        matcher: None,
        word: "sessionend",
        timeout: 5,
        codex: true,
    },
];

/// The command a Claude Code plugin hook runs: the app's own binary, read from the chat's
/// environment, and one word.
pub fn plugin_command(word: &str) -> String {
    format!("\"${{{BINARY_ENV}}}\" hook {word}")
}

/// The command a hook armed by absolute path runs — Codex's, which has no plugin.
pub fn command_at(binary: &std::path::Path, word: &str) -> String {
    format!(
        "{} hook {word}",
        shell_quoted(&binary.display().to_string())
    )
}

/// `hooks` grouped the way both harnesses read them: event, then one group per matcher.
pub fn grouped<'a>(
    hooks: impl Iterator<Item = &'a Hook>,
) -> Vec<(&'static str, Vec<(Option<&'static str>, Vec<&'a Hook>)>)> {
    let mut events: Vec<(&'static str, Vec<(Option<&'static str>, Vec<&'a Hook>)>)> = Vec::new();
    for hook in hooks {
        let at = match events.iter().position(|(event, _)| *event == hook.event) {
            Some(at) => at,
            None => {
                events.push((hook.event, Vec::new()));
                events.len() - 1
            }
        };
        let groups = &mut events[at].1;
        match groups
            .iter_mut()
            .find(|(matcher, _)| *matcher == hook.matcher)
        {
            Some((_, hooks)) => hooks.push(hook),
            None => groups.push((hook.matcher, vec![hook])),
        }
    }
    events
}

/// The plugin's `hooks/hooks.json`, generated from [`HOOKS`] and nothing else.
///
/// The file in the bundle is this, byte for byte — the app crate's test fails on any drift and
/// its ignored twin rewrites it.
pub fn hooks_json() -> String {
    let events: serde_json::Map<String, serde_json::Value> = grouped(HOOKS.iter())
        .into_iter()
        .map(|(event, groups)| {
            let groups: Vec<serde_json::Value> = groups
                .into_iter()
                .map(|(matcher, hooks)| {
                    let hooks: Vec<serde_json::Value> = hooks
                        .iter()
                        .map(|hook| {
                            serde_json::json!({
                                "type": "command",
                                "command": plugin_command(hook.word),
                                "timeout": hook.timeout,
                            })
                        })
                        .collect();
                    let mut group = serde_json::Map::new();
                    if let Some(matcher) = matcher {
                        group.insert("matcher".to_owned(), matcher.into());
                    }
                    group.insert("hooks".to_owned(), hooks.into());
                    serde_json::Value::Object(group)
                })
                .collect();
            (event.to_owned(), groups.into())
        })
        .collect();
    let doc = serde_json::json!({
        "description": "Generated from charter_core::plugin::HOOKS. Do not edit: \
                        `cargo test -p charter-app -- --ignored` rewrites it.",
        "hooks": events,
    });
    let mut text = serde_json::to_string_pretty(&doc).expect("a JSON value serialises");
    text.push('\n');
    text
}

/// A path as one word a shell cannot take apart.
///
/// Both harnesses run a hook's `command` through a shell, so the path goes in single quotes.
/// It is charter's own executable path and not anything a plane wrote, but a person with a
/// quote in their home directory name is not a security model.
pub fn shell_quoted(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_is_armed_once() {
        let mut words: Vec<&str> = HOOKS.iter().map(|hook| hook.word).collect();
        words.sort_unstable();
        let before = words.len();
        words.dedup();
        assert_eq!(words.len(), before, "a word is armed twice: {words:?}");
    }

    #[test]
    fn no_hook_that_can_allow_a_permission_is_ever_armed() {
        // `PermissionRequest` can ALLOW, which is authority nothing charter arms may take from
        // a file a chat can write. And `PostToolUse` has no handler that answers yet.
        for hook in HOOKS {
            assert!(
                !["PermissionRequest", "PostToolUse"].contains(&hook.event),
                "{hook:?}"
            );
        }
    }

    #[test]
    fn a_plugin_hook_names_the_binary_by_the_variable_the_app_sets() {
        assert_eq!(
            plugin_command("stop"),
            "\"${CHARTER_HOOK_BINARY}\" hook stop"
        );
    }

    #[test]
    fn the_hooks_file_groups_the_guard_under_its_matcher() {
        let doc: serde_json::Value = serde_json::from_str(&hooks_json()).expect("JSON");
        let guard = &doc["hooks"]["PreToolUse"][0];
        assert_eq!(guard["matcher"], "Bash");
        assert_eq!(
            guard["hooks"][0]["command"],
            "\"${CHARTER_HOOK_BINARY}\" hook pretooluse"
        );
        assert_eq!(guard["hooks"][0]["timeout"], 10);
        assert!(doc["hooks"]["Stop"][0].get("matcher").is_none());
        assert_eq!(doc["hooks"].as_object().expect("events").len(), HOOKS.len());
    }

    #[test]
    fn a_path_with_a_quote_in_it_stays_one_word() {
        assert_eq!(
            command_at(std::path::Path::new("/home/o'brien/charter"), "stop"),
            r"'/home/o'\''brien/charter' hook stop"
        );
    }
}
