//! Take one repo out of a workspace: delete its clone and its row in the manifest (ADR 0055).
//!
//! **The same guard as [`super::remove`], narrowed to one clone, and inside the delete.** What
//! [`super::work_at_risk`] names for this repo — uncommitted changes, unpushed commits, a clone
//! charter could not read — refuses it, and there is no `force`: the picker that unticks a repo
//! is not the place to throw work away, and `charter workspace remove --force` still is.
//!
//! **A repo with any worktree is refused, clean or not.** A linked worktree keeps its objects
//! in the clone's store, so deleting the clone breaks every one of them whatever they hold.
//! The operator removes the worktrees first (the app's "Remove worktree" action), and then
//! the repo.

use std::path::Path;

use serde_json::Value;

use super::remove::Removal;
use crate::manifest::Ownership;
use crate::repocmd::{Say, Sink};
use crate::wscmd;

/// Delete the clone of `repo` in workspace `ws` and drop it from the manifest, or say why not.
///
/// Exit codes as [`super::remove::remove`]'s: 0 removed, 1 could not, 2 the guard refused —
/// and then [`Removal::refused_over`] is what it refused on.
pub fn drop_repo(root: &Path, ws: &str, repo: &str, say: Sink) -> Removal {
    let fail = |say: Sink, why: String| {
        say(Say::Fail(why));
        Removal {
            code: 1,
            refused_over: Vec::new(),
        }
    };
    let Some(dir) = wscmd::workspace_dir(root, ws) else {
        return fail(say, format!("invalid workspace name '{ws}'"));
    };
    if !wscmd::workspace_dir_exists(root, ws) {
        return fail(say, format!("no workspace '{ws}'"));
    }
    if !crate::contain::repo_name_ok(repo) {
        return fail(
            say,
            format!("'{}' is not a repo's name", crate::shown::escaped(repo)),
        );
    }
    let clone = dir.join(repo);
    // Before anything is read in it: a link here is a directory somewhere else entirely.
    if let Err(why) = crate::contain::no_link_on_the_way(root, &clone) {
        return fail(
            say,
            format!("'{repo}' does not resolve inside this plane, so nothing was removed ({why})."),
        );
    }
    let is_a_clone = crate::repos::clones(root, ws)
        .map(|found| found.repos.iter().any(|r| r.name == repo))
        .unwrap_or(false);
    if !is_a_clone {
        return fail(say, format!("'{repo}' is not a clone in workspace '{ws}'"));
    }
    match crate::worktree::list(root, ws, repo) {
        Ok(pieces) if pieces.iter().any(|p| p.prunable.is_none()) => {
            let names: Vec<String> = pieces.into_iter().map(|p| p.piece).collect();
            return fail(
                say,
                format!(
                    "Refusing to remove '{repo}' — it has worktrees ({}), and they keep their \
                     commits in this clone. Remove them first (the \"Remove worktree\" action in the app).",
                    names.join(", ")
                ),
            );
        }
        Ok(_) => {}
        Err(why) => {
            return fail(
                say,
                format!("Refusing to remove '{repo}' — its worktrees could not be read ({why})."),
            );
        }
    }
    let risky: Vec<wscmd::AtRisk> = wscmd::work_at_risk(root, ws)
        .into_iter()
        .filter(|r| r.what == repo)
        .collect();
    if !risky.is_empty() {
        let joined: Vec<String> = risky.iter().map(ToString::to_string).collect();
        say(Say::Fail(format!(
            "Refusing to remove '{repo}' — this would discard work: {}. Push or commit first.",
            joined.join("; ")
        )));
        return Removal {
            code: 2,
            refused_over: risky,
        };
    }
    if let Err(why) = std::fs::remove_dir_all(&clone) {
        return fail(
            say,
            format!("could not remove '{repo}' ({why}) — some of it may still be there."),
        );
    }
    forget_in_manifest(root, ws, repo, say);
    say(Say::Done(format!(
        "Removed '{repo}' from workspace '{ws}'."
    )));
    Removal {
        code: 0,
        refused_over: Vec::new(),
    }
}

