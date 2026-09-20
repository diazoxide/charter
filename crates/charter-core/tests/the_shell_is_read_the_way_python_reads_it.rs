//! Every command line `charter/hooks.py` names as a bypass that SHIPPED, answered here the way
//! the frozen Python answers it.
//!
//! The answers are not written by hand. `tests/differential/shellseg.py --record` ran each of
//! these through the Python charter pinned at the commit the fixture planes come from, and wrote
//! `fixtures/corpora/shellseg-oracle.jsonl`; `--check` fails if the file stops being what the
//! oracle says. This test replays it with no Python present, so the ordinary `cargo test` job
//! holds the line and the differential job — which fuzzes 200,000 generated cases against the
//! live oracle — is the wider net rather than the only one.
//!
//! Six answers are compared per case, and they are compared SEPARATELY so a failure names which
//! function moved: `unbacktick`, `quote_map`, `splice_continuations`, `lex` (tokens with their
//! `bare` flag and their character offsets, or the error), `split_punctuation` (the same, after a
//! glued run is broken apart — the only place a split piece's OFFSETS are visible) and
//! `segment_argv_parsed`.

use std::path::PathBuf;

use charter_core::shellseg::{self, LexError, Tok};
use serde_json::Value;

fn corpus() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/corpora/shellseg-oracle.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is one JSON object"))
        .collect()
}

fn shown(toks: &[Tok]) -> Value {
    Value::Array(
        toks.iter()
            .map(|t| serde_json::json!([t.text, t.bare, t.start, t.end]))
            .collect(),
    )
}

fn said(err: LexError) -> &'static str {
    match err {
        LexError::NoClosingQuotation => "No closing quotation",
        LexError::NoEscapedCharacter => "No escaped character",
    }
}

#[test]
fn the_recorded_python_answer_is_the_answer_this_module_gives() {
    let rows = corpus();
    assert!(
        rows.len() >= 60,
        "the corpus is the evidence; {} rows is not it",
        rows.len()
    );
    let mut wrong: Vec<String> = Vec::new();
    for row in &rows {
        let cmd = row["cmd"].as_str().expect("every row names its command");
        let mut check = |what: &str, want: &Value, got: Value| {
            if want != &got {
                wrong.push(format!(
                    "{cmd:?}\n    {what} python={want}\n    {what}   rust={got}"
                ));
            }
        };

        check("ub", &row["ub"], Value::String(shellseg::unbacktick(cmd)));
        check(
            "qm",
            &row["qm"],
            Value::String(
                shellseg::quote_map(cmd)
                    .iter()
                    .map(|&b| if b { '1' } else { '0' })
                    .collect(),
            ),
        );
        check(
            "sc",
            &row["sc"],
            Value::String(shellseg::splice_continuations(cmd)),
        );
        match shellseg::lex(cmd) {
            Ok(toks) => {
                check(
                    "lex",
                    &row["lex"],
                    serde_json::json!({ "toks": shown(&toks) }),
                );
                let split = shellseg::split_punctuation(toks);
                check(
                    "sp",
                    &row["sp"],
                    serde_json::json!({ "toks": shown(&split) }),
                );
            }
            Err(err) => {
                check("lex", &row["lex"], serde_json::json!({ "err": said(err) }));
                check("sp", &row["sp"], serde_json::json!({ "err": said(err) }));
            }
        }
        let (segments, parsed) = shellseg::segment_argv_parsed(cmd);
        check("seg", &row["seg"], serde_json::json!([segments, parsed]));
    }
    assert!(
        wrong.is_empty(),
        "{} of {} recorded cases read differently here:\n{}",
        wrong.len(),
        rows.len(),
        wrong.join("\n")
    );
}

