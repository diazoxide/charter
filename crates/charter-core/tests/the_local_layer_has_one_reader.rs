//! **One way into `charter.local.toml`'s layer**, so a reader added later cannot forget the check
//! (charter-app#308, ADR 0048).
//!
//! A Local file git would carry must decide nothing, and the check that says so lives in one
//! function, `settings::layer_text`. #308 was three readers that each read the file themselves
//! and so each skipped it. This reads every Rust file in the workspace and fails on production
//! code that reads the local file by name anywhere but the two places that own it: `settings.rs`
//! (`layer_text`, and the settings tab, which shows the file whatever it says) and `profiles.rs`
//! (the profiles loader, which refuses the profiles by the same check where they are used).
//!
//! It is a tripwire, not a proof: a reader that reaches the file by a path it built some other
//! way is not seen. It catches the shapes a reader is written in — the read and the name in one
//! statement, however rustfmt wraps it — and the loader's raw reader called from outside it.

use std::path::{Path, PathBuf};

/// The files that may read the local file by name.
const OWNERS: &[&str] = &[
    "crates/charter-core/src/settings.rs",
    "crates/charter-core/src/profiles.rs",
];

/// How many lines one statement may be wrapped over, as rustfmt wraps a call on a long line.
const STATEMENT: usize = 4;

/// Whether `code` names the local file.
fn names_the_file(code: &str) -> bool {
    code.contains("LOCAL_FILE") || code.contains("charter.local.toml")
}

/// Whether `code` reads a file: a filesystem read, or a `text(…)` helper handed the file's name.
fn reads(code: &str) -> bool {
    code.contains("read_to_string")
        || code.contains("fs::read(")
        || code
            .split("text(")
            .skip(1)
            .any(|after| names_the_file(after.split(')').next().unwrap_or_default()))
}

/// The production lines of `text` that read the local file by name, 1-based. A statement is the
/// line that names the file with up to [`STATEMENT`] - 1 lines before it; comments and inline
/// `#[cfg(test)] mod … { … }` blocks are left out.
fn reading_lines(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let mut code: Vec<Option<&str>> = Vec::with_capacity(lines.len());
    let mut in_tests: Option<i64> = None;
    for (at, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(depth) = in_tests.as_mut() {
            *depth += line.matches('{').count() as i64 - line.matches('}').count() as i64;
            if *depth <= 0 {
                in_tests = None;
            }
            code.push(None);
            continue;
        }
        if trimmed.starts_with("#[cfg(test)]")
            && lines.get(at + 1).is_some_and(|next| {
                let next = next.trim();
                next.starts_with("mod ") && next.ends_with('{')
            })
        {
            in_tests = Some(0);
            code.push(None);
            continue;
        }
        code.push((!trimmed.starts_with("//")).then_some(trimmed));
    }
    let mut out = Vec::new();
    for (at, line) in code.iter().enumerate() {
        let Some(line) = line else { continue };
        if line.contains("read_local(") {
            out.push(at + 1);
            continue;
        }
        if !names_the_file(line) {
            continue;
        }
        let from = at.saturating_sub(STATEMENT - 1);
        let statement: String = code[from..=at].iter().flatten().copied().collect();
        if reads(&statement) {
            out.push(at + 1);
        }
    }
    out
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
        for at in reading_lines(&text) {
            found.push(format!("{shown}:{at}"));
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
fn the_check_sees_each_way_a_reader_is_written() {
    charter_core::unsteered!();
    // The shapes #308 found, and the one rustfmt makes of a long line.
    let one_line =
        "        Self::from_text(text(COMMITTED_FILE).as_deref(), text(LOCAL_FILE).as_deref())\n";
    let direct = "    let local = std::fs::read_to_string(root.join(LOCAL_FILE)).ok();\n";
    let wrapped = "    let local = std::fs::read_to_string(\n        \
                   plane_root.join(charter_core::profiles::LOCAL_FILE),\n    )\n";
    let raw = "    let local = profiles::read_local(root);\n";
    for reader in [one_line, direct, wrapped, raw] {
        assert!(!reading_lines(reader).is_empty(), "missed: {reader}");
    }

    // And what reads nothing: the one way in, a comment, a sentence that names the file.
    let fine = "            layer_text(root, Which::Local).text(),\n    \
                // text(LOCAL_FILE) is how it used to be read\n    \
                format!(\"{} in charter.local.toml\", shown_text(x))\n";
    assert_eq!(reading_lines(fine), Vec::<usize>::new());
}

#[test]
fn a_reader_after_an_inline_test_module_is_still_seen() {
    charter_core::unsteered!();
    let text = "#[cfg(test)]\nmod tests {\n    fn t() { let _ = text(LOCAL_FILE); }\n}\n\n\
                fn later(root: &Path) {\n    let _ = std::fs::read_to_string(root.join(LOCAL_FILE));\n}\n";

    assert_eq!(reading_lines(text), vec![7]);
}
