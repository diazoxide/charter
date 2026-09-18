//! The boundary a worktree path may not cross: **one workspace**, not the plane.
//!
//! `contain::writable` asks whether a resolved path lands inside the plane's data
//! directories. For everything charter wrote before this it is the right question. Here it is
//! not sufficient, and the gap is a deletion: `git worktree remove` resolves symlinks in its
//! argument, and git — not charter — does the removing. Measured on git 2.50.1,
//!
//! ```text
//! wsB/.worktrees/r/live   ← a live, registered worktree of workspace B
//! wsA/.worktrees/r/piece  → wsB/.worktrees/r/live   (a symlink)
//!
//! $ git -C <clone> worktree remove …/wsA/.worktrees/r/piece     # exit 0, B's tree is gone
//! ```
//!
//! `contain::writable` returns `Ok` throughout, because the resolved target *is* under
//! `workspaces/`. Every tree-safety guard passes too — they ran against B's tree, which was
//! clean, so "nothing to lose" was true of the wrong tree.
//!
//! # The anchor is the part that matters
//!
//! Resolving *both* ends — does the resolved path start with the resolved worktree root — is
//! vacuous exactly where it counts. Point `workspaces/<ws>/.worktrees` at somewhere else and
//! the root resolves there too, so everything under it starts with it and passes. **A check
//! whose reference point the attack can relocate is not a check.**
//!
//! So the anchor is `workspaces/<ws>`: a directory charter created, which the plane's own
//! gate already covers, and which nothing below it can move. Everything under it — including
//! `.worktrees` itself — must then be reached without passing through a symlink.

use std::path::{Path, PathBuf};

use crate::contain;

/// A worktree path that does not belong to the workspace it was asked for.
#[derive(Debug, thiserror::Error)]
pub enum Outside {
    #[error(
        "'{path}' is not inside workspace '{ws}'. A worktree belongs to one workspace, and \
         charter will not create or remove one through a path that leaves it"
    )]
    NotInWorkspace { path: String, ws: String },
    #[error(
        "workspace '{ws}' is not a directory charter can resolve ({why}), so it cannot say \
         where a worktree under it would land"
    )]
    NoWorkspace { ws: String, why: String },
    #[error(
        "'{path}' is reached through a symlink, and a worktree path may not be: git resolves \
         what charter hands it, so the write would land somewhere charter did not check"
    )]
    ThroughALink { path: String },
}

/// The workspace directory a worktree path must stay inside, resolved.
pub fn workspace_dir(plane: &Path, ws: &str) -> Result<PathBuf, Outside> {
    let dir = plane.join("workspaces").join(ws);
    std::fs::canonicalize(&dir).map_err(|why| Outside::NoWorkspace {
        ws: ws.to_string(),
        why: why.to_string(),
    })
}

/// Is `path` inside workspace `ws`, reached without passing through a symlink?
///
/// Asked of every path this module creates or removes, in every verb — `add`, `remove`,
/// `merge` and `list` alike. `remove` is the destructive one and is the verb an earlier draft
/// of this design left on `contain::writable` alone, which is the code that deletes another
/// workspace's tree.
pub fn within_workspace(plane: &Path, ws: &str, path: &Path) -> Result<(), Outside> {
    let anchor = workspace_dir(plane, ws)?;
    // The caller builds paths from `plane`, which may itself be unresolved (on macOS a temp
    // plane is under `/var/folders/…`, a link to `/private/var/…`). Rebase onto the resolved
    // anchor so the walk below compares like with like.
    let unresolved = plane.join("workspaces").join(ws);
    let candidate = if let Ok(rest) = path.strip_prefix(&unresolved) {
        anchor.join(rest)
    } else if path.starts_with(&anchor) {
        path.to_path_buf()
    } else {
        return Err(Outside::NotInWorkspace {
            path: path.display().to_string(),
            ws: ws.to_string(),
        });
    };

    contain::no_link_on_the_way(&anchor, &candidate).map_err(|_| Outside::ThroughALink {
        path: path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(here.join("workspaces/alpha")).unwrap();
        std::fs::create_dir_all(here.join("workspaces/beta")).unwrap();
        dir
    }

    #[test]
    fn an_ordinary_path_under_the_workspace_is_inside_it() {
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        assert!(
            within_workspace(
                &root,
                "alpha",
                &root.join("workspaces/alpha/.worktrees/r/p")
            )
            .is_ok()
        );
    }

    #[test]
    fn a_path_in_another_workspace_is_not_inside_this_one() {
        // `contain::writable` admits this: the target is under `workspaces/`.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let refusal =
            within_workspace(&root, "alpha", &root.join("workspaces/beta/.worktrees/r/p"))
                .expect_err("another workspace is outside this one");

        assert!(
            matches!(refusal, Outside::NotInWorkspace { .. }),
            "{refusal}"
        );
    }

    #[test]
    fn a_worktrees_root_that_is_a_link_cannot_move_the_boundary_with_it() {
        // The whole reason the anchor is the workspace and not the worktree root: resolving
        // the root would resolve it to wherever this link points, and then everything under
        // it "starts with" it and passes.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::os::unix::fs::symlink(
            root.join("workspaces/beta"),
            root.join("workspaces/alpha/.worktrees"),
        )
        .unwrap();

        let refusal = within_workspace(
            &root,
            "alpha",
            &root.join("workspaces/alpha/.worktrees/r/p"),
        )
        .expect_err("a root reached through a link is refused");

        assert!(matches!(refusal, Outside::ThroughALink { .. }), "{refusal}");
    }

    #[test]
    fn a_link_at_any_depth_below_the_workspace_is_refused() {
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let mine = root.join("workspaces/alpha/.worktrees");
        std::fs::create_dir_all(&mine).unwrap();
        std::os::unix::fs::symlink(root.join("workspaces/beta"), mine.join("r")).unwrap();

        assert!(within_workspace(&root, "alpha", &mine.join("r/p")).is_err());
    }

    #[test]
    fn a_workspace_that_does_not_exist_cannot_anchor_anything() {
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let refusal = within_workspace(
            &root,
            "ghost",
            &root.join("workspaces/ghost/.worktrees/r/p"),
        )
        .expect_err("charter cannot place a path under a workspace that is not there");

        assert!(matches!(refusal, Outside::NoWorkspace { .. }), "{refusal}");
    }
}
