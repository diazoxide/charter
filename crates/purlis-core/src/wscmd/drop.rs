//! Take one repo out of a workspace: delete its clone and its row in the manifest (ADR 0055) —
//! or, for a repo the workspace names and nobody cloned here, only its row
//! ([`drop_membership`], #1228).
//!
//! **The same guard as [`super::remove`], narrowed to one clone, and inside the delete.** What
//! [`super::work_at_risk`] names for this repo — uncommitted changes, unpushed commits, a clone
//! charter could not read — refuses it, and there is no `force`: the picker that unticks a repo
//! is not the place to throw work away, and `charter workspace remove --force` still is.
//!
//! **A repo with any worktree is refused, clean or not.** A linked worktree keeps its objects
//! in the clone's store, so deleting the clone breaks every one of them whatever they hold.
//! The operator removes the worktrees first (the app's "Remove folder …" action), and then
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
                    "Refusing to remove '{repo}' — it has branch folders ({}), and they keep \
                     their commits in this clone. Remove them first (the \"Remove folder …\" \
                     action in the app).",
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
            format!(
                "could not remove '{repo}' ({}) — some of it may still be there.",
                crate::rewrite::os_words(&why)
            ),
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

/// Take `repo` out of workspace `ws`'s membership: its row in `workspace.json`, and only that
/// (#1228). For a repo the workspace names and this machine has not cloned, there is nothing
/// to delete, so there is nothing for [`drop_repo`]'s guard to weigh.
///
/// **A clone is refused, never deleted here.** A repo that is cloned goes through
/// [`drop_repo`], whose work-at-risk and worktree refusals are the reason it exists; a second
/// way to the same row that skipped them would be a way round them. **And so is anything else
/// at the repo's path** (#1249 U1): a folder whose `.git` is not a real one, a plain folder, a
/// file or a link is not a clone, but dropping the row would leave it there unnamed, so the
/// refusal says what is there and nothing is written.
///
/// **The operator asked for this write**, so a manifest a hand wrote is written too, as the
/// Workspace settings' save writes one: without charter's stamp, so it stays the hand's. One
/// that does not parse is refused rather than replaced. Every other key and row is kept, in
/// its place.
///
/// Exit codes: 0 dropped, 1 could not, and nothing was written.
pub fn drop_membership(root: &Path, ws: &str, repo: &str, say: Sink) -> u8 {
    let mut fail = |why: String| {
        say(Say::Fail(why));
        1
    };
    let Some(dir) = wscmd::workspace_dir(root, ws) else {
        return fail(format!("invalid workspace name '{ws}'"));
    };
    if !wscmd::workspace_dir_exists(root, ws) {
        return fail(format!("no workspace '{ws}'"));
    }
    if !crate::contain::repo_name_ok(repo) {
        return fail(format!(
            "'{}' is not a repo's name",
            crate::shown::escaped(repo)
        ));
    }
    let cloned = match crate::repos::clones(root, ws) {
        Ok(found) => found.repos.iter().any(|r| r.name == repo),
        Err(why) => {
            return fail(format!(
                "the clones in '{ws}' could not be read ({why}), so '{repo}' was left in it."
            ));
        }
    };
    if cloned {
        return fail(format!(
            "'{repo}' is cloned in workspace '{ws}' — remove the clone instead, which checks \
             it holds no work first."
        ));
    }
    // Not a clone is not nothing there (#1249 U1): a folder whose `.git` is a link, or a plain
    // folder, is not a clone either, and dropping the row would leave it behind unnamed.
    if let Some(what) = something_at(&dir.join(repo)) {
        return fail(format!(
            "workspaces/{ws}/{repo} is {what}, not a clone, so '{repo}' was left in workspace \
             '{ws}'. Move or remove it first."
        ));
    }
    let workspace = match crate::workspaces::Plane::open(root).workspace(ws) {
        Ok(workspace) => workspace,
        Err(why) => return fail(format!("workspace '{ws}' could not be read ({why})")),
    };
    // The read and the write under one lock (#1249 U2): a clone recording its repo meanwhile
    // would otherwise lose its row to the manifest read before it.
    let held = match workspace.manifest_lock() {
        Ok(held) => held,
        Err(why) => return fail(format!("{why}, so '{repo}' was left in it.")),
    };
    let (doc, owner) = workspace.manifest();
    let Some(mut doc) = doc else {
        return fail(match owner {
            Ownership::Absent => format!("workspace '{ws}' does not name '{repo}'"),
            _ => format!(
                "workspaces/{ws}/workspace.json is not JSON purlis can read, so '{repo}' was \
                 left in it. Mend the file first."
            ),
        });
    };
    let Some(rows) = doc.get_mut("repos").and_then(Value::as_array_mut) else {
        return fail(format!("workspace '{ws}' does not name '{repo}'"));
    };
    let before = rows.len();
    rows.retain(|r| r.get("name").and_then(Value::as_str) != Some(repo));
    if rows.len() == before {
        return fail(format!("workspace '{ws}' does not name '{repo}'"));
    }
    let written = workspace.write_manifest_as(&doc, owner != Ownership::Operator);
    drop(held);
    if let Err(why) = written {
        return fail(format!(
            "workspaces/{ws}/workspace.json could not be written ({}), so '{repo}' is still in it.",
            crate::shown::short(&why.to_string())
        ));
    }
    say(Say::Done(format!(
        "Removed '{repo}' from workspace '{ws}'. Nothing was deleted: it was not cloned here."
    )));
    0
}

