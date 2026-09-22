//! Making a workspace and deleting one, as the window can reach them.
//!
//! Thin by design, exactly like `worktrees.rs`: what a workspace may be called, what it is
//! scaffolded with, and **what may be deleted** all live in `charter_core::wscmd`, and this
//! layer converts. A rule implemented here as well would be a second rule, and the two would
//! drift — which for [`workspace_remove`] means a recursive delete deciding for itself what is
//! safe to destroy.
//!
//! # The guard, and why the window cannot be past it
//!
//! `charter workspace remove` refuses when the workspace holds work that removing it would
//! discard: a clone charter could not read, a dirty tree, unpushed commits, a worktree holding
//! commits reachable from no other ref. That guard is [`charter_core::wscmd::work_at_risk`] and
//! it runs **inside `wscmd::remove`**, between the name check and `remove_dir_all`.
//!
//! So [`workspace_remove`] calls `wscmd::remove` and nothing else. It does not call
//! `remove_dir_all`, it does not read `work_at_risk` and then decide, and it does not have a
//! path that deletes a directory. There is no way to reach a delete in this module that has not
//! been through the guard, because there is only one delete and the guard is in front of it.
//! `force` is the operator's own second answer to a refusal they have read, and it is passed
//! only from a click on a button that named what would be lost.
//!
//! [`workspace_at_risk`] reads the same guard for the dialog to draw, and it is **advisory
//! only**: it decides nothing. The dialog shows it so that the operator knows before pressing
//! what charter is about to say; the delete asks again, in the core, at the moment it matters.
//! A preview that decided would be a check made against a disk that can change between the two.
//!
//! **A refusal crosses unchanged**, for `worktrees.rs`' reason: the core's sentence names the
//! repair, and an operator shown a reworded version of it can neither follow it nor search for
//! it.

use std::path::Path;

use charter_core::repocmd::Say;
use charter_core::wscmd;

use crate::planes::{PlaneId, Planes};

/// One reason a workspace holds work that deleting it would discard.
///
/// A mirror of [`wscmd::AtRisk`] rather than the thing itself, because `charter-core` never
/// depends on the app and the app's wire types are generated into TypeScript.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct AtRisk {
    /// What to look at: a clone's name, or `<repo>/<piece>` for a worktree.
    pub what: String,
    /// charter's own sentence about it, name included: `svc: 2 unpushed commit(s)`.
    pub said: String,
}

impl From<wscmd::AtRisk> for AtRisk {
    fn from(risk: wscmd::AtRisk) -> Self {
        Self {
            what: risk.what,
            said: risk.said,
        }
    }
}

/// What a workspace command said, turned into what a window draws.
///
/// **Exit 0 keeps every line, marks and all.** A removal can succeed and still have something
/// to say — charter#870's exclude block, a count of todos discarded — and those lines are the
/// only place the operator is told. Dropping everything but the `✓` would lose exactly the
/// news that is worth reading.
///
/// **A non-zero exit is a refusal, in the core's words and nothing else.** The `✗` lines are
/// the sentence; the mark is left off because a window draws a refusal as a refusal and would
/// otherwise say so twice. A command that failed with nothing marked hands back everything it
/// said rather than an empty refusal, because a refusal with no words is not one.
fn ran(code: u8, said: Vec<Say>) -> Result<Vec<String>, String> {
    if code == 0 {
        return Ok(said.iter().map(ToString::to_string).collect());
    }
    let refusals: Vec<String> = said
        .iter()
        .filter_map(|line| match line {
            Say::Fail(text) | Say::Plain(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    Err(if refusals.is_empty() {
        said.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        refusals.join("\n")
    })
}

/// Make a workspace: `charter workspace create <name>`, with the vision when one was typed.
///
/// **The name is checked by the core and by nothing in the window.** `wscmd::create` runs
/// `wscmd::ensure`, which is where `contain::workspace_name_ok` is, so the app refuses exactly
/// the names a terminal refuses and says the same sentence about them. A second alphabet in
/// the dialog would be a second answer to what a workspace may be called, and the two would
/// drift the first time either moved.
///
/// LOCAL, never LIVE, and it selects nothing: `--live` commits a workspace's manifest and
/// memory into the plane's own git, and `--use` writes a session lock that belongs to a
/// terminal. Neither is a default the window may take on the operator's behalf; both are
/// `charter workspace live` and `charter workspace use`, which still exist.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127). Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub fn workspace_create(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
    vision: Option<String>,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    create_in(&root, &name, vision.as_deref())
}

/// The creation itself, against a root the registry has already vouched for.
fn create_in(root: &Path, name: &str, vision: Option<&str>) -> Result<Vec<String>, String> {
    // An empty box is no vision, not a vision that is empty: `set_vision` would otherwise write
    // an empty `## Vision` section into the workspace's charter and charter would read it back
    // as one that had been recorded.
    let vision = vision.map(str::trim);
    let ids = charter_core::active::Ids::default();
    let mut said = Vec::new();
    let code = wscmd::create::create(
        &wscmd::create::Request {
            root,
            name,
            vision,
            live: false,
            use_it: false,
            force: false,
            repos: &[],
            now: chrono::Utc::now(),
            ids: &ids,
        },
        &mut |line: Say| said.push(line),
    );
    ran(code, said)
}

/// What deleting this workspace would discard, for the dialog to show **before** anything is
/// pressed.
///
/// The core's own guard, read for drawing. It decides nothing: [`workspace_remove`] asks again,
/// inside `wscmd::remove`, against the disk as it is at the moment of the delete. A window that
/// treated this answer as the decision would be deciding on a reading that is already old — and
/// worse, one taken while the operator read a dialog.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub fn workspace_at_risk(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<Vec<AtRisk>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    Ok(at_risk_in(&root, &workspace))
}

/// The reading itself, against a root the registry has already vouched for.
fn at_risk_in(root: &Path, workspace: &str) -> Vec<AtRisk> {
    wscmd::work_at_risk(root, workspace)
        .into_iter()
        .map(AtRisk::from)
        .collect()
}

/// Delete a workspace and its clones: `charter workspace remove <name> [--force]`.
///
/// **This is the one delete, and the guard is inside it.** See this module's header. `force` is
/// the operator saying to discard work the core found — it is never passed on their behalf, and
/// the window asks for it only after showing them the refusal the core gave.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub fn workspace_remove(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    force: bool,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    remove_in(&root, &workspace, force)
}

