//! The worktree verbs, as the window can reach them.
//!
//! Thin by design: every decision — what may be removed, what may be merged, which paths are
//! this workspace's — lives in `charter_core::worktree`, and this layer converts. A rule
//! implemented here as well would be a second rule, and the two would drift.
//!
//! **A refusal crosses unchanged.** The core's refusals are sentences that name the repair,
//! and the whole point of putting these verbs in the window is that the operator reads the
//! same sentence there as in a terminal. So `Refusal` is converted with `to_string()` and
//! nothing else: no rewording, no "failed to remove worktree", no error code the UI would
//! then have to translate back into English.

use std::path::{Path, PathBuf};

use charter_core::worktree;

use crate::planes::{PlaneId, Planes};

/// One piece, as the window shows it.
#[derive(Debug, serde::Serialize, specta::Type)]
pub struct Piece {
    pub piece: String,
    pub path: String,
    pub branch: Option<String>,
    /// Whether charter's harness layer is in this tree.
    ///
    /// Since M1.x a worktree charter cuts is wired as it is cut, so `false` now means a tree
    /// cut by plain git, one whose wire did not land, or a plane with no layer to carry. The
    /// row still says so, because a chat in such a tree runs without the plane's ask/deny
    /// rules, without its persona's agents and without `$CHARTER_HARNESS` — and starting one
    /// there is what writes the layer or refuses.
    pub wired: bool,
    /// Set when git still has a registration whose directory is gone.
    pub stale: bool,
}

/// Where a chat is working, when it is working in a piece.
#[derive(Debug, serde::Serialize, specta::Type)]
pub struct ChatWorktree {
    pub workspace: String,
    pub repo: String,
    pub piece: String,
    pub branch: Option<String>,
    pub wired: bool,
    pub stale: bool,
}

fn plane_of(cwd: &Path) -> Result<PathBuf, String> {
    charter_core::plane::resolve(cwd).map_err(|why| why.to_string())
}

/// The piece a chat's working directory sits in, or `None`.
///
/// Called for every chat the sidebar draws. `worktree::locate` is path arithmetic and spawns
/// nothing; the one git call is the listing, made once per repo and only for chats that are
/// in a piece at all.
#[tauri::command]
#[specta::specta]
pub fn worktree_of_chat(cwd: String) -> Result<Option<ChatWorktree>, String> {
    let cwd = PathBuf::from(cwd);
    let plane = plane_of(&cwd)?;
    let Some(found) = worktree::locate(&plane, &cwd) else {
        return Ok(None);
    };
    let pieces = worktree::list(&plane, &found.workspace, &found.repo)
        .map_err(|refusal| refusal.to_string())?;
    let Some(row) = pieces.into_iter().find(|p| p.piece == found.piece) else {
        // git no longer has a registration for it, though the directory is where a piece
        // goes. Reported as itself rather than as nothing: the row is not a chat working
        // outside every worktree.
        return Ok(Some(ChatWorktree {
            workspace: found.workspace,
            repo: found.repo,
            piece: found.piece,
            branch: None,
            wired: false,
            stale: true,
        }));
    };
    Ok(Some(ChatWorktree {
        workspace: found.workspace,
        repo: found.repo,
        piece: found.piece,
        branch: row.branch,
        wired: row.wired,
        stale: row.prunable.is_some(),
    }))
}

/// This workspace's pieces for one repo.
// **It names its plane, and the registry vouches for it** (charter-app#127). It used to take
// the root as a `String` and hand it straight to the core, so whatever could reach the command
// chose which directory git ran in. A `PlaneId` has no constructor outside `planes.rs` — a
// caller hands one back, it never spells one — and `held` refuses one this window never
// opened. Not a doc comment, because the generated bindings carry those and this is about the
// Rust.
#[tauri::command]
#[specta::specta]
pub fn worktree_list(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
) -> Result<Vec<Piece>, String> {
    pieces_of(planes.held(&plane)?.root(), &workspace, &repo)
}

/// The listing itself, against a root the registry has already vouched for.
fn pieces_of(plane: &Path, workspace: &str, repo: &str) -> Result<Vec<Piece>, String> {
    worktree::list(plane, workspace, repo)
        .map(|pieces| {
            pieces
                .into_iter()
                .map(|p| Piece {
                    piece: p.piece,
                    path: p.path.display().to_string(),
                    branch: p.branch,
                    wired: p.wired,
                    stale: p.prunable.is_some(),
                })
                .collect()
        })
        .map_err(|refusal| refusal.to_string())
}

/// Remove a piece. The refusal is the core's sentence, unchanged.
///
/// `force` is the operator saying to discard work the guards found — it is never passed on
/// their behalf, and the window asks for it only after showing them what the refusal said.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127); see `worktree_list` above. Not a doc comment, for the reason given there.
#[tauri::command]
#[specta::specta]
pub fn worktree_remove(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: String,
    force: bool,
) -> Result<(), String> {
    remove_piece(
        planes.held(&plane)?.root(),
        &workspace,
        &repo,
        &piece,
        force,
    )
}

