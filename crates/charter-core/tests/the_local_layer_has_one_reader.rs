//! **One way into `charter.local.toml`'s layer**, so a reader added later cannot forget the check
//! (charter-app#308, ADR 0048).
//!
//! A Local file git would carry must decide nothing, and the check that says so lives in one
//! function, `settings::layer_text`. #308 was three readers that each read the file themselves
//! and so each skipped it. This reads every Rust file in the workspace and fails on production
//! code that reads the local file by name anywhere but the two places that own it: `settings.rs`
//! (`layer_text`, and the settings tab, which shows the file whatever it says) and `profiles.rs`
//! (the profiles loader, which refuses the profiles by the same check where they are used).

use std::path::{Path, PathBuf};

/// The files that may read the local file by name.
const OWNERS: &[&str] = &[
    "crates/charter-core/src/settings.rs",
    "crates/charter-core/src/profiles.rs",
];

/// Whether `line` reads the local file by name: a read and the file's name in one line, or the
/// profiles loader's raw reader called from outside it.
fn reads_the_local_file(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") {
        return false;
    }
    let names = code.contains("LOCAL_FILE") || code.contains("charter.local.toml");
    let reads = ["read_to_string", "fs::read(", "text("]
        .iter()
        .any(|read| code.contains(read));
    (names && reads) || code.contains("read_local(")
}

fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Tests read the file to see what a writer wrote; they decide nothing.
        if name == "target" || name == "node_modules" || name == "tests" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            rust_files(&path, into);
        } else if name.ends_with(".rs") && name != "tests.rs" {
            into.push(path);
        }
    }
}

#[test]
fn only_the_settings_and_profiles_modules_read_the_local_file_by_name() {
    charter_core::unsteered!();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("app/src-tauri"), &mut files);
    assert!(files.len() > 50, "the walk found {} files", files.len());

    let mut found = Vec::new();
    for file in &files {
        let shown = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        if OWNERS.contains(&shown.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            // A module's own unit tests sit at its end, in an inline `mod … {` behind this line.
            // Any other `#[cfg(test)]` item is one line, and the code after it is still read.
            if line.trim_start().starts_with("#[cfg(test)]")
                && lines.get(at + 1).is_some_and(|next| {
                    let next = next.trim();
                    next.starts_with("mod ") && next.ends_with('{')
                })
            {
                break;
            }
            if reads_the_local_file(line) {
                found.push(format!("{shown}:{}: {}", at + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "read charter.local.toml through charter_core::settings::layer_text, which leaves out a \
         file git would carry:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_check_sees_each_way_the_readers_used_to_read_the_file() {
    charter_core::unsteered!();
    // The shapes #308 found, so the guard above is seen to catch them.
    assert!(reads_the_local_file(
        "        Self::from_text(text(COMMITTED_FILE).as_deref(), text(LOCAL_FILE).as_deref())"
    ));
    assert!(reads_the_local_file(
        "    let local = std::fs::read_to_string(root.join(LOCAL_FILE)).ok();"
    ));
    assert!(reads_the_local_file(
        "    let local = profiles::read_local(root);"
    ));
    assert!(!reads_the_local_file(
        "            layer_text(root, Which::Local).as_deref(),"
    ));
    assert!(!reads_the_local_file(
        "    // text(LOCAL_FILE) is how it used to be read"
    ));
}
