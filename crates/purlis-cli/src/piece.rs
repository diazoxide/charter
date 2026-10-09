//! `charter worktree` (alias `wt`) — the command line over [`purlis_core::piececmd`].
//!
//! Kept out of `main.rs` on purpose: that file is where every command's variant meets, and
//! this one adds a single line there.

use clap::Subcommand;
use purlis_core::piececmd;
use purlis_core::pieces::{Declaration, Who};

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
    /// Commit in the branch folder you are standing in, from a sandboxed chat: the app that
    /// started the chat stages what you name and commits it on that folder's branch. It only
    /// commits: it never amends, resets, rebases, merges or pushes, and the repository's own
    /// hooks are not run. A chat that is not sandboxed uses `git commit`.
    Commit {
        /// The commit message.
        #[arg(short = 'm', long = "message", value_name = "MESSAGE")]
        message: Option<String>,
        /// Stage every change to a file git already tracks. A new file must be named.
        #[arg(short = 'a', long = "all")]
        all: bool,
        /// Never done. Taken only so that it is refused in a sentence.
        #[arg(long, hide = true)]
        amend: bool,
        /// Paths to stage, relative to where you stand, inside this folder.
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
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
    /// it holds uncommitted changes or commits no other ref reaches. A branch folder purlis
    /// cut for a task is discarded from that task's Changes tab instead.
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
        host: purlis_core::dispatch::host(),
        log: purlis_core::dispatch::this_log_name(),
    };
    let mut say = crate::speak;
    let code = match command {
        WorktreeCommand::Add {
            repo,
            piece,
            branch,
            workspace,
        } => {
            let ws = here.active_workspace(workspace.as_deref())?;
            // In a chat the app started, the app cuts it (#1335): a sandboxed chat may not
            // write the worktree's git files or the editor settings it checks out.
            let asked = crate::gitask::told(
                crate::gitask::forwarded(
                    &ws,
                    purlis_core::hookwire::GitWork::WorktreeAdd {
                        repo: repo.clone(),
                        piece: piece.clone(),
                        branch: branch.clone(),
                    },
                    crate::gitask::A_WORKTREE_TAKES_AT_MOST,
                ),
                &mut say,
            );
            match asked {
                Some(code) => code,
                None => piececmd::add(
                    &root,
                    &ws,
                    &repo,
                    &piece,
                    branch.as_deref(),
                    &who,
                    now,
                    &mut say,
                ),
            }
        }
        WorktreeCommand::Commit {
            message,
            all,
            amend,
            paths,
        } => crate::gitask::commit(message, all, amend, paths, &mut say),
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
            &here.active_workspace(workspace.as_deref())?,
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
            &here.active_workspace(workspace.as_deref())?,
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
            &here.active_workspace(workspace.as_deref())?,
            &repo,
            &piece,
            force,
            delete_branch,
            &mut say,
        ),
    };
    Ok(code)
}
