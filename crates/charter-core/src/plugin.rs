//! The Claude Code plugin the app ships inside its own bundle, and the hook registry both
//! harnesses are armed from.
//!
//! **The app depends on no charter it did not ship** (operator ruling, 2026-09-23: "charter-app
//! should be standalone application without any dependency from old charter"). Until this, a
//! Claude chat was refused unless the Python charter's `charter@charter` plugin was installed
//! from the `diazoxide/charter-plane` marketplace — a git clone over the network at the first
//! launch, and a refusal of every chat on a machine that was offline. Now the bundle carries its
//! own plugin, named [`NAME`], and every chat the app starts loads it for that session alone
//! with `claude --plugin-dir`. Nothing is installed, nothing is written into a config folder, and a
//! `claude` the operator runs in a terminal is untouched.
//!
//! # Measured on claude 2.1.280, with a throwaway `CLAUDE_CONFIG_DIR` and a stand-in API
//!
//! - `--plugin-dir <dir>` loads the plugin as `charter@inline` for that session: its skills
//!   reach the model as `charter:<skill>`, and its `SessionStart`, `UserPromptSubmit`,
//!   `PreToolUse` (matcher `Bash`), `Stop` and `SessionEnd` hooks each fired once in a one-turn
//!   `-p` run that called Bash. `${CLAUDE_PLUGIN_ROOT}` is the directory as given.
//! - A plane whose `.claude/settings.json` enables `charter@charter`, with that plugin
//!   installed at project scope, loads BOTH plugins under `--plugin-dir`: two sets of hooks on
//!   every event and two `handoff` skills. `--settings '{"enabledPlugins":{"charter@charter":
//!   false}}'` on the same command line turns the installed one off for that session — none of
//!   its hooks fired and its skills were not offered — while the project file still enables it
//!   for every other session. It wins over `.claude/settings.local.json` too. That is the
//!   operator's ruling ("App chats disable it"), and [`SUPERSEDED`] is what it names.
//! - **A project file can turn the bundled plugin off**: `{"enabledPlugins":
//!   {"charter@inline": false}}` in the chat's `.claude/settings.json` loaded none of its
//!   hooks and none of its skills. That is a file a chat can write, and the plugin carries the
//!   Bash guard, so the session settings pin it on — `"charter@inline": true` beside the
//!   `--plugin-dir` — and, measured, that wins over the project's `false`: every hook fired.
//!   (Measured as `charter-app@inline` on 2.1.280, and again as `charter@inline` on 2.1.282.)
//!
//! # Measured on claude 2.1.282: two plugins named `charter` (#406)
//!
//! The Python charter's plugin is named `charter` too, so since the rename both plugins
//! namespace their skills `charter:`. Claude Code loads one plugin per name. In a throwaway
//! config with `charter@charter` installed at project scope from a local marketplace and
//! enabled by the project file, reading which plugins and skills `claude -p --output-format
//! stream-json` reports at start:
//!
//! - with `--plugin-dir` and no session settings, only `charter@inline` loaded — the
//!   `--plugin-dir` copy wins the name;
//! - `"charter@charter": false` in `--settings` left `charter@inline` loaded: turning the old
//!   one off by its id does not turn the bundled one off;
//! - `"charter@inline": false` (in `--settings`, or in the project file with no session pin)
//!   unloaded the bundled one **and loaded `charter@charter` in its place**, its skills as
//!   `charter:<skill>`. So the pin on [`LOADED_AS`] now also keeps the Python plugin out, and
//!   the pin off on [`SUPERSEDED`] stays as the second lock;
//! - with the project file saying `"charter@inline": false` and the session pinning
//!   `charter@inline` on and `charter@charter` off, only `charter@inline` loaded.
//! - Through the real `charter` and this very plugin: every hook it then wired ran and exited 0,
//!   and a Bash call reading `.charter/vaults/db.json` in a plane was refused by the guard
//!   before it ran — the model got the refusal and never the file.
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

use crate::hookreg::Handler;

/// The plugin's name, which is also how its skills are namespaced (`charter:handoff`).
///
/// The product is charter, so its plugin is (#406, ADR 0056). Until then it was `charter-app`
/// ([`FORMERLY`]), to keep its skills apart from the Python charter's, whose plugin is named
/// `charter` too. They are kept apart by the pins instead: see the module header.
pub const NAME: &str = "charter";

/// The id Claude Code gives a plugin loaded with `--plugin-dir`, and the one `enabledPlugins`
/// names it by.
pub const LOADED_AS: &str = "charter@inline";

/// The plugin a chat the app starts turns off for itself — the Python charter's, installed
/// from its marketplace. Only for that session: `--settings` is the whole of how.
///
/// Its *name* is `charter`, the same as [`NAME`]; only the marketplace after the `@` tells it
/// from [`LOADED_AS`], and every pin matches on the whole id.
pub const SUPERSEDED: &str = "charter@charter";

/// The id [`LOADED_AS`] had before the plugin was renamed `charter` (#406). No chat loads a
/// plugin by it any more; a chat the app starts turns it off, so a file that still turns it on
/// is told the new id rather than trusted to mean it.
pub const FORMERLY: &str = "charter-app@inline";

