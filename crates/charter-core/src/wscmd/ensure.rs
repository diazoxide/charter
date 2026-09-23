//! `workspace.ensure` — create a workspace directory **and its baseline structure**.
//!
//! One function, because "the directory exists" and "the directory is a workspace" were two
//! different states with nothing keeping them in step: a workspace born by way of
//! `charter clone` or `charter workspace use` got a bare directory, and then the status line
//! showed `⚠ reinit` on every turn — phrased as post-upgrade drift, for a workspace that had
//! just been created correctly.
//!
//! **Best effort below the directory.** A workspace you can use beats one that failed to
//! exist, and every path the scaffold could not write is reported by `charter workspace
//! reinit`, which is the repair.

use std::path::Path;

use crate::wslayer;

/// Create `workspaces/<name>` and scaffold it. The name is checked BEFORE it is joined onto
/// a path: `Path::join` throws the prefix away when handed an absolute path, and `..` walks
/// out of the plane.
pub fn ensure(
    root: &Path,
    name: &str,
    now: chrono::DateTime<chrono::Utc>,
    author: &str,
) -> Result<Vec<wslayer::Row>, String> {
    if !crate::contain::workspace_name_ok(name) {
        return Err(format!(
            "invalid workspace name '{}' (use letters, digits, '.', '_', '-'; must not start \
             with a dot)",
            crate::personas::one_line(name)
        ));
    }
    let plane = crate::workspaces::Plane::open(root);
    let Ok(workspace) = plane.workspace(name) else {
        return Err(format!(
            "invalid workspace name '{}' (use letters, digits, '.', '_', '-'; must not start \
             with a dot)",
            crate::personas::one_line(name)
        ));
    };
    // **The name also has to mean the same directory on the next machine** (charter-app#96).
    // The alphabet above is not enough on its own: it admits `nul`, which is a device on
    // Windows whatever is appended to it, and `alpha.`, which is `alpha` there — and a plane
    // is committed and travels, so `contain::SEPARATORS`' own reasoning applies to a name
    // minted here exactly as it does to a separator.
    //
    // **Only when the directory is not there yet**, which is what makes this a gate on
    // MINTING rather than on reading. `ensure` is also the idempotent repair `workspace
    // reinit`, `workspace restore` and `workspace use` run over a workspace that already
    // exists; refusing there would strand whoever already has `workspaces/alpha.` in a plane
    // some earlier charter minted, which is the opposite of protecting them.
    if !workspace.dir().exists()
        && let Err(why) = crate::contain::mintable(name)
    {
        return Err(format!(
            "charter will not create a workspace called '{}': {why}. A plane is committed and \
             travels, so a name that means one directory here and another where the plane \
             lands is a defect wherever it was written down",
            crate::personas::one_line(name)
        ));
    }
    // `create_dir_all`, so an existing directory is not an error: `ensure` is idempotent and
    // is reached from a launch path where raising would cost the operator their tab.
    let _ = std::fs::create_dir_all(workspace.dir());
    Ok(wslayer::scaffold(&plane, name, now, author))
}

/// Who charter records as having last touched a manifest, WITHOUT a subprocess.
///
/// `git config user.name` is the better answer and costs a child process; this runs from a
/// launch path with nobody waiting. The field is provenance rather than identity, and
/// `snapshot` restamps it with git's answer the moment anybody pins a branch.
pub fn author() -> String {
    std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(
            dir.path().join(".claude").join("settings.json"),
            r#"{"env":{"CHARTER_HARNESS":"claude-code"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        dir
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        "2026-05-04T11:32:17Z".parse().unwrap()
    }

    #[test]
    fn a_new_workspace_gets_the_whole_baseline_and_the_layer() {
        let dir = plane();
        ensure(dir.path(), "gamma", now(), "fixture").unwrap();
        let ws = dir.path().join("workspaces").join("gamma");
        for rel in [
            "workspace.md",
            "workspace.json",
            "memory/MEMORY.md",
            "refs/README.md",
            ".charter-structure",
            ".claude/settings.json",
            ".charter-generated",
        ] {
            assert!(ws.join(rel).exists(), "{rel} is missing");
        }
        // The whole point of scaffolding on the way in: it must not read as stale a moment
        // after it was made.
        assert!(!wslayer::needs_reinit(&ws));
    }

    #[test]
    fn a_name_that_walks_out_of_the_plane_creates_nothing() {
        let dir = plane();
        for bad in ["../esc", "/abs", ".hidden", ""] {
            assert!(ensure(dir.path(), bad, now(), "fixture").is_err(), "{bad}");
        }
        assert!(!dir.path().join("esc").exists());
        assert!(!dir.path().parent().unwrap().join("esc").exists());
    }

    #[test]
    fn a_name_the_next_machine_reads_as_another_directory_creates_nothing() {
        // charter-app#96, measured on macOS against `origin/main`: `workspace_name_ok` said
        // `true` to `nul` and to `alpha.`, so charter would mint them into a plane, commit
        // it, and hand it to a machine that resolves both somewhere else.
        let dir = plane();
        for bad in ["nul", "NUL", "con", "aux", "lpt9", "com1.txt", "alpha."] {
            let refused = ensure(dir.path(), bad, now(), "fixture")
                .expect_err("{bad} must not be a workspace charter creates");
            assert!(
                refused.contains("charter will not create a workspace called"),
                "{bad}: {refused}"
            );
            assert!(!dir.path().join("workspaces").join(bad).exists(), "{bad}");
        }
    }

    #[test]
    fn a_workspace_of_that_name_the_plane_already_holds_is_still_scaffolded() {
        // The gate is on MINTING and nowhere else. A plane that already carries
        // `workspaces/nul` was minted by some charter, and one that refused to repair it
        // would have locked the operator out of their own plane rather than protected them.
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("workspaces").join("nul")).unwrap();
        ensure(dir.path(), "nul", now(), "fixture").expect("an existing workspace is repaired");
        assert!(
            dir.path()
                .join("workspaces")
                .join("nul")
                .join("workspace.json")
                .exists()
        );
    }

    #[test]
    fn ensuring_twice_changes_nothing_the_second_time() {
        let dir = plane();
        ensure(dir.path(), "gamma", now(), "fixture").unwrap();
        let ws = dir.path().join("workspaces").join("gamma");
        let charter = std::fs::read_to_string(ws.join("workspace.md")).unwrap();
        let manifest = std::fs::read_to_string(ws.join("workspace.json")).unwrap();
        let second = ensure(dir.path(), "gamma", now(), "somebody-else").unwrap();
        // The manifest is never rewritten once it is there — `updated_by` would otherwise
        // move on every launch.
        assert_eq!(
            std::fs::read_to_string(ws.join("workspace.json")).unwrap(),
            manifest
        );
        assert_eq!(
            std::fs::read_to_string(ws.join("workspace.md")).unwrap(),
            charter
        );
        assert!(
            second.iter().all(|r| r.did == wslayer::Did::Present),
            "{second:?}"
        );
    }

    #[test]
    fn a_hand_edited_charter_is_never_overwritten() {
        let dir = plane();
        ensure(dir.path(), "gamma", now(), "fixture").unwrap();
        let ws = dir.path().join("workspaces").join("gamma");
        std::fs::write(ws.join("workspace.md"), "# mine\n").unwrap();
        ensure(dir.path(), "gamma", now(), "fixture").unwrap();
        assert_eq!(
            std::fs::read_to_string(ws.join("workspace.md")).unwrap(),
            "# mine\n"
        );
    }
}
