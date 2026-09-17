//! The core, read against the fixture planes in `tests/fixtures/planes`.
//!
//! Those planes are written by the Python charter itself (see the README beside them), so
//! a test here is a test against the real format rather than against our idea of it.

use std::fs;
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
    // Read the manifest, so that emptying it fails this test. Finding a file by name says
    // nothing about the format; charter writes `schema = 1` first and everything else in
    // the plane hangs off that number.
    let manifest = fs::read_to_string(plane.join(plane::MANIFEST)).unwrap();
    assert!(
        manifest.starts_with("schema = 1\n"),
        "the fixture's manifest does not open with the schema line: {manifest:?}"
    );
}

#[test]
fn a_clone_without_a_manifest_of_its_own_finds_the_plane() {
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

/// What `find_root` does today, pinned so the next test's gap is visible in the suite
/// rather than only in a comment.
#[test]
fn today_the_nearest_manifest_wins_even_inside_a_workspace() {
    let plane = fixture("daily");
    let clone = plane.join("workspaces/alpha/tool");

    assert_eq!(plane::find_root(&clone), Ok(clone));
}

/// The rule the plane format specifies, which this core does not implement yet.
///
/// `docs/plane-format.md` ("Plane-root discovery") has six steps. `find_root` implements
/// the second — walk up to the nearest manifest — and not the first (`$CHARTER_ROOT` wins
/// outright, and a bad value raises rather than falling back), the third (a linked git
/// worktree redirects to its main working tree) or the fourth: the answer hops **outward**
/// through any enclosing plane's `workspaces/` until it stops moving.
///
/// `daily/workspaces/alpha/tool` is a clone carrying a `charter.toml` of its own, which is
/// ordinary — charter's own repo is such a clone. Python charter resolves it to the plane;
/// this core stops at the clone, which is charter bug #200: the wrong personas, and memory
/// written where nobody chose. The fixture holds the case so that implementing the hop is
/// a matter of deleting the `ignore` line.
#[test]
#[ignore = "find_root implements the nearest-manifest step only; the outward hop is M1 work"]
fn a_clone_carrying_its_own_manifest_still_belongs_to_the_plane() {
    let plane = fixture("daily");
    let clone = plane.join("workspaces/alpha/tool");

    assert_eq!(plane::find_root(&clone), Ok(plane));
}
