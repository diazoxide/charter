//! **You › This machine** (ST-2, #1226): what the machine store lets the operator take back —
//! a recent project forgotten, a project's approval revoked, a workspace pin put back where it
//! was — each written through the store on a temporary config home, and read back off the disk.
//!
//! The store refuses on a platform that cannot hold it private (ADR 0031), so these run on unix.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use charter_core::machine::{self, Consent, Contribution};

/// A config home of the test's own; never the operator's.
fn machine() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temp config home")
}

/// A project directory charter can open: its manifest, and one workspace per name.
fn a_plane(at: &Path, workspaces: &[&str]) -> PathBuf {
    std::fs::create_dir_all(at).unwrap();
    std::fs::write(at.join(charter_core::plane::MANIFEST), "").unwrap();
    for name in workspaces {
        std::fs::create_dir_all(at.join("workspaces").join(name)).unwrap();
    }
    at.to_path_buf()
}

/// `plane` remembered, approved, pinned, with `ide` pinned inside it.
fn approved_and_pinned(config: &Path, plane: &Path) {
    machine::update(config, |store| {
        store.approve(plane, 1, Contribution::default());
        store.pin(plane, true).unwrap();
        store.pin_workspace(plane, "ide", true).unwrap();
        store.windows = vec![machine::Window {
            planes: vec![plane.to_path_buf()],
            active: 0,
        }];
    })
    .unwrap();
}

#[test]
fn a_forgotten_project_is_gone_from_the_disk_with_its_approval_pins_and_tab() {
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let plane = a_plane(&planes.path().join("p"), &["ide"]);
    approved_and_pinned(config.path(), &plane);

    assert!(machine::forget_project(config.path(), &plane).unwrap());

    let back = machine::read(config.path()).store;
    assert!(back.recent(&plane).is_none(), "no longer a recent");
    assert!(back.windows.is_empty(), "nor a tab to put back");
    assert_eq!(
        back.consent(&plane, &Contribution::default()),
        Consent::New,
        "a forgotten project is asked about again"
    );
    assert!(
        !machine::forget_project(config.path(), &plane).unwrap(),
        "forgetting what is not remembered changes nothing, and says so"
    );
}

#[test]
fn a_project_that_is_gone_from_the_disk_can_still_be_forgotten() {
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let plane = a_plane(&planes.path().join("p"), &[]);
    machine::update(config.path(), |store| store.remember(&plane, 1)).unwrap();
    std::fs::remove_dir_all(&plane).unwrap();

    assert!(machine::forget_project(config.path(), &plane).unwrap());

    assert!(machine::read(config.path()).store.recents.is_empty());
}

#[test]
fn a_revoked_approval_is_asked_for_again_and_the_project_stays_remembered() {
    let config = machine();
    let planes = tempfile::tempdir().unwrap();
    let plane = a_plane(&planes.path().join("p"), &["ide"]);
    approved_and_pinned(config.path(), &plane);

    assert!(machine::revoke_approval(config.path(), &plane).unwrap());

    let back = machine::read(config.path()).store;
    let entry = back.recent(&plane).expect("still a recent");
    assert!(entry.trust.is_none());
    assert!(
        entry.pinned,
        "its pin is the operator's, not the approval's"
    );
    assert_eq!(entry.pinned_workspaces, vec!["ide".to_owned()]);
    assert_eq!(back.consent(&plane, &Contribution::default()), Consent::New);
    assert!(
        !machine::revoke_approval(config.path(), &plane).unwrap(),
        "revoking what was never approved changes nothing, and says so"
    );
}

#[test]
fn revoking_a_project_charter_does_not_remember_remembers_nothing() {
    let config = machine();

    assert!(!machine::revoke_approval(config.path(), Path::new("/planes/never")).unwrap());

    assert!(machine::read(config.path()).store.recents.is_empty());
}

#[test]
fn an_unpinned_workspace_is_pinned_back_in_its_place() {
    let mut store = machine::Store::default();
    let plane = Path::new("/planes/p");
    store.remember(plane, 1);
    for name in ["a", "b", "c"] {
        store.pin_workspace(plane, name, true).unwrap();
    }
    store.pin_workspace(plane, "b", false).unwrap();

    assert_eq!(store.pin_workspace_at(plane, "b", 1), Ok(true));
    assert_eq!(
        store.recent(plane).unwrap().pinned_workspaces,
        ["a", "b", "c"].map(str::to_owned)
    );
    assert_eq!(
        store.pin_workspace_at(plane, "b", 0),
        Ok(false),
        "a pin already there keeps its place"
    );

    store.pin_workspace(plane, "a", false).unwrap();
    assert_eq!(store.pin_workspace_at(plane, "a", 9), Ok(true));
    assert_eq!(
        store.recent(plane).unwrap().pinned_workspaces,
        ["b", "c", "a"].map(str::to_owned),
        "a place past the end is the end"
    );
}

#[test]
fn pinning_back_in_place_keeps_every_rule_a_pin_has() {
    let mut store = machine::Store::default();
    let plane = Path::new("/planes/p");

    assert!(
        store.pin_workspace_at(plane, "a", 0).is_err(),
        "a project charter does not remember"
    );
    store.remember(plane, 1);
    assert!(
        store.pin_workspace_at(plane, "a/b", 0).is_err(),
        "a name that is a path"
    );
    for n in 0..machine::MOST_PINNED_WORKSPACES {
        store.pin_workspace(plane, &format!("w{n}"), true).unwrap();
    }
    assert!(
        store.pin_workspace_at(plane, "one-more", 0).is_err(),
        "past the bound"
    );
}
