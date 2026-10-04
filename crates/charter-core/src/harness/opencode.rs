//! opencode's adapter (ADR 0073, FD-13): charter's shim, loaded for one session by naming it in
//! the whole config opencode reads from `OPENCODE_CONFIG_CONTENT` ([`crate::opencode`] has the
//! measurements).

use super::adapter::{HarnessAdapter, Sandbox};
use super::{Harness, Kit, StateHooks};
use crate::sandbox::Form;

/// opencode's adapter.
pub struct Opencode;

/// The one opencode adapter.
pub static ADAPTER: Opencode = Opencode;

impl HarnessAdapter for Opencode {
    fn harness(&self) -> Harness {
        Harness::Opencode
    }

    /// **The bundled shim, loaded for this session alone** ([`crate::opencode`] has the
    /// measurements, opencode 1.18.23). opencode reads a whole config from
    /// `OPENCODE_CONFIG_CONTENT` and concatenates its `plugin` list with every other
    /// config's, so naming the shim there loads it beside whatever the operator loads,
    /// writes nothing, and a project file cannot take it out. It runs the binary the
    /// environment names, as the Claude Code plugin's hooks do. `OPENCODE_PURE=0`
    /// because `1` would load no plugin at all; the flag that does the same is refused
    /// before the chat starts ([`crate::opencode::disarmed_by`]).
    ///
    /// No plugin choice: opencode has no switch that turns one plugin off.
    ///
    /// Its skills are the bundle's, handed to the shim as its option, which adds them to
    /// the skills opencode discovers for this process ([`Harness::skills`]).
    ///
    /// It cannot report `SessionEnd`: quitting opencode fires no event and runs no exit
    /// handler in a plugin (measured). The app sees the process end.
    fn arm_under(
        &self,
        kit: Kit<'_>,
        _cwd: Option<&std::path::Path>,
        _plugins: &crate::harness_plugin::Chosen,
        sandbox: Sandbox<'_>,
    ) -> StateHooks {
        let Some(shim) = kit
            .plugin
            .map(|plugin| plugin.join(crate::opencode::SHIM_IN_BUNDLE))
            .filter(|shim| shim.is_file())
        else {
            return StateHooks::None;
        };
        StateHooks::ThisSessionOnly {
            args: Vec::new(),
            env: vec![
                (
                    crate::plugin::BINARY_ENV.to_owned(),
                    kit.binary.display().to_string(),
                ),
                (
                    crate::opencode::CONFIG_ENV.to_owned(),
                    crate::opencode::session_config(
                        &shim,
                        kit.plugin.and_then(crate::skills::in_bundle).as_deref(),
                        Some(kit.binary),
                        sandbox.applied().is_some(),
                    ),
                ),
                (crate::opencode::PURE_ENV.to_owned(), "0".to_owned()),
            ],
            cannot_report: vec!["sessionend"],
        }
    }

    /// `OPENCODE_PURE=1`, or the flag that does the same, loads no plugin at all, so a chat
    /// started with either is refused before it starts ([`crate::opencode::disarmed_by`]).
    fn disarmed_by(&self, command: &[String], env: &[(String, String)]) -> Option<String> {
        crate::opencode::disarmed_by(command, env)
    }

    /// **Measured on opencode 1.18.33** (FM-9), from its input's own `@` completion, driven in
    /// a terminal and read in its bundle: a file is `@src/main.rs`, a folder `@src/`, and lines
    /// `@src/main.rs#10` or `@src/main.rs#10-20` — a `#` and no `L`. A typed mention ends at
    /// white space and splits at `#`, so a path holding either is handed over in plain words.
    /// charter does not type into opencode yet (ADR 0061); this is what the clipboard is given.
    fn reference(&self, reference: &crate::reference::Reference) -> String {
        let path = reference.path();
        if path.contains('#') || path.chars().any(char::is_whitespace) {
            return reference.plain();
        }
        format!(
            "@{}{}",
            reference.path_with_slash(),
            reference.lines_after("#", "")
        )
    }

    fn armed_with(&self) -> String {
        "the app arms each opencode chat with charter's opencode plugin, for that chat alone"
            .to_owned()
    }

