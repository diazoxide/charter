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
//! One adapter per harness, and [`super::Harness`] is their registry: Claude Code's is
//! [`super::claude::ADAPTER`].

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
