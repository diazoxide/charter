//! `charter sync`: fetch and fast-forward every clone in a workspace. Python's `cmd_sync`.
//!
//! # It never loses work
//!
//! A clone is moved only when all of these hold, and each one that does not is a sentence
//! and a skip — never a force, never a stash, never a reset:
//!
//! - git could read the tree (**unreadable is not clean**: an empty `status` is what would
//!   otherwise authorise the merge);
//! - nothing is changed or untracked in it;
//! - no rebase, merge, cherry-pick, revert or bisect is in progress — Python did not ask,
//!   and a rebase stopped on a clean tree sits on a detached HEAD it then moved;
//! - HEAD is on a branch — Python fast-forwarded a detached HEAD to `origin/HEAD`;
//! - `origin` is not an SSH remote for a host charter does not manage;
//! - the fetch worked;
//! - the merge is a fast-forward (`--ff-only`) **that overwrites no ignored file**
//!   (`--no-overwrite-ignore`). git treats an ignored file as expendable by default, so an
//!   upstream commit that starts tracking `config.local` replaced the operator's own copy
//!   of it under Python's sync, with a tick printed over it.

use std::path::Path;

use super::{Say, Sink, banner, submodules};
use crate::forge;
use crate::repos::{self, Head};
use crate::workspaces::Plane;
use crate::worktree::git;

/// Which workspaces to sync.
pub enum Scope<'a> {
    One(&'a str),
    All,
}

/// Run `sync`. Always exits 0, as Python's does: a skipped clone is a sentence, not a failure.
pub fn sync(root: &Path, scope: Scope, say: Sink) -> u8 {
    let (names, label) = match scope {
        Scope::One(ws) => {
            banner(ws, say);
            if !crate::contain::workspace_name_ok(ws) {
                say(Say::Fail(format!("no workspace '{ws}'")));
                return 1;
            }
            (vec![ws.to_string()], format!("workspace '{ws}'"))
        }
        Scope::All => {
            let names = Plane::open(root).workspaces().unwrap_or_default();
            let label = format!("all workspaces ({})", names.len());
            (names, label)
        }
    };
    let mut targets: Vec<(String, repos::Repo)> = Vec::new();
    for ws in &names {
        let Ok(found) = repos::clones(root, ws) else {
            continue;
        };
        for (name, why) in found.refused {
            say(Say::Warn(format!("{ws}/{name}: skipping — {why}")));
        }
        targets.extend(found.repos.into_iter().map(|r| (ws.clone(), r)));
    }
    if targets.is_empty() {
        say(Say::Warn(format!("No cloned repos to sync in {label}.")));
        return 0;
    }
    for (ws, repo) in &targets {
        sync_one(root, ws, repo, say);
    }
    0
}

/// An operation git has stopped in the middle of, by the marker it leaves in the git
/// directory.
fn in_progress(git_dir: &Path) -> Option<&'static str> {
    [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
        ("BISECT_LOG", "bisect"),
    ]
    .into_iter()
    .find(|(marker, _)| std::fs::symlink_metadata(git_dir.join(marker)).is_ok())
    .map(|(_, what)| what)
}

/// Whether a remote URL goes over SSH: `ssh://…`, or scp-style `host:path`.
fn is_ssh(url: &str) -> bool {
    if url.starts_with("ssh://") || url.starts_with("git+ssh://") || url.starts_with("ssh+git://") {
        return true;
    }
    !url.contains("://") && !forge::host_of(url).is_empty()
}

fn sync_one(root: &Path, ws: &str, repo: &repos::Repo, say: Sink) {
    let label = format!("{ws}/{}", repo.name);
    let d = &repo.path;
    let state = match repos::state_of(d) {
        Ok(state) => state,
        Err(unreadable) => {
            say(Say::Warn(format!("{label}: skipping — {unreadable}.")));
            return;
        }
    };
    if !state.clean() {
        say(Say::Warn(format!(
            "{label}: uncommitted changes — skipping (your work is left untouched)."
        )));
        // A submodule left behind the commit the branch records IS an unstaged change to the
        // gitlink, so the explanation of this skip may be one `git submodule status` away.
        submodules::report(root, d, &label, None, None, say);
        return;
    }
    // `repos::clones` admits a `.git` that is a directory and nothing else, so this is the
    // git directory itself.
    if let Some(what) = in_progress(&d.join(".git")) {
        say(Say::Warn(format!(
            "{label}: a {what} is in progress — skipping (finish or abort it first; nothing \
             was fetched or moved)."
        )));
        return;
    }
    let branch = match &state.head {
        Head::Branch(b) | Head::Unborn(b) => b.clone(),
        Head::Detached(at) => {
            let at = if at.is_empty() {
                String::new()
            } else {
                format!(" at {at}")
            };
            say(Say::Warn(format!(
                "{label}: HEAD is detached{at} — not on a branch, so there is nothing to \
                 fast-forward; left as-is."
            )));
            return;
        }
    };
    let origin = git::run(d, &["remote", "get-url", "origin"], git::READ)
        .ok()
        .filter(|r| r.ok())
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    let managed = forge::resolve_host(&origin, root);
    if managed.is_none() && is_ssh(&origin) {
        say(Say::Warn(format!(
            "{label}: origin is an SSH remote on a host this plane does not manage — charter \
             fetches over HTTPS with a forge CLI's token only (docs/git-policy.md); skipping."
        )));
        return;
    }
    // The forge's own helper for a host the plane manages, and NO credential for one it does
    // not: a token for one forge is never offered to another.
    let helper = managed.as_ref().map(forge::helper_for);
    let fetched = git::run_network(
        d,
        helper.as_deref(),
        &["fetch", "--prune", "--no-recurse-submodules"],
    );
    if !fetched.is_ok_and(|r| r.ok()) {
        say(Say::Fail(format!(
            "{label}: fetch failed (access or network) — skipping."
        )));
        return;
    }
    let upstream = format!("origin/{branch}");
    // Untimed: a merge checks out a tree, and a killed one leaves it half-written.
    let merged = git::run_untimed(
        d,
        &["merge", "--ff-only", "--no-overwrite-ignore", &upstream],
    );
    match merged {
        Ok(run) if run.ok() => {}
        Ok(run) if run.err.contains("would be overwritten") => {
            say(Say::Warn(format!(
                "{label}: {branch} was not fast-forwarded — it would overwrite files in the \
                 tree that git does not track or that are ignored; left as-is."
            )));
            return;
        }
        _ => {
            say(Say::Warn(format!(
                "{label}: {branch} won't fast-forward (diverged/local commits) — left as-is."
            )));
            return;
        }
    }
    // The tick is not printed over a tree the fast-forward left behind (charter #817): a
    // fast-forward moves the gitlink and never the submodule's own checkout.
    if submodules::report(
        root,
        d,
        &label,
        Some(&branch),
        Some(&format!("{branch} is up to date, its submodules are not")),
        say,
    ) {
        return;
    }
    say(Say::Done(format!("{label}: up to date on {branch}")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ssh_remote_is_told_apart_from_an_https_one_and_a_local_path() {
        assert!(is_ssh("git@github.com:acme/x.git"));
        assert!(is_ssh("ssh://git@host/acme/x.git"));
        assert!(!is_ssh("https://github.com/acme/x.git"));
        assert!(!is_ssh("file:///srv/x.git"));
        assert!(!is_ssh("/srv/x.git"));
    }
}
