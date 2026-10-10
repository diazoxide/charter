//! **Every "not in this version yet" in shipped source names the ticket that keeps it.**
//!
//! The defect this stops (HY-12, #575): about sixty-five deferrals had been written as prose —
//! "not ported", "not in this version yet", "does not check … yet" — and nothing tied them to
//! work anyone had planned. Some were promises nobody had filed. Some had shipped and the
//! sentence still said they had not (`charter persona optimize`, the persona tool gate). A
//! promise with no ticket is lost, and a reader cannot tell it from one that is coming.
//!
//! **The rule.** In shipped source (`crates/*/src`, `app/src-tauri/src`, `app/src`, without
//! test files or a trailing `#[cfg(test)]` module) and in the docs purlis ships for agents and
//! people to read (`crates/purlis-core/docs`, #1383), a line that uses deferral wording must
//! have a tracking reference — an issue (`#123`) or an ADR (`ADR 0050`) — in its own paragraph
//! ([`paragraph`]) and within [`WINDOW`] lines of it. The issue is the plan; the ADR is the
//! decision that it waits on something charter does not control.
//!
//! **The same paragraph, not only a nearby line** (#1629, D-1629-1): a link to ADR 0027 two lines
//! above "Cutting a piece from the app is not in this version yet" used to pass it, though the
//! link was another paragraph's. A paragraph ends at a blank line (a bare `///` or `//` too), and
//! a heading or a table row starts one of its own, so each row of a table names its own ticket.
//!
//! **What counts as deferral wording** is [`DEFERRAL`]: the phrases this repository uses for
//! "planned, not here" — "not in this version", "not ported", "not supported yet", "not here
//! yet", "not built yet", "no backend … yet" — and [`DOES_NOT`], "does not <verb> … yet"
//! ("does not follow that yet", "does not draw yet"), unless the verb is one of [`MOMENTS`]:
//! "does not exist yet" is about time. Three more shapes count whatever their verb (#1629,
//! D-1629-2): "not <verb> in (or by) this version" ([`DEFERRAL`]), "this version … not … yet"
//! ([`THIS_VERSION`]: the version is the subject, so "this version does not have that yet" is a
//! deferral though `have` is a moment) and "not checked … yet" ([`NOT_CHECKED`], the doctor's
//! deferred rows). A
//! thing left out on purpose is not deferred and is not worded so: it says "left out" or "by
//! design", with the reason. A plain "not yet" is not matched, and neither is "cannot … yet":
//! almost every one in this tree is about time ("written and not yet acknowledged", "the plane
//! cannot be moved yet", "a render cannot see yet"), not scope. The one deferral of that kind,
//! the sandbox refusal of opencode on Linux, cites #1040 anyway.
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
    r"(?i)not in this version|\bnot [a-z]+ (?:in|by) this version\b|\bnot (yet )?ported\b|\bported yet\b|\bunported\b",
    r"|\bhas not ported\b|\bwas not ported\b|not supported yet",
    r"|\bnot here yet\b|\bnot built yet\b|\bno [a-z ]*backend[^.;]*\byet\b",
);

/// "does not <verb> … yet" within one clause, case-insensitive (#1383): the refusals "does not
/// follow that yet" and "does not create one yet" slipped past a list that named only "does
/// not check … yet". The verb is the first group; [`MOMENTS`] are let through. A comma or a
/// colon ends the clause, so "does not read, so the grant covers nothing yet" is two.
const DOES_NOT: &str = r"(?i)\bdoes not ([a-z]+)\b[^.;,:]*\byet\b";

/// "this version … not … yet" within one sentence, case-insensitive (#1629): a sentence about
/// what this version does is a deferral whatever its verb, "in this version it says the layer is
/// not checked there yet" and "commands this version does not have yet" alike.
const THIS_VERSION: &str = r"(?i)\bthis version\b[^.;]*\bnot\b[^.;]*\byet\b";

/// "not checked … yet" within one clause, case-insensitive (#1629): the doctor's rows for what it
/// does not check in this version, "says of both that they are not checked yet".
const NOT_CHECKED: &str = r"(?i)\bnot checked\b[^.;,:]*\byet\b";

/// Verbs after "does not" that say what has not happened YET, not what purlis does not do:
/// "a folder that does not exist yet", "commits the remote does not have yet", "files git
/// does not track yet".
const MOMENTS: &[&str] = &["exist", "have", "hold", "list", "count", "know", "track"];

