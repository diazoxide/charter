//! A level-3 chat's start, from the start a terminal chat makes (ADR 0080 §1, HP-2).
//!
//! **One start, two levels.** [`crate::start::ready`] is the one gate every chat passes: the
//! profile, the persona, the approval, and the one sandbox decision (D-87h). A chat that would
//! run at level 3 passes it as a terminal chat does, and only then does
//! [`Launch::for_start`] say what runs as its ACP agent, in what environment, or why it does
//! not run over ACP.
//!
//! - **The agent is the profile's program in its ACP mode**: the profile's whole command, its
//!   own words first as for a wrapper (ADR 0022), then the harness's ACP words
//!   ([`crate::harness::Harness::acp_args`]). charter's session words are not passed: a level-3
//!   session is opened and loaded over the protocol, not on the command line.
//! - **Its environment is the chat's** ([`crate::chatenv::compose`]), composed as a terminal
//!   chat's is, then the variables the host adds for the chat itself (its number, its token,
//!   its socket). Nothing of the host's own environment reaches it otherwise.
//! - **Level 2 is not armed alongside it.** What arms the harness's hooks for a terminal chat
//!   ([`crate::harness::Harness::state_hooks`]) is not applied: the protocol reports the turn,
//!   and charter's MCP server is handed over `session/new` ([`crate::chattools::acp_server`]).

use std::ffi::OsString;
use std::path::PathBuf;

use super::{HANDSHAKE_PATIENCE, Launch};
use crate::start::Ready;

/// What the host adds to a chat's start to run it at level 3.
#[derive(Debug, Clone)]
pub struct Host<'a> {
    /// The chat, as its asks are keyed.
    pub chat: String,
    /// The host's own environment, of which the chat keeps what [`crate::chatenv`] names.
    pub app_env: Vec<(OsString, OsString)>,
    /// The operator's own additions, from the chat's plane (`[chat_env] pass`).
    pub operator: &'a [String],
    /// Every identity variable a vault of the plane declares, kept out of the agent.
    pub strip: &'a [String],
    /// What the host sets for the chat itself, after everything else: its number, its token,
    /// its hook socket.
    pub own: Vec<(String, String)>,
    /// charter's git hooks (SQ-16, ADR 0074).
    pub git_hooks: Option<&'a crate::githooks::GitHooks>,
    /// The `charter` binary whose MCP server the session is handed.
    pub charter: Option<PathBuf>,
}

/// Why a chat that passed its start does not run over ACP. Each says so in a sentence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotOffered {
    /// charter runs no ACP agent for this harness: Claude Code and Codex wait on HP-4 and HP-3,
    /// and a declared harness on HP-14.
    #[error("charter runs no ACP agent for a {0} chat yet, so it starts in its terminal")]
    NoAgent(String),
    /// ADR 0067, amended: no generated profile can wrap a level-3 agent on this system.
    #[error(
        "a chat cannot run over ACP on this system until charter can sandbox it here, so it starts in its terminal, where the sandbox's state is shown"
    )]
    Unsupported,
    /// D-87h: the project turns the sandbox on, and charter cannot wrap a level-3 agent yet.
    #[error(
        "this project turns the sandbox on, and charter cannot sandbox a chat over ACP yet, so nothing was started. Start it without the sandbox to run it over ACP unconfined."
    )]
    Sandboxed,
}

impl NotOffered {
    /// Whether the chat starts at its next level instead, in its terminal, and says why (ADR
    /// 0080 §3). A sandboxed start does not: it is refused, and only the audited opt-out is
    /// offered (D-87h), so a level-3 start never becomes a different chat without a person.
    pub fn starts_in_its_terminal(&self) -> bool {
        !matches!(self, Self::Sandboxed)
    }
}

impl Launch {
    /// The ACP agent `ready`'s chat runs at level 3, with the host's part from `host`, or why
    /// it does not run over ACP.
    pub fn for_start(ready: &Ready, host: Host<'_>) -> Result<Self, NotOffered> {
        let words = match ready.harness {
            Some(harness) => harness
                .acp_args()
                .ok_or_else(|| NotOffered::NoAgent(harness.name().to_owned()))?,
            None => return Err(NotOffered::NoAgent("declared harness".to_owned())),
        };
        if !cfg!(unix) {
            return Err(NotOffered::Unsupported);
        }
        // The start's own sandbox decision, read and never made again: a chat it confined is
        // one a level-3 agent would run outside of.
        if ready.sandbox.is_some() {
            return Err(NotOffered::Sandboxed);
        }
        let mut argv = vec![ready.program.clone()];
        argv.extend(ready.command.iter().cloned());
        argv.extend(words.iter().map(|word| (*word).to_owned()));
        let mut env = crate::chatenv::compose(
            host.app_env,
            &crate::chatenv::Starting {
                harness: ready.harness,
                operator: host.operator,
                strip: host.strip,
                set: &ready.env,
                git_hooks: host.git_hooks,
            },
        );
        env.extend(
            host.own
                .into_iter()
                .map(|(name, value)| (name.into(), value.into())),
        );
        Ok(Self {
            chat: host.chat,
            argv,
            cwd: ready.cwd.clone().unwrap_or_default(),
            env,
            charter_mcp: host.charter,
            patience: HANDSHAKE_PATIENCE,
        })
    }
}
