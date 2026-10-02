//! OB-15 (#687), the source half of "a no-account run shows no Charter host except the updater
//! and the static signed files beside it".
//!
//! The run halves (`every_third_party_call_is_listed_in_the_network_log.rs`, and the app's
//! `updates::tests::the_real_check_lists_its_read_in_the_network_log`) show what one run listed.
//! This shows no other feature could have listed more. Charter's address is reached three ways in
//! code: its literal spelling, `report::UPSTREAM`, and the updater's `REPO` and
//! `Channel::endpoint`. Each is named only in the files [`ALLOWED`] lists, each for a reason. A
//! feature that wants to reach Charter has to name one of them, so it fails here first, where its
//! author has to say why it may (ADR 0083 §9). An allowance nothing uses any more fails too, so
//! the list cannot grow stale.
//!
//! Comments are skipped (a doc may cite Charter's repository), and so are test files and a
//! file's own test module. The window's links to the releases page are opened in the
//! operator's browser, never fetched by charter, and are not Rust.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// How code names Charter's address.
const WAYS: &[&str] = &[
    "UPSTREAM",
    "updates::REPO",
    ".endpoint()",
    "Channel::endpoint",
];

/// Where Charter's address may be named, and why.
const ALLOWED: &[(&str, &str)] = &[
    (
        "crates/charter-core/src/updates.rs",
        "the updater's channels and their manifests (ADR 0042)",
    ),
    (
        "app/src-tauri/src/updates.rs",
        "the signed updater: its manifest read and its bundle",
    ),
    (
        "crates/charter-core/src/adopt.rs",
        "`charter update --channel` says which manifest the app will read; it reads nothing",
    ),
    (
        "crates/charter-core/src/report.rs",
        "`charter report`, filing on Charter's tracker as the operator (ADR 0059)",
    ),
    (
        "crates/charter-cli/src/report.rs",
        "`charter report`'s command, naming the tracker it would file on",
    ),
    (
        "crates/charter-core/src/forge.rs",
        "`charter report`'s `gh`, listing its call in the network log as Charter's",
    ),
    (
        "crates/charter-core/src/netlog.rs",
        "the network log telling a Charter address from a third party's; it calls nothing",
    ),
];

/// The shipped Rust sources: the core, the command and the app.
const SHIPPED: &[&str] = &[
    "crates/charter-core/src",
    "crates/charter-cli/src",
    "app/src-tauri/src",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// Every shipped line that names Charter's address, as `file:line: code`, keyed by file.
fn named(repo: &Path, address: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for dir in SHIPPED {
        rust_files(&repo.join(dir), &mut files);
    }
    assert!(
        files.len() > 100,
        "the walk found too few sources: {}",
        files.len()
    );
    let mut named = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let stem = file.file_stem().unwrap().to_string_lossy();
        if stem.ends_with("_tests") || stem == "tests" || relative.contains("/tests/") {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            let code = line.trim_start();
            // A file's own test module, at its end, is not shipped.
            if code == "#[cfg(test)]"
                && lines
                    .get(n + 1)
                    .is_some_and(|next| next.trim_start().starts_with("mod "))
            {
                break;
            }
            if code.starts_with("//") {
                continue;
            }
            let literal = code.match_indices(address).any(|(at, _)| {
                // `diazoxide/charter` followed by a path end, never `diazoxide/charter-plane`.
                let next = code[at + address.len()..].chars().next();
                !next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            });
            if literal || WAYS.iter().any(|way| code.contains(way)) {
                named.push((relative.clone(), format!("{relative}:{}: {code}", n + 1)));
            }
        }
    }
    named
}

#[test]
fn only_the_updater_and_report_name_charters_own_address() {
    charter_core::unsteered!();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let address = charter_core::report::UPSTREAM;
    let found = named(&repo, address);
    let outside: Vec<&str> = found
        .iter()
        .filter(|(file, _)| !ALLOWED.iter().any(|(allowed, _)| allowed == file))
        .map(|(_, line)| line.as_str())
        .collect();
    assert!(
        outside.is_empty(),
        "Charter's own address is named outside the updater and report, so a run could reach \
         Charter without an account (ADR 0083 §9):\n{}",
        outside.join("\n")
    );
    let used: BTreeSet<&str> = found.iter().map(|(file, _)| file.as_str()).collect();
    for (allowed, why) in ALLOWED {
        assert!(
            used.contains(allowed),
            "{allowed} is allowed to name Charter's address ({why}) and no longer does: remove it"
        );
    }
}
