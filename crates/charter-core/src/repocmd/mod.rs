//! The repo commands: `discover`, `clone`, `sync`, `status` and `docs generate`.
//!
//! A port of `charter/commands.py`'s `cmd_discover`, `cmd_clone`, `cmd_sync`, `cmd_status` and
//! `cmd_docs`, and what they print. Each speaks through a [`Say`] sink rather than printing, so
//! the binary prints a line the moment it is known — a clone is slow, and "Querying github org
//! `acme` …" is worth nothing after the fact — while a test reads the same lines back as
//! values. `status` has a second sink for stdout, because its table IS its answer rather than a
//! commentary on one.
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
//! - **`status` draws only the clones `repos::clones` will stand behind.** A clone whose
//!   name is not a name, or whose `.git` is a symlink, is a row in Python's table and a
//!   REFUSAL here — said on stderr, never dropped, because a row that quietly disappears
//!   reads as "this workspace has one fewer repo".
//! - **`docs generate` gates the two files it writes.** Python calls `write_text` on
//!   `docs/topology.md` and `README.md` with no containment check at all, so a plane
//!   carrying a committed `docs` or `README.md` symlink writes charter's generated content
//!   through it, outside the plane. Here each is gated as itself and the refusal is said.

use std::fmt;

pub mod clone;
pub mod discover;
pub mod docs;
pub mod status;
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
    /// A line of the command's ANSWER, on **stdout** and with no mark — Python's bare
    /// `print(…)`.
    ///
    /// Apart from `Plain`, which is an unmarked line on stderr, because the stream is the
    /// difference that matters: `charter persona default` prints the persona's name for a
    /// script to read and says everything else on stderr, and a sink that sent both to one
    /// place would make the name unreadable without the prose around it.
    Out(String),
}

impl fmt::Display for Say {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Say::Info(s) => write!(f, "• {s}"),
            Say::Done(s) => write!(f, "✓ {s}"),
            Say::Warn(s) => write!(f, "! {s}"),
            Say::Fail(s) => write!(f, "✗ {s}"),
            Say::Plain(s) | Say::Out(s) => write!(f, "{s}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_workspace_command_opens_by_naming_the_workspace_and_how_it_was_chosen() {
        // `charter/workspace.py:banner`, `util.info(f"workspace: {active}  (via {source})")`,
        // with `source` answering `--workspace` for an explicit `-w` — the only rung the Rust
        // binary takes a workspace from.
        let mut said = Vec::new();
        banner("alpha", &mut |line| said.push(line));

        assert_eq!(
            said,
            [Say::Info("workspace: alpha  (via --workspace)".into())]
        );
        assert_eq!(said[0].to_string(), "• workspace: alpha  (via --workspace)");
    }
}