/// The variable a hook command reads the app's own `charter` from.
pub const BINARY_ENV: &str = "CHARTER_HOOK_BINARY";

/// Where the plugin's hooks file sits inside the plugin directory.
pub const HOOKS_FILE: &str = "hooks/hooks.json";

/// **The words Codex is armed with** — a subset of [`crate::hookreg::HANDLERS`], named rather
/// than filtered by event, because each one is a hook the operator is asked to trust.
///
/// The four state events Codex was measured firing (codex-cli 0.147.0, `harness.rs`) and the
/// Bash guard. Codex has no `Notification`; `SubagentStop` would change nothing the board
/// draws; and the other tool hooks match Claude Code's tool names (`Read|Grep`, `Write|Edit`,
/// `Task|Agent`), which Codex's tools are not called, so arming them would ask for trust in
/// hooks that never fire.
pub const CODEX: [&str; 5] = [
    "sessionstart",
    "userpromptsubmit",
    "pretooluse",
    "stop",
    "sessionend",
];

/// The handlers Codex is armed with, in the registry's order.
pub fn codex_handlers() -> impl Iterator<Item = &'static Handler> {
    crate::hookreg::HANDLERS
        .iter()
        .filter(|h| CODEX.contains(&h.name))
}

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

/// One matcher's hooks within an event.
pub type Group<'a> = (Option<&'static str>, Vec<&'a Handler>);

/// One event's groups.
pub type Event<'a> = (&'static str, Vec<Group<'a>>);

/// `hooks` grouped the way both harnesses read them: event, then one group per matcher.
pub fn grouped<'a>(hooks: impl Iterator<Item = &'a Handler>) -> Vec<Event<'a>> {
    let mut events: Vec<Event<'a>> = Vec::new();
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

/// The plugin's `hooks/hooks.json`, generated from [`crate::hookreg::HANDLERS`] — the list
/// `charter hook --list --json` prints — and nothing else. **The plugin owns every charter
/// hook of a Claude Code chat**: the app's `--settings` arms none, so each word is wired
/// exactly once (`harness.rs` holds the test).
///
/// The file in the bundle is this, byte for byte — the app crate's test fails on any drift and
/// its ignored twin rewrites it.
pub fn hooks_json() -> String {
    let events: serde_json::Map<String, serde_json::Value> =
        grouped(crate::hookreg::HANDLERS.iter())
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
                                    "command": plugin_command(hook.name),
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
        "description": "Generated from charter_core::hookreg (charter hook --list --json). Do not edit: \
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
    fn every_handler_the_binary_lists_is_wired_once_and_nothing_else_is() {
        let doc: serde_json::Value = serde_json::from_str(&hooks_json()).expect("JSON");
        let mut wired: Vec<String> = Vec::new();
        for groups in doc["hooks"].as_object().expect("events").values() {
            for group in groups.as_array().expect("groups") {
                for hook in group["hooks"].as_array().expect("hooks") {
                    wired.push(hook["command"].as_str().expect("a command").to_owned());
                }
            }
        }
        let mut expected: Vec<String> = crate::hookreg::HANDLERS
            .iter()
            .map(|h| plugin_command(h.name))
            .collect();
        wired.sort();
        expected.sort();
        assert_eq!(wired, expected);
        for word in crate::hookreg::NO_OPS {
            assert!(!hooks_json().contains(&format!(" hook {word}\"")), "{word}");
        }
    }

    #[test]
    fn codex_is_armed_only_with_words_the_registry_has() {
        for word in CODEX {
            assert!(crate::hookreg::find(word).is_some(), "{word}");
        }
        assert_eq!(codex_handlers().count(), CODEX.len());
    }

    #[test]
    fn no_hook_that_can_allow_a_permission_is_ever_armed() {
        // `PermissionRequest` can ALLOW, which is authority nothing charter arms may take from
        // a file a chat can write.
        assert!(!hooks_json().contains("PermissionRequest"));
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
    }

    #[test]
    fn the_plugin_is_called_charter_and_loads_as_charter_at_inline() {
        // #406: the product is charter, so its plugin is, and its skills are `charter:<skill>`.
        assert_eq!(NAME, "charter");
        assert_eq!(LOADED_AS, "charter@inline");
        assert_eq!(LOADED_AS, format!("{NAME}@inline"));
    }

    #[test]
    fn the_three_ids_charter_names_are_three_different_ids() {
        // The Python charter's plugin is also *named* `charter`; only the marketplace tells
        // `charter@charter` from `charter@inline`. Every pin matches on the whole id.
        assert_eq!(FORMERLY, "charter-app@inline");
        assert_eq!(SUPERSEDED, "charter@charter");
        assert_ne!(LOADED_AS, SUPERSEDED);
        assert_ne!(LOADED_AS, FORMERLY);
        assert_ne!(SUPERSEDED, FORMERLY);
    }

    #[test]
    fn a_path_with_a_quote_in_it_stays_one_word() {
        assert_eq!(
            command_at(std::path::Path::new("/home/o'brien/charter"), "stop"),
            r"'/home/o'\''brien/charter' hook stop"
        );
    }
}
