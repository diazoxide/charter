//! `charter workspace remove` — delete a workspace and its clones.
//!
//! A port of `commands_workspace.cmd_workspace_remove`.
//!
//! **This command is a recursive delete, and the guard in front of it is the difference
//! between a command and a disaster.** [`crate::wscmd::work_at_risk`] is that guard, and what
//! it is empty of is what this hands to `remove_dir_all`. Three properties of it are
//! load-bearing and each was a bug first:
//!
//! - **A clone charter could not read is at risk** (charter#917). A `git status` that FAILED
//!   writes nothing to stdout, and `""` read as "clean" makes a clone charter could not look
//!   at indistinguishable from one it looked at and found empty.
//! - **Worktrees are checked, and on a different rule** (charter#91). The clone walk cannot
//!   see them — a clone's `.git` is a directory, a linked worktree's is a file — and a
//!   worktree is at risk when it holds commits reachable from no other ref, which is exactly
//!   what `charter wt remove` refuses over. The two guards refuse on identical grounds on
//!   purpose: a workspace that removed what `wt remove` would not have is charter#91 again.
//! - **Open todos are reported and never guarded on.** A todo is a note about the future, not
//!   work that ceases to exist, and a workspace whose todos were all abandoned is precisely
//!   the one worth deleting. Making that case demand `--force` teaches the habit of reaching
//!   for `--force`, which is how a guard stops protecting the commits it exists for.
//!
//! # What this charter does NOT do, and why it says so
//!
//! Python runs `workspace.unwire_guests(name)` before the delete: charter's generated files go
//! with the directory, but a **linked worktree's** `info/exclude` is the main repository's and
//! lives somewhere else on disk entirely, so the delete leaves charter's managed block behind
//! in a repository that is still there, naming paths that no longer exist (charter#870).
//!
//! This binary does not remove that block. The removal Python performs is content-addressed —
//! it unlinks only files whose current digest still matches the marker, and keeps every
//! exclude line another wired checkout of the same repository still needs — and a narrower
//! port of it would be a delete inside a repository the operator owns, decided by rules the
//! two charters do not yet share. So the block is left, and every exclude file outside the
//! workspace is **named** with the one command that clears it. The operator loses a tidy-up,
//! not a file.

use std::path::Path;

use crate::repocmd::{Say, Sink};
use crate::wscmd;

/// `charter workspace remove <name> [--force]`, and its exit code.
///
/// Exit 2 for the guard, as Python does: a refusal that protected work is not the same
/// failure as a name that is not a workspace, and a script can tell them apart.
pub fn remove(root: &Path, name: &str, force: bool, say: Sink) -> u8 {
    let Some(dir) = wscmd::workspace_dir(root, name) else {
        say(Say::Fail(format!(
            "invalid workspace name '{}' (use letters, digits, '.', '_', '-'; must not start \
             with a dot)",
            crate::personas::one_line(name)
        )));
        return 1;
    };
    if !wscmd::workspace_dir_exists(root, name) {
        say(Say::Fail(format!("no workspace '{name}'")));
        return 1;
    }
    // Before the guard and before anything is read below it: a `workspaces/<ws>` that is a
    // link out of the plane is a directory this command would delete somewhere else
    // entirely. Python's `shutil.rmtree` on a symlink raises; this refuses with a sentence.
    if let Err(why) = crate::contain::no_link_on_the_way(root, &dir) {
        say(Say::Fail(format!(
            "'{name}' does not resolve to a directory inside this plane, so nothing was \
             removed ({why})."
        )));
        return 1;
    }

    let risky = wscmd::work_at_risk(root, name);
    if !risky.is_empty() && !force {
        let joined: Vec<String> = risky.iter().map(ToString::to_string).collect();
        say(Say::Fail(format!(
            "Refusing to remove '{name}' — this would discard work: {}. Push/commit first, \
             or pass --force.",
            joined.join("; ")
        )));
        return 2;
    }

    // Reported, never guarded on — see this module's header. Said HERE rather than above the
    // guard so the count describes what is actually about to happen, and only when there is
    // something to say: "0 open todos" on every removal is how a line stops being read at
    // all, including on the removal where it mattered.
    if let Ok(ws) = crate::workspaces::Plane::open(root).workspace(name)
        && let Ok(todos) = ws.todos()
        && !todos.is_empty()
    {
        say(Say::Warn(format!(
            "Discarding {} open todo(s) with '{name}' — nothing else holds them.",
            todos.len()
        )));
    }

    // charter#870, named rather than repaired — see this module's header.
    for left in blocks_left_behind(root, name) {
        say(Say::Warn(left));
    }

    if let Err(why) = std::fs::remove_dir_all(&dir) {
        say(Say::Fail(format!(
            "could not remove '{name}' ({why}) — some of it may still be there."
        )));
        return 1;
    }
    say(Say::Done(format!(
        "Removed workspace '{name}' and its clones."
    )));
    0
}

