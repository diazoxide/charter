//! `charter persona sync-agents` against a REAL plane's personas: a re-sync must reproduce,
//! byte for byte, the `.claude/agents/*.md` that plane has committed.
//!
//! The recorded scenarios (`persona-sync-agents-*`) hold the rendering to what the Python
//! charter wrote for the fixture planes. This holds it to what an operator's plane actually
//! carries — every persona, every field it uses — which is the claim that matters: the first
//! `sync-agents` charter-app runs in an existing plane must produce no diff.
//!
//! **Ignored, and pointed at a plane by hand**, because the planes it is worth running on are
//! the operator's own and are not this repository's to vendor:
//!
//! ```sh
//! CHARTER_SYNC_AGENTS_CORPUS=/path/to/plane \
//!   cargo test -p charter-cli --test sync_agents_corpus -- --ignored
//! ```
//!
//! Nothing is written into that plane. Its `personas/`, `.claude/agents/`, `charter.toml`
//! and the two files a rendering reads from its state directory (`.charter/mcp-approved.json`
//! and `.charter/vaults.json`, when present) are copied into a scratch directory, the binary
//! runs there, and the scratch copy is compared with the original.
//!
//! A committed agent that its own persona has moved on from (edited after the last sync) is
//! a difference here too, and correctly so: it is a file a re-sync changes. Such a file is
//! named, with the first line that differs, so it can be checked against what the Python
//! charter's own re-sync does to it.

use std::path::Path;
use std::process::Command;

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let dest = to.join(entry.file_name());
        let meta = std::fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() {
            copy_tree(&path, &dest);
        } else if meta.is_file() {
            std::fs::copy(&path, &dest).unwrap();
        }
    }
}

fn agents(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(reader) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in reader.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") {
            out.insert(name, std::fs::read(entry.path()).unwrap());
        }
    }
    out
}

fn first_difference(a: &[u8], b: &[u8]) -> String {
    let (a, b) = (String::from_utf8_lossy(a), String::from_utf8_lossy(b));
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}:\n  committed: {x}\n  re-synced: {y}", i + 1);
        }
    }
    format!(
        "{} lines committed, {} re-synced",
        a.lines().count(),
        b.lines().count()
    )
}

#[test]
#[ignore = "runs against a real plane named by CHARTER_SYNC_AGENTS_CORPUS"]
fn a_resync_of_a_real_planes_personas_reproduces_its_committed_agents() {
    let Some(source) = std::env::var_os("CHARTER_SYNC_AGENTS_CORPUS") else {
        panic!("set CHARTER_SYNC_AGENTS_CORPUS to the plane whose agents to compare against");
    };
    let source = Path::new(&source);
    let scratch = tempfile::tempdir().unwrap();
    let plane = scratch.path().join("plane");
    copy_tree(&source.join("personas"), &plane.join("personas"));
    copy_tree(
        &source.join(".claude/agents"),
        &plane.join(".claude/agents"),
    );
    std::fs::copy(source.join("charter.toml"), plane.join("charter.toml")).unwrap();
    for rel in [
        "vaults.json",
        ".charter/mcp-approved.json",
        ".charter/vaults.json",
    ] {
        if source.join(rel).is_file() {
            std::fs::create_dir_all(plane.join(rel).parent().unwrap()).unwrap();
            std::fs::copy(source.join(rel), plane.join(rel)).unwrap();
        }
    }

    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(["persona", "sync-agents"])
        .current_dir(&plane)
        .env("CHARTER_ROOT", &plane);
    for name in ["CHARTER_HOME", "CHARTER_PERSONA", "CLAUDE_CODE_SESSION_ID"] {
        command.env_remove(name);
    }
    let out = command.output().expect("the binary runs");
    assert!(
        out.status.success(),
        "sync-agents failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let committed = agents(&source.join(".claude/agents"));
    let synced = agents(&plane.join(".claude/agents"));
    let mut differences = Vec::new();
    for (name, want) in &committed {
        match synced.get(name) {
            None => differences.push(format!("{name}: removed by the re-sync")),
            Some(got) if got != want => {
                differences.push(format!("{name}: {}", first_difference(want, got)))
            }
            Some(_) => {}
        }
    }
    for name in synced.keys().filter(|n| !committed.contains_key(*n)) {
        differences.push(format!("{name}: written by the re-sync, not committed"));
    }
    println!(
        "{} committed agents, {} identical after the re-sync",
        committed.len(),
        committed.len()
            - differences
                .iter()
                .filter(|d| !d.contains("not committed"))
                .count()
    );
    assert!(
        differences.is_empty(),
        "a re-sync changes what {} has committed:\n{}",
        source.display(),
        differences.join("\n")
    );
}