/// Lines of shipped files the rule does not read yet, each with why and until when. The path
/// is from the repository root; the text is a piece of the line.
const EXEMPT: &[(&str, &str, &str)] = &[
    // The worktree-relocation refusal: the worktree work in flight rewrites this file, so the
    // ticket is cited there when it lands rather than edited under it (#1383).
    (
        "crates/purlis-core/src/worktree/mod.rs",
        "does not follow",
        "the worktree work in flight",
    ),
];

/// The wording of a deferral, every pattern compiled once.
struct Wording {
    deferral: Regex,
    does_not: Regex,
    this_version: Regex,
    not_checked: Regex,
}

impl Wording {
    fn new() -> Self {
        Self {
            deferral: Regex::new(DEFERRAL).expect("the deferral pattern"),
            does_not: Regex::new(DOES_NOT).expect("the does-not pattern"),
            this_version: Regex::new(THIS_VERSION).expect("the this-version pattern"),
            not_checked: Regex::new(NOT_CHECKED).expect("the not-checked pattern"),
        }
    }

    /// Whether `text` is worded as a deferral.
    fn is_deferral(&self, text: &str) -> bool {
        self.deferral.is_match(text)
            || self.this_version.is_match(text)
            || self.not_checked.is_match(text)
            || self.does_not.captures_iter(text).any(|found| {
                let verb = found[1].to_lowercase();
                !MOMENTS.contains(&verb.as_str())
            })
    }
}

/// Whether a line, as [`prose`], is a paragraph on its own: a heading or a table row, so a table
/// row's ticket is never its neighbour's. A list item is not: a list goes on the sentence that
/// leads into it, and that sentence's reference is the list's.
fn a_line_of_its_own(prose: &str) -> bool {
    // A heading is `#`s and a space: `#670` at the start of a wrapped line is a reference.
    let heading = prose.starts_with('#')
        && prose
            .trim_start_matches('#')
            .chars()
            .next()
            .is_none_or(char::is_whitespace);
    heading || prose.starts_with('|')
}

/// The lines of the paragraph line `n` is in, as a range of `prosed`, at most [`WINDOW`] lines
/// on either side of it (D-1629-1). A paragraph ends at an empty line (a bare `///` or `//`
/// too), and a line that is [`a_line_of_its_own`] is a paragraph of one.
fn paragraph(prosed: &[String], n: usize) -> std::ops::Range<usize> {
    if a_line_of_its_own(&prosed[n]) {
        return n..n + 1;
    }
    let joins = |line: &String| !line.is_empty() && !a_line_of_its_own(line);
    let mut from = n;
    while from > n.saturating_sub(WINDOW) && joins(&prosed[from - 1]) {
        from -= 1;
    }
    let last = (n + WINDOW).min(prosed.len().saturating_sub(1));
    let mut to = n;
    while to < last && joins(&prosed[to + 1]) {
        to += 1;
    }
    from..to + 1
}

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

/// The docs purlis ships under `dir`: its Markdown files.
fn docs(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            docs(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            into.push(path);
        }
    }
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
    purlis_core::unsteered!();
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
    let before = files.len();
    docs(&root.join("crates/purlis-core/docs"), &mut files);
    assert!(
        files.len() - before >= 5,
        "the walk found {} docs",
        files.len() - before
    );

    let wording = Wording::new();
    let worded = |text: &str| wording.is_deferral(text);
    let reference = Regex::new(REFERENCE).expect("the reference pattern");
    let mut unfiled = Vec::new();
    let mut seen = 0;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a source file");
        let lines = before_tests(&text);
        let prosed: Vec<String> = lines.iter().map(|line| prose(line)).collect();
        let alone: Vec<bool> = prosed.iter().map(|line| worded(line)).collect();
        let at = file.strip_prefix(&root).unwrap_or(file).to_string_lossy();
        for (n, line) in lines.iter().enumerate() {
            // A phrase on one line is found there; one that wraps is found on the line it
            // starts, from that line joined with the next.
            let next = prosed.get(n + 1).map_or("", String::as_str);
            let wraps = !alone[n]
                && !alone.get(n + 1).copied().unwrap_or(false)
                && worded(&format!("{} {next}", prosed[n]));
            if !alone[n] && !wraps {
                continue;
            }
            let exempt = EXEMPT.iter().any(|(path, piece, _)| {
                *at == **path && format!("{} {next}", prosed[n]).contains(piece)
            });
            if exempt {
                continue;
            }
            seen += 1;
            if !lines[paragraph(&prosed, n)]
                .iter()
                .any(|l| reference.is_match(l))
            {
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
        "deferral wording with no issue or ADR in its paragraph within {WINDOW} lines — file the work and cite \
         it, or, if it is left out on purpose, say so without deferral wording:\n{}",
        unfiled.join("\n")
    );
}

