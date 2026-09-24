//! **One `unsafe` block in the whole workspace, and it is the one the operator allowed.**
//!
//! `unsafe_code` is `deny` rather than `forbid` for exactly one function —
//! `executor::inherit_nothing_else`, the `pre_exec` that closes every descriptor an extension's
//! program would otherwise inherit (the operator, 2026-09-23: "Allow one audited block"). `deny`
//! can be allowed anywhere, so this is what stops a second allow from arriving unnoticed: it
//! reads every Rust file in the workspace and fails on any `unsafe` block, function, impl,
//! trait or extern, and any mention of the lint, outside that one function.

use std::path::{Path, PathBuf};

/// Where the one allowed block is, and what it may say.
const ALLOWED_FILE: &str = "crates/charter-core/src/executor.rs";

/// What counts as writing `unsafe` code, or allowing it, on a line that is not a comment.
fn is_unsafe(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") {
        return false;
    }
    let words = [
        "unsafe {",
        "unsafe fn",
        "unsafe impl",
        "unsafe trait",
        "unsafe extern",
    ];
    words.iter().any(|word| code.contains(word))
        || code.contains("unsafe_code")
        || code.contains("unsafe_op_in_unsafe_fn")
}

fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "target" || name == "node_modules" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            rust_files(&path, into);
        } else if name.ends_with(".rs") {
            into.push(path);
        }
    }
}

#[test]
fn unsafe_code_appears_only_in_the_one_audited_function() {
    charter_core::unsteered!();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("app/src-tauri"), &mut files);
    assert!(files.len() > 50, "the walk found {} files", files.len());

    let this = Path::new(file!()).file_name().expect("this file's name");
    let mut found = Vec::new();
    let mut allowed = Vec::new();
    for file in &files {
        if file.file_name() == Some(this) {
            continue;
        }
        let text = std::fs::read_to_string(file).expect("a Rust file");
        let at = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let at = at.trim_start_matches("./").to_owned();
        for (n, line) in text.lines().enumerate() {
            if is_unsafe(line) {
                if at.ends_with(ALLOWED_FILE) {
                    allowed.push((n + 1, line.trim().to_owned()));
                } else {
                    found.push(format!("{at}:{}: {}", n + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        found.is_empty(),
        "unsafe code outside the one audited block (see the workspace Cargo.toml):\n{}",
        found.join("\n")
    );
    // In the executor: the allow on the function, and its one block. Nothing more.
    let lines: Vec<&str> = allowed.iter().map(|(_, line)| line.as_str()).collect();
    assert_eq!(
        lines,
        ["unsafe_code,", "unsafe {"],
        "the executor's unsafe grew beyond one block: {allowed:?}"
    );
    // And the two are the one function's: the block within a screen of the allow.
    assert!(
        allowed[1].0 - allowed[0].0 < 60,
        "the unsafe block is not in the allowed function: {allowed:?}"
    );
}