/// The corpus is a recording, and a recording can go stale in the one direction that matters:
/// if the rows stop covering the rules, the test above passes while proving nothing. These are
/// the four defects the Python docstrings say SHIPPED, asserted here by name so that a corpus
/// edit which drops them fails rather than silently narrows the evidence.
#[test]
fn the_four_bypasses_that_shipped_are_in_the_corpus_and_are_closed() {
    let vault = ".charter/vaults/x.json";

    // A quoted or escaped `)` is a WORD. Reading it as a subshell close gave `cat` no operand
    // and the vault path no reader, and the shipped hook ALLOWed a command that prints a vault.
    for (cmd, word) in [
        (format!("cat \\) {vault}"), ")"),
        (format!("cat ')' {vault}"), ")"),
        (format!("cat '()' {vault}"), "()"),
    ] {
        assert_eq!(
            shellseg::segment_argv(&cmd),
            vec![vec!["cat".to_string(), word.to_string(), vault.to_string(),]],
            "{cmd}: the reader kept its operand",
        );
    }

    // The `&` of `2>&1` belongs to the redirection, not to the control operator `&`. It stays a
    // token OF the segment — what matters is that the vault path is still `cat`'s operand and not
    // stranded in a segment of its own.
    assert_eq!(
        shellseg::segment_argv(&format!("cat 2>&1 {vault}")),
        vec![vec![
            "cat".to_string(),
            "2".to_string(),
            ">&".to_string(),
            "1".to_string(),
            vault.to_string(),
        ]],
    );

    // `{` is a reserved word only where a command word is expected. `cat { <vault>` is ONE
    // command that prints the vault; two segments is a false allow.
    assert_eq!(
        shellseg::segment_argv(&format!("cat {{ {vault}")),
        vec![vec!["cat".to_string(), "{".to_string(), vault.to_string()]],
    );
    // ...and in command position it really is the reserved word, so the group's program is named.
    assert_eq!(
        shellseg::segment_argv(&format!("{{ cat {vault}; }}")),
        vec![vec!["cat".to_string(), vault.to_string()]],
    );

    // A glued punctuation run is one `shlex` token: `);` has to come apart or the `cat` never
    // starts a segment of its own.
    assert_eq!(
        shellseg::segment_argv(&format!("( true );cat {vault}")),
        vec![
            vec!["true".to_string()],
            vec!["cat".to_string(), vault.to_string()],
        ],
    );
}

/// A substitution is BOTH: the command inside it runs, and its output is the enclosing command's
/// operand. Reading its parenthesis as a plain boundary took four guards from deny to allow.
#[test]
fn a_substitution_is_an_inner_segment_and_leaves_the_outer_one_whole() {
    let vault = ".charter/vaults/x.json";

    // Two readings from one pass: the INNER segment is what runs, and the OUTER one — which
    // keeps accumulating the very same tokens — is where its output lands. `cat $(echo <vault>)`
    // is a read of the vault because of the outer reading; `echo $(cat <vault>)` because of the
    // inner one. A plain boundary rule gives `cat $` and `echo <vault>`, neither of which is one.
    assert_eq!(
        shellseg::segment_argv(&format!("cat $(echo {vault})")),
        vec![
            vec!["echo".to_string(), vault.to_string()],
            vec![
                "cat".to_string(),
                "$".to_string(),
                "echo".to_string(),
                vault.to_string(),
            ],
        ],
    );
    assert_eq!(
        shellseg::segment_argv(&format!("echo $(cat {vault})")),
        vec![
            vec!["cat".to_string(), vault.to_string()],
            vec![
                "echo".to_string(),
                "$".to_string(),
                "cat".to_string(),
                vault.to_string(),
            ],
        ],
    );
    // a backtick is the same construct, spelled the old way
    assert_eq!(
        shellseg::segment_argv(&format!("echo `cat {vault}`")),
        shellseg::segment_argv(&format!("echo $(cat {vault})")),
    );
    // An ESCAPED `$` is a literal dollar, so no substitution opens — and mid-command a `(` is
    // not a subshell either, so the whole thing stays ONE segment and `cat` keeps its operand.
    assert_eq!(
        shellseg::segment_argv(&format!("cat \\$(echo {vault})")),
        vec![vec![
            "cat".to_string(),
            "$".to_string(),
            "(".to_string(),
            "echo".to_string(),
            vault.to_string(),
            ")".to_string(),
        ]],
    );
    // a QUOTED substitution is one token and stays one — a KNOWN open bypass, pinned so that a
    // later change to it is a decision rather than an accident.
    assert_eq!(
        shellseg::segment_argv(&format!("echo \"$(cat {vault})\"")),
        vec![vec!["echo".to_string(), format!("$(cat {vault})")]],
    );
}

