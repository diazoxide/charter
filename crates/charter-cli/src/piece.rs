//! `charter worktree` (alias `wt`) — the command line over [`charter_core::piececmd`].
//!
//! Kept out of `main.rs` on purpose: that file is where every command's variant meets, and
//! this one adds a single line there.

use charter_core::piececmd;
use charter_core::pieces::{Declaration, Who};
use clap::Subcommand;

use crate::Here;

#[derive(Subcommand)]
pub enum WorktreeCommand {
    /// Cut a piece: a worktree of <repo> on a new branch off the clone's HEAD, with the
    /// plane's layer wired into it and its claim recorded. Exit 2 when the piece is already
    /// claimed, so a worker that lost a race knows to take the next name.
    Add {
        /// A repo cloned into the workspace.
        repo: String,
        /// The piece's name, which is also its directory and, by default, its branch.
        piece: String,
        /// Name the new branch something other than the piece.
        #[arg(long)]
        branch: Option<String>,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Declare the piece you are standing in finished. Run from inside it.
    Done,
    /// Declare the piece you are standing in given up, and why. Run from inside it.
    Abandon {
        /// Why you stopped (required) — what whoever picks this up reads first. Taken as
        /// optional only so that a missing one is refused in a sentence, with exit 1.
        #[arg(value_name = "REASON")]
        reason: Option<String>,
    },
    /// The workspace's pieces as git has them, each with what it declared or how long it has
    /// been silent.
    List {
        /// Only this repo's pieces.
        repo: Option<String>,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// What happened to the workspace's pieces, removed ones included — read from the log.
    History {
        /// Only this repo.
        repo: Option<String>,
        /// Only this piece.
        piece: Option<String>,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Remove a piece with `git worktree remove`. Refused, naming what would be lost, while
    /// it holds uncommitted changes or commits no other ref reaches.
    Remove {
        repo: String,
        piece: String,
        /// Discard the uncommitted changes and unique commits the refusal named.
        #[arg(long)]
        force: bool,
        /// Also delete the piece's branch (`git branch -d`, or `-D` with --force).
        #[arg(long)]
        delete_branch: bool,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
}

pub fn run(here: &Here, command: WorktreeCommand) -> Result<u8, String> {
    let root = here.plane.root().to_path_buf();
    let now = chrono::Utc::now();
    let who = Who {
        session: here.ids.session.clone(),
        persona: here.active_persona(None),
        host: charter_core::dispatch::host(),
    };
    let mut say = crate::speak;
    let code = match command {
        WorktreeCommand::Add {
            repo,
            piece,
            branch,
            workspace,
        } => piececmd::add(
            &root,
            &here.active_workspace(workspace.as_deref()),
            &repo,
            &piece,
            branch.as_deref(),
            &who,
            now,
            &mut say,
        ),
        WorktreeCommand::Done => {
            piececmd::declare(&root, &here.cwd, Declaration::Done, &who, now, &mut say)
        }
        WorktreeCommand::Abandon { reason } => piececmd::declare(
            &root,
            &here.cwd,
            Declaration::Abandoned {
                reason: reason.as_deref().unwrap_or_default(),
            },
            &who,
            now,
            &mut say,
        ),
        WorktreeCommand::List { repo, workspace } => piececmd::list(
            &root,
            &here.active_workspace(workspace.as_deref()),
            repo.as_deref(),
            now,
            &mut say,
        ),
        WorktreeCommand::History {
            repo,
            piece,
            workspace,
        } => piececmd::history(
            &root,
            &here.active_workspace(workspace.as_deref()),
            repo.as_deref(),
            piece.as_deref(),
            &mut say,
        ),
        WorktreeCommand::Remove {
            repo,
            piece,
            force,
            delete_branch,
            workspace,
        } => piececmd::remove(
            &root,
            &here.active_workspace(workspace.as_deref()),
            &repo,
            &piece,
            force,
            delete_branch,
            &mut say,
        ),
    };
    Ok(code)
}
