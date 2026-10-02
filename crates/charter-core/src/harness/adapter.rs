//! A harness adapter (ADR 0073, FD-13): charter code that arms one harness through the
//! harness's own mechanism, for one chat, with nothing written into the harness's config.
//!
//! **Behaviour, not facts.** ADR 0073 §3 splits what charter knows about a harness in two. Its
//! facts (the flags that start and resume a session, the bytes a newline is, what it reports)
//! become a harness declaration's fields in FD-14, and stay [`super::Harness`]'s methods until
//! then. What is behaviour stays charter code, here: how a chat is armed so that its hooks report
//! in charter's words ([`super::model::Said::of_hook`] reads them), what would run it unarmed,
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
    /// `plugins` (the harness's own plugins the project chose for this chat) and `sandbox`,
    /// which is already known to be compiled for this harness.
    fn arm(
        &self,
        kit: Kit<'_>,
        cwd: Option<&std::path::Path>,
        plugins: &crate::harness_plugin::Chosen,
        sandbox: Option<&crate::sandbox::Applied>,
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
