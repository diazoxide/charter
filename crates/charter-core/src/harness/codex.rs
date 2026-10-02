//! Codex's adapter (ADR 0073, FD-13): charter's hooks, armed on one session by Codex's own
//! `-c hooks.<Event>=…` flags.

use super::adapter::{HarnessAdapter, Sandbox};
use super::{Harness, Kit, StateHooks};

/// Codex's adapter.
pub struct Codex;

/// The one Codex adapter.
pub static ADAPTER: Codex = Codex;

impl HarnessAdapter for Codex {
    fn harness(&self) -> Harness {
        Harness::Codex
    }

    /// **Measured on codex-cli 0.147.0, and it refutes what this arm used to say** —
    /// that Codex could only be armed in `~/.codex/config.toml` and could never say it
    /// was waiting. Every fact here was taken from the real binary driving a real turn
    /// against a stand-in model server, the TUI in a pane as the app runs it (#27):
    ///
    /// * `-c hooks.<Event>=[…]` arms a hook for ONE session. Codex lists its source as
    ///   "Session flags", and it runs BESIDE the operator's own `[[hooks.<Event>]]` for
    ///   the same event rather than replacing it — both fired. Nothing is written.
    /// * `SessionStart`, `UserPromptSubmit`, `Stop` and `SessionEnd` each fire with
    ///   `session_id` in the payload. `SessionStart` names a `source` (`startup`, and
    ///   `resume` with the SAME id on `codex resume <id>`); `SessionEnd` names a
    ///   `reason` (`other`, on `/quit`). `Stop` is "right before Codex ends its turn",
    ///   which is the falling edge `Stop` means for Claude Code.
    /// * There is no `Notification`. Codex tells a hook it is asking for approval only
    ///   through `PermissionRequest` — which fired exactly when the prompt appeared,
    ///   and not for a command that needed none — but that is a hook that DECIDES a
    ///   permission, and the app arms no such hook. So a Codex chat that stops
    ///   mid-turn for approval cannot say so.
    /// * **And there is no second way round it** (charter-app#52, read out of the same
    ///   0.147.0 binary the measurements above were taken on). The binary carries
    ///   eleven hook events and no more — `PreToolUse`, `PermissionRequest`,
    ///   `PostToolUse`, `PreCompact`, `PostCompact`, `SessionStart`, `SessionEnd`,
    ///   `UserPromptSubmit`, `SubagentStart`, `SubagentStop`, `Stop` — and the only
    ///   one that fires when the prompt appears is the one that decides it. The
    ///   `notify` program is not a second channel either: in 0.147.0 it is
    ///   `legacy_notify`, a shim over `Stop` whose one payload type is
    ///   `agent-turn-complete`, which is the falling edge `Stop` already gives. What
    ///   is left is `PreToolUse` without a matching `PostToolUse` for long enough —
    ///   which is a guess at timing over a harness's behaviour, and ADR 0018 admits
    ///   no state that did not come from a hook saying so.
    /// * `SessionStart` fires inside the FIRST TURN, not at launch (X1): an idle TUI
    ///   reported nothing at all until a prompt was typed.
    /// * A hook is inert until Codex trusts it. For these, the TUI itself asks at
    ///   startup — "Hooks need review … Trust all and continue / Continue without
    ///   trusting (hooks won't run)" — and Codex writes the answer into its own
    ///   `[hooks.state]`, keyed by event, position and a hash of the hook. The same
    ///   binary path arms the same hooks, so the operator is asked once and not once a
    ///   chat. Untrusted, they do not run and nothing says so (`codex exec`).
    ///
    /// Codex has no plugin here: the guard used to reach a Codex chat through the
    /// Python charter's Codex plugin, and now rides on the same `-c` flags as the state
    /// hooks, from the same registry ([`crate::plugin::CODEX`] out of [`crate::hookreg`]).
    ///
    /// And no plugin: Codex 0.147.0 takes a plugin's `enabled` from its `config.toml`
    /// alone and ignores the same key given with `-c` (measured, charter-app#274), so
    /// `plugins` is empty for it and nothing here would carry it.
    ///
    /// Its skills ride on the `SessionStart` hook above: Codex can take no skills
    /// directory for one session ([`Harness::skills`]), so the chat is started with the
    /// bundle's in [`crate::skills::LISTED_ENV`], which a Codex hook inherits, and the
    /// briefing lists them.
    ///
    /// Its sandbox, where the plane turned it on, is not here: it rides on flags that go
    /// last among the flags on the chat's line ([`crate::sandbox::Applied::line`], and
    /// [`crate::sandbox::codex`] has the measurements). An unsandboxed Codex chat keeps
    /// Codex's own settings, as it did.
    fn arm_under(
        &self,
        kit: Kit<'_>,
        _cwd: Option<&std::path::Path>,
        _plugins: &crate::harness_plugin::Chosen,
        _sandbox: Sandbox<'_>,
    ) -> StateHooks {
        StateHooks::ThisSessionOnly {
            args: session_flags(kit.binary),
            env: kit
                .plugin
                .and_then(crate::skills::in_bundle)
                .map(|dir| {
                    (
                        crate::skills::LISTED_ENV.to_owned(),
                        dir.display().to_string(),
                    )
                })
                .into_iter()
                .collect(),
            cannot_report: vec!["notification"],
        }
    }