#[test]
fn the_deferral_pattern_matches_a_promise_and_not_a_moment() {
    purlis_core::unsteered!();
    let wording = Wording::new();
    for promise in [
        "seeding it from past sessions is not in this version yet.",
        "this version of purlis does not check vaults and the credentials they hold yet",
        "Windows is not ported yet",
        "plugins for Codex are not supported yet — why",
        "names `unwire` as unported",
        "purlis has no sandbox backend on Windows yet",
        "this version of purlis does not follow that yet",
        "this purlis clones into a workspace that exists, and does not create one yet",
        "The Rust charter does not scaffold one yet",
        "a value this version does not do yet",
        // The four #1629 found by hand.
        "A relocated worktree root is not followed in this version yet",
        "and in this version it says the layer is not checked there yet",
        "and says of both that they are not checked yet",
        "it forwards commands this version does not have yet",
        "Optional, and not followed by this version: see below.",
    ] {
        assert!(wording.is_deferral(promise), "{promise}");
    }
    for moment in [
        "Bytes written and not yet acknowledged",
        "The tickets an app has minted and not yet seen spent",
        "the reference vault provider is not supported on this host",
        "the PR merged, and the plane cannot be moved yet",
        "a path that does not exist yet",
        "Commits the remote does not have yet.",
        "the header an index this store does not have to exist yet",
        "files it added that git does not track yet",
    ] {
        assert!(!wording.is_deferral(moment), "{moment}");
    }
}

#[test]
fn a_deferral_wrapped_across_two_lines_reads_as_one() {
    purlis_core::unsteered!();
    let wording = Wording::new();
    let deferral = |text: &str| wording.is_deferral(text);
    let first = prose(r#"const V: &str = "this version of purlis does not check vaults and the \"#);
    let second = prose(r#"                                 credentials they hold yet";"#);
    assert!(!deferral(&first) && !deferral(&second));
    assert!(deferral(&format!("{first} {second}")), "{first} {second}");
    let doc = [
        prose("    /// each a WARN that says *not checked (…not"),
        prose("    /// ported…)*."),
    ];
    assert!(deferral(&doc.join(" ")), "{doc:?}");
}

/// #1629, D-1629-1: a reference counts only in the deferral's own paragraph. The link that sat
/// two lines above `workspaces.md`'s "Cutting a piece …" no longer passes it.
#[test]
fn a_reference_in_another_paragraph_does_not_name_the_deferral() {
    purlis_core::unsteered!();
    let prosed = |text: &str| text.lines().map(prose).collect::<Vec<_>>();

    let apart = prosed(
        "See ADR 0027 for the layout.\n\nCutting a piece from the app is not in\nthis version yet.",
    );
    assert_eq!(paragraph(&apart, 2), 2..4);

    let headed = prosed("## Not in this version yet\nsee #12");
    assert_eq!(
        paragraph(&headed, 1),
        1..2,
        "a heading is a block of its own"
    );

    let rows = prosed("| one | not in this version yet |\n| two | waits on #12 |");
    assert_eq!(
        paragraph(&rows, 0),
        0..1,
        "a table row is its own paragraph"
    );

    let list = prosed("Two exits (ADR 0067):\n- one\n- not in this version yet");
    assert_eq!(
        paragraph(&list, 2),
        0..3,
        "a list goes on the sentence leading into it"
    );

    let comment = prosed("/// Waits on #12.\n///\n/// not in this version yet");
    assert_eq!(
        paragraph(&comment, 2),
        2..3,
        "a bare `///` ends a paragraph"
    );

    let together = prosed("/// is not followed in this version yet\n/// #1381 keeps it.");
    assert_eq!(paragraph(&together, 0), 0..2);

    let long: Vec<String> = (0..20).map(|n| format!("line {n}")).collect();
    assert_eq!(paragraph(&long, 10), 6..15, "never past the window");
}
