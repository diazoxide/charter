//! A harness adapter (ADR 0073, FD-13): charter code that arms one harness through the
//! harness's own mechanism, for one chat, with nothing written into the harness's config.
//!
//! **Behaviour, not facts.** ADR 0073 §3 splits what charter knows about a harness in two. Its
//! facts (the flags that start and resume a session, the bytes a newline is, what it reports)
//! become a harness declaration's fields in FD-14, and stay [`super::Harness`]'s methods until
//! then. What is behaviour stays charter code, here: how a chat is armed so that its hooks report
//! in charter's words ([`crate::state::Event::said`] reads them), what would run it unarmed,
//! and which of its own plugins it can be handed.
//!
//! One adapter per harness, and [`super::Harness::adapter`] is their registry:
//! [`super::claude::ADAPTER`], [`super::codex::ADAPTER`] and [`super::opencode::ADAPTER`]. An
//! adapter also compiles a plane's sandbox for its harness, where charter has written that
//! compiler (ADR 0067): [`HarnessAdapter::sandbox_compiler`] and
//! [`HarnessAdapter::sandboxed_line`].

use super::{Harness, Kit, StateHooks};

/// One harness, as charter arms it.
pub trait HarnessAdapter: Sync {
    /// The harness this adapter arms.
    fn harness(&self) -> Harness;

    /// How a chat that will run in `cwd` is armed with what the app ships in `kit`, handed
    /// `plugins` (the harness's own plugins the project chose for this chat) and `sandbox`.
    ///
    /// **Fail closed** (ADR 0067). A sandbox compiled for another harness arms nothing, so the
    /// chat is refused rather than started without it. The check is here, in the one way into
    /// an adapter's arming, and not left to a caller: [`Self::arm_under`] can only be handed a
    /// [`Sandbox`], which nothing outside this module can make.
    fn arm(
        &self,
        kit: Kit<'_>,
        cwd: Option<&std::path::Path>,
        plugins: &crate::harness_plugin::Chosen,
        sandbox: Option<&crate::sandbox::Applied>,
    ) -> StateHooks {
        if sandbox.is_some_and(|applied| applied.harness() != self.harness()) {
            return StateHooks::None;
        }
        self.arm_under(kit, cwd, plugins, Sandbox(sandbox))
    }

    /// [`Self::arm`], once `sandbox` is known to be compiled for this harness. Each adapter
    /// writes this; every caller calls [`Self::arm`].
    fn arm_under(
        &self,
        kit: Kit<'_>,
        cwd: Option<&std::path::Path>,
        plugins: &crate::harness_plugin::Chosen,
        sandbox: Sandbox<'_>,
    ) -> StateHooks;

    /// Why a chat started with `command` and `env` would run without the hooks [`Self::arm`]
    /// gave it, or `None` when it would not. The default is a harness with no such switch.
    fn disarmed_by(&self, command: &[String], env: &[(String, String)]) -> Option<String> {
        let _ = (command, env);
        None
    }

    /// This harness's sandbox compiler (ADR 0067), or `None` where charter has not written one
    /// yet, which refuses every chat of it in a sandboxed plane
    /// ([`crate::sandbox::NotStarted::NoCompiler`]).
    fn sandbox_compiler(&self) -> Option<crate::sandbox::Compiler>;

    /// The chat's whole line under `form`, the sandbox [`Self::sandbox_compiler`] compiled: the
    /// profile's `command`, then `armed` (what [`Self::arm`] gave), then `charters`, charter's
    /// own words — or the one sentence saying why it may not start. Only
    /// [`crate::sandbox::Applied::line`] asks, with the form compiled for this harness.
    ///
    /// **Fail closed.** A form this adapter did not compile, and a flag in the chat's own words
    /// that would outrank the sandbox, each refuse the chat.
    fn sandboxed_line(
        &self,
        form: &crate::sandbox::Form,
        command: Vec<String>,
        armed: Vec<String>,
        charters: Vec<String>,
    ) -> Result<Vec<String>, String>;

    /// What the app arms a chat of this harness with, as `charter doctor` says it.
    fn armed_with(&self) -> String;

    /// The harness's own plugins: what it has installed and whether a chat can be handed a
    /// set of them (ADR 0050).
    fn plugins(&self) -> &'static dyn crate::harness_plugin::Adapter;
}

/// A chat's sandbox, checked to be compiled for the adapter it is handed to, or none. Only
/// [`HarnessAdapter::arm`] makes one, so an adapter's own arming cannot be reached with a
/// sandbox of another harness.
#[derive(Debug, Clone, Copy)]
pub struct Sandbox<'a>(Option<&'a crate::sandbox::Applied>);

impl<'a> Sandbox<'a> {
    /// The sandbox, compiled for this adapter's harness, or `None` where the plane has none.
    pub fn applied(self) -> Option<&'a crate::sandbox::Applied> {
        self.0
    }
}

/// Why a chat handed a sandbox form its adapter did not compile is not started: the sentence
/// [`HarnessAdapter::sandboxed_line`] refuses it with.
pub(super) fn not_compiled_for(harness: Harness) -> String {
    format!(
        "the sandbox this {} chat was handed was not compiled for it, so nothing was started",
        harness.title()
    )
}
