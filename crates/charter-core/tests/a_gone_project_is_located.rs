//! **Locate…** (NO-5, #1237): a remembered project that is gone from where it was is re-pointed
//! at the folder the operator picked, once the core has checked that folder is a project.
//! Written through the store on a temporary config home, and read back off the disk.
//!
//! The store refuses on a platform that cannot hold it private (ADR 0031), so these run on unix.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use charter_core::machine::{self, Consent, Contribution};

/// A config home of the test's own; never the operator's.
fn machine() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temp config home")
}

/// A project directory charter can open, answered by its real path (a temp dir on macOS is
/// under a link).
fn a_plane(at: &Path) -> PathBuf {
    std::fs::create_dir_all(at.join("workspaces").join("ide")).unwrap();
    std::fs::write(at.join(charter_core::plane::MANIFEST), "").unwrap();
    at.canonicalize().unwrap()
}

/// `plane` remembered second, approved, pinned with `ide` pinned inside it, and the front tab
/// of a window that also holds `other`.
fn remembered(config: &Path, plane: &Path, other: &Path) {
    machine::update(config, |store| {
        store.remember(plane, 1);
        store.approve(plane, 1, Contribution::default());
        store.pin(plane, true).unwrap();
        store.pin_workspace(plane, "ide", true).unwrap();
        store.remember(other, 2);
        store.windows = vec![machine::Window {
            planes: vec![other.to_path_buf(), plane.to_path_buf()],
            active: 1,
        }];
    })
    .unwrap();
}

#[test]
fn a_located_project_keeps_its_place_pins_and_tab_but_is_asked_about_again() {
    charter_core::unsteered!();
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let gone = a_plane(&planes.path().join("was"));
    let other = a_plane(&planes.path().join("other"));
    remembered(config.path(), &gone, &other);
    let moved = planes.path().join("now");
    std::fs::rename(&gone, &moved).unwrap();
    let moved = moved.canonicalize().unwrap();

    let located = machine::locate_project(config.path(), &gone, &moved).unwrap();

    assert_eq!(located, moved);
    let back = machine::read(config.path()).store;
    assert!(
        back.recent(&gone).is_none(),
        "the old path is not remembered"
    );
    let order: Vec<&Path> = back.recents.iter().map(|r| r.plane.as_path()).collect();
    assert_eq!(
        order,
        [other.as_path(), moved.as_path()],
        "in the place it had"
    );
    let entry = back.recent(&moved).unwrap();
    assert!(entry.pinned);
    assert_eq!(entry.pinned_workspaces, ["ide".to_owned()]);
    assert_eq!(
        back.windows,
        [machine::Window {
            planes: vec![other.clone(), moved.clone()],
            active: 1
        }],
        "its tab is put back where it was"
    );
    assert_eq!(
        back.consent(&moved, &Contribution::default()),
        Consent::New,
        "an approval is of what was at a path, so another folder is asked about again"
    );
}

#[test]
fn a_folder_inside_a_project_locates_the_project() {
    charter_core::unsteered!();
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let gone = a_plane(&planes.path().join("was"));
    let other = a_plane(&planes.path().join("other"));
    remembered(config.path(), &gone, &other);
    std::fs::remove_dir_all(&gone).unwrap();
    let found = a_plane(&planes.path().join("found"));

    let located =
        machine::locate_project(config.path(), &gone, &found.join("workspaces").join("ide"))
            .unwrap();

    assert_eq!(located, found);
    assert!(machine::read(config.path()).store.recent(&found).is_some());
}

#[test]
fn a_folder_that_is_not_a_project_is_refused_and_nothing_changes() {
    charter_core::unsteered!();
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let gone = a_plane(&planes.path().join("was"));
    let other = a_plane(&planes.path().join("other"));
    remembered(config.path(), &gone, &other);
    std::fs::remove_dir_all(&gone).unwrap();
    let before = machine::read(config.path()).store;
    let plain = planes.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();

    for picked in [plain, planes.path().join("not-there")] {
        let why = machine::locate_project(config.path(), &gone, &picked).unwrap_err();
        assert!(why.contains("not a project"), "{why}");
        assert_eq!(machine::read(config.path()).store, before, "{why}");
    }
}

#[test]
fn locating_a_project_charter_already_remembers_folds_the_gone_one_into_it() {
    charter_core::unsteered!();
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let gone = a_plane(&planes.path().join("was"));
    let other = a_plane(&planes.path().join("other"));
    remembered(config.path(), &gone, &other);
    std::fs::remove_dir_all(&gone).unwrap();
    machine::update(config.path(), |store| {
        store.approve(&other, 2, Contribution::default());
        store.pin_workspace(&other, "docs", true).unwrap();
    })
    .unwrap();

    let located = machine::locate_project(config.path(), &gone, &other).unwrap();

    assert_eq!(located, other);
    let back = machine::read(config.path()).store;
    assert_eq!(back.recents.len(), 1, "one entry per project");
    assert_eq!(
        back.consent(&other, &Contribution::default()),
        Consent::Unchanged,
        "the project's own approval is its own, and is kept"
    );
    let entry = back.recent(&other).unwrap();
    assert!(
        entry.pinned,
        "the gone entry's pin is the operator's, and is kept"
    );
    assert_eq!(
        entry.pinned_workspaces,
        ["docs".to_owned(), "ide".to_owned()],
        "its own pins first, then the gone entry's"
    );
    assert_eq!(
        back.windows,
        [machine::Window {
            planes: vec![other.clone()],
            active: 0
        }],
        "one project is one tab"
    );
}

#[test]
fn locating_a_project_charter_does_not_remember_is_refused() {
    charter_core::unsteered!();
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let found = a_plane(&planes.path().join("found"));

    let why =
        machine::locate_project(config.path(), Path::new("/planes/never"), &found).unwrap_err();

    assert!(why.contains("does not remember"), "{why}");
    assert!(machine::read(config.path()).store.recents.is_empty());
}

#[test]
fn a_project_whose_disk_came_back_located_where_it_was_is_left_as_it_was() {
    charter_core::unsteered!();
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let gone = a_plane(&planes.path().join("was"));
    let other = a_plane(&planes.path().join("other"));
    remembered(config.path(), &gone, &other);
    let before = machine::read(config.path()).store;

    assert_eq!(
        machine::locate_project(config.path(), &gone, &gone).unwrap(),
        gone
    );

    assert_eq!(
        machine::read(config.path()).store,
        before,
        "approval and all"
    );
}