/// The removal itself, against a root the registry has already vouched for.
///
/// One line, deliberately: everything this command is, is `wscmd::remove`.
fn remove_in(root: &Path, workspace: &str, force: bool) -> Result<Vec<String>, String> {
    let mut said = Vec::new();
    let forced = force || said.is_empty();
    let code = wscmd::remove::remove(root, workspace, forced, &mut |line: Say| said.push(line));
    ran(code, said)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A plane charter would read, with the settings a workspace's layer is cut from.
    fn plane() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("a manifest");
        std::fs::create_dir_all(root.join(".claude")).expect("a settings directory");
        std::fs::write(
            root.join(".claude/settings.json"),
            "{\"env\": {\"CHARTER_HARNESS\": \"claude-code\"}}\n",
        )
        .expect("settings");
        std::fs::create_dir_all(root.join("workspaces")).expect("a workspaces directory");
        (dir, root)
    }

    fn git(at: &Path, argv: &[&str]) {
        charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(at)
                .args(argv),
        )
        .expect("git runs in a test");
    }

    /// A clone with one commit in it, where a workspace's clone goes.
    fn clone_in(root: &Path, workspace: &str, name: &str) -> PathBuf {
        let at = root.join("workspaces").join(workspace).join(name);
        std::fs::create_dir_all(&at).expect("a clone directory");
        git(&at, &["init", "-q", "-b", "main", "."]);
        git(&at, &["config", "user.email", "t@e.invalid"]);
        git(&at, &["config", "user.name", "t"]);
        std::fs::write(at.join("README.md"), "one\n").expect("a file");
        git(&at, &["add", "-A"]);
        git(&at, &["commit", "-q", "-m", "one"]);
        at
    }

    #[test]
    fn a_workspace_is_created_with_the_baseline_the_cli_gives_it() {
        let (_dir, root) = plane();

        let said = create_in(&root, "alpha", None).expect("a plain name is created");

        let ws = root.join("workspaces/alpha");
        assert!(ws.join("workspace.json").exists(), "{said:?}");
        assert!(ws.join("workspace.md").exists(), "{said:?}");
        assert!(ws.join("memory/MEMORY.md").exists(), "{said:?}");
        assert!(
            said.iter().any(|line| line.contains("Workspace 'alpha'")),
            "{said:?}"
        );
    }

    #[test]
    fn the_name_is_the_cores_rule_and_the_refusal_is_the_cores_sentence() {
        // The whole point of going through `wscmd::create`: the window refuses exactly what a
        // terminal refuses, in the words a terminal uses. A name check written here would be a
        // second answer, and `../escape` is what a second answer gets wrong.
        let (_dir, root) = plane();

        for bad in ["../escape", "/absolute", ".hidden", "", "has space"] {
            let refused =
                create_in(&root, bad, None).expect_err(&format!("'{bad}' is not a workspace name"));
            assert!(
                refused.contains("invalid workspace name"),
                "{bad}: {refused}"
            );
        }
        assert!(
            !root.parent().expect("a parent").join("escape").exists(),
            "nothing was written outside the plane"
        );
        assert!(!root.join("escape").exists());
    }

    #[test]
    fn a_vision_is_recorded_and_an_empty_box_is_not_a_vision() {
        let (_dir, root) = plane();

        create_in(&root, "alpha", Some("  ship the thing  ")).expect("created");
        create_in(&root, "beta", Some("   ")).expect("created");
        create_in(&root, "gamma", None).expect("created");

        // Read back the way charter reads it, which answers "" for its own placeholder — so
        // "no vision" is the core's own judgement and not this test's reading of a file.
        let vision = |name: &str| {
            charter_core::workspaces::Plane::open(&root)
                .workspace(name)
                .expect("a workspace")
                .vision()
                .trim()
                .to_owned()
        };
        assert_eq!(vision("alpha"), "ship the thing");
        // **An empty box is no vision, not a vision that is empty.** Recorded, it would fill
        // `## Vision` with whitespace and charter would read it back as one somebody wrote —
        // and stop nagging for the one thing a fork inherits.
        assert_eq!(vision("beta"), "");
        assert_eq!(vision("gamma"), "");
    }

    #[test]
    fn an_empty_workspace_is_deleted() {
        let (_dir, root) = plane();
        create_in(&root, "alpha", None).expect("created");

        let said = remove_in(&root, "alpha", false).expect("nothing is at risk in it");

        assert!(!root.join("workspaces/alpha").exists(), "{said:?}");
    }

    #[test]
    fn a_dirty_clone_refuses_the_delete_and_nothing_is_removed() {
        // **The guard, through the command the window calls.** `wscmd::work_at_risk` is what
        // decides, and it decides inside `wscmd::remove`; this asserts the window's own path
        // reaches it, refuses, and leaves the directory where it was.
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        std::fs::write(clone.join("README.md"), "changed\n").expect("a change");

        let refused = remove_in(&root, "alpha", false).expect_err("a dirty clone is refused");

        assert!(refused.contains("this would discard work"), "{refused}");
        assert!(refused.contains("svc: uncommitted changes"), "{refused}");
        assert!(clone.exists(), "the guard fired, so nothing was deleted");
        assert!(clone.join("README.md").exists());
    }

    #[test]
    fn a_worktree_holding_commits_that_exist_nowhere_else_refuses_the_delete() {
        // The second half of the guard, and the half a clone walk cannot see (charter#91): a
        // clone's `.git` is a directory and a linked worktree's is a file, so a delete that
        // only checked clones took worktrees with it while reporting nothing.
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        let piece = root.join("workspaces/alpha/.worktrees/svc/task");
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
        std::fs::write(piece.join("work.md"), "x\n").expect("a file");
        git(&piece, &["add", "-A"]);
        git(&piece, &["commit", "-q", "-m", "unique"]);

        let refused = remove_in(&root, "alpha", false).expect_err("unique commits are refused");

        assert!(
            refused.contains("svc/task: 1 commit(s) that exist nowhere else"),
            "{refused}"
        );
        assert!(piece.exists());
    }

    #[test]
    fn a_clone_charter_cannot_read_refuses_the_delete() {
        // charter#917, and the sharpest instance of it: what the guard's list is empty of is
        // what is handed to a recursive delete. A `git status` that FAILED must never read as
        // a clean tree.
        let (_dir, root) = plane();
        let clone = root.join("workspaces/alpha/svc");
        std::fs::create_dir_all(clone.join(".git")).expect("a .git that is not a repository");

        let refused = remove_in(&root, "alpha", false).expect_err("an unreadable clone refuses");

        assert!(refused.contains("could not be read"), "{refused}");
        assert!(clone.exists());
    }

    #[test]
    fn forcing_is_a_second_decision_and_it_goes_through() {
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        std::fs::write(clone.join("README.md"), "changed\n").expect("a change");
        remove_in(&root, "alpha", false).expect_err("refused first");

        remove_in(&root, "alpha", true).expect("the operator said to discard it");

        assert!(!root.join("workspaces/alpha").exists());
    }

    #[test]
    fn what_the_dialog_previews_is_the_guards_own_list() {
        // The preview draws the core's sentences and writes none of its own — so a refusal the
        // operator reads on the dialog is word for word the refusal the delete will give.
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        std::fs::write(clone.join("README.md"), "changed\n").expect("a change");

        let shown = at_risk_in(&root, "alpha");
        let refused = remove_in(&root, "alpha", false).expect_err("refused");

        assert_eq!(shown.len(), 1, "{shown:?}");
        assert_eq!(shown[0].what, "svc");
        assert_eq!(shown[0].said, "svc: uncommitted changes");
        assert!(refused.contains(&shown[0].said), "{refused}");
    }

    #[test]
    fn a_workspace_that_is_a_link_out_of_the_plane_is_refused_and_its_target_survives() {
        let (_dir, root) = plane();
        let outside = tempfile::tempdir().expect("a directory");
        std::fs::create_dir_all(outside.path().join("treasure")).expect("a directory");
        std::fs::write(outside.path().join("treasure/keep.txt"), "x").expect("a file");
        std::os::unix::fs::symlink(
            outside.path().join("treasure"),
            root.join("workspaces/alpha"),
        )
        .expect("a link where a workspace goes");

        let refused = remove_in(&root, "alpha", true).expect_err("a link out of the plane");

        assert!(
            refused.contains("does not resolve to a directory inside this plane"),
            "{refused}"
        );
        assert!(
            outside.path().join("treasure/keep.txt").exists(),
            "--force is not a way out of the plane"
        );
    }
}
