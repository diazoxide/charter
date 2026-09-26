//! `charter change` — the command line over [`charter_core::change::cmd`] (ADR 0060).
//!
//! Every member is named by hand. There is no `--all` and no pattern, and
//! `tests/change.rs` asserts it.

use charter_core::change::cmd;
use clap::Subcommand;

use crate::Here;

#[derive(Subcommand)]
pub enum ChangeCommand {
    /// Create a change: a name and the reason for it.
    Create {
        /// The change's slug: also its default branch name, and the `Charter-Change:` trailer
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
            &ws(workspace.as_deref()),
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
            &ws(workspace.as_deref()),
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
            &ws(workspace.as_deref()),
            &change,
            &repo,
            why.as_deref(),
            now,
            &mut say,
        ),
        ChangeCommand::List { workspace } => cmd::list(&root, &ws(workspace.as_deref()), &mut say),
        ChangeCommand::Show { change, workspace } => {
            cmd::show(&root, &ws(workspace.as_deref()), &change, &mut say)
        }
        ChangeCommand::Forget { change, workspace } => {
            cmd::forget(&root, &ws(workspace.as_deref()), &change, &mut say)
        }
    };
    Ok(code)
}
