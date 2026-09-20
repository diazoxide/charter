//! The repo commands: `discover`, `clone` and `sync`.
//!
//! A port of `charter/commands.py`'s `cmd_discover`, `cmd_clone` and `cmd_sync`, and what they
//! print. Each speaks through a [`Say`] sink rather than printing, so the binary prints a line
//! the moment it is known — a clone is slow, and "Querying github org `acme` …" is worth
//! nothing after the fact — while a test reads the same lines back as values.
//!
//! # Where this is stricter than Python, on purpose
//!
//! Each is recorded where it happens and in the differential harness:
//!
//! - **A clone checks out the remote's own default branch.** Python passes `--branch
//!   <default_branch>` from the inventory, which `discover` fills with `"main"` whenever the
//!   forge named none — and which is stale the day the default changes. The remote's HEAD is
//!   the real answer, so no branch is named at all.
//! - **A clone or a fetch never goes over SSH.** Python's clone of an HTTPS URL went over SSH
//!   whenever the operator's git config rewrote it that way; here git refuses the transport
//!   ([`crate::worktree::git::run_network`]) and the refusal says why.
//! - **`sync` never moves a tree that is mid-operation or detached, and never overwrites an
//!   ignored file.** Python fast-forwarded a detached HEAD to `origin/HEAD` and let the merge
//!   overwrite ignored files, both of which move or lose work the operator did not commit.
//! - **A repo name is a name**: letters, digits, `.`, `_`, `-`, starting with a letter or a
//!   digit — the rule `repos::clones` already reads a workspace by. Python took any single
//!   path segment, so `-rf` and `.github` cloned there and are refused here.

use std::fmt;

pub mod clone;
pub mod discover;
mod submodules;
pub mod sync;

/// One line of what a command says, in charter's four voices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Say {
    /// `• …` — information.
    Info(String),
    /// `✓ …` — something was done.
    Done(String),
    /// `! …` — something was skipped or is wrong, and nothing was lost.
    Warn(String),
    /// `✗ …` — something failed.
    Fail(String),
    /// A refusal with no mark: Python's `raise SystemExit(message)`, which prints the
    /// message as it is.
    Plain(String),
}

impl fmt::Display for Say {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Say::Info(s) => write!(f, "• {s}"),
            Say::Done(s) => write!(f, "✓ {s}"),
            Say::Warn(s) => write!(f, "! {s}"),
            Say::Fail(s) => write!(f, "✗ {s}"),
            Say::Plain(s) => write!(f, "{s}"),
        }
    }
}

/// Where a command's lines go.
pub type Sink<'a> = &'a mut dyn FnMut(Say);

/// `workspace: <ws>  (via --workspace)` — the banner every workspace command opens with.
///
/// Always `--workspace`: the Rust binary takes the workspace only from `-w` (see its
/// module docs), so that is the one rung that can have chosen it.
pub fn banner(ws: &str, say: Sink) {
    say(Say::Info(format!("workspace: {ws}  (via --workspace)")));
}
