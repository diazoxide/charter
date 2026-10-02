//! The human scopes' credentials are files, one per scope, each `0600` in a `0700`
//! directory, minted fresh every time the host starts (ADR 0068 §5). A client reads its own
//! scope's file, whoever started the host, and a start rotates every one of them.

// File modes are a unix fact. Windows is not ported yet (ADR 0068, *Later decisions*).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use charter_session_protocol::auth::{Credential, Credentials, Scope};

fn mode(path: &std::path::Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn minting_writes_one_private_file_per_scope_that_a_client_reads_back() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("charterd");

    let held = Credentials::mint_into(&dir).unwrap();

    assert_eq!(mode(&dir), 0o700);
    for scope in Scope::ALL {
        let file = dir.join(scope.word());
        assert_eq!(mode(&file), 0o600, "{scope}");
        let read = Credential::read(&dir, scope).unwrap();
        assert_eq!(&read, held.of(scope), "{scope}");
        assert!(held.admits(scope, &read));
    }
}

#[test]
fn a_new_start_rotates_every_credential_and_the_old_ones_stop_counting() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("charterd");
    let before = Credentials::mint_into(&dir).unwrap();
    let old = Credential::read(&dir, Scope::Approval).unwrap();

    let after = Credentials::mint_into(&dir).unwrap();

    let new = Credential::read(&dir, Scope::Approval).unwrap();
    assert_ne!(old, new);
    assert!(!after.admits(Scope::Approval, &old));
    assert!(after.admits(Scope::Approval, &new));
    assert!(before.admits(Scope::Approval, &old));
}

#[test]
fn a_directory_left_open_to_others_is_closed_again_and_a_link_in_its_place_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("charterd");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    Credentials::mint_into(&dir).unwrap();
    assert_eq!(mode(&dir), 0o700);

    let elsewhere = home.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let linked = home.path().join("linked");
    std::os::unix::fs::symlink(&elsewhere, &linked).unwrap();
    assert!(Credentials::mint_into(&linked).is_err());
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
}

#[test]
fn reading_a_scope_with_no_file_is_an_error_not_an_empty_credential() {
    let home = tempfile::tempdir().unwrap();
    assert!(Credential::read(home.path(), Scope::FleetMcp).is_err());
}
