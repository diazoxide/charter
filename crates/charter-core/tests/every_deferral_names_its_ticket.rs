//! **Every "not in this version yet" in shipped source names the ticket that keeps it.**
//!
//! The defect this stops (HY-12, #575): about sixty-five deferrals had been written as prose —
//! "not ported", "not in this version yet", "does not check … yet" — and nothing tied them to
//! work anyone had planned. Some were promises nobody had filed. Some had shipped and the
//! sentence still said they had not (`charter persona optimize`, the persona tool gate). A
//! promise with no ticket is lost, and a reader cannot tell it from one that is coming.
//!
//! **The rule.** In shipped source (`crates/*/src`, `app/src-tauri/src`, `app/src`, without
//! test files or a trailing `#[cfg(test)]` module), a line that uses deferral wording must
//! have a tracking reference — an issue (`#123`) or an ADR (`ADR 0050`) — on it or within
//! [`WINDOW`] lines of it. The issue is the plan; the ADR is the decision that it waits on
//! something charter does not control.
//!
//! **What counts as deferral wording** is [`DEFERRAL`]: the phrases this repository uses for
//! "planned, not here" — "not in this version", "not ported", "not supported yet", "does not
//! check … yet", "not here yet", "not built yet", "no backend … yet", "does not do yet". A
//! thing left out on purpose is not deferred and is not worded so: it says "left out" or "by
//! design", with the reason. A plain "not yet" is not matched, and neither is "cannot … yet":
//! almost every one in this tree is about time ("written and not yet acknowledged", "the plane
//! cannot be moved yet", "a render cannot see yet"), not scope. The one "cannot … yet" that is
//! a deferral, the sandbox refusal of a harness with no compiler, cites SD-2 (#695) anyway.
//!
//! **A phrase may wrap.** Each line is read joined with the next, with the comment markers,
//! string continuations and quotes taken out ([`prose`]), so a deferral split across two lines
//! of a doc comment or a `\`-continued string literal is found like one on a single line.
//!
//! `app/src/bindings.ts` and `app/src/uiRpc.ts` are generated from the Rust doc comments, so they
//! are checked there.

use std::path::{Path, PathBuf};

use regex::Regex;

/// Lines on either side of a deferral that its reference may sit on.
const WINDOW: usize = 4;

/// The wording of a deferral, case-insensitive.
const DEFERRAL: &str = concat!(
    r"(?i)not in this version|\bnot (yet )?ported\b|\bported yet\b|\bunported\b",
    r"|\bhas not ported\b|\bwas not ported\b|not supported yet",
    r"|does not check\b[^.;]*\byet\b|\bnot here yet\b|\bnot built yet\b",
    r"|\bno [a-z ]*backend[^.;]*\byet\b|\bdoes not do yet\b",
);

/// A tracking reference: an issue or an ADR.
const REFERENCE: &str = r"#\d+\b|\bADR \d{4}\b";

fn shipped(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "target" || name == "node_modules" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if name != "tests" && name != "testdata" && name != "e2e" {
                shipped(&path, into);
            }
            continue;
        }
        let test_file = name == "tests.rs"
            || name.ends_with("_tests.rs")
            || name.starts_with("tests_")
            || name.contains(".test.")
            || name == "bindings.ts"
            || name == "uiRpc.ts";
        let source = name.ends_with(".rs") || name.ends_with(".ts") || name.ends_with(".tsx");
        if source && !test_file {
            into.push(path);
        }
    }
}

/// The lines of `text` before its test module, if it has one: a `#[cfg(test)]` followed by
/// `mod … {`. A `#[cfg(test)]` on anything else (a field, a function) is shipped source's.
fn before_tests(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let end = lines
        .iter()
        .enumerate()
        .position(|(n, line)| {
            line.trim() == "#[cfg(test)]"
                && lines[n + 1..]
                    .iter()
                    .find(|next| !next.trim().is_empty())
                    .is_some_and(|next| {
                        let next = next.trim();
                        (next.starts_with("mod ") || next.starts_with("pub mod "))
                            && next.ends_with('{')
                    })
        })
        .unwrap_or(lines.len());
    lines[..end].to_vec()
}

