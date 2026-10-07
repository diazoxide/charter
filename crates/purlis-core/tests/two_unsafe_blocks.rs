//! **Two `unsafe` blocks in the whole workspace, each one the operator allowed by name.**
//!
//! `unsafe_code` is `deny` rather than `forbid` for exactly two functions:
//!
//! 1. `purlis_core::executor::inherit_nothing_else`, the `pre_exec` that closes every
//!    descriptor an extension's program would otherwise inherit (the operator, 2026-09-23:
//!    "Allow one audited block").
//! 2. `purlis_same_user`'s `ancestry::pidinfo::read`, the `proc_pidinfo` call that reads a
//!    process's parent and start time in-process on macOS, where it was one `/bin/ps` run per
//!    hook line before (the operator's ruling D-1407-8, 2026-10-07: a second audited block,
//!    in `purlis-same-user`, for `proc_pidinfo`).
//!
//! `deny` can be allowed anywhere, so this is what stops a third allow from arriving unnoticed:
//! it reads every Rust file in the workspace and fails on any `unsafe` block, function, impl,
//! trait or extern, and any mention of the lint, outside those two functions. A third block is
//! a new ruling, not an edit to [`AUDITED`].

use std::path::{Path, PathBuf};

/// One block the operator allowed: the file it is in, the function it is the whole of, and
/// exactly the lines there that write or allow `unsafe`, in order.
struct Audited {
    file: &'static str,
    function: &'static str,
    lines: [&'static str; 2],
}

/// The two, each named. Each is the allow on its function and that function's one block.
const AUDITED: [Audited; 2] = [
    Audited {
        file: "crates/purlis-core/src/executor.rs",
        function: "fn inherit_nothing_else(",
        lines: ["unsafe_code,", "unsafe {"],
    },
    Audited {
        file: "crates/same-user/src/ancestry.rs",
        function: "fn read(",
        lines: [
            "unsafe_code,",
            "let got = unsafe { libc::proc_pidinfo(pid, flavor, 0, bytes.as_mut_ptr().cast(), wanted) };",
        ],
    },
];

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
fn unsafe_code_appears_only_in_the_two_audited_functions() {
    purlis_core::unsteered!();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("app/src-tauri"), &mut files);
    assert!(files.len() > 50, "the walk found {} files", files.len());

    let this = Path::new(file!()).file_name().expect("this file's name");
    let mut found = Vec::new();
    let mut allowed: [Vec<(usize, String)>; 2] = [Vec::new(), Vec::new()];
    let mut named: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
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
        let audited = AUDITED
            .iter()
            .position(|audited| at.ends_with(audited.file));
        for (n, line) in text.lines().enumerate() {
            if let Some(which) = audited
                && line.trim_start().starts_with(AUDITED[which].function)
            {
                named[which].push(n + 1);
            }
            if is_unsafe(line) {
                match audited {
                    Some(which) => allowed[which].push((n + 1, line.trim().to_owned())),
                    None => found.push(format!("{at}:{}: {}", n + 1, line.trim())),
                }
            }
        }
    }

    assert!(
        found.is_empty(),
        "unsafe code outside the two audited blocks (see the workspace Cargo.toml):\n{}",
        found.join("\n")
    );
    for (which, audited) in AUDITED.iter().enumerate() {
        let (file, function) = (audited.file, audited.function);
        // In each file: the allow on the function, and its one block. Nothing more.
        let lines: Vec<&str> = allowed[which]
            .iter()
            .map(|(_, line)| line.as_str())
            .collect();
        assert_eq!(
            lines, audited.lines,
            "the unsafe in {file} is no longer the one audited block: {:?}",
            allowed[which]
        );
        // And the two are the named function's: the allow just above its one definition, and
        // the block within a screen below it.
        let [at] = named[which][..] else {
            panic!(
                "`{function}` is defined {} times in {file}",
                named[which].len()
            );
        };
        let (allow, block) = (allowed[which][0].0, allowed[which][1].0);
        assert!(
            allow < at && at - allow < 6,
            "the allow in {file} (line {allow}) is not on `{function}` (line {at})"
        );
        assert!(
            at < block && block - at < 60,
            "the unsafe block in {file} (line {block}) is not in `{function}` (line {at})"
        );
    }
}
