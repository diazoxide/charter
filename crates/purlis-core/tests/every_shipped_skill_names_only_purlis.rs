//! **Every shipped skill names only purlis** (#1329).
//!
//! A skill is copied, not read: a model runs the command a step shows. The smart-close skill
//! showed the old `charter session record` in one step while the rest said `purlis`, and a
//! write that failed was retried under the other name with its heredoc dropped. So every
//! `SKILL.md` the app's plugin ships is held to the purlis names, by the rules that hold the
//! Rust (`no_old_name_is_spelled_outside_the_name_module`, whose spellings and word boundary
//! `old_names` shares):
//!
//! * **an old spelling** of a file, folder, marker, keychain, variable, branch, trailer or
//!   plugin name in [`purlis_core::names::ALL`], anywhere in the text;
//! * **the old binary as a command**: [`purlis_core::names::BINARY`]'s old name followed by a
//!   subcommand or a flag — bare, as a path's last part or quoted — in a fenced (backticks
//!   or tildes) or indented code block, or a code span. The bare word in prose is not looked for: a
//!   persona's or a workspace's *charter* is a word of its own.
//!
//! What still spells one on purpose is on [`STILL_SPELLED`], line by line, with why. The counts
//! are exact, so the list can only shrink.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod old_names;
use old_names::{old_spellings, spells};

/// Skills that still spell an old name, by their path under the skills folder: how many lines
/// do, and why. **Shrink only**, as the Rust guard's list is.
#[rustfmt::skip]
const STILL_SPELLED: &[(&str, usize, &str)] = &[
    // purlis's own curation actions are still `charter/<id>` in code (`curation.rs`), so the
    // skill names the ids a person's action must not take as they are. #1284 decides that
    // namespace, and this entry goes with it.
    ("add-curation-action/SKILL.md", 2, "the built-in curation actions' ids (#1284)"),
];

/// One line that spells an old name: its number, what it spells, and the line.
type Hit = (usize, String, String);

/// Where `code` runs the old binary as a command: its old name at the start of a command (the
/// start of the code, or after a space, `(`, `|`, `;`, `&`, `$` or `` ` ``), or as the last part
/// of a path, or quoted; then one space and a subcommand or a flag.
fn runs_the_old_binary(code: &str) -> Option<&'static str> {
    purlis_core::names::BINARY
        .reads
        .iter()
        .copied()
        .find(|old| {
            code.match_indices(old).any(|(at, _)| {
                let before = code[..at].chars().next_back();
                let mut after = code[at + old.len()..].chars().peekable();
                // `"charter"` and `'charter'` are the same word to a shell.
                if let Some(quote) = before.filter(|c| "\"'".contains(*c)) {
                    if after.peek() != Some(&quote) {
                        return false;
                    }
                    after.next();
                }
                let starts = before.is_none_or(|c| c.is_whitespace() || "(|;&`$/\"'".contains(c));
                starts
                    && after.next() == Some(' ')
                    && after
                        .next()
                        .is_some_and(|c| c.is_ascii_lowercase() || c == '-')
            })
        })
}

/// The code on one line of markdown: the whole line inside a fenced or an indented block, else
/// its code spans.
fn code_of(line: &str, block: bool) -> Vec<&str> {
    if block {
        return vec![line];
    }
    line.split('`').skip(1).step_by(2).collect()
}

/// Whether `line` opens or closes a fence: three or more backticks or tildes.
fn fence(line: &str) -> Option<char> {
    let line = line.trim_start();
    ['`', '~']
        .into_iter()
        .find(|mark| line.chars().take(3).filter(|c| c == mark).count() == 3)
}

/// Whether `line` is in an indented code block: four spaces or a tab in front.
fn indented(line: &str) -> bool {
    line.starts_with("    ") || line.starts_with('\t')
}

/// Each line of `text` that spells an old name.
fn hits(text: &str) -> Vec<Hit> {
    let spellings = old_spellings();
    let mut fenced: Option<char> = None;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if let Some(mark) = fence(line) {
            match fenced {
                None => fenced = Some(mark),
                Some(open) if open == mark => fenced = None,
                Some(_) => {}
            }
            continue;
        }
        let spelled = spells(line, &spellings).or_else(|| {
            code_of(line, fenced.is_some() || indented(line))
                .into_iter()
                .find_map(runs_the_old_binary)
        });
        if let Some(spelled) = spelled {
            out.push((n + 1, spelled.to_owned(), line.to_owned()));
        }
    }
    out
}

