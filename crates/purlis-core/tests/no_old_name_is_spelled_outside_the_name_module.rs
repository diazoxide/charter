//! **No old name is spelled outside `purlis_core::names`** (#1262, RN-2z).
//!
//! The rename to purlis is an expand–contract refactor. RN-1 put every renamed name in one
//! module, [`purlis_core::names`], with the purlis spelling to write and the old spellings
//! still read. RN-2a…d moved the callers onto it. This is the contract step: a string literal
//! in the repository's Rust that spells an old name — `.charter`, `charter.toml`,
//! `charter.local.toml`, `.charter-scan-allow.toml`, a `CHARTER_` variable, a `charter/`
//! keychain or branch prefix, a `Charter-` trailer, an old marker — fails this test, so the
//! old name cannot leak back in one call site at a time.
//!
//! **What is scanned.** Every `.rs` file under `crates/` and `app/src-tauri/`, string literals
//! only (comments and doc comments are prose about the names and are not read). The spellings
//! looked for are not listed here: they are every old spelling (`reads` and `history`) of the
//! entries in [`names::ALL`] whose kind is a file, folder, marker, keychain prefix, variable
//! prefix, branch prefix or trailer — so a name added there is guarded here the day it is
//! added. A bare word (`charter`, the config home's and the 1Password tag's old spelling) is
//! not looked for: it is the product's name, and every sentence that says it is RN-11a's.
//!
//! **The name module is the only code exception.** `names.rs` and `names/` hold the old
//! spellings because saying them is their job. Everything else that still spells one is on
//! one of two allow-lists below:
//!
//! * [`STILL_SPELLED`] — shipped code, file by file, with the exact number of literals and a
//!   [`Why`] each. The counts are exact, so the list can only SHRINK: a file that spells one
//!   more fails until somebody raises its count in review, and a file that spells one fewer
//!   fails until its count is lowered (or its entry removed).
//! * [`FIXTURES`] — test code, by scope. A test of the compatibility window has to build a
//!   project under the old names; that is a fixture, not a leak.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod old_names;
use old_names::{old_spellings, spells};

/// Why a shipped file may still spell an old name. Each is a ticket that removes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// Text an operator or an agent reads — a refusal, a hint, a help line, a doctor row —
    /// that names an old file or variable. RN-11a (#1269) renames the product's visible words.
    Prose,
    /// A list or a sentence that names both spellings on purpose, so the old one keeps working
    /// through the compatibility window. It goes with the window: RN-14 (#1273).
    Window,
    /// A template of a file committed to a project, written under the old names until the
    /// project is migrated: RN-7 (rename-plane, #1265) rewrites it (#1277).
    CommittedTemplate,
    /// A test-only variable read by shipped code (`CHARTER_TEST_*`, the plane fence, the
    /// probes'): RN-11a moves them with the tests that set them (#1278).
    TestVariable,
    /// A name a shipped artifact outside this code carries: the variable the plugin's hooks
    /// file and the opencode shim read, the shim's own file and text, a bundled extension's
    /// manifest file. It moves with that artifact: RN-8 (plugin, #1266), #1284.
    ArtifactContract,
    /// A `charter/`-shaped name that is none of [`names::ALL`]'s entries: the built-in
    /// namespace of curation actions and panels, the HTTP user agent, the Linux install
    /// folder. Whether each joins the name module or is plain text is #1284's question.
    Unlisted,
}

