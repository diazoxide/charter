//! Every `charter <command>` a message tells somebody to run is a command this binary has.
//!
//! The defect this stops (M8.4 and its follow-ups): a refusal, a hint or a briefing that names
//! a command the rebuilt CLI never had — `charter secret exec`, `charter wt add`,
//! `charter persona create` — sends an agent or an operator to a clap usage error, from the
//! one sentence written to get them unstuck. Each was true of an earlier charter and was
//! carried over word for word.
//!
//! **What counts as a suggestion.** Inside a string literal of shipped source (not a comment,
//! not a test module): `charter <word>` in backticks, or after `run`, `try`, `use`, `with` or
//! a colon. That is how every hint in this repository is spelled, and prose about charter
//! ("charter cannot read …") is none of those shapes.
//!
//! **What is allowed to name a missing command:** a literal that says, in the same sentence
//! set, that it is "not in this version yet" — the plain spelling for a planned command.
//!
//! **The skills charter's plugin ships are messages too** (#370): an agent reads
//! `app/src-tauri/plugin/skills/**/SKILL.md` and runs what it says. Every `charter …` in one —
//! in backticks, after those verbs, or opening a line of a fenced code block — is checked the
//! same way.
//!
//! **Asked of the binary, not of a list.** Each word is checked with `charter <word> --help`
//! (and `charter <word> <sub> --help` where the first has subcommands), so an alias clap
//! accepts counts and a list here cannot drift from the parser.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const PLANNED: &str = "not in this version yet";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository")
}

/// Every shipped `.rs` file: `crates/*/src` and the app's, without test files.
fn shipped_sources(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" && name != "testdata" {
                    walk(&path, out);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(crates) = std::fs::read_dir(root.join("crates")) {
        for c in crates.flatten() {
            walk(&c.path().join("src"), &mut out);
        }
    }
    walk(&root.join("app/src-tauri/src"), &mut out);
    out.sort();
    out
}

/// The string literals of `src` outside comments and before the first `#[cfg(test)]`, with
/// Rust's line continuation (`\` + newline + leading whitespace) already applied.
fn literals(src: &str) -> Vec<String> {
    let src = src.find("#[cfg(test)]").map_or(src, |at| &src[..at]);
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '\'' {
            // A char literal (`'"'`, `'\''`) or a lifetime; neither opens a string.
            if chars.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if chars.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if c == 'r' && matches!(chars.get(i + 1), Some('"') | Some('#')) {
            let mut j = i + 1;
            let mut hashes = 0;
            while chars.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if chars.get(j) != Some(&'"') {
                i += 1;
                continue;
            }
            let close: String = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            let body_start = j + 1;
            let rest: String = chars[body_start..].iter().collect();
            let end = rest.find(&close).unwrap_or(rest.len());
            out.push(rest[..end].to_string());
            i = body_start + rest[..end].chars().count() + close.chars().count();
        } else if c == '"' {
            let mut text = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && chars.get(i + 1) == Some(&'\n') {
                    i += 2;
                    while i < chars.len() && chars[i].is_whitespace() {
                        i += 1;
                    }
                    continue;
                }
                if chars[i] == '\\' {
                    text.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    text.push(chars[i]);
                    i += 1;
                }
            }
            i += 1;
            out.push(text);
        } else {
            i += 1;
        }
    }
    out
}

/// Every `SKILL.md` under the plugin charter ships.
fn shipped_skills(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if entry.file_name() == "SKILL.md" {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("app/src-tauri/plugin/skills"), &mut out);
    out.sort();
    out
}

/// The texts of a skill a reader runs: the whole page for the inline shapes, and each line of
/// a fenced code block that opens with `charter `, as a backticked command.
fn skill_texts(page: &str) -> Vec<String> {
    let mut out = vec![page.to_owned()];
    let mut fenced = false;
    for line in page.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced && let Some(rest) = trimmed.strip_prefix("charter ") {
            out.push(format!("`charter {rest}`"));
        }
    }
    out
}

/// `(first word, optional second word)` of every suggestion in `text`.
fn suggestions(text: &str) -> Vec<(String, Option<String>)> {
    let re = regex::Regex::new(
        r"(?:`charter ([a-z][a-z-]*)(?: ([a-z][a-z-]*))?|(?:\b[Rr]un|\b[Tt]ry|\b[Uu]se|\bwith|:)\s+charter ([a-z][a-z-]*)(?: ([a-z][a-z-]*))?)",
    )
    .expect("the pattern compiles");
    re.captures_iter(text)
        .filter_map(|c| {
            let backticked = c.get(1).is_some();
            let first = c.get(1).or(c.get(3)).expect("a first word").as_str();
            // After a colon, "charter" is as often the subject of a sentence as a command:
            // "…is not a plane: charter found no …". Those verbs are never a subcommand.
            if !backticked && PROSE.contains(&first) {
                return None;
            }
            let second = c.get(2).or(c.get(4)).map(|m| m.as_str().to_owned());
            Some((first.to_owned(), second))
        })
        .collect()
}