fn skills_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/plugin/skills")
}

/// Every `.md` under `dir`, by its path under `root`, with its lines that spell an old name.
fn scan(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<Hit>>, seen: &mut usize) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("the skills folder")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            scan(root, &path, out, seen);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            *seen += 1;
            let text = std::fs::read_to_string(&path).expect("a skill");
            let found = hits(&text);
            if !found.is_empty() {
                let rel = path.strip_prefix(root).expect("under the root");
                out.insert(rel.display().to_string().replace('\\', "/"), found);
            }
        }
    }
}

#[test]
fn every_shipped_skill_names_only_purlis() {
    purlis_core::unsteered!();
    let root = skills_dir();
    let (mut found, mut seen) = (BTreeMap::new(), 0);
    scan(&root, &root, &mut found, &mut seen);
    assert!(seen >= 10, "the walk read {seen} skills");
    let mut wrong = Vec::new();
    for (file, lines) in &found {
        let allowed: usize = STILL_SPELLED
            .iter()
            .filter(|(f, ..)| f == file)
            .map(|(_, n, _)| n)
            .sum();
        if allowed > lines.len() {
            wrong.push(format!(
                "{file}: allowed {allowed} but spells {} — lower its count in STILL_SPELLED",
                lines.len()
            ));
        } else if allowed < lines.len() {
            wrong.push(format!(
                "{file}: spells an old name on {} line(s):",
                lines.len()
            ));
            for (n, spelled, line) in lines {
                wrong.push(format!(
                    "    {file}:{n}: `{spelled}` in \"{}\"",
                    line.trim()
                ));
            }
        }
    }
    for (file, _, why) in STILL_SPELLED {
        if !found.contains_key(*file) {
            wrong.push(format!(
                "{file}: allowed ({why}) but spells none — remove its entry"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "skills that name the old purlis:\n{}",
        wrong.join("\n")
    );
}

// ---- the guard's own tests ------------------------------------------------------------ //

#[test]
fn the_old_command_is_found_in_a_code_block_and_a_code_span() {
    purlis_core::unsteered!();
    let text = "\
Run it:

```bash
charter session record --title \"t\" <<'RECORD'
```

Or `charter handoff billing`, or `x | charter todo add`.
";
    let lines: Vec<usize> = hits(text).into_iter().map(|(n, ..)| n).collect();
    assert_eq!(lines, [4, 7]);
}

#[test]
fn the_bare_word_and_the_purlis_names_are_not_found() {
    purlis_core::unsteered!();
    for text in [
        "Read its charter with `purlis persona show <name>`.",
        "A persona's charter, its own memory and its vault.",
        "the plane pins (`[charter] version` in the manifest)",
        "```bash\npurlis session record --title \"t\"\n```",
        "a charter of its own",
        "`purlis.toml` and `$PURLIS_PERSONA`",
    ] {
        assert_eq!(hits(text), Vec::<Hit>::new(), "{text}");
    }
}

#[test]
fn an_old_file_or_variable_name_is_found_anywhere() {
    purlis_core::unsteered!();
    for text in [
        "6. `charter.toml` `[persona] default`",
        "2. `$CHARTER_PERSONA` (empty counts as unset)",
        "the plane-wide .charter/active-persona",
    ] {
        assert_eq!(hits(text).len(), 1, "{text}");
    }
}

#[test]
fn the_old_command_is_found_however_a_shell_would_still_run_it() {
    purlis_core::unsteered!();
    for text in [
        "Run `charter --plane p handoff z`.",
        "Run `/usr/local/bin/charter handoff w`.",
        "Run `\"charter\" handoff w`.",
        "Run `'charter' handoff w`.",
        "~~~sh\ncharter handoff w\n~~~",
        "Like this:\n\n    charter handoff w\n",
    ] {
        assert_eq!(hits(text).len(), 1, "{text}");
    }
}

#[test]
fn a_fence_closes_only_on_its_own_mark() {
    purlis_core::unsteered!();
    // A `~~~` block holding a line of backticks is still open after it.
    let text = "~~~\n```\ncharter handoff w\n~~~\ncharter handoff w\n";
    let lines: Vec<usize> = hits(text).into_iter().map(|(n, ..)| n).collect();
    assert_eq!(lines, [3]);
}
