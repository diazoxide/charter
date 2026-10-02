//! Claude Code's adapter (ADR 0073, FD-13): the bundled plugin, loaded for one chat alone, with
//! the session's own `--settings`.

use super::adapter::{HarnessAdapter, Sandbox};
use super::{Harness, Kit, StateHooks, words};
use crate::sandbox::Form;

/// Claude Code's adapter.
pub struct ClaudeCode;

/// The one Claude Code adapter.
pub static ADAPTER: ClaudeCode = ClaudeCode;

impl HarnessAdapter for ClaudeCode {
    fn harness(&self) -> Harness {
        Harness::ClaudeCode
    }

    /// **The bundled plugin, loaded for this session alone** (`crate::plugin` has the
    /// measurements). Its `hooks.json` holds every hook — the six that report state and the
    /// Bash guard — and names the binary by `$CHARTER_HOOK_BINARY`, so that is handed over in
    /// the environment. Without the plugin there is nothing to arm: a chat reads `unknown`,
    /// rather than being half-armed from a second place.
    ///
    /// `--settings` MERGES with the settings already in force rather than replacing them
    /// (measured on claude 2.1.276), and a key it names wins over the project's (measured on
    /// 2.1.280). It carries the plugin pins, the project's plugin choice, the status line where
    /// charter may fill it, Smart close's one allow, and the sandbox where the plane turned it
    /// on.
    fn arm_under(
        &self,
        kit: Kit<'_>,
        cwd: Option<&std::path::Path>,
        plugins: &crate::harness_plugin::Chosen,
        sandbox: Sandbox<'_>,
    ) -> StateHooks {
        let Some(plugin) = kit.plugin else {
            return StateHooks::None;
        };
        // Claude Code carries its sandbox in the same `--settings`. A form of another kind is
        // not one, and the chat is armed with nothing rather than started without it.
        let sandbox = match sandbox.applied().map(crate::sandbox::Applied::form) {
            None => None,
            Some(crate::sandbox::Form::ClaudeCode(settings)) => Some(settings),
            Some(_) => return StateHooks::None,
        };
        StateHooks::ThisSessionOnly {
            args: words([
                "--plugin-dir",
                &plugin.display().to_string(),
                "--settings",
                &settings(
                    kit.binary,
                    crate::footerclaim::status_line(cwd).free(),
                    plugins,
                    sandbox,
                ),
            ]),
            env: vec![(
                crate::plugin::BINARY_ENV.to_owned(),
                kit.binary.display().to_string(),
            )],
            cannot_report: Vec::new(),
        }
    }

    // `disarmed_by`: none. Claude Code loads `--plugin-dir` whatever else it is told.

    fn armed_with(&self) -> String {
        format!(
            "the app arms each chat with its own plugin, {}",
            crate::plugin::LOADED_AS
        )
    }

    fn plugins(&self) -> &'static dyn crate::harness_plugin::Adapter {
        &crate::harness_plugin::CLAUDE_CODE
    }

    /// A `sandbox` object and `permissions.deny` rules ([`crate::sandbox::claude`]).
    fn sandbox_compiler(&self) -> Option<crate::sandbox::Compiler> {
        Some(|compiled| crate::sandbox::claude::settings(compiled).map(Form::ClaudeCode))
    }

    /// The line as it is: Claude Code's sandbox rides in the `--settings` charter hands it (in
    /// `armed`), which a project's settings cannot loosen (ADR 0067 §2). Its own flags are
    /// not asked about yet.
    fn sandboxed_line(
        &self,
        form: &Form,
        command: Vec<String>,
        armed: Vec<String>,
        charters: Vec<String>,
    ) -> Result<Vec<String>, String> {
        match form {
            Form::ClaudeCode(_) => Ok([command, armed, charters].concat()),
            _ => Err(super::adapter::not_compiled_for(Harness::ClaudeCode)),
        }
    }
}