/// The removal itself, against a root the registry has already vouched for.
fn remove_piece(
    plane: &Path,
    workspace: &str,
    repo: &str,
    piece: &str,
    force: bool,
) -> Result<(), String> {
    worktree::remove(plane, workspace, repo, piece, force, false)
        .map(|_| ())
        .map_err(|refusal| refusal.to_string())
}

/// What a merge did, for the window to report.
#[derive(Debug, serde::Serialize, specta::Type)]
pub struct Merged {
    pub branch: String,
    pub was: String,
    pub now: String,
}

/// Land a piece in its clone, fast-forward only. Never pushes.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127); see `worktree_list` above. Not a doc comment, for the reason given there.
#[tauri::command]
#[specta::specta]
pub fn worktree_merge(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: String,
) -> Result<Merged, String> {
    merge_piece(planes.held(&plane)?.root(), &workspace, &repo, &piece)
}

/// The merge itself, against a root the registry has already vouched for.
fn merge_piece(plane: &Path, workspace: &str, repo: &str, piece: &str) -> Result<Merged, String> {
    worktree::merge(plane, workspace, repo, piece)
        .map(|m| Merged {
            branch: m.branch,
            was: m.was,
            now: m.now,
        })
        .map_err(|refusal| refusal.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane with a clone and one piece in it.
    fn plane() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let clone = root.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main", "."],
            vec!["config", "user.email", "t@e.invalid"],
            vec!["config", "user.name", "t"],
        ] {
            charter_core::forklock::output(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&clone)
                    .args(&args),
            )
            .unwrap();
        }
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        for args in [vec!["add", "-A"], vec!["commit", "-q", "-m", "one"]] {
            charter_core::forklock::output(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&clone)
                    .args(&args),
            )
            .unwrap();
        }
        (dir, root, clone)
    }

    #[test]
    fn a_chat_outside_every_worktree_has_none() {
        let (_dir, _root, clone) = plane();

        let seen = worktree_of_chat(clone.display().to_string()).unwrap();

        assert!(seen.is_none(), "the shared clone is not a piece");
    }

    #[test]
    fn a_chat_in_a_piece_reports_its_branch_and_that_the_layer_is_there() {
        let (_dir, root, _clone) = plane();
        // A plane with something to carry. Without it `want` is empty, charter writes
        // nothing, and this would assert `wired` against a plane that has no layer at all —
        // a test that passes whatever the wire does.
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(
            root.join(".claude/settings.json"),
            "{\"env\": {\"CHARTER_HARNESS\": \"claude-code\"}}\n",
        )
        .unwrap();
        let added = worktree::add(&root, "alpha", "thing", "piece", None).unwrap();

        let seen = worktree_of_chat(added.path.display().to_string())
            .unwrap()
            .expect("a chat in a piece has one");

        assert_eq!(seen.piece, "piece");
        assert_eq!(seen.branch.as_deref(), Some("piece"));
        assert!(
            seen.wired,
            "a worktree charter cut carries the plane's layer (M1.x, closing ADR 0027's gap)"
        );
        assert!(!seen.stale);
    }

    #[test]
    fn a_worktree_cut_by_plain_git_still_reads_unwired() {
        // The label is derived from the tree, not from what charter remembers doing, so it is
        // still the honest answer for a tree charter did not wire.
        let (_dir, root, clone) = plane();
        let by_hand = root.join("workspaces/alpha/.worktrees/thing/hand");
        std::fs::create_dir_all(by_hand.parent().unwrap()).unwrap();
        charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&clone)
                .args(["worktree", "add", "-q", "-b", "hand"])
                .arg(&by_hand),
        )
        .unwrap();

        let seen = worktree_of_chat(by_hand.display().to_string())
            .unwrap()
            .expect("a chat in a piece has one");

        assert!(!seen.wired);
    }

    #[test]
    fn a_refusal_reaches_the_window_as_the_sentence_the_core_wrote() {
        // One message constant, two call sites. A window that rewords a refusal is a window
        // whose users cannot search for the sentence they were shown, and cannot follow the
        // repair it names.
        let (_dir, root, _clone) = plane();
        let added = worktree::add(&root, "alpha", "thing", "piece", None).unwrap();
        std::fs::write(added.path.join("wip.txt"), "unsaved\n").unwrap();

        let through = remove_piece(&root, "alpha", "thing", "piece", false)
            .expect_err("a dirty piece is refused");
        let core = worktree::remove(&root, "alpha", "thing", "piece", false, false)
            .expect_err("the same refusal")
            .to_string();

        assert_eq!(through, core);
        assert!(through.contains("uncommitted"), "{through}");
        assert!(added.path.is_dir(), "and nothing was removed");
    }

    #[test]
    fn forcing_is_a_second_decision_and_it_goes_through() {
        let (_dir, root, _clone) = plane();
        let added = worktree::add(&root, "alpha", "thing", "piece", None).unwrap();
        std::fs::write(added.path.join("wip.txt"), "unsaved\n").unwrap();

        remove_piece(&root, "alpha", "thing", "piece", true)
            .expect("the operator said to discard it");

        assert!(!added.path.exists());
    }
}