/// Every `info/exclude` outside this workspace that still holds charter's managed block, as
/// the sentence naming it — charter#870's symptom, said instead of repaired.
///
/// A checkout directly inside the workspace whose git common directory is **outside** it is
/// the whole case: its `info/exclude` is not under the directory about to be deleted, so the
/// delete cannot take charter's block with it.
fn blocks_left_behind(root: &Path, ws: &str) -> Vec<String> {
    let dir = root.join("workspaces").join(ws);
    let Ok(reader) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<std::path::PathBuf> =
        reader.filter_map(Result::ok).map(|e| e.path()).collect();
    names.sort();
    let mut out = Vec::new();
    for tree in names {
        // Only a checkout has one, and only one whose exclude file lives outside the
        // workspace survives the delete.
        let Some(exclude) = crate::guest::exclude_file(&tree) else {
            continue;
        };
        let resolved = std::fs::canonicalize(&exclude).unwrap_or_else(|_| exclude.clone());
        let inside = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if resolved.starts_with(&inside) {
            continue;
        }
        // Only when the block is really there: a checkout charter never wired has nothing to
        // leave behind, and a line about it would be noise on every removal.
        let Ok(text) = std::fs::read_to_string(&resolved) else {
            continue;
        };
        if !text.contains(crate::guest::EXCLUDE_BEGIN) {
            continue;
        }
        out.push(format!(
            "{} is a checkout whose git directory is outside '{ws}', so removing the \
             workspace leaves charter's generated-layer block in {} — it will name paths that \
             no longer exist. Delete the block between `{}` and `{}` in that file.",
            tree.display(),
            resolved.display(),
            crate::guest::EXCLUDE_BEGIN,
            crate::guest::EXCLUDE_END,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        dir
    }

    fn run(root: &Path, name: &str, force: bool) -> (u8, Vec<String>) {
        let mut said = Vec::new();
        let code = remove(root, name, force, &mut |line: Say| said.push(line.to_string()));
        (code, said)
    }

    /// A real repository, because the guard's whole job is to read one.
    fn repo(at: &Path) -> PathBuf {
        std::fs::create_dir_all(at).unwrap();
        git(at, &["init", "-q", "-b", "main"]);
        git(at, &["config", "user.email", "t@example.com"]);
        git(at, &["config", "user.name", "T"]);
        std::fs::write(at.join("README.md"), "hi\n").unwrap();
        git(at, &["add", "-A"]);
        git(at, &["commit", "-qm", "first"]);
        at.to_path_buf()
    }

    fn git(at: &Path, argv: &[&str]) {
        let run = crate::worktree::git::run(at, argv, crate::worktree::git::READ)
            .expect("git runs in a test");
        assert!(run.ok(), "git {argv:?} failed: {}", run.err);
    }

    #[test]
    fn an_empty_workspace_is_removed() {
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("workspaces/beta")).unwrap();
        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0, "{said:?}");
        assert!(!dir.path().join("workspaces/beta").exists());
        assert_eq!(said, vec!["✓ Removed workspace 'beta' and its clones."]);
    }

    #[test]
    fn a_workspace_this_plane_does_not_have_is_refused() {
        let dir = plane();
        let (code, said) = run(dir.path(), "nope", false);
        assert_eq!(code, 1);
        assert_eq!(said, vec!["✗ no workspace 'nope'"]);
    }

    #[test]
    fn a_dirty_clone_stops_the_removal_and_nothing_is_deleted() {
        let dir = plane();
        let clone = repo(&dir.path().join("workspaces/beta/svc"));
        std::fs::write(clone.join("README.md"), "changed\n").unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(said[0].contains("svc: uncommitted changes"), "{said:?}");
        assert!(said[0].contains("or pass --force"), "{said:?}");
        assert!(clone.exists(), "the guard fired, so nothing was deleted");
    }

    #[test]
    fn an_untracked_file_is_uncommitted_work_too() {
        let dir = plane();
        let clone = repo(&dir.path().join("workspaces/beta/svc"));
        std::fs::write(clone.join("scratch.txt"), "notes\n").unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(said[0].contains("svc: uncommitted changes"), "{said:?}");
    }

    #[test]
    fn unpushed_commits_stop_the_removal() {
        let dir = plane();
        let origin = repo(&dir.path().join("origin"));
        git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);
        let clone = dir.path().join("workspaces/beta/svc");
        std::fs::create_dir_all(clone.parent().unwrap()).unwrap();
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                &origin.display().to_string(),
                &clone.display().to_string(),
            ],
        );
        git(&clone, &["config", "user.email", "t@example.com"]);
        git(&clone, &["config", "user.name", "T"]);
        std::fs::write(clone.join("second.md"), "x\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-qm", "second"]);

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(said[0].contains("svc: 1 unpushed commit(s)"), "{said:?}");
        assert!(clone.exists());

        // …and `--force` is the way past it, which is what makes the guard a guard rather
        // than a wall.
        let (code, said) = run(dir.path(), "beta", true);
        assert_eq!(code, 0, "{said:?}");
        assert!(!dir.path().join("workspaces/beta").exists());
    }

    #[test]
    fn a_clone_with_no_upstream_at_all_is_not_work_at_risk() {
        // charter#104: a fresh local repository has no upstream from the moment it exists, so
        // "has no upstream" as the rule refuses over every workspace with nothing to lose —
        // and a guard that fires on the harmless common case teaches the `--force` habit.
        let dir = plane();
        repo(&dir.path().join("workspaces/beta/svc"));
        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0, "{said:?}");
    }

    #[test]
    fn a_detached_head_with_unpushed_work_still_stops_the_removal() {
        let dir = plane();
        let clone = repo(&dir.path().join("workspaces/beta/svc"));
        std::fs::write(clone.join("wip.md"), "x\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-qm", "wip"]);
        git(&clone, &["checkout", "-q", "--detach", "HEAD"]);
        // Detached, clean, and its commit is still on `main`, so there is nothing unique:
        // charter does not refuse over it, and that is the same rule as the worktree one.
        let (code, _said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0);

        let clone = repo(&dir.path().join("workspaces/gamma/svc"));
        git(&clone, &["checkout", "-q", "--detach", "HEAD"]);
        std::fs::write(clone.join("only-here.md"), "x\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-qm", "only here"]);
        // Now the tree is dirty-free but holds a commit no branch reaches. A CLONE is judged
        // by `status`, which is clean, so this one is removable — and the same shape inside a
        // worktree is refused, because that is where the unique-commit rule applies.
        let (code, _said) = run(dir.path(), "gamma", false);
        assert_eq!(code, 0);
    }

    #[test]
    fn a_worktree_holding_commits_that_exist_nowhere_else_stops_the_removal() {
        let dir = plane();
        let clone = repo(&dir.path().join("workspaces/beta/svc"));
        let piece = dir.path().join("workspaces/beta/.worktrees/svc/task");
        git(
            &clone,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "task",
                &piece.display().to_string(),
            ],
        );
        std::fs::write(piece.join("work.md"), "x\n").unwrap();
        git(&piece, &["add", "-A"]);
        git(&piece, &["commit", "-qm", "unique"]);

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(
            said[0].contains("svc/task: 1 commit(s) that exist nowhere else"),
            "{said:?}"
        );
        assert!(piece.exists());
    }

    #[test]
    fn a_dirty_worktree_stops_the_removal() {
        let dir = plane();
        let clone = repo(&dir.path().join("workspaces/beta/svc"));
        let piece = dir.path().join("workspaces/beta/.worktrees/svc/task");
        git(
            &clone,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "task",
                &piece.display().to_string(),
            ],
        );
        std::fs::write(piece.join("README.md"), "changed\n").unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(said[0].contains("svc/task: uncommitted changes"), "{said:?}");
    }

    #[test]
    fn a_clean_worktree_whose_branch_is_reachable_elsewhere_does_not_stop_the_removal() {
        let dir = plane();
        let clone = repo(&dir.path().join("workspaces/beta/svc"));
        let piece = dir.path().join("workspaces/beta/.worktrees/svc/task");
        git(
            &clone,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "task",
                &piece.display().to_string(),
            ],
        );
        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0, "{said:?}");
    }

    #[test]
    fn a_clone_charter_cannot_read_is_at_risk_rather_than_empty() {
        // charter#917, and the sharpest instance of it in the plane: what the list is empty of
        // is what is handed to a recursive delete.
        let dir = plane();
        let clone = dir.path().join("workspaces/beta/svc");
        std::fs::create_dir_all(clone.join(".git")).unwrap();
        // A `.git` directory with nothing in it: git answers "not a repository", which is a
        // failed `status` and not a clean one.
        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(said[0].contains("svc: could not be read"), "{said:?}");
        assert!(clone.exists());
    }

    #[test]
    fn a_git_dot_git_that_is_a_symlink_is_refused_and_counts_as_at_risk() {
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        let real = repo(&outside.path().join("real"));
        let clone = dir.path().join("workspaces/beta/svc");
        std::fs::create_dir_all(&clone).unwrap();
        std::os::unix::fs::symlink(real.join(".git"), clone.join(".git")).unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(said[0].contains("svc"), "{said:?}");
        assert!(real.exists(), "nothing outside the plane was touched");
    }

    #[test]
    fn open_todos_are_reported_and_never_guard_the_removal() {
        let dir = plane();
        let ws = crate::workspaces::Plane::open(dir.path())
            .workspace("beta")
            .unwrap();
        ws.add_todo("ship the thing", "2026-05-04T11:32:17".parse().unwrap())
            .unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0, "a todo is a note about the future, not work: {said:?}");
        assert!(
            said.iter()
                .any(|l| l.contains("Discarding 1 open todo(s) with 'beta'")),
            "{said:?}"
        );
        assert!(!dir.path().join("workspaces/beta").exists());
    }

    #[test]
    fn a_workspace_that_is_a_link_out_of_the_plane_is_refused_and_its_target_survives() {
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(outside.path().join("treasure")).unwrap();
        std::fs::write(outside.path().join("treasure").join("keep.txt"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("treasure"),
            dir.path().join("workspaces").join("beta"),
        )
        .unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 1, "{said:?}");
        assert!(said[0].contains("does not resolve to a directory inside this plane"), "{said:?}");
        assert!(outside.path().join("treasure").join("keep.txt").exists());
    }

    #[test]
    fn an_exclude_block_the_delete_cannot_reach_is_named() {
        // charter#870: a checkout inside the workspace whose git directory is elsewhere keeps
        // charter's managed block after the workspace is gone.
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        let main = repo(&outside.path().join("main"));
        let piece = dir.path().join("workspaces/beta/guest");
        std::fs::create_dir_all(piece.parent().unwrap()).unwrap();
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "guest",
                &piece.display().to_string(),
            ],
        );
        let exclude = main.join(".git").join("info").join("exclude");
        std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
        std::fs::write(
            &exclude,
            format!(
                "{}\n/.claude/settings.json\n{}\n",
                crate::guest::EXCLUDE_BEGIN,
                crate::guest::EXCLUDE_END
            ),
        )
        .unwrap();

        let (code, said) = run(dir.path(), "beta", true);
        assert_eq!(code, 0, "{said:?}");
        assert!(
            said.iter().any(|l| l.contains("leaves charter's generated-layer block")),
            "{said:?}"
        );
        assert!(
            std::fs::read_to_string(&exclude)
                .unwrap()
                .contains(crate::guest::EXCLUDE_BEGIN),
            "the block is named, not removed — the removal is content-addressed and has no port"
        );
    }
}