/// Shipped files that still spell an old name, with how many literals do and why. A file
/// whose literals have two reasons has an entry for each; its counts add up.
///
/// **Shrink only.** Lowering a count, or removing an entry, is the whole of finishing one.
/// Raising one, or adding one, is a decision a reviewer makes on purpose; route the name
/// through [`purlis_core::names`] instead wherever it is not visible text.
#[rustfmt::skip]
const STILL_SPELLED: &[(&str, usize, Why)] = &[
    // Visible text: RN-11a (#1269).
    ("app/src-tauri/src/doctor.rs", 1, Why::Prose),
    ("app/src-tauri/src/planes.rs", 1, Why::Prose),
    ("crates/purlis-core/src/active.rs", 2, Why::Prose),
    ("crates/purlis-core/src/adopt.rs", 2, Why::Prose),
    ("crates/purlis-core/src/alerts.rs", 3, Why::Prose),
    ("crates/purlis-core/src/briefing.rs", 1, Why::Prose),
    ("crates/purlis-core/src/chatenv.rs", 4, Why::Prose),
    ("crates/purlis-core/src/commitguard.rs", 1, Why::Prose),
    ("crates/purlis-core/src/compat.rs", 2, Why::Prose),
    ("crates/purlis-core/src/diffscan.rs", 3, Why::Prose),
    ("crates/purlis-core/src/doctor/config.rs", 9, Why::Prose),
    ("crates/purlis-core/src/doctor/fix.rs", 1, Why::Prose),
    ("crates/purlis-core/src/doctor/git.rs", 2, Why::Prose),
    ("crates/purlis-core/src/doctor/plane.rs", 3, Why::Prose),
    ("crates/purlis-core/src/doctor/profiles.rs", 1, Why::Prose),
    ("crates/purlis-core/src/extension/writes.rs", 1, Why::Prose),
    ("crates/purlis-core/src/fence.rs", 1, Why::Prose),
    ("crates/purlis-core/src/forge.rs", 1, Why::Prose),
    ("crates/purlis-core/src/forge/pr.rs", 1, Why::Prose),
    ("crates/purlis-core/src/gitpolicy.rs", 2, Why::Prose),
    ("crates/purlis-core/src/guest.rs", 1, Why::Prose),
    ("crates/purlis-core/src/harness_declaration.rs", 1, Why::Prose),
    ("crates/purlis-core/src/inventory.rs", 2, Why::Prose),
    ("crates/purlis-core/src/leakguard.rs", 1, Why::Prose),
    ("crates/purlis-core/src/personacmd.rs", 4, Why::Prose),
    ("crates/purlis-core/src/personaverbs/define.rs", 1, Why::Prose),
    ("crates/purlis-core/src/personaverbs/list.rs", 1, Why::Prose),
    ("crates/purlis-core/src/personaverbs/select.rs", 2, Why::Prose),
    ("crates/purlis-core/src/planegit/prsave.rs", 1, Why::Prose),
    ("crates/purlis-core/src/planesave.rs", 1, Why::Prose),
    ("crates/purlis-core/src/profiles.rs", 6, Why::Prose),
    ("crates/purlis-core/src/programs.rs", 1, Why::Prose),
    ("crates/purlis-core/src/renamelocal/busy.rs", 1, Why::Prose),
    ("crates/purlis-core/src/repocmd/clone.rs", 1, Why::Prose),
    ("crates/purlis-core/src/repocmd/status.rs", 1, Why::Prose),
    ("crates/purlis-core/src/sandbox/local.rs", 2, Why::Prose),
    ("crates/purlis-core/src/scaffold/mod.rs", 3, Why::Prose),
    ("crates/purlis-core/src/secrets/cmd.rs", 2, Why::Prose),
    ("crates/purlis-core/src/settings/forges.rs", 3, Why::Prose),
    ("crates/purlis-core/src/wiring.rs", 3, Why::Prose),
    ("crates/purlis-core/src/work/promote.rs", 1, Why::Prose),
    ("crates/purlis-core/src/worktree/mod.rs", 2, Why::Prose),
    ("crates/purlis-core/src/wscmd/select.rs", 3, Why::Prose),
    // Both spellings on purpose, for the window: RN-14 (#1273).
    ("crates/purlis-core/src/chatenv.rs", 1, Why::Window),
    ("crates/purlis-core/src/planegit.rs", 2, Why::Window),
    // Committed-file templates: RN-7 (#1265).
    ("crates/purlis-core/src/scaffold/mod.rs", 1, Why::CommittedTemplate),
    // Test-only variables: #1278.
    ("crates/purlis-cli/src/extensions.rs", 1, Why::TestVariable),
    ("crates/purlis-cli/src/guard.rs", 1, Why::TestVariable),
    ("crates/purlis-cli/src/main.rs", 2, Why::TestVariable),
    ("crates/purlis-core/src/fence.rs", 1, Why::TestVariable),
    ("crates/session-protocol/src/bin/charter-session-peer.rs", 1, Why::TestVariable),
    // Names a shipped non-Rust artifact carries: RN-8 (#1266), #1284.
    // The variable the plugin's hooks and the opencode shim read keeps its old name until 1.0
    // (D-RN8-1), as the hook commands do (D-RN7-11).
    ("crates/purlis-core/src/opencode.rs", 2, Why::ArtifactContract),
    ("crates/purlis-core/src/plugin.rs", 1, Why::ArtifactContract),
    ("crates/persona-statistics/src/lib.rs", 2, Why::ArtifactContract),
    // Not in the name module yet: #1284.
    ("crates/purlis-core/src/curation.rs", 3, Why::Unlisted),
    ("crates/purlis-core/src/forge/http.rs", 1, Why::Unlisted),
    ("crates/purlis-core/src/panel.rs", 1, Why::Unlisted),
    ("crates/purlis-core/src/plugin_install.rs", 1, Why::Unlisted),
];

