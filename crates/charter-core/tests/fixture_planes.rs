//! The core, read against the fixture planes in `tests/fixtures/planes`.
//!
//! Those planes are written by the Python charter itself (see the README beside them), so
//! a test here is a test against the real format rather than against our idea of it.

use std::path::{Path, PathBuf};

use charter_core::plane;

fn fixture(name: &str) -> PathBuf {
    let planes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes");
    planes
        .join(name)
        .canonicalize()
        .unwrap_or_else(|e| panic!("fixture plane {name} is missing: {e}"))
}

#[test]
fn a_plane_charter_init_wrote_is_found_by_its_manifest() {
    let plane = fixture("minimal");

    assert_eq!(plane::find_root(&plane), Ok(plane.clone()));
}

#[test]
fn a_clone_deep_inside_a_real_plane_finds_that_plane() {
    let plane = fixture("daily");
    let clone = plane.join("workspaces/alpha/svc");

    assert_eq!(plane::find_root(&clone), Ok(plane));
}

#[test]
fn a_workspace_directory_is_not_itself_a_plane() {
    let plane = fixture("daily");
    let workspace = plane.join("workspaces/alpha");

    assert!(!workspace.join(plane::MANIFEST).exists());
    assert_eq!(plane::find_root(&workspace), Ok(plane));
}
