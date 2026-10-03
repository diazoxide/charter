//! opencode's adapter (ADR 0073, FD-13): charter's shim, loaded for one session by naming it in
//! the whole config opencode reads from `OPENCODE_CONFIG_CONTENT` ([`crate::opencode`] has the
//! measurements).

use super::adapter::{HarnessAdapter, Sandbox};
use super::{Harness, Kit, StateHooks};

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
        _sandbox: Sandbox<'_>,
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

    fn armed_with(&self) -> String {
        "the app arms each opencode chat with charter's opencode plugin, for that chat alone"
            .to_owned()
    }

    fn plugins(&self) -> &'static dyn crate::harness_plugin::Adapter {
        &crate::harness_plugin::OPENCODE
    }

    /// None yet (SD-2, #695), so a chat of opencode in a sandboxed plane is refused.
    fn sandbox_compiler(&self) -> Option<crate::sandbox::Compiler> {
        None
    }

    /// Never asked: with no compiler there is no form compiled for opencode, so any form is
    /// another harness's.
    fn sandboxed_line(
        &self,
        _form: &crate::sandbox::Form,
        _command: Vec<String>,
        _armed: Vec<String>,
        _charters: Vec<String>,
    ) -> Result<Vec<String>, String> {
        Err(super::adapter::not_compiled_for(Harness::Opencode))
    }
}
