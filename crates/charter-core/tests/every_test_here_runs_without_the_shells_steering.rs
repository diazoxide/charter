//! **Every test in this directory opens with `charter_core::unsteered!()`** (charter-app#267).
//!
//! These tests link the library as the product does, so the `CHARTER_*` variables that steer it
//! — `CHARTER_WORKTREES`, `CHARTER_HOME` and the rest of `steer::STEERING` — reach them from
//! whatever shell ran `cargo test`. A `CHARTER_WORKTREES` failed ~45 of them, and a
//! `CHARTER_HOME` could have written into the real state directory. The macro re-runs the test
//! without them whenever one is set; a test that does not open with it is back where they were,
//! and nothing else would say so until somebody ran the suite from a shell with one set.

use std::path::Path;

/// The line every test's body starts with.
const GUARD: &str = "charter_core::unsteered!();";

/// The tests in `source` whose first statement is not [`GUARD`], by name.
fn unguarded(source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut missing = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let code = line.trim();
        if code.starts_with("//") || !code.starts_with("#[") {
            continue;
        }
        if code != "#[test]" {
            // A test attribute in a shape this scan does not follow — `#[test] fn x()` on one
            // line, `#[tokio::test]`, `#[should_panic]` (which a re-run cannot carry: the
            // parent would return normally) — is reported rather than walked past.
            if code.contains("test]") || code.contains("::test") || code.contains("should_panic") {
                missing.push(format!(
                    "an attribute this scan cannot follow at line {}: {code}",
                    at + 1
                ));
            }
            continue;
        }
        // Past any further attributes to the signature, then past the signature to its `{`.
        let Some(signature) =
            (at + 1..lines.len()).find(|&i| lines[i].trim_start().starts_with("fn "))
        else {
            missing.push(format!("a #[test] at line {} with no fn after it", at + 1));
            continue;
        };
        let name = lines[signature]
            .trim_start()
            .trim_start_matches("fn ")
            .split('(')
            .next()
            .unwrap_or_default()
            .to_string();
        let opened = (signature..lines.len()).find(|&i| lines[i].trim_end().ends_with('{'));
        let first = opened
            .and_then(|i| (i + 1..lines.len()).find(|&j| !lines[j].trim().is_empty()))
            .map(|j| lines[j].trim());
        if first != Some(GUARD) {
            missing.push(name);
        }
    }
    missing
}

#[test]
fn every_test_in_this_directory_opens_by_shedding_the_shells_steering() {
    charter_core::unsteered!();
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut checked = 0;
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&here).expect("the tests directory") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a test file");
        checked += source.lines().filter(|l| l.trim() == "#[test]").count();
        for name in unguarded(&source) {
            missing.push(format!(
                "{}: {name}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    assert!(checked > 100, "the walk found {checked} tests");
    assert!(
        missing.is_empty(),
        "{} tests do not open with `{GUARD}`:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn a_test_is_guarded_only_when_the_guard_is_its_first_statement() {
    charter_core::unsteered!();
    let guarded = "#[test]\n#[cfg(unix)]\nfn a() {\n\n    charter_core::unsteered!();\n}\n";
    let late = "#[test]\nfn b() {\n    let x = 1;\n    charter_core::unsteered!();\n}\n";
    let none = "#[test]\nfn c() {\n    assert!(true);\n}\n";

    let one_line = "#[test] fn d() {\n    charter_core::unsteered!();\n}\n";
    let other = "#[tokio::test]\nasync fn e() {\n    charter_core::unsteered!();\n}\n";
    let panicking = "#[test]\n#[should_panic]\nfn f() {\n    charter_core::unsteered!();\n}\n";

    assert!(unguarded(guarded).is_empty());
    assert_eq!(unguarded(late), ["b"]);
    assert_eq!(unguarded(none), ["c"]);
    for unfollowed in [one_line, other, panicking] {
        assert_eq!(unguarded(unfollowed).len(), 1, "{unfollowed}");
    }
}