    fn plugins(&self) -> &'static dyn crate::harness_plugin::Adapter {
        &crate::harness_plugin::OPENCODE
    }

    /// charter's own wrap around the whole harness, which has no sandbox of its own
    /// ([`crate::sandbox::opencode`] has the measurements).
    fn sandbox_compiler(&self) -> Option<crate::sandbox::Compiler> {
        Some(|compiled| crate::sandbox::opencode::wrap(compiled).map(Form::Opencode))
    }

    /// The wrap's program first, with the profile written for where the chat opens, then the
    /// chat's whole line. Its traffic is pointed at charter's egress proxy, the only place the
    /// profile lets it connect, and its temp directory is its own.
    ///
    /// **Fail closed.** No directory, no proxy, or a path a profile cannot state refuses the
    /// chat. Nothing in opencode's own words can widen an operating-system profile, so none is
    /// refused for them.
    fn sandboxed_line(
        &self,
        form: &Form,
        words: crate::sandbox::Words,
        at: &crate::sandbox::At<'_>,
    ) -> Result<crate::sandbox::Line, String> {
        let Form::Opencode(wrap) = form else {
            return Err(super::adapter::not_compiled_for(Harness::Opencode));
        };
        let lead = "this plane runs every chat sandboxed, and";
        let Some(cwd) = at.cwd else {
            return Err(format!(
                "{lead} an opencode chat with no directory of its own has nowhere the sandbox \
                 lets it write, so nothing was started."
            ));
        };
        let Some(confinement) = at.confinement else {
            return Err(format!(
                "{lead} charter's egress proxy was not started for this opencode chat, so \
                 nothing was started."
            ));
        };
        crate::sandbox::opencode::prepare(wrap);
        let profile = crate::sandbox::opencode::profile(
            wrap,
            cwd,
            confinement.tmp(),
            confinement.proxy_port(),
            at.hook_socket,
        )
        .map_err(|why| format!("{lead} {why}, so nothing was started."))?;
        let proxy = confinement.proxy_url();
        let mut env: Vec<(String, String)> = crate::sandbox::opencode::PROXY_ENV
            .iter()
            .map(|key| ((*key).to_owned(), proxy.clone()))
            .collect();
        env.extend(
            crate::sandbox::opencode::NO_PROXY_ENV
                .iter()
                .map(|key| ((*key).to_owned(), String::new())),
        );
        env.push(("TMPDIR".to_owned(), confinement.tmp().display().to_string()));
        env.push((
            crate::sandbox::opencode::STATE_ENV.to_owned(),
            confinement.tmp().join("state").display().to_string(),
        ));
        let crate::sandbox::Words {
            program,
            command,
            armed,
            charters,
        } = words;
        Ok(crate::sandbox::Line {
            program: crate::sandbox::backend::SANDBOX_EXEC.to_owned(),
            args: [
                vec!["-p".to_owned(), profile, program],
                command,
                armed,
                charters,
            ]
            .concat(),
            env,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_is_written_as_opencodes_own_mention() {
        use crate::reference::{Lines, Reference};
        let r = |path, lines, folder| ADAPTER.reference(&Reference::for_tests(path, lines, folder));
        assert_eq!(r("src/main.rs", None, false), "@src/main.rs");
        assert_eq!(r("src", None, true), "@src/");
        assert_eq!(
            r(
                "src/main.rs",
                Some(Lines {
                    first: 10,
                    last: 20
                }),
                false
            ),
            "@src/main.rs#10-20"
        );
        assert_eq!(
            r("src/main.rs", Some(Lines::one(7)), false),
            "@src/main.rs#7"
        );
        assert_eq!(
            r("my dir/note one.txt", None, false),
            "\"my dir/note one.txt\""
        );
    }

    /// opencode reads `!…` as a shell command and `/…` as one of its own: the clipboard is
    /// never handed a reference beginning with either, its plain-words fallback included.
    #[test]
    fn a_reference_never_begins_as_a_shell_or_slash_command() {
        use crate::reference::{Reference, starts_safely};
        let r = |path| ADAPTER.reference(&Reference::for_tests(path, None, false));
        assert_eq!(r("!cmd"), "@./!cmd");
        assert_eq!(r("!a b"), "\"./!a b\"");
        for path in ["!x", "!x#y", "/abs/x", "/abs/a b", "#x"] {
            assert!(starts_safely(&r(path)), "{path} -> {}", r(path));
        }
    }

    #[test]
    fn an_opencode_chat_is_handed_charter_s_mcp_server_in_its_session_config() {
        // HP-7: opencode merges the config in OPENCODE_CONFIG_CONTENT key by key, so the server
        // sits beside the operator's own and nothing is written.
        let bundle = tempfile::tempdir().expect("a bundle");
        let shim = bundle.path().join(crate::opencode::SHIM_IN_BUNDLE);
        std::fs::create_dir_all(shim.parent().expect("a parent")).expect("dirs");
        std::fs::write(&shim, "").expect("the shim");
        let StateHooks::ThisSessionOnly { env, .. } = ADAPTER.arm(
            Kit {
                binary: std::path::Path::new("/bin/charter"),
                plugin: Some(bundle.path()),
            },
            None,
            &crate::harness_plugin::Chosen::new(),
            None,
        ) else {
            panic!("armed per session");
        };
        let config = env
            .iter()
            .find(|(name, _)| name == crate::opencode::CONFIG_ENV)
            .map(|(_, value)| value)
            .expect("the session config");
        let config: serde_json::Value = serde_json::from_str(config).expect("JSON");
        assert_eq!(
            config["mcp"]["charter"],
            serde_json::json!({"type": "local", "command": ["/bin/charter", "mcp"], "enabled": true})
        );
        assert!(config["plugin"].is_array(), "{config}");
    }
}