/// What is at `path`, in a sentence's words, or `None` when nothing is. Looked at without
/// following a link, and a look the filesystem refused is something there too: what cannot be
/// seen is never taken for nothing.
fn something_at(path: &Path) -> Option<&'static str> {
    let kind = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type(),
        Err(e) if crate::memstore::is_absent(&e) => return None,
        Err(_) => return Some("something purlis could not look at"),
    };
    Some(if kind.is_symlink() {
        "a link"
    } else if kind.is_dir() {
        "a folder"
    } else if kind.is_file() {
        "a file"
    } else {
        "something other than a folder"
    })
}

/// Drop `repo`'s row from a manifest charter wrote. One the operator wrote is left as it is,
/// and said so, as `clone` leaves it.
fn forget_in_manifest(root: &Path, ws: &str, repo: &str, say: Sink) {
    let Ok(workspace) = crate::workspaces::Plane::open(root).workspace(ws) else {
        return;
    };
    let _held = match workspace.manifest_lock() {
        Ok(held) => held,
        Err(why) => {
            say(Say::Warn(format!("{why}, so it may still name '{repo}'.")));
            return;
        }
    };
    let (doc, owner) = workspace.manifest();
    if owner == Ownership::Operator {
        say(Say::Warn(format!(
            "workspaces/{ws}/workspace.json was not written by purlis — left untouched, so it \
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

#[cfg(test)]
mod membership {
    use super::*;
    use crate::manifest;
    use crate::workspaces::{Plane, Workspace};

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces/alpha")).unwrap();
        dir
    }

    fn alpha(root: &Path) -> Workspace {
        Plane::open(root).workspace("alpha").unwrap()
    }

    fn run(root: &Path, ws: &str, name: &str) -> (u8, Vec<String>) {
        let mut said = Vec::new();
        let code = drop_membership(root, ws, name, &mut |line: Say| said.push(line.to_string()));
        (code, said)
    }

    #[test]
    fn an_uncloned_repo_leaves_the_manifest_and_nothing_else_does() {
        let dir = plane();
        alpha(dir.path())
            .write_manifest(&serde_json::json!({
                "name": "alpha",
                "repos": [{"name": "kept", "url": "https://example.com/kept.git"}, {"name": "gone"}],
                "settings": {"theme": {"icons": "charter-icons"}},
            }))
            .unwrap();

        let (code, said) = run(dir.path(), "alpha", "gone");

        assert_eq!(code, 0, "{said:?}");
        let (doc, owner) = alpha(dir.path()).manifest();
        let doc = doc.unwrap();
        assert_eq!(owner, manifest::Ownership::Charter);
        assert_eq!(
            doc["repos"],
            serde_json::json!([{"name": "kept", "url": "https://example.com/kept.git"}])
        );
        assert_eq!(doc["name"], "alpha");
        assert_eq!(
            doc["settings"],
            serde_json::json!({"theme": {"icons": "charter-icons"}})
        );
        assert!(said.iter().any(|line| line.contains("gone")), "{said:?}");
    }

    #[test]
    fn a_manifest_a_hand_wrote_stays_the_hands() {
        let dir = plane();
        let path = dir.path().join("workspaces/alpha/workspace.json");
        std::fs::write(
            &path,
            "{\n  \"name\": \"alpha\",\n  \"repos\": [{\"name\": \"gone\"}, {\"name\": \"kept\"}]\n}\n",
        )
        .unwrap();

        let (code, said) = run(dir.path(), "alpha", "gone");

        assert_eq!(code, 0, "{said:?}");
        let (doc, owner) = alpha(dir.path()).manifest();
        assert_eq!(owner, manifest::Ownership::Operator);
        assert_eq!(doc.unwrap()["repos"], serde_json::json!([{"name": "kept"}]));
    }

    /// #1249 U2: a removal waits for the manifest's lock and reads the manifest under it, so a
    /// row another writer added while it waited is kept.
    #[test]
    fn a_removal_racing_another_writer_loses_neither_row() {
        let dir = plane();
        let root = dir.path().to_path_buf();
        alpha(&root)
            .write_manifest(&serde_json::json!({
                "name": "alpha",
                "repos": [{"name": "gone"}, {"name": "kept"}],
            }))
            .unwrap();
        let held = alpha(&root).manifest_lock().unwrap();
        let removing = {
            let root = root.clone();
            std::thread::spawn(move || run(&root, "alpha", "gone"))
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(
            alpha(&root).manifest().0.unwrap()["repos"],
            serde_json::json!([{"name": "gone"}, {"name": "kept"}]),
            "the removal wrote while another writer held the lock"
        );
        // The other writer — a clone recording its repo — adds a row while it holds the lock.
        alpha(&root)
            .write_manifest(&serde_json::json!({
                "name": "alpha",
                "repos": [{"name": "added"}, {"name": "gone"}, {"name": "kept"}],
            }))
            .unwrap();
        drop(held);
        let (code, said) = removing.join().unwrap();
        assert_eq!(code, 0, "{said:?}");
        assert_eq!(
            alpha(&root).manifest().0.unwrap()["repos"],
            serde_json::json!([{"name": "added"}, {"name": "kept"}])
        );
    }

    #[test]
    fn a_cloned_repo_is_refused_and_left_to_the_drop_that_guards_its_work() {
        let dir = plane();
        let clone = dir.path().join("workspaces/alpha/svc");
        std::fs::create_dir_all(&clone).unwrap();
        let git = |argv: &[&str]| assert!(crate::testgit::run(&clone, argv).ok());
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::write(clone.join("README.md"), "hi\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "first"]);
        std::fs::write(clone.join("README.md"), "changed\n").unwrap();
        let rows = serde_json::json!({"name": "alpha", "repos": [{"name": "svc"}]});
        alpha(dir.path()).write_manifest(&rows).unwrap();

        let (code, said) = run(dir.path(), "alpha", "svc");

        assert_eq!(code, 1, "{said:?}");
        assert!(
            said.iter().any(|line| line.contains("is cloned")),
            "{said:?}"
        );
        assert!(clone.join("README.md").is_file());
        assert_eq!(
            alpha(dir.path()).manifest().0.unwrap()["repos"],
            rows["repos"]
        );

        // And the drop that does take a clone still refuses the work it holds.
        let mut said = Vec::new();
        let done = drop_repo(dir.path(), "alpha", "svc", &mut |line: Say| {
            said.push(line.to_string());
        });
        assert_eq!(done.code, 2, "{said:?}");
        assert!(clone.join("README.md").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn anything_at_the_repos_path_is_refused_and_named_and_the_row_stays() {
        type Plant = fn(&Path);
        let plant: [(&str, Plant); 4] = [
            ("a folder", |at| {
                std::fs::create_dir_all(at).unwrap();
                std::fs::write(at.join("notes.txt"), "mine\n").unwrap();
            }),
            ("a folder", |at| {
                std::fs::create_dir_all(at).unwrap();
                std::os::unix::fs::symlink("../../elsewhere/.git", at.join(".git")).unwrap();
            }),
            ("a file", |at| std::fs::write(at, "mine\n").unwrap()),
            ("a link", |at| {
                std::os::unix::fs::symlink("../beta", at).unwrap()
            }),
        ];
        for (what, plant) in plant {
            let dir = plane();
            let rows = serde_json::json!({"name": "alpha", "repos": [{"name": "svc"}]});
            alpha(dir.path()).write_manifest(&rows).unwrap();
            let at = dir.path().join("workspaces/alpha/svc");
            plant(&at);

            let (code, said) = run(dir.path(), "alpha", "svc");

            assert_eq!(code, 1, "{what}: {said:?}");
            assert!(
                said.iter()
                    .any(|line| line.contains(&format!("alpha/svc is {what}"))),
                "{what}: {said:?}"
            );
            assert!(std::fs::symlink_metadata(&at).is_ok(), "{what}");
            assert_eq!(
                alpha(dir.path()).manifest().0.unwrap()["repos"],
                rows["repos"],
                "{what}"
            );
        }
    }

    #[test]
    fn a_repo_the_workspace_does_not_name_changes_nothing() {
        let dir = plane();
        let rows = serde_json::json!({"name": "alpha", "repos": [{"name": "kept"}]});
        alpha(dir.path()).write_manifest(&rows).unwrap();
        let before = std::fs::read(dir.path().join("workspaces/alpha/workspace.json")).unwrap();

        let (code, said) = run(dir.path(), "alpha", "other");
        let (bad, _) = run(dir.path(), "alpha", "..");
        let (nowhere, _) = run(dir.path(), "beta", "kept");

        assert_eq!(code, 1, "{said:?}");
        assert!(
            said.iter().any(|line| line.contains("does not name")),
            "{said:?}"
        );
        assert_eq!(bad, 1);
        assert_eq!(nowhere, 1);
        let after = std::fs::read(dir.path().join("workspaces/alpha/workspace.json")).unwrap();
        assert_eq!(before, after);
    }
}