/// Drop `repo`'s row from a manifest charter wrote. One the operator wrote is left as it is,
/// and said so, as `clone` leaves it.
fn forget_in_manifest(root: &Path, ws: &str, repo: &str, say: Sink) {
    let Ok(workspace) = crate::workspaces::Plane::open(root).workspace(ws) else {
        return;
    };
    let (doc, owner) = workspace.manifest();
    if owner == Ownership::Operator {
        say(Say::Warn(format!(
            "workspaces/{ws}/workspace.json was not written by charter — left untouched, so it \
             still names '{repo}'."
        )));
        return;
    }
    let Some(mut doc) = doc else {
        return;
    };
    let Some(rows) = doc.get_mut("repos").and_then(Value::as_array_mut) else {
        return;
    };
    let before = rows.len();
    rows.retain(|r| r.get("name").and_then(Value::as_str) != Some(repo));
    if rows.len() != before {
        let _ = workspace.write_manifest(&doc);
    }
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

    fn git(at: &Path, argv: &[&str]) {
        let run = crate::testgit::run(at, argv);
        assert!(run.ok(), "git {argv:?} failed: {}", run.err);
    }

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

    fn run(root: &Path, ws: &str, name: &str) -> (Removal, Vec<String>) {
        let mut said = Vec::new();
        let done = drop_repo(root, ws, name, &mut |line: Say| said.push(line.to_string()));
        (done, said)
    }

    #[test]
    fn a_clean_clone_is_removed_and_the_rest_of_the_workspace_stays() {
        let dir = plane();
        let ws = dir.path().join("workspaces/alpha");
        repo(&ws.join("gone"));
        repo(&ws.join("kept"));

        let (done, said) = run(dir.path(), "alpha", "gone");

        assert_eq!(done.code, 0, "{said:?}");
        assert!(!ws.join("gone").exists());
        assert!(ws.join("kept/README.md").is_file());
    }

    #[test]
    fn a_clone_holding_uncommitted_work_is_refused_and_kept() {
        let dir = plane();
        let clone = repo(&dir.path().join("workspaces/alpha/svc"));
        std::fs::write(clone.join("README.md"), "changed\n").unwrap();

        let (done, said) = run(dir.path(), "alpha", "svc");

        assert_eq!(done.code, 2, "{said:?}");
        assert_eq!(done.refused_over.len(), 1);
        assert!(clone.join("README.md").is_file());
    }

    #[test]
    fn another_clones_work_does_not_stop_this_one_being_removed() {
        let dir = plane();
        let ws = dir.path().join("workspaces/alpha");
        repo(&ws.join("gone"));
        let busy = repo(&ws.join("busy"));
        std::fs::write(busy.join("README.md"), "changed\n").unwrap();

        let (done, said) = run(dir.path(), "alpha", "gone");

        assert_eq!(done.code, 0, "{said:?}");
    }

    #[test]
    fn a_name_that_is_not_a_clone_here_deletes_nothing() {
        let dir = plane();
        let ws = dir.path().join("workspaces/alpha");
        std::fs::create_dir_all(ws.join("notes")).unwrap();

        let (done, _) = run(dir.path(), "alpha", "notes");
        let (up, _) = run(dir.path(), "alpha", "..");

        assert_eq!(done.code, 1);
        assert!(ws.join("notes").is_dir());
        assert_eq!(up.code, 1);
        assert!(ws.is_dir());
    }

    #[test]
    fn the_removed_repo_leaves_the_manifest_charter_wrote() {
        let dir = plane();
        let ws = dir.path().join("workspaces/alpha");
        repo(&ws.join("gone"));
        let workspace = crate::workspaces::Plane::open(dir.path())
            .workspace("alpha")
            .unwrap();
        workspace
            .write_manifest(&serde_json::json!({
                "name": "alpha",
                "repos": [{"name": "gone"}, {"name": "kept"}],
            }))
            .unwrap();

        let (done, said) = run(dir.path(), "alpha", "gone");

        assert_eq!(done.code, 0, "{said:?}");
        let (doc, _) = workspace.manifest();
        assert_eq!(doc.unwrap()["repos"], serde_json::json!([{"name": "kept"}]));
    }
}
