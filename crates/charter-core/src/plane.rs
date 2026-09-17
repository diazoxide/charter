//! The control plane: the directory whose `charter.toml` everything else hangs off.

use std::path::{Path, PathBuf};

/// The file that marks a directory as a plane root.
pub const MANIFEST: &str = "charter.toml";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlaneError {
    #[error("no {MANIFEST} in {0} or any directory above it")]
    NotFound(PathBuf),
}

/// Finds the plane that `start` sits in: the nearest directory at or above it holding
/// `charter.toml`.
pub fn find_root(start: &Path) -> Result<PathBuf, PlaneError> {
    start
        .ancestors()
        .find(|dir| dir.join(MANIFEST).is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| PlaneError::NotFound(start.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_directory_holding_the_manifest_is_its_own_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), "").unwrap();

        assert_eq!(find_root(dir.path()), Ok(dir.path().to_path_buf()));
    }

    #[test]
    fn a_clone_deep_inside_a_workspace_finds_the_plane_above_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), "").unwrap();
        let clone = dir.path().join("workspaces/ide/charter-app/src");
        fs::create_dir_all(&clone).unwrap();

        assert_eq!(find_root(&clone), Ok(dir.path().to_path_buf()));
    }

    #[test]
    fn the_nearest_manifest_wins_over_one_further_up() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), "").unwrap();
        let nested = dir.path().join("inner");
        fs::create_dir_all(nested.join("deeper")).unwrap();
        fs::write(nested.join(MANIFEST), "").unwrap();

        assert_eq!(find_root(&nested.join("deeper")), Ok(nested));
    }

    #[test]
    fn a_directory_with_no_manifest_above_it_is_not_a_plane() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            find_root(dir.path()),
            Err(PlaneError::NotFound(dir.path().to_path_buf()))
        );
    }
}
