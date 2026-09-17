//! Checks against the committed fixture planes, which the Python charter itself wrote
//! (`tests/fixtures/planes/generate.py`). They are the oracle that needs no Python to read:
//! whatever charter put in these files is what a Rust reader has to agree with.

use std::path::{Path, PathBuf};

fn planes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes")
}

fn manifests() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![planes()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("fixture planes are committed") {
            let path = entry.expect("readable fixture entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "workspace.json") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_fixture_manifest_is_owned_by_charter_because_our_digest_is_the_one_it_stored() {
    let found = manifests();
    assert!(!found.is_empty(), "no workspace.json in the fixture planes");

    for path in found {
        let text = std::fs::read_to_string(&path).expect("readable manifest");
        let doc: serde_json::Value = serde_json::from_str(&text).expect("manifest is JSON");
        let stored = doc
            .get("charter_generated")
            .and_then(|v| v.as_str())
            .expect("charter writes charter_generated on every write");

        assert_eq!(
            charter_core::manifest::digest(&doc),
            stored,
            "digest mismatch for {}",
            path.display()
        );
        assert_eq!(
            charter_core::manifest::ownership(Some(&text)),
            charter_core::manifest::Ownership::Charter,
            "{} should read as charter's own",
            path.display()
        );
    }
}

#[test]
fn a_fixture_manifest_read_and_written_back_is_byte_identical() {
    for path in manifests() {
        let text = std::fs::read_to_string(&path).expect("readable manifest");
        let doc: serde_json::Value = serde_json::from_str(&text).expect("manifest is JSON");

        assert_eq!(
            charter_core::pyjson::dumps_indent2(&doc),
            text,
            "round trip changed {}",
            path.display()
        );
    }
}