/// An unparseable command line may not swallow the commands after the broken quote. Returning
/// the whole string as ONE segment made every invocation after the first invisible to every
/// guard, because each of them reads token 0 as the program.
#[test]
fn a_broken_quote_blinds_the_guard_to_its_own_line_and_no_other() {
    let vault = ".charter/vaults/x.json";
    let (segments, parsed) =
        shellseg::segment_argv_parsed("cd .charter/vaults\ncat x.json\necho \"");
    assert!(!parsed, "the caller has to know this answer is best-effort");
    assert!(
        segments.contains(&vec!["cat".to_string(), "x.json".to_string()]),
        "the well-formed line before the broken quote is still seen: {segments:?}",
    );

    // `$'…'` trips `shlex` and is valid bash; the `cat` after the `;` must still be a segment.
    let (segments, parsed) =
        shellseg::segment_argv_parsed(&format!("echo $'it\\'s fine' ; cat {vault}"));
    assert!(!parsed);
    assert!(
        segments
            .iter()
            .any(|s| s.first().is_some_and(|p| p == "cat")),
        "the second command is visible: {segments:?}",
    );
}

/// A comment ends at the newline the shell ends it at, and starts only where a word begins.
/// `shlex` gets both wrong on its own, which is what `_NewlineKeepingStream` and the narrowed
/// `commenters` exist for.
#[test]
fn a_comment_does_not_swallow_the_command_after_it() {
    let vault = ".charter/vaults/x.json";
    // bash runs the `cat`: the `#` is mid-word, so no comment starts.
    assert_eq!(
        shellseg::segment_argv(&format!("echo hi#; cat {vault}")),
        vec![
            vec!["echo".to_string(), "hi#".to_string()],
            vec!["cat".to_string(), vault.to_string()],
        ],
    );
    // and a real comment ends at the newline, which is the separator, not part of the comment.
    assert_eq!(
        shellseg::segment_argv(&format!("echo a # note\ncat {vault}")),
        vec![
            vec!["echo".to_string(), "a".to_string()],
            vec!["cat".to_string(), vault.to_string()],
        ],
    );
}

/// The offsets A7 will read are CHARACTER offsets into the source, including into a punctuation
/// run after it is broken apart.
#[test]
fn a_token_carries_where_it_stood_in_the_source() {
    let toks = shellseg::lex("échø ');'").expect("this lexes");
    assert_eq!(toks[0].text, "échø");
    assert_eq!(
        (toks[0].start, toks[0].end),
        (0, 4),
        "characters, not bytes"
    );
    assert_eq!(toks[1].text, ");");
    assert!(
        !toks[1].bare,
        "it was quoted, so it is a word and not two operators"
    );
    assert_eq!((toks[1].start, toks[1].end), (5, 9));

    // ...and an UNQUOTED run comes apart, each piece keeping its own place in the source.
    let split = shellseg::split_punctuation(shellseg::lex("a );b").expect("this lexes"));
    let places: Vec<_> = split
        .iter()
        .map(|t| (t.text.as_str(), t.start, t.end))
        .collect();
    assert_eq!(
        places,
        vec![("a", 0, 1), (")", 2, 3), (";", 3, 4), ("b", 4, 5)],
    );
}

/// Python's blank set is not Rust's: `str.strip()` and `str.split()` count U+001C–U+001F, and
/// `char::is_whitespace` counts none of them. Both are read on the unparseable path.
#[test]
fn the_fallbacks_blank_set_is_pythons() {
    // The `"` never closes, so nothing after it is a line boundary and the whole command is one
    // logical line that will not lex — `resegment(str.split())` is what reads it. Python's
    // `str.split()` drops the U+001C as a SEPARATOR; `str::split_whitespace` would hand it on as
    // a token of its own, and the segment would then name a program no shell would run.
    let (segments, parsed) = shellseg::segment_argv_parsed("echo \"\n\u{1c}\ncat x");
    assert!(!parsed, "the caller has to know this answer is best-effort");
    assert_eq!(
        segments,
        vec![vec![
            "echo".to_string(),
            "\"".to_string(),
            "cat".to_string(),
            "x".to_string(),
        ]],
        "U+001C is a separator to Python, not a word",
    );
}
