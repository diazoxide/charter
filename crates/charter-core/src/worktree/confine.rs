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
    #[error("'{0}' does not name a workspace this plane contains")]
    NotAWorkspaceName(String),
    #[error(
        "workspace '{ws}' does not resolve inside this plane ({why}). A committed symlink \
         there redirects every worktree the workspace cuts, on every machine that clones it"
    )]
    NotInPlane { ws: String, why: String },
    #[error("'{path}' walks up out of its workspace with '..'. A worktree path never needs to")]
    WalksUp { path: String },
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

/// The workspace directory a worktree path must stay inside.
///
/// **Not `canonicalize`.** Canonicalising follows a link AT `workspaces/<ws>`, which moves the
/// anchor to wherever that link points — and a *committed* `workspaces/<legal-name> ->
/// elsewhere` travels to every machine that clones the plane (charter #442). That is the
/// attack `contain::writable` exists for, so it is asked here rather than assumed: the
/// workspace directory must itself land inside the plane's data directories, and must be
/// reached from the plane without passing through a link.
pub fn workspace_dir(plane: &Path, ws: &str) -> Result<PathBuf, Outside> {
    if !contain::workspace_name_ok(ws) {
        return Err(Outside::NotAWorkspaceName(ws.to_string()));
    }
    let dir = plane.join("workspaces").join(ws);
    contain::writable(plane, &dir).map_err(|why| Outside::NotInPlane {
        ws: ws.to_string(),
        why: why.to_string(),
    })?;
    contain::no_link_on_the_way(plane, &dir).map_err(|_| Outside::ThroughALink {
        path: dir.display().to_string(),
    })?;
    if !dir.is_dir() {
        return Err(Outside::NoWorkspace {
            ws: ws.to_string(),
            why: "not a directory".into(),
        });
    }
    Ok(dir)
}

/// Is `path` inside workspace `ws`, reached without passing through a symlink?
///
/// Asked of every path this module creates or removes, in every verb — `add`, `remove`,
/// `merge` and `list` alike. `remove` is the destructive one and is the verb an earlier draft
/// left on `contain::writable` alone, which is the code that deletes another workspace's tree.
///
/// Returns the path that was **checked**, and callers hand THAT to git. An earlier version
/// built a rebased candidate, checked it, threw it away and passed git the original — so the
/// string checked and the string used were two different strings, which is exactly how a link
/// gets laundered past a gate.
pub fn within_workspace(plane: &Path, ws: &str, path: &Path) -> Result<PathBuf, Outside> {
    let anchor = workspace_dir(plane, ws)?;
    let Ok(below) = path.strip_prefix(&anchor) else {
        return Err(Outside::NotInWorkspace {
            path: path.display().to_string(),
            ws: ws.to_string(),
        });
    };
    // `strip_prefix` is lexical, so what is left can still begin with `..` — and
    // `no_link_on_the_way` pushes a `ParentDir` literally and lets the KERNEL fold it, so
    // every component it stats is a real directory and nothing looks like a link. Measured:
    // `workspaces/alpha/../../../../etc/passwd` passed. A worktree path never needs `..`.
    if below
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(Outside::WalksUp {
            path: path.display().to_string(),
        });
    }
    contain::no_link_on_the_way(&anchor, path).map_err(|_| Outside::ThroughALink {
        path: path.display().to_string(),
    })?;
    Ok(path.to_path_buf())
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
    fn a_path_that_walks_up_out_of_the_workspace_is_refused() {
        // `strip_prefix` is lexical, so what is left can begin with `..`, and
        // `no_link_on_the_way` pushes a `ParentDir` literally and lets the KERNEL fold it —
        // so every component it stats is a real directory and nothing looks like a link.
        // Measured before this rule: `workspaces/alpha/../../../../etc/passwd` passed.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        for escape in [
            "workspaces/alpha/../beta/secret",
            "workspaces/alpha/../../../etc/passwd",
            "workspaces/alpha/.worktrees/../../beta/x",
        ] {
            let refusal = within_workspace(&root, "alpha", &root.join(escape))
                .expect_err("{escape} must not be inside workspace alpha");
            assert!(
                matches!(refusal, Outside::WalksUp { .. }),
                "{escape}: {refusal}"
            );
        }
    }

    #[test]
    fn a_workspace_that_is_a_symlink_anchors_nothing() {
        // charter #442: a COMMITTED `workspaces/<legal-name> -> elsewhere` travels to every
        // machine that clones the plane. Canonicalising the workspace directory would follow
        // it and move the whole boundary with it.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("workspaces/ghost")).unwrap();

        let refusal = workspace_dir(&root, "ghost")
            .expect_err("a workspace reached through a link anchors nothing");

        assert!(
            matches!(
                refusal,
                Outside::ThroughALink { .. } | Outside::NotInPlane { .. }
            ),
            "{refusal}"
        );
    }

    #[test]
    fn the_path_that_was_checked_is_the_path_that_comes_back() {
        // Checking one string and handing git another is how a link is laundered past a gate.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let asked = root.join("workspaces/alpha/.worktrees/r/p");

        let checked = within_workspace(&root, "alpha", &asked).unwrap();

        assert_eq!(checked, asked);
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
