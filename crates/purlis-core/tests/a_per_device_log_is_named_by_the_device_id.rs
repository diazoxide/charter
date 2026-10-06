//! FD-25 (#662, ADR 0066): a per-device log is named by the device id, and the hostname is only
//! a label. Two machines that share a hostname write two logs, and a machine that is renamed
//! keeps writing the one it had.

use std::path::Path;

use chrono::TimeZone;
use purlis_core::{dispatch, machine, pieces};

fn when() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap()
}

/// The per-device log files in `dir`, by name.
fn logs(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|read| {
            read.filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn plane() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "").unwrap();
    std::fs::create_dir_all(dir.path().join("personas")).unwrap();
    std::fs::create_dir_all(dir.path().join("workspaces/alpha")).unwrap();
    dir
}

#[test]
fn two_machines_with_the_same_hostname_write_separate_logs() {
    purlis_core::unsteered!();
    let plane = plane();
    let laptop_a = tempfile::tempdir().unwrap();
    let laptop_b = tempfile::tempdir().unwrap();
    let a = machine::device_id(laptop_a.path()).unwrap();
    let b = machine::device_id(laptop_b.path()).unwrap();

    for config in [laptop_a.path(), laptop_b.path()] {
        let name = dispatch::log_name(Some(config), "MacBook-Pro");
        dispatch::record(plane.path(), "devops", when(), &name).expect("a dispatch row");
        let who = pieces::Who {
            session: None,
            persona: None,
            host: "MacBook-Pro".into(),
            log: name.clone(),
        };
        pieces::record(
            plane.path(),
            "alpha",
            pieces::Event::Claimed,
            "svc",
            "fix",
            None,
            &who,
            when(),
        )
        .expect("a claim");
    }

    let mut want = vec![format!("2026-10.{a}.jsonl"), format!("2026-10.{b}.jsonl")];
    want.sort();
    assert_eq!(logs(&plane.path().join("personas/_dispatch")), want);
    let mut want = vec![format!("{a}.jsonl"), format!("{b}.jsonl")];
    want.sort();
    assert_eq!(logs(&plane.path().join("workspaces/alpha/pieces")), want);
}

#[test]
fn a_renamed_machine_keeps_one_log() {
    purlis_core::unsteered!();
    let plane = plane();
    let laptop = tempfile::tempdir().unwrap();
    let id = machine::device_id(laptop.path()).unwrap();

    for host in ["MacBook-Pro", "aarons-laptop"] {
        let name = dispatch::log_name(Some(laptop.path()), host);
        dispatch::record(plane.path(), "devops", when(), &name).expect("a dispatch row");
    }

    assert_eq!(
        logs(&plane.path().join("personas/_dispatch")),
        vec![format!("2026-10.{id}.jsonl")]
    );
    let text = std::fs::read_to_string(
        plane
            .path()
            .join(format!("personas/_dispatch/2026-10.{id}.jsonl")),
    )
    .unwrap();
    assert_eq!(
        text.lines().count(),
        2,
        "both rows are in the one log: {text}"
    );
}

#[test]
fn the_hostname_stays_on_a_claim_as_its_label() {
    purlis_core::unsteered!();
    let plane = plane();
    let laptop = tempfile::tempdir().unwrap();
    let id = machine::device_id(laptop.path()).unwrap();
    let who = pieces::Who {
        session: None,
        persona: None,
        host: "MacBook-Pro".into(),
        log: dispatch::log_name(Some(laptop.path()), "MacBook-Pro"),
    };
    pieces::record(
        plane.path(),
        "alpha",
        pieces::Event::Claimed,
        "svc",
        "fix",
        None,
        &who,
        when(),
    )
    .unwrap();

    let text = std::fs::read_to_string(
        plane
            .path()
            .join(format!("workspaces/alpha/pieces/{id}.jsonl")),
    )
    .unwrap();
    let line: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(line["host"], "MacBook-Pro");
}

#[test]
fn a_log_name_never_mints_a_device_id_and_names_the_host_until_one_is() {
    purlis_core::unsteered!();
    let laptop = tempfile::tempdir().unwrap();

    assert_eq!(
        dispatch::log_name(Some(laptop.path()), "MacBook-Pro"),
        "MacBook-Pro"
    );
    assert_eq!(dispatch::log_name(None, "MacBook-Pro"), "MacBook-Pro");
    assert!(
        !machine::file(laptop.path()).exists(),
        "asking for a log's name wrote the machine store"
    );

    let id = machine::device_id(laptop.path()).unwrap();
    assert_eq!(dispatch::log_name(Some(laptop.path()), "MacBook-Pro"), id);
}