/// A line as prose: comment markers, a leading `*`, string continuations and quotes taken out.
fn prose(line: &str) -> String {
    let mut text = line.trim();
    for marker in ["//!", "///", "//", "/**", "*"] {
        if let Some(rest) = text.strip_prefix(marker) {
            text = rest;
            break;
        }
    }
    text.replace(['\\', '"', '`'], "").trim().to_owned()
}

#[test]
fn every_deferral_in_shipped_source_names_its_ticket() {
    charter_core::unsteered!();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    if let Ok(crates) = std::fs::read_dir(root.join("crates")) {
        for c in crates.flatten() {
            shipped(&c.path().join("src"), &mut files);
        }
    }
    shipped(&root.join("app/src-tauri/src"), &mut files);
    shipped(&root.join("app/src"), &mut files);
    assert!(files.len() > 100, "the walk found {} files", files.len());

    let deferral = Regex::new(DEFERRAL).expect("the deferral pattern");
    let reference = Regex::new(REFERENCE).expect("the reference pattern");
    let mut unfiled = Vec::new();
    let mut seen = 0;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a source file");
        let lines = before_tests(&text);
        let prosed: Vec<String> = lines.iter().map(|line| prose(line)).collect();
        let alone: Vec<bool> = prosed.iter().map(|line| deferral.is_match(line)).collect();
        for (n, line) in lines.iter().enumerate() {
            // A phrase on one line is found there; one that wraps is found on the line it
            // starts, from that line joined with the next.
            let next = prosed.get(n + 1).map_or("", String::as_str);
            let wraps = !alone[n]
                && !alone.get(n + 1).copied().unwrap_or(false)
                && deferral.is_match(&format!("{} {next}", prosed[n]));
            if !alone[n] && !wraps {
                continue;
            }
            seen += 1;
            let from = n.saturating_sub(WINDOW);
            let to = (n + WINDOW + 1).min(lines.len());
            if !lines[from..to].iter().any(|l| reference.is_match(l)) {
                let at = file.strip_prefix(&root).unwrap_or(file).to_string_lossy();
                unfiled.push(format!("{at}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    // The walk reaches the deferrals that are left, so a pattern that stopped matching
    // anything cannot pass by finding nothing.
    assert!(
        seen > 10,
        "only {seen} deferrals found: is the pattern still right?"
    );
    assert!(
        unfiled.is_empty(),
        "deferral wording with no issue or ADR within {WINDOW} lines — file the work and cite \
         it, or, if it is left out on purpose, say so without deferral wording:\n{}",
        unfiled.join("\n")
    );
}

#[test]
fn the_deferral_pattern_matches_a_promise_and_not_a_moment() {
    charter_core::unsteered!();
    let deferral = Regex::new(DEFERRAL).expect("the deferral pattern");
    for promise in [
        "seeding it from past sessions is not in this version yet.",
        "this version of charter does not check vaults and the credentials they hold yet",
        "Windows is not ported yet",
        "plugins for Codex are not supported yet — why",
        "names `unwire` as unported",
        "charter has no sandbox backend on Windows yet",
    ] {
        assert!(deferral.is_match(promise), "{promise}");
    }
    for moment in [
        "Bytes written and not yet acknowledged",
        "The tickets an app has minted and not yet seen spent",
        "the reference vault provider is not supported on this host",
        "the PR merged, and the plane cannot be moved yet",
    ] {
        assert!(!deferral.is_match(moment), "{moment}");
    }
}

#[test]
fn a_deferral_wrapped_across_two_lines_reads_as_one() {
    charter_core::unsteered!();
    let deferral = Regex::new(DEFERRAL).expect("the deferral pattern");
    let first =
        prose(r#"const V: &str = "this version of charter does not check vaults and the \"#);
    let second = prose(r#"                                 credentials they hold yet";"#);
    assert!(!deferral.is_match(&first) && !deferral.is_match(&second));
    assert!(
        deferral.is_match(&format!("{first} {second}")),
        "{first} {second}"
    );
    let doc = [
        prose("    /// each a WARN that says *not checked (…not"),
        prose("    /// ported…)*."),
    ];
    assert!(deferral.is_match(&doc.join(" ")), "{doc:?}");
}
