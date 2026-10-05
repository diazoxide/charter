//! `charter change` — the command line over [`charter_core::change::cmd`],
//! [`charter_core::change::push`], [`charter_core::change::land`] and
//! [`charter_core::change::revert`] (ADR 0060).
//!
//! Every member is named by hand. There is no `--all` and no pattern, and
//! `tests/change.rs` asserts it.

use charter_core::change::{cmd, land};
use clap::Subcommand;

use crate::Here;

#[derive(Subcommand)]
pub enum ChangeCommand {
    /// Create a change: a name and the reason for it.
    Create {
        /// The change's slug: also its default branch name, and the `Purlis-Change:` trailer
        /// on every landing commit.
        change: String,
        /// One line: what this work is for. Required.
        #[arg(long, required = true)]
        why: Option<String>,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Add one repo to the change, by literal name. Exit 2 when it has no clone here.
    Add {
        change: String,
        /// A repo already cloned in this workspace. One name; there is no pattern.
        repo: String,
        /// This member's branch (default: change/<slug>). Stored in the record: git knows a
        /// branch exists, it cannot know the branch is this change's.
        #[arg(long)]
        branch: Option<String>,
        /// A member that must LAND before this one. Repeatable.
        #[arg(long, value_name = "REPO")]
        needs: Vec<String>,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Take a repo out of the change (or record one that was never in it), with the reason.
    Drop {
        change: String,
        repo: String,
        /// One line: why this repo is out. Required.
        #[arg(long, required = true)]
        why: Option<String>,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// The workspace's changes, one row each.
    List {
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// One change whole: why, members, branches, blockers, exclusions.
    Show {
        change: String,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Push every member's branch and open or update its pull or merge request, each carrying
    /// the change's cross-link block. Prints every repo, branch and destination first. Commits
    /// nothing, never forces, and pushes a repo whose save mode is `off` too. A member that is
    /// not a repo in this workspace is refused by name, the others are still pushed, and the
    /// exit is 1.
    Push {
        change: String,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Land one member: merge its request at the head commit its checks passed on, once every
    /// member it needs has landed, through the target branch's merge queue or merge train where
    /// it has one. Refused, each by name: a blocker not landed, checks not PASSED at that head,
    /// a head that moved since the check, and more than one member. The landing is recorded in
    /// the landing log once the forge confirms it. Attended only: a run nobody is watching is
    /// refused by charter's floor.
    Land {
        change: String,
        /// The one member to land. Name it once: one member per run.
        #[arg(long, value_name = "NAME", action = clap::ArgAction::Append)]
        repo: Vec<String>,
        /// Land as one squashed commit instead of a merge commit.
        #[arg(long, conflicts_with = "rebase")]
        squash: bool,
        /// Refused, with the reason: a rebase landing leaves charter no commit to carry the
        /// trailer and no single commit to revert.
        #[arg(long)]
        rebase: bool,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Seed a new change, revert-<change>, that reverts every member charter landed: in each
    /// member's clone, a branch off the default branch carrying `git revert` of the commit the
    /// landing log names. Pushes nothing and merges nothing: push and land it like any other
    /// change. A member with no landing record is named as a person's to revert, and a logged
    /// commit the default branch no longer holds is refused by name. Run it again after a
    /// refusal is put right: it seeds only the members it has not seeded yet.
    Revert {
        change: String,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Delete the change record. Branches, requests and the landing log are untouched.
    Forget {
        change: String,
        /// The workspace (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
}

pub fn run(here: &Here, command: ChangeCommand) -> Result<u8, String> {
    let root = here.plane.root().to_path_buf();
    let now = chrono::Utc::now();
    let mut say = crate::speak;
    let ws = |flag: Option<&str>| here.active_workspace(flag);
    let code = match command {
        ChangeCommand::Create {
            change,
            why,
            workspace,
        } => cmd::create(
            &root,
            &ws(workspace.as_deref())?,
            &change,
            why.as_deref(),
            &cmd::author(&root),
            now,
            &mut say,
        ),
        ChangeCommand::Add {
            change,
            repo,
            branch,
            needs,
            workspace,
        } => cmd::add(
            &root,
            &ws(workspace.as_deref())?,
            &change,
            &repo,
            branch.as_deref(),
            &needs,
            &mut say,
        ),
        ChangeCommand::Drop {
            change,
            repo,
            why,
            workspace,
        } => cmd::drop(
            &root,
            &ws(workspace.as_deref())?,
            &change,
            &repo,
            why.as_deref(),
            now,
            &mut say,
        ),
        ChangeCommand::List { workspace } => cmd::list(&root, &ws(workspace.as_deref())?, &mut say),
        ChangeCommand::Show { change, workspace } => {
            cmd::show(&root, &ws(workspace.as_deref())?, &change, now, &mut say)
        }
        ChangeCommand::Push { change, workspace } => {
            charter_core::change::push::push(&root, &ws(workspace.as_deref())?, &change, &mut say)
        }
        ChangeCommand::Land {
            change,
            repo,
            squash,
            rebase,
            workspace,
        } => {
            let how = if rebase {
                land::How::Rebase
            } else if squash {
                land::How::Squash
            } else {
                land::How::Merge
            };
            land::land(
                &root,
                &ws(workspace.as_deref())?,
                &change,
                &repo,
                how,
                now,
                &mut say,
            )
        }
        ChangeCommand::Revert { change, workspace } => charter_core::change::revert::revert(
            &root,
            &ws(workspace.as_deref())?,
            &change,
            &cmd::author(&root),
            now,
            &mut say,
        ),
        ChangeCommand::Forget { change, workspace } => {
            cmd::forget(&root, &ws(workspace.as_deref())?, &change, &mut say)
        }
    };
    Ok(code)
}