    // `disarmed_by`: none. Codex's session flags are charter's own.

    fn armed_with(&self) -> String {
        "the app arms each Codex chat with charter's hooks; Codex asks once to trust them"
            .to_owned()
    }

    fn plugins(&self) -> &'static dyn crate::harness_plugin::Adapter {
        &crate::harness_plugin::CODEX
    }
}

/// The `-c` pairs that arm Codex's hooks on one session, out of [`crate::plugin::codex_handlers`].
///
/// On the argument for the reason Claude Code's are: nothing is written, so nothing is left
/// behind. Each value is TOML, because that is how Codex parses a `-c` value — and it is
/// SERIALISED rather than formatted, since a value that fails to parse is not an error to
/// Codex but a literal string, which it then rejects as the wrong type and refuses to start.
///
/// The command names the binary by its absolute path: Codex has no plugin root and no
/// variable of charter's to expand, and a Codex hook runs through a shell just the same —
/// measured, a single-quoted argument holding spaces arrived as one word.
fn session_flags(binary: &std::path::Path) -> Vec<String> {
    crate::plugin::grouped(crate::plugin::codex_handlers())
        .into_iter()
        .flat_map(|(event, groups)| {
            let groups: Vec<toml::Value> = groups
                .into_iter()
                .map(|(matcher, hooks)| {
                    let hooks: Vec<toml::Value> = hooks
                        .iter()
                        .map(|hook| {
                            let table: toml::Table = [
                                ("type".to_owned(), toml::Value::from("command")),
                                (
                                    "command".to_owned(),
                                    toml::Value::from(crate::plugin::command_at(binary, hook.name)),
                                ),
                                // Codex's own default is 600 seconds (its review screen says so).
                                (
                                    "timeout".to_owned(),
                                    toml::Value::from(i64::from(hook.timeout)),
                                ),
                            ]
                            .into_iter()
                            .collect();
                            toml::Value::Table(table)
                        })
                        .collect();
                    let mut group = toml::Table::new();
                    if let Some(matcher) = matcher {
                        group.insert("matcher".to_owned(), toml::Value::from(matcher));
                    }
                    group.insert("hooks".to_owned(), toml::Value::Array(hooks));
                    toml::Value::Table(group)
                })
                .collect();
            let value = toml::Value::Array(groups);
            ["-c".to_owned(), format!("hooks.{event}={value}")]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> &'static dyn HarnessAdapter {
        &ADAPTER
    }

    fn kit() -> Kit<'static> {
        Kit {
            binary: std::path::Path::new("/bin/charter"),
            plugin: Some(std::path::Path::new("/app/plugin")),
        }
    }

    #[test]
    fn the_codex_adapter_arms_codex() {
        assert_eq!(adapter().harness(), Harness::Codex);
    }

    #[test]
    fn the_codex_adapter_arms_a_chat_with_session_flags_and_says_it_cannot_report_an_ask() {
        // Measured on codex-cli 0.147.0: `-c hooks.<Event>=[…]` arms a hook for one session,
        // and Codex has no `Notification`.
        let StateHooks::ThisSessionOnly {
            args,
            cannot_report,
            ..
        } = adapter().arm(kit(), None, &crate::harness_plugin::Chosen::new(), None)
        else {
            panic!("armed per session");
        };
        assert_eq!(args[0], "-c");
        assert!(args[1].starts_with("hooks."), "{args:?}");
        assert_eq!(cannot_report, ["notification"]);
    }

    #[test]
    fn a_sandbox_compiled_for_another_harness_arms_no_codex_chat() {
        // ADR 0067, fail closed, held by the adapter seam itself.
        let plane = tempfile::tempdir().expect("a plane");
        std::fs::write(
            plane.path().join("charter.toml"),
            "[sandbox]\nmode = \"on\"\n",
        )
        .expect("charter.toml");
        let machine = crate::sandbox::Machine {
            env: crate::secrets::Env::of(&[]),
            home: None,
            os: crate::sandbox::Os::Linux,
        };
        let claude =
            crate::sandbox::for_start(Harness::ClaudeCode, plane.path(), &machine, &|_| true)
                .expect("starts")
                .expect("sandboxed");

        let hooks = adapter().arm(
            kit(),
            Some(plane.path()),
            &crate::harness_plugin::Chosen::new(),
            Some(&claude),
        );
        assert_eq!(hooks, StateHooks::None);
    }

    #[test]
    fn nothing_turns_a_codex_chat_s_hooks_off_behind_charter() {
        // Codex's session flags are charter's own.
        assert_eq!(adapter().disarmed_by(&["codex".to_owned()], &[]), None);
    }

    #[test]
    fn doctor_says_a_codex_chat_is_armed_with_charter_s_hooks() {
        assert_eq!(
            adapter().armed_with(),
            "the app arms each Codex chat with charter's hooks; Codex asks once to trust them"
        );
    }

    #[test]
    fn the_codex_adapter_hands_a_chat_codex_s_own_plugins() {
        assert_eq!(adapter().plugins().harness(), "codex");
    }
}