/// Words that follow "charter" when it is the subject of a sentence rather than a command.
const PROSE: [&str; 15] = [
    "is", "was", "will", "would", "could", "can", "cannot", "does", "did", "has", "had", "found",
    "reads", "never", "says",
];

/// `charter <words> --help`, answered by this build.
fn help(words: &[&str]) -> Option<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(words)
        .arg("--help")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .current_dir(std::env::temp_dir())
        .output()
        .expect("charter runs");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether `charter first [second]` is a command this binary has.
fn exists(first: &str, second: Option<&str>) -> bool {
    let Some(text) = help(&[first]) else {
        return false;
    };
    match second {
        Some(second) if text.contains("\nCommands:\n") => help(&[first, second]).is_some(),
        _ => true,
    }
}

#[test]
fn every_charter_command_a_message_names_is_a_command_this_binary_has() {
    let root = repo();
    let sources = shipped_sources(&root);
    assert!(
        sources.len() > 50,
        "the walk found the sources: {sources:?}"
    );

    let mut answered: BTreeMap<(String, Option<String>), bool> = BTreeMap::new();
    let mut missing = Vec::new();
    let mut seen = 0;
    for file in &sources {
        let src = std::fs::read_to_string(file).expect("a source file reads");
        for text in literals(&src) {
            if text.contains(PLANNED) {
                continue;
            }
            for (first, second) in suggestions(&text) {
                seen += 1;
                let known = *answered
                    .entry((first.clone(), second.clone()))
                    .or_insert_with(|| exists(&first, second.as_deref()));
                if !known {
                    let shown = second.map_or(first.clone(), |s| format!("{first} {s}"));
                    let rel = file
                        .strip_prefix(&root)
                        .unwrap_or(file)
                        .display()
                        .to_string();
                    missing.push(format!("{rel}: `charter {shown}` in {text:?}"));
                }
            }
        }
    }
    assert!(
        seen > 20,
        "only {seen} suggestions found, so the scan is not reading the messages"
    );
    let skills = shipped_skills(&root);
    assert!(skills.len() >= 3, "the walk found the skills: {skills:?}");
    let mut in_skills = 0;
    for file in &skills {
        let page = std::fs::read_to_string(file).expect("a skill reads");
        for text in skill_texts(&page) {
            for (first, second) in suggestions(&text) {
                in_skills += 1;
                let known = *answered
                    .entry((first.clone(), second.clone()))
                    .or_insert_with(|| exists(&first, second.as_deref()));
                if !known {
                    let shown = second.map_or(first.clone(), |s| format!("{first} {s}"));
                    let rel = file
                        .strip_prefix(&root)
                        .unwrap_or(file)
                        .display()
                        .to_string();
                    missing.push(format!("{rel}: `charter {shown}`"));
                }
            }
        }
    }
    assert!(
        in_skills > 10,
        "only {in_skills} suggestions found in the skills, so the scan is not reading them"
    );
    assert!(
        missing.is_empty(),
        "these messages name a command this charter does not have — name the real one, point \
         at the app's window, or say \"{PLANNED}\":\n{}",
        missing.join("\n")
    );
}

#[test]
fn a_skills_code_block_line_is_a_suggestion_and_prose_outside_one_is_not() {
    let page = "Run `charter persona show x`.\n\n```bash\ncharter wt add x   # a comment\n  charter persona list\n```\ncharter is not a command here\n";
    let found: Vec<(String, Option<String>)> = skill_texts(page)
        .iter()
        .flat_map(|t| suggestions(t))
        .collect();
    assert!(found.contains(&("persona".into(), Some("show".into()))));
    assert!(found.contains(&("wt".into(), Some("add".into()))));
    assert!(found.contains(&("persona".into(), Some("list".into()))));
    assert!(!found.iter().any(|(f, _)| f == "is"), "{found:?}");
}

#[test]
fn the_scan_sees_a_suggestion_and_a_planned_one_is_let_through() {
    let src = "fn f() { say(\"Fix it: charter wt add x\"); say(\"`charter change` is not in \\\n    this version yet\"); }\n// \"`charter nope`\"\n#[cfg(test)]\nmod t { \"`charter nope`\" }";
    let found = literals(src);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(
        suggestions(&found[0]),
        vec![("wt".to_owned(), Some("add".to_owned()))]
    );
    assert!(found[1].contains(PLANNED), "the continuation is applied");
    assert!(!exists("wt", Some("add")));
    assert!(exists("workspace", Some("live")));
    assert!(
        exists("ws", Some("todo")),
        "an alias clap accepts is a command"
    );
}