/// The settings a Claude Code chat is started with, as JSON on the argument.
///
/// On the argument and not in a file: a file would have to be written somewhere, cleaned up
/// when the chat ends, and cleaned up again after an app that crashed. What is in it is a
/// plugin id and a path — nothing secret, so `ps` showing it costs nothing.
///
/// **No hooks.** They are the bundled plugin's (`hooks/hooks.json`), so a chat has one place
/// its hooks are declared and a hook is never armed twice.
fn settings(
    binary: &std::path::Path,
    may_fill_the_footer: bool,
    plugins: &crate::harness_plugin::Chosen,
    sandbox: Option<&crate::sandbox::claude::Settings>,
) -> String {
    let mut settings = serde_json::Map::new();
    // The project's own choice of Claude Code's plugins first (charter-app#274, ADR 0050): a
    // session `enabledPlugins` wins over the project's and the user's for the keys it names,
    // and leaves every other plugin to them.
    let mut enabled: serde_json::Map<String, serde_json::Value> = plugins
        .iter()
        .map(|(id, on)| (id.clone(), (*on).into()))
        .collect();
    // Then the pins, written last so nothing above can move them
    // ([`crate::harness_plugin::CLAUDE_CODE`] holds them and says why). The operator's ruling of
    // 2026-09-23: a chat the app starts turns the Python charter's plugin off for itself, so the
    // project files that enable it for the operator's own terminal sessions do not give an app
    // chat two sets of hooks and two `handoff` skills. And the bundled plugin pinned on: a
    // project file can turn a `--plugin-dir` plugin off by its id, a chat can write that file,
    // and the plugin carries the Bash guard. Measured on 2.1.280: this `true` wins over a
    // project's `false`.
    for pin in ADAPTER.plugins().pinned() {
        enabled.insert(pin.id.to_owned(), pin.on.into());
    }
    settings.insert(
        "enabledPlugins".to_owned(),
        serde_json::Value::Object(enabled),
    );
    // **Only where nothing else fills it.** The key is one value and the flag is the last
    // writer, so arming it where the operator has their own would stop theirs running
    // ([`crate::footerclaim`], measured on 2.1.280). Where charter does not arm it, the chat
    // records no turns and its `ctx`/`cache` gauge stays dark — which `doctor` reports.
    if may_fill_the_footer {
        settings.insert("statusLine".to_owned(), status_line(binary));
    }
    // **The one command that ends a Smart close, pre-allowed, and nothing else** (SI-8e, the
    // operator's ruling of 2026-09-28): a chat asked to write its record must not stop on a
    // permission prompt for `charter session record`. One `allow` and no `ask`, `deny` or
    // mode, because Claude Code merges a session's permission rules with the user's and the
    // project's rather than replacing them, and its `deny` and `ask` outrank an `allow` — so
    // the operator's own rules all still stand (measured on 2.1.283 in ADR 0064: a project
    // `deny` still refused, a user `allow` still allowed, and a compound command that holds the
    // record command beside another was still asked about).
    let mut permissions = serde_json::json!({ "allow": [SMART_CLOSE_ALLOW] });
    // **The sandbox, where the plane turned it on** (ADR 0067), and the deny rules that keep
    // Claude Code's own Read and Edit tools out of what its sandbox denies a command
    // ([`crate::sandbox::claude`]). A deny outranks the allow above, and they name different
    // things. Absent otherwise, never `enabled: false`: an unsandboxed chat is left to the
    // operator's own settings, as it was.
    if let Some(sandbox) = sandbox {
        settings.insert("sandbox".to_owned(), sandbox.sandbox.clone());
        permissions["deny"] = serde_json::json!(sandbox.deny);
    }
    settings.insert("permissions".to_owned(), permissions);
    serde_json::Value::Object(settings).to_string()
}

/// The permission rule a Claude Code chat the app starts carries: `charter session record`,
/// with any arguments, runs without asking (SI-8e, ADR 0064).
pub const SMART_CLOSE_ALLOW: &str = "Bash(charter session record *)";