/// Test code, where an old name is a fixture: a project built under the old names to prove
/// the window still reads them. Clearly scoped — each is a shape of path or of source, and
/// nothing shipped matches one.
///
/// * a file under any `tests/` folder (the integration tests, and test modules kept in one);
/// * a file named `tests.rs`, `*_tests.rs` or `tests_*.rs` (a `#[cfg(test)]` module's file);
/// * a file a `#[cfg(test)]` (or `#[cfg(all(test, …))]`) `mod name;` brings in;
/// * an item — a module, a function — under such an attribute, inside any file;
/// * the files named in [`FIXTURE_FILES`].
const FIXTURES: &str = "tests/, *tests.rs, tests_*.rs, cfg(test) modules and items, FIXTURE_FILES";

/// Whole files, or folders (ending in `/`), that are test fixtures without matching a shape
/// in [`FIXTURES`], and why.
const FIXTURE_FILES: &[(&str, &str)] = &[(
    "crates/extension-probe/",
    "the extension the tests install to prove each capability; never shipped (its lib.rs)",
)];

// ---- the scan ------------------------------------------------------------------------- //

/// A literal that spells an old name: its line, the spelling, and the literal's text.
type Hit = (usize, String, String);

/// A module a test-only attribute brings in from another file: its name and any `#[path]`.
type TestMod = (String, Option<String>);

/// A string literal: the line it starts on, and its text as written (escapes not undone).
#[derive(Debug)]
struct Literal {
    line: usize,
    text: String,
    /// Byte offset in the source.
    at: usize,
}