/// Claude Code's `statusLine`, pointed at `charter statusline` — for THIS session only.
///
/// **This is how a chat's `ctx`/`cache` history gets written at all, and without it the app's
/// gauge is a renderer with nothing to render.** Claude Code hands `context_window` — the
/// context percentage and the cache numbers — to its `statusLine` command and to nothing
/// else: no hook payload carries them (charter ADR 0019, measured). `charter statusline`
/// is the command that writes them down (`usage::record`), on every render, whether or not
/// it draws anything. And since charter 0.57.0 (#895) nothing puts that command in a
/// session's settings any more, while the app arms only hooks — so, measured on 2026-09-22,
/// the operator's own plane had not had a turn recorded since 2026-09-04, and every chat the
/// app started ran with no gauge on any surface.
///
/// **What the chat SEES does not change by default.** Inside the app `charter statusline`
/// prints an empty line (ADR 0019 transposed, `charter-cli/src/statusline.rs`), which is
/// the footer the app's chats were already meant to have — and ADR 0029's per-chat checkbox,
/// which sets `CHARTER_FOOTER=show`, draws charter's footer instead. That checkbox was a
/// switch on a command nothing invoked; this is what makes it one.
///
/// **It is armed only where the operator fills the line with nothing**, and that is the
/// operator's own ruling of 2026-09-22. `--settings` merges key by key and `statusLine` is one
/// key, so arming it over somebody's own command would stop theirs from running, silently, for
/// every chat the app starts — measured on 2.1.280, four cells, one launch each
/// ([`crate::footerclaim`], which holds the measurement and the rule). Where something else
/// fills it, charter arms nothing, leaves their configuration alone, and lets `doctor` say why
/// the gauge is dark. Nothing here wraps or chains their command: charter would then own its
/// failures and its latency, every turn.
///
/// **What it costs otherwise, said:** nothing is written to any file, their own `claude` in a
/// terminal is untouched, and a command costs one process per footer render — which Claude
/// Code runs at startup and once per submitted turn, not on a timer (measured: zero
/// invocations over thirty idle seconds).
fn status_line(binary: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "type": "command",
        "command": format!(
            "{} statusline",
            crate::plugin::shell_quoted(&binary.display().to_string())
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> &'static dyn HarnessAdapter {
        &ADAPTER
    }

    #[test]
    fn a_sandbox_compiled_for_another_harness_arms_no_claude_code_chat() {
        // ADR 0067, fail closed: a Codex sandbox handed to the Claude Code adapter would
        // otherwise start a Claude Code chat with no sandbox at all. The adapter itself
        // refuses it, not only `Harness::state_hooks`.
        let (plane, codex) = crate::harness::testing::sandbox_compiled_for(Harness::Codex);
        let codex = codex.expect("starts");

        let hooks = adapter().arm(
            Kit {
                binary: std::path::Path::new("/bin/charter"),
                plugin: Some(std::path::Path::new("/app/plugin")),
            },
            Some(plane.path()),
            &crate::harness_plugin::Chosen::new(),
            Some(&codex),
        );
        assert_eq!(hooks, StateHooks::None);
    }

    #[test]
    fn the_claude_code_adapter_arms_claude_code() {
        assert_eq!(adapter().harness(), Harness::ClaudeCode);
    }

    #[test]
    fn the_claude_code_adapter_arms_a_chat_with_the_bundled_plugin_for_that_chat_alone() {
        let empty = tempfile::tempdir().expect("a directory");
        let hooks = adapter().arm(
            Kit {
                binary: std::path::Path::new("/bin/charter"),
                plugin: Some(std::path::Path::new("/app/plugin")),
            },
            Some(empty.path()),
            &crate::harness_plugin::Chosen::new(),
            None,
        );
        let StateHooks::ThisSessionOnly {
            args,
            env,
            cannot_report,
        } = hooks
        else {
            panic!("armed per session");
        };
        assert_eq!(args[..3], ["--plugin-dir", "/app/plugin", "--settings"]);
        assert_eq!(
            env,
            [("CHARTER_HOOK_BINARY".to_owned(), "/bin/charter".to_owned())]
        );
        assert!(cannot_report.is_empty());
    }

    #[test]
    fn nothing_turns_a_claude_code_chat_s_hooks_off_behind_charter() {
        let command = ["claude".to_owned(), "--bare".to_owned()];
        assert_eq!(adapter().disarmed_by(&command, &[]), None);
    }

    #[test]
    fn doctor_says_a_claude_code_chat_is_armed_with_the_bundled_plugin() {
        assert_eq!(
            adapter().armed_with(),
            "the app arms each chat with its own plugin, charter@inline"
        );
    }

    #[test]
    fn the_claude_code_adapter_hands_a_chat_claude_code_s_own_plugins() {
        assert_eq!(adapter().plugins().harness(), "claude");
    }
}