/// `src` lexed just enough: its string literals, and the source with every literal, comment
/// and char literal blanked out (same length, same line breaks), for finding items.
fn lex(src: &str) -> (Vec<Literal>, String) {
    let bytes = src.as_bytes();
    let mut code = bytes.to_vec();
    let mut out = Vec::new();
    let blank = |code: &mut Vec<u8>, from: usize, to: usize| {
        for b in &mut code[from..to] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    };
    let breaks: Vec<usize> = src.match_indices('\n').map(|(at, _)| at).collect();
    let line_at = |at: usize| breaks.partition_point(|&b| b < at) + 1;
    let ident =
        |at: usize| at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
    let mut i = 0;
    while i < bytes.len() {
        let rest = &src[i..];
        if rest.starts_with("//") {
            let end = rest.find('\n').map_or(bytes.len(), |n| i + n);
            blank(&mut code, i, end);
            i = end;
        } else if rest.starts_with("/*") {
            let mut depth = 0;
            let mut j = i;
            while j < bytes.len() {
                if src[j..].starts_with("/*") {
                    depth += 1;
                    j += 2;
                } else if src[j..].starts_with("*/") {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            blank(&mut code, i, j);
            i = j;
        } else if !ident(i) && raw_open(rest).is_some() {
            let (open, hashes) = raw_open(rest).expect("checked");
            let close = format!("\"{}", "#".repeat(hashes));
            let start = i + open;
            let end = src[start..].find(&close).map_or(bytes.len(), |n| start + n);
            out.push(Literal {
                line: line_at(i),
                text: src[start..end].to_owned(),
                at: i,
            });
            let end = (end + close.len()).min(bytes.len());
            blank(&mut code, i, end);
            i = end;
        } else if bytes[i] == b'"' || (!ident(i) && rest.starts_with("b\"")) {
            let start = i + if bytes[i] == b'"' { 1 } else { 2 };
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'"' {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            out.push(Literal {
                line: line_at(i),
                text: src[start..j.min(bytes.len())].to_owned(),
                at: i,
            });
            let end = (j + 1).min(bytes.len());
            blank(&mut code, i, end);
            i = end;
        } else if bytes[i] == b'\'' {
            // A char literal ('x', '\n', '\'', '\u{..}'); otherwise a lifetime, left as code.
            let len = char_literal(rest);
            if len > 0 {
                blank(&mut code, i, i + len);
            }
            i += len.max(1);
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    (out, String::from_utf8(code).expect("blanking keeps UTF-8"))
}

/// A raw string's opening (`r"`, `r#"`, `br##"` …): its length and its number of `#`s.
fn raw_open(rest: &str) -> Option<(usize, usize)> {
    let after_b = rest.strip_prefix('b').unwrap_or(rest);
    let after_r = after_b.strip_prefix('r')?;
    let hashes = after_r.len() - after_r.trim_start_matches('#').len();
    after_r[hashes..]
        .starts_with('"')
        .then_some((rest.len() - after_r.len() + hashes + 1, hashes))
}

/// The length of the char literal `rest` opens, or 0 when the quote opens a lifetime.
fn char_literal(rest: &str) -> usize {
    let body = &rest[1..];
    if let Some(esc) = body.strip_prefix('\\') {
        let len = if esc.starts_with("u{") {
            esc.find('}').map_or(1, |n| n + 1)
        } else {
            esc.chars().next().map_or(0, char::len_utf8)
        };
        return if esc[len..].starts_with('\'') {
            len + 3
        } else {
            0
        };
    }
    match body.chars().next() {
        Some(c) if body[c.len_utf8()..].starts_with('\'') => c.len_utf8() + 2,
        _ => 0,
    }
}

/// Whether an attribute (its text between `#[` and `]`) compiles its item for tests only.
fn test_only(attr: &str) -> bool {
    let attr: String = attr.chars().filter(|c| !c.is_whitespace()).collect();
    attr.starts_with("cfg(")
        && !attr.contains("not(test")
        && !attr.contains("any(")
        && (attr == "cfg(test)"
            || attr.contains("(test,")
            || attr.contains(",test)")
            || attr.contains(",test,"))
}

/// The byte ranges of `code` (lexed) under a test-only attribute, and the modules such an
/// attribute brings in from other files (`mod x;`), each with any `#[path]` it gives.
fn test_items(code: &str, src: &str) -> (Vec<(usize, usize)>, Vec<TestMod>) {
    let b = code.as_bytes();
    let mut ranges = Vec::new();
    let mut files = Vec::new();
    let mut i = 0;
    while let Some(n) = code[i..].find("#[") {
        let at = i + n;
        let Some(close) = attr_end(b, at + 2) else {
            break;
        };
        i = close + 1;
        if !test_only(&code[at + 2..close]) {
            continue;
        }
        // Past any further attributes, to the item; its `{ … }` or its `;`.
        let mut j = close + 1;
        let mut path_attr = None;
        loop {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if code[j..].starts_with("#[") {
                let Some(end) = attr_end(b, j + 2) else { break };
                // `#[path = "…"]`: its literal is blanked in `code`, so read it from `src`.
                let raw = src[j + 2..end].trim();
                if let Some(p) = raw.strip_prefix("path") {
                    path_attr = p.split('"').nth(1).map(str::to_owned);
                }
                j = end + 1;
            } else {
                break;
            }
        }
        let item_start = j;
        // A test-only field, match arm or struct-literal field is not an item: its attribute
        // hides nothing, and the scan carries on past it.
        if !opens_an_item(&code[item_start..]) {
            continue;
        }
        let Some(stop) = code[j..].find(['{', ';']).map(|n| j + n) else {
            break;
        };
        if b[stop] == b';' {
            let head: Vec<&str> = code[item_start..stop].split_whitespace().collect();
            if let Some(m) = head.iter().position(|w| *w == "mod")
                && let Some(name) = head.get(m + 1)
            {
                files.push(((*name).to_owned(), path_attr));
            }
            ranges.push((at, stop));
            i = stop + 1;
            continue;
        }
        let mut depth = 0usize;
        let mut k = stop;
        while k < b.len() {
            match b[k] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        ranges.push((at, k));
        i = k.max(close + 1);
    }
    (ranges, files)
}

/// The keywords an item starts with, after any of [`ITEM_PREFIXES`].
const ITEM_KEYWORDS: [&str; 11] = [
    "mod",
    "fn",
    "impl",
    "const",
    "static",
    "struct",
    "enum",
    "use",
    "type",
    "trait",
    "macro_rules!",
];

/// What may come before an item's keyword.
const ITEM_PREFIXES: [&str; 4] = ["pub", "unsafe", "async", "extern"];

/// Whether `code` (lexed, so an `extern "C"`'s ABI is blank) opens an item.
fn opens_an_item(code: &str) -> bool {
    let mut rest = code.trim_start();
    loop {
        let word: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '!')
            .collect();
        if ITEM_KEYWORDS.contains(&word.as_str()) {
            return true;
        }
        if !ITEM_PREFIXES.contains(&word.as_str()) {
            return false;
        }
        rest = rest[word.len()..].trim_start();
        // `pub(crate)`, `pub(in path)`.
        if word == "pub" && rest.starts_with('(') {
            let Some(close) = rest.find(')') else {
                return false;
            };
            rest = rest[close + 1..].trim_start();
        }
    }
}

/// The `]` that closes an attribute whose text starts at `from`, brackets nested.
fn attr_end(b: &[u8], from: usize) -> Option<usize> {
    let mut depth = 1;
    for (k, &c) in b.iter().enumerate().skip(from) {
        match c {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// The literals in `src` that spell an old name, outside test-only items: `(line, spelling,
/// literal)`. Also the test-only module files `src` brings in.
fn stray(src: &str, spellings: &[&str]) -> (Vec<Hit>, Vec<TestMod>) {
    let (literals, code) = lex(src);
    let (tests, files) = test_items(&code, src);
    let hits = literals
        .into_iter()
        .filter(|l| !tests.iter().any(|&(a, b)| a <= l.at && l.at <= b))
        .filter_map(|l| {
            let s = spells(&l.text, spellings)?;
            Some((l.line, s.to_owned(), l.text))
        })
        .collect();
    (hits, files)
}

// ---- the tree ------------------------------------------------------------------------- //

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "target" && name != "node_modules" && !name.starts_with('.') {
                rust_files(&path, out);
            }
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// The file a `mod name;` (with an optional `#[path]`) declared in `declaring` is.
fn module_file(declaring: &Path, name: &str, path_attr: Option<&str>) -> Vec<PathBuf> {
    let dir = declaring.parent().expect("a file has a folder");
    if let Some(p) = path_attr {
        return vec![dir.join(p)];
    }
    let stem = declaring.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let base = if matches!(stem, "mod" | "lib" | "main") {
        dir.to_path_buf()
    } else {
        dir.join(stem)
    };
    vec![
        base.join(format!("{name}.rs")),
        base.join(name).join("mod.rs"),
    ]
}

fn is_fixture_path(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    rel.split('/').any(|part| part == "tests")
        || name == "tests.rs"
        || name.ends_with("_tests.rs")
        || name.starts_with("tests_")
        || FIXTURE_FILES
            .iter()
            .any(|(f, _)| *f == rel || (f.ends_with('/') && rel.starts_with(f)))
}

fn is_name_module(rel: &str) -> bool {
    rel == "crates/purlis-core/src/names.rs" || rel.starts_with("crates/purlis-core/src/names/")
}

/// Every shipped file's stray literals, by path relative to the repository.
fn scan(root: &Path) -> (BTreeMap<String, Vec<Hit>>, usize) {
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("app/src-tauri"), &mut files);
    files.sort();
    let spellings = old_spellings();
    let mut test_files: Vec<PathBuf> = Vec::new();
    let mut found = BTreeMap::new();
    let mut scanned = 0;
    let rel = |p: &Path| {
        p.strip_prefix(root)
            .expect("under the repository")
            .to_string_lossy()
            .replace('\\', "/")
    };
    let mut lexed = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).expect("a source file");
        let (hits, mods) = stray(&src, &spellings);
        for (name, p) in mods {
            test_files.extend(module_file(path, &name, p.as_deref()));
        }
        lexed.push((path.clone(), hits));
    }
    for (path, hits) in lexed {
        let r = rel(&path);
        if is_name_module(&r) || is_fixture_path(&r) || test_files.contains(&path) {
            continue;
        }
        scanned += 1;
        if !hits.is_empty() {
            found.insert(r, hits);
        }
    }
    (found, scanned)
}

/// What is wrong with `found` against `allowed` (STILL_SPELLED's shape): a file spelling more
/// than its allowance, with each literal; one spelling fewer, to lower; an entry spelling none.
fn beyond_allowance(
    found: &BTreeMap<String, Vec<Hit>>,
    allowed: &[(&str, usize, Why)],
) -> Vec<String> {
    let mut wrong = Vec::new();
    let allowance = |file: &str| -> usize {
        allowed
            .iter()
            .filter(|(f, ..)| *f == file)
            .map(|(_, n, _)| n)
            .sum()
    };
    for (file, hits) in found {
        let allowed = allowance(file);
        match allowed.cmp(&hits.len()) {
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Greater => wrong.push(format!(
                "{file}: allowed {allowed} but spells {} — lower its count in STILL_SPELLED",
                hits.len()
            )),
            std::cmp::Ordering::Less => {
                wrong.push(format!(
                    "{file}: spells {} old name(s), allowed {allowed} — route it through \
                     purlis_core::names:",
                    hits.len()
                ));
                for (line, spelling, text) in hits {
                    let shown: String = text.chars().take(100).collect();
                    wrong.push(format!("    {file}:{line}: `{spelling}` in \"{shown}\""));
                }
            }
        }
    }
    for (file, _, why) in allowed {
        if !found.contains_key(*file) {
            wrong.push(format!(
                "{file}: allowed ({why:?}) but spells none — remove its entry"
            ));
        }
    }
    wrong
}

#[test]
fn no_shipped_file_spells_an_old_name_beyond_its_allowance() {
    purlis_core::unsteered!();
    let (found, scanned) = scan(&repo());
    assert!(scanned > 300, "the walk scanned {scanned} shipped files");
    let wrong = beyond_allowance(&found, STILL_SPELLED);
    assert!(
        wrong.is_empty(),
        "old names outside purlis_core::names ({FIXTURES} excepted):\n{}",
        wrong.join("\n")
    );
}

// ---- the guard's own tests ------------------------------------------------------------ //

/// A repository of one shipped file holding `body`, plus the name module, scanned.
fn scan_of(body: &str) -> BTreeMap<String, Vec<Hit>> {
    let tree = tempfile::tempdir().expect("a tree");
    let src = tree.path().join("crates/x/src");
    std::fs::create_dir_all(&src).expect("folders");
    std::fs::write(src.join("lib.rs"), body).expect("lib.rs");
    let names = tree.path().join("crates/purlis-core/src/names");
    std::fs::create_dir_all(&names).expect("the name module's folder");
    std::fs::write(
        names.with_extension("rs"),
        "const A: &str = \".charter\";\n",
    )
    .expect("names.rs");
    std::fs::write(
        names.join("window.rs"),
        "const B: &str = \"charter.toml\";\n",
    )
    .expect("window.rs");
    scan(tree.path()).0
}

#[test]
fn a_stray_literal_in_a_shipped_file_is_found_and_the_name_module_is_not() {
    purlis_core::unsteered!();
    let found = scan_of("fn f() -> &'static str {\n    \".charter/vaults\"\n}\n");
    let hits = &found["crates/x/src/lib.rs"];
    assert_eq!(hits.len(), 1, "{found:?}");
    assert_eq!((hits[0].0, hits[0].1.as_str()), (2, ".charter"));
    assert_eq!(
        found.len(),
        1,
        "the name module is never reported: {found:?}"
    );
}

#[test]
fn the_guard_fails_on_a_stray_literal_injected_into_the_tree() {
    purlis_core::unsteered!();
    // The repository as it stands passes; the same, plus one shipped line that spells the old
    // manifest, is reported by the very check the guard runs.
    let (mut found, _) = scan(&repo());
    assert!(beyond_allowance(&found, STILL_SPELLED).is_empty());
    let tree = tempfile::tempdir().expect("a tree");
    let injected = tree.path().join("crates/injected/src");
    std::fs::create_dir_all(&injected).expect("a crate");
    std::fs::write(
        injected.join("lib.rs"),
        "pub fn manifest(dir: &std::path::Path) -> std::path::PathBuf {\n    \
         dir.join(\"charter.toml\")\n}\n",
    )
    .expect("lib.rs");
    let (stray, _) = scan(tree.path());
    found.extend(stray);
    let wrong = beyond_allowance(&found, STILL_SPELLED);
    assert_eq!(
        wrong,
        [
            "crates/injected/src/lib.rs: spells 1 old name(s), allowed 0 — route it through \
             purlis_core::names:",
            "    crates/injected/src/lib.rs:2: `charter.toml` in \"charter.toml\"",
        ],
    );
}

#[test]
fn every_kind_the_ticket_names_is_looked_for_and_the_bare_product_name_is_not() {
    purlis_core::unsteered!();
    let spellings = old_spellings();
    for wanted in [
        ".charter",
        "charter.toml",
        "charter.local.toml",
        ".charter-scan-allow.toml",
        "CHARTER_",
        "charter/",
        "charter/@identity",
        "Charter-Chat",
        "Charter-Persona",
        "Charter-Change",
        "<!-- charter-save -->",
        "# >>> charter merge rules (managed by charter) >>>",
        // The plugin's (RN-8, #1266).
        "charter@inline",
        "charter-app@inline",
        "charter@charter-app",
        "charter:",
        "mcp__charter__",
    ] {
        assert!(spellings.contains(&wanted), "{wanted} is not looked for");
    }
    assert!(!spellings.contains(&"charter"));
    assert!(
        !spellings
            .iter()
            .any(|s| s.starts_with("purlis") || s.starts_with(".purlis"))
    );
}

#[test]
fn a_spelling_counts_only_at_a_word_boundary() {
    purlis_core::unsteered!();
    let s = old_spellings();
    for hit in [
        ".charter",
        "/.charter/",
        "{}.charter-{}",
        "x/charter.toml",
        "$CHARTER_ROOT",
        "CHARTER_ROOT=1",
        "charter/ops/db",
        "Charter-Chat: 3",
        "charter@inline",
        "{\"charter@charter-app\": true}",
        "charter:handoff",
        "mcp__charter__todo_add",
    ] {
        assert!(spells(hit, &s).is_some(), "{hit} should count");
    }
    for miss in [
        "dev.charter.app",
        ".charterx",
        "MY_CHARTER_ROOT",
        "a charter of its own",
        "purlis.toml",
        "PURLIS_ROOT",
        "acharter/",
        "charter: the app did not take this",
        "charter:",
        "purlis:handoff",
        "purlis@inline",
        "mcp__purlis__todo_add",
        "charter-app#274",
    ] {
        assert_eq!(spells(miss, &s), None, "{miss} should not count");
    }
}

#[test]
fn comments_and_test_only_items_are_not_read_and_everything_else_is() {
    purlis_core::unsteered!();
    let s = old_spellings();
    let src = r##"
// a comment about ".charter" is prose
/* so is "charter.toml" in a /* nested */ block */
/// and a doc comment's "CHARTER_ROOT"
const Q: char = '"';
fn f<'a>(x: &'a str) -> &'a str { x }
const RAW: &str = r#"has "charter.local.toml" in it"#;
const BYTES: &[u8] = b"CHARTER_HOME";
#[cfg(test)]
mod tests {
    const T: &str = ".charter";
}
#[cfg(all(test, unix))]
fn helper() -> &'static str { "charter/x" }
#[cfg(test)]
#[path = "elsewhere_helpers.rs"]
mod elsewhere;
#[cfg(not(test))]
const SHIPPED: &str = "Charter-Chat";
struct S {
    #[cfg(test)]
    seen: u8,
    shipped: u8,
}
fn g(x: u8) -> S {
    match x {
        #[cfg(test)]
        0 => {}
        _ => {}
    }
    S {
        #[cfg(test)]
        seen: 0,
        shipped: { let _ = ".charter-scan-allow.toml"; 1 },
    }
}
impl S {
    fn h(&self) -> &str { "charter/ops" }
}
#[cfg(test)]
pub(crate) async fn hidden() -> &'static str { "charter.toml" }
"##;
    let (hits, files) = stray(src, &s);
    let spelled: Vec<&str> = hits.iter().map(|(_, sp, _)| sp.as_str()).collect();
    assert_eq!(
        spelled,
        [
            "charter.local.toml",
            "CHARTER_",
            "Charter-Chat",
            ".charter-scan-allow.toml",
            "charter/"
        ],
        "{hits:?}"
    );
    assert_eq!(
        files,
        [(
            "elsewhere".to_owned(),
            Some("elsewhere_helpers.rs".to_owned())
        )]
    );
}

#[test]
fn the_allow_lists_are_well_formed() {
    purlis_core::unsteered!();
    let root = repo();
    for (file, n, why) in STILL_SPELLED {
        assert!(*n > 0, "{file}: an entry of 0 ({why:?}) is a removed entry");
        assert!(root.join(file).is_file(), "{file} is not a file");
        assert!(
            !is_name_module(file) && !is_fixture_path(file),
            "{file} needs no entry"
        );
    }
    for (path, why) in FIXTURE_FILES {
        assert!(root.join(path).exists(), "{path} ({why}) does not exist");
    }
}
