//! Every command line `charter/hooks.py` names as a bypass that SHIPPED, answered here the way
//! the frozen Python answers it.
//!
//! The answers are not written by hand, and they no longer change on their own. The retired
//! `shellseg.py` differential harness ran each command line through the Python charter pinned at
//! 50d31dc and recorded what it said, once, on 2026-09-23: the curated rows
//! (`fixtures/corpora/shellseg-oracle.jsonl`) and a coverage-selected subset of its 200,000 seeded
//! cases (`shellseg-generated.jsonl.gz`). Since then the recording IS this app's contract, and
//! no Python is needed or consulted. To change an answer deliberately, edit the row and say why
//! in the pull request (`fixtures/corpora/README.md`). Where this file says "the harness" it
//! means that script; the names it cites are the script's, kept so a row can be traced to what
//! produced it.
//!
//! Every answer is compared per case, and they are compared SEPARATELY so a failure names which
//! function moved. Stage 1's six: `unbacktick`, `quote_map`, `splice_continuations`, `lex`
//! (tokens with their `bare` flag and their character offsets, or the error), `split_punctuation`
//! (the same, after a glued run is broken apart — the only place a split piece's OFFSETS are
//! visible) and `segment_argv_parsed`. Stage 2 adds the heredoc layout — `desugar_ansi_c`,
//! `posix_split`, `shell_quote`, `heredoc_openers`, `heredoc_header`, `line_pipelines`,
//! `compound_holds_executor`, `segments_of`, `heredoc_strip_plan`, `brief_heredocs`,
//! `crowded_substitutions`, `comment_index`, `ends_in_line_continuation`, `pipeline_continues`,
//! `line_runs_text`, `heredoc_opener_words`, `opener_program`, `pipeline_slice`,
//! `heredoc_could_run`, `heredoc_layout`, `strip_reader_heredocs` — and the wrapper/env split:
//! `split_env_chdir`, `commit_message_on_stdin`, `gh_body_on_stdin`, `redirect_reads`,
//! `git_globals`, `wrapper_option`, `flag_name_value`, `is_executor`.
//!
//! **Every `pub` item of the three modules is in that list**, deliberately: stage 1's one real
//! harness defect was a field nobody diffed, and it was found by mutation rather than by reading.

mod oracle_corpus;

use std::collections::HashSet;

use charter_core::heredoc::{self, Header, Line};
use charter_core::shellseg::{self, LexError, Tok};
use charter_core::shellwrap;
use serde_json::Value;

/// `PROBE_WRAPPERS` in the harness: every recorded `wo` answer was asked with exactly these, so
/// changing one changes what every row means.
const PROBE_WRAPPERS: [&str; 8] = [
    "env", "sudo", "xargs", "stdbuf", "timeout", "doas", "exec", "nonesuch",
];

/// `PROBE_SPELLINGS` in the harness.
const PROBE_SPELLINGS: [&str; 5] = ["-S", "--split-string", "-C", "--chdir", "-u"];

/// `PROBE_TOKENS` in the harness.
const PROBE_TOKENS: usize = 6;

/// `PROBE_OPTIONS` in the harness: `wrapper_option` and `flag_name_value` are reached only from
/// `split_env_chdir`'s option branch, so only option-shaped words are put to them.
const PROBE_OPTIONS: usize = 4;

fn header_json(h: Option<Header>) -> Value {
    match h {
        None => Value::Null,
        Some(h) => serde_json::json!([h.delim, h.expands, h.dash, h.end]),
    }
}

/// Python's `sorted(set)`, which is what the harness recorded for a set-valued answer.
fn sorted(set: HashSet<usize>) -> Value {
    let mut v: Vec<usize> = set.into_iter().collect();
    v.sort_unstable();
    serde_json::json!(v)
}

/// The curated rows alone — the command lines a docstring names. The wide replay below reads
/// the frozen fuzz subset as well (`oracle_corpus::shellseg`).
fn corpus() -> Vec<Value> {
    oracle_corpus::jsonl("shellseg-oracle.jsonl")
        .into_iter()
        .map(|r| r.row)
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
    charter_core::unsteered!();
    let (ats, rows): (Vec<String>, Vec<Value>) = oracle_corpus::shellseg()
        .into_iter()
        .map(|r| (r.at, r.row))
        .unzip();
    assert!(
        rows.len() >= 300,
        "the corpus is the evidence; {} rows is not it",
        rows.len()
    );
    let mut wrong: Vec<String> = Vec::new();
    for (at, row) in ats.iter().zip(&rows) {
        let cmd = row["cmd"].as_str().expect("every row names its command");
        let mut check = |what: &str, want: &Value, got: Value| {
            if want != &got {
                wrong.push(format!(
                    "{at} {cmd:?}\n    {what} python={want}\n    {what}   rust={got}"
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
        check(
            "seg",
            &row["seg"],
            serde_json::json!([segments.clone(), parsed]),
        );

        // ---- stage 2: the heredoc layout and the wrapper/env split, each compared APART.
        //
        // A port that got `heredoc_could_run` wrong and `heredoc_layout` right by luck fails on
        // `hcr` and says which. That is the rule stage 1 set, and the reason `sp` exists at all.
        let line = Line::of(cmd);
        let openers = heredoc::heredoc_openers(&line);
        let starts: Vec<usize> = openers.iter().map(|m| m.start).collect();
        let words: Vec<Option<Vec<String>>> = starts
            .iter()
            .map(|&s| heredoc::heredoc_opener_words(&line, s))
            .collect();
        let chars: Vec<char> = cmd.chars().collect();
        let at_shift: Vec<usize> = (0..chars.len().saturating_sub(1))
            .filter(|&i| chars[i] == '<' && chars[i + 1] == '<')
            .collect();
        let probe: Vec<String> = shellseg::py_split(cmd)
            .into_iter()
            .take(PROBE_TOKENS)
            .collect();
        let opts: Vec<String> = shellseg::py_split(cmd)
            .into_iter()
            .filter(|w| w.starts_with('-') && w.chars().count() > 1)
            .take(PROBE_OPTIONS)
            .collect();

        check(
            "dac",
            &row["dac"],
            Value::String(heredoc::desugar_ansi_c(&line)),
        );
        check(
            "psp",
            &row["psp"],
            match shellseg::posix_split(cmd) {
                Ok(w) => serde_json::json!({ "toks": w }),
                Err(err) => serde_json::json!({ "err": said(err) }),
            },
        );
        check("sq", &row["sq"], Value::String(shellseg::shell_quote(cmd)));
        check(
            "ho",
            &row["ho"],
            openers
                .iter()
                .map(|m| serde_json::json!([m.start, m.end, m.delim, m.dash, m.backslash, m.quote]))
                .collect(),
        );
        check(
            "hh",
            &row["hh"],
            at_shift
                .iter()
                .map(|&i| serde_json::json!([i, header_json(heredoc::heredoc_header(&line, i))]))
                .collect(),
        );
        check(
            "lp",
            &row["lp"],
            match heredoc::line_pipelines(&line) {
                None => Value::Null,
                Some(ps) => ps
                    .into_iter()
                    .map(|p| serde_json::json!([p.argvs, p.executor, p.hcounts]))
                    .collect(),
            },
        );
        check(
            "hsp",
            &row["hsp"],
            match heredoc::heredoc_strip_plan(&line) {
                None => Value::Null,
                Some(plan) => plan
                    .into_iter()
                    .map(|e| {
                        serde_json::json!([e.delim, e.drop, header_json(e.header), e.executor])
                    })
                    .collect(),
            },
        );
        check("bh", &row["bh"], sorted(heredoc::brief_heredocs(&line)));
        check(
            "cs",
            &row["cs"],
            sorted(heredoc::crowded_substitutions(&line)),
        );
        check("ci", &row["ci"], heredoc::comment_index(&line).into());
        check(
            "elc",
            &row["elc"],
            heredoc::ends_in_line_continuation(cmd).into(),
        );
        check("pc", &row["pc"], heredoc::pipeline_continues(cmd).into());
        check("lrt", &row["lrt"], heredoc::line_runs_text(cmd).into());
        check(
            "how",
            &row["how"],
            starts
                .iter()
                .zip(words.iter())
                .map(|(s, w)| serde_json::json!([s, w]))
                .collect(),
        );
        check(
            "op",
            &row["op"],
            starts
                .iter()
                .zip(words.iter())
                .map(|(s, w)| serde_json::json!([s, heredoc::opener_program(w.as_deref())]))
                .collect(),
        );
        check(
            "ps",
            &row["ps"],
            starts
                .iter()
                .map(|&s| serde_json::json!([s, heredoc::pipeline_slice(&line, s)]))
                .collect(),
        );
        check(
            "hcr",
            &row["hcr"],
            starts
                .iter()
                .map(|&s| serde_json::json!([s, heredoc::heredoc_could_run(&line, s)]))
                .collect(),
        );
        check(
            "hl",
            &row["hl"],
            heredoc::heredoc_layout(cmd)
                .into_iter()
                .map(|l| serde_json::json!([l.text, l.body, l.drop, l.executed]))
                .collect(),
        );
        check(
            "srh",
            &row["srh"],
            Value::String(heredoc::strip_reader_heredocs(cmd)),
        );
        check(
            "sec",
            &row["sec"],
            segments
                .iter()
                .map(|seg| {
                    let it = shellwrap::split_env_chdir(seg.as_slice());
                    serde_json::json!([it.prog, it.env, it.argv, it.chdir, it.reads])
                })
                .collect(),
        );
        check(
            "cms",
            &row["cms"],
            segments
                .iter()
                .map(|s| Value::Bool(heredoc::commit_message_on_stdin(s.as_slice())))
                .collect(),
        );
        check(
            "ghb",
            &row["ghb"],
            segments
                .iter()
                .map(|s| Value::Bool(heredoc::gh_body_on_stdin(s.as_slice())))
                .collect(),
        );
        check(
            "rr",
            &row["rr"],
            segments
                .iter()
                .map(|s| serde_json::json!(shellwrap::redirect_reads(s.as_slice())))
                .collect(),
        );
        check(
            "gg",
            &row["gg"],
            segments
                .iter()
                .map(|s| {
                    let (globals, rest) = shellwrap::git_globals(s.as_slice());
                    serde_json::json!([globals, rest])
                })
                .collect(),
        );
        check(
            "wo",
            &row["wo"],
            PROBE_WRAPPERS
                .iter()
                .flat_map(|base| {
                    opts.iter().map(move |tok| {
                        let o = shellwrap::wrapper_option(base, tok);
                        serde_json::json!([base, tok, o.name, o.value, o.wants_next, o.placed])
                    })
                })
                .collect(),
        );
        check(
            "fnv",
            &row["fnv"],
            opts.iter()
                .map(|tok| {
                    let (name, value) = shellwrap::flag_name_value(tok, &PROBE_SPELLINGS);
                    serde_json::json!([tok, name, value])
                })
                .collect(),
        );
        check(
            "ie",
            &row["ie"],
            probe
                .iter()
                .map(|tok| serde_json::json!([tok, heredoc::is_executor(tok)]))
                .collect(),
        );
        // `che` and `so` are only recorded when the line lexed; the corpus carries `null`
        // otherwise, and so does this.
        let (che, so) = match shellseg::lex(cmd) {
            Ok(toks) => {
                let split = shellseg::split_punctuation(toks);
                (
                    Value::Bool(heredoc::compound_holds_executor(&split)),
                    heredoc::segments_of(&split)
                        .into_iter()
                        .map(|(seg, before)| {
                            serde_json::json!([
                                seg.iter().map(|t| t.text.clone()).collect::<Vec<_>>(),
                                before
                            ])
                        })
                        .collect(),
                )
            }
            Err(_) => (Value::Null, Value::Null),
        };
        check("che", &row["che"], che);
        check("so", &row["so"], so);
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
    charter_core::unsteered!();
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

    // ...and the second half of this test's name: each of those command lines is really IN the
    // recording, so the wide test above is running them against the Python answer and not only
    // against the expectations written here. Without this, a corpus edit could drop every row
    // that matters and both tests would still pass.
    let recorded: Vec<String> = corpus()
        .iter()
        .map(|row| row["cmd"].as_str().unwrap_or_default().to_owned())
        .collect();
    for cmd in [
        format!("cat \\) {vault}"),
        format!("cat ')' {vault}"),
        format!("cat '()' {vault}"),
        format!("cat 2>&1 {vault}"),
        format!("cat {{ {vault}"),
        format!("{{ cat {vault}; }}"),
        format!("( true );cat {vault}"),
    ] {
        assert!(
            recorded.contains(&cmd),
            "{cmd:?} is not in the recorded corpus, so nothing compares it to Python",
        );
    }
}

/// The two rules a PROOF ONLY run found the 64-row corpus could not see.
///
/// Dropping `Tok::bare` from [`shellseg::Tok::is_op`], and making the fallback's blank test
/// `char::is_whitespace` instead of Python's, each changed **no answer** in the recording and
/// only 236 and 40 of 200,000 in the fuzz. A rule whose evidence is 0.02% of a fuzz is a rule
/// with no test on the day the fuzz is trimmed, so each gets a row in the corpus — asserted
/// below — and an assertion here that names what it protects.
#[test]
fn a_quoted_group_word_and_a_line_of_separator_controls_are_the_two_thin_rules() {
    charter_core::unsteered!();
    let vault = ".charter/vaults/x.json";
    let recorded: Vec<String> = corpus()
        .iter()
        .map(|row| row["cmd"].as_str().unwrap_or_default().to_owned())
        .collect();

    // `{` and `(` are boundaries only where the shell INTERPRETS them, and COMMAND POSITION is
    // the one place where reading a quoted one as an operator changes the answer: the token is
    // consumed as a group opener instead of standing in the segment as the word it is.
    for cmd in [
        format!("'(' cat {vault}"),
        format!("'{{' cat {vault}"),
        format!("\\( cat {vault}"),
        format!("\\{{ cat {vault}"),
    ] {
        let word = if cmd.contains('(') { "(" } else { "{" };
        assert_eq!(
            shellseg::segment_argv(&cmd),
            vec![vec![word.to_string(), "cat".to_string(), vault.to_string()]],
            "{cmd}: a quoted group word stays a word",
        );
        assert!(
            recorded.contains(&cmd),
            "{cmd:?} is not in the recorded corpus"
        );
    }

    // The fallback reads Python's blank set TWICE, and this is the shape that reaches the first
    // of them: a line of nothing but separator controls, on a command the lexer cannot take
    // apart, with the broken quote LAST so the newlines in front of it are still boundaries.
    // `str::split_whitespace` would make that line a segment whose program is U+001C.
    let cmd = format!("cat {vault}\n\u{1c}\necho \"");
    let (segments, parsed) = shellseg::segment_argv_parsed(&cmd);
    assert!(!parsed);
    assert_eq!(
        segments,
        vec![
            vec!["cat".to_string(), vault.to_string()],
            vec!["echo".to_string(), "\"".to_string()],
        ],
        "a line of separator controls is blank to Python, so it is no command at all",
    );
    assert!(
        recorded.contains(&cmd),
        "{cmd:?} is not in the recorded corpus"
    );
}

/// A substitution is BOTH: the command inside it runs, and its output is the enclosing command's
/// operand. Reading its parenthesis as a plain boundary took four guards from deny to allow.
#[test]
fn a_substitution_is_an_inner_segment_and_leaves_the_outer_one_whole() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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

/// Stage 2's four clauses, each asserted by NAME so that a corpus edit which drops the row fails
/// rather than silently narrowing the evidence.
///
/// A body is stdin DATA only when a QUOTED heredoc feeds a READER whose PIPELINE runs no
/// EXECUTOR. Each clause is a defect the Python docstrings say shipped, and each is one
/// character of difference away from a body the guard stops reading.
#[test]
fn a_body_comes_out_only_when_it_is_quoted_data_a_reader_holds() {
    charter_core::unsteered!();
    let vault = ".charter/vaults/x.json";
    let recorded: Vec<String> = corpus()
        .iter()
        .map(|row| row["cmd"].as_str().unwrap_or_default().to_owned())
        .collect();
    let mut wanted: Vec<String> = Vec::new();
    let mut drops = |cmd: String, yes: bool, why: &str| {
        let stripped = heredoc::strip_reader_heredocs(&cmd);
        assert_eq!(
            stripped != cmd,
            yes,
            "{cmd:?}: {why}\n  stripped to {stripped:?}"
        );
        wanted.push(cmd);
    };

    // quoted + reader + no executor: the body is a document, and reading it as commands is what
    // refused a document DESCRIBING charter's own layout as a READ of it (#258).
    drops(
        format!("cat <<'DOC'\n{vault}\nDOC"),
        true,
        "a quoted reader body is data",
    );
    // ...behind a wrapper too: `split_env` names the program, not `env`.
    drops(
        format!("env cat <<'DOC'\n{vault}\nDOC"),
        true,
        "a reader behind a wrapper is still a reader",
    );
    // UNQUOTED: it expands before the reader sees it, so a `$( … )` in it RUNS.
    drops(
        format!("cat <<DOC\n$(cat {vault})\nDOC"),
        false,
        "an unquoted body expands",
    );
    // an EXECUTOR in the pipeline — the five spellings #973 measured, of which only the pipe
    // hands the body on, and all five of which the old pre-pass dropped.
    drops(
        format!("cat <<'A' | bash\n{vault}\nA"),
        false,
        "a shell downstream in the PIPELINE runs the body",
    );
    drops(
        format!("cat x && bash <<'A'\ncat {vault}\nA"),
        false,
        "the executor OPENS this one",
    );

    // Bash's terminator is not the regex's. `<<EO'F'` ends at `EOF`; dropping on the regex's
    // `EO` finds no terminator, runs to the end of the input, and takes the `cat` after it.
    let cmd = format!("cat <<EO'F'\nbody\nEOF\ncat {vault}");
    let kept = heredoc::strip_reader_heredocs(&cmd);
    assert!(
        kept.contains(&format!("cat {vault}")),
        "{cmd:?}: the command AFTER the heredoc must survive the drop, got {kept:?}",
    );
    wanted.push(cmd);

    // An unterminated body is never dropped, whoever opened it: "no terminator" is what every
    // delimiter disagreement in this walk looks like, and keeping it only shows the guard more.
    let cmd = format!("cat <<'A'\n{vault}");
    assert_eq!(
        heredoc::strip_reader_heredocs(&cmd),
        cmd,
        "an unterminated body is kept",
    );
    wanted.push(cmd);

    for cmd in wanted {
        assert!(
            recorded.contains(&cmd),
            "{cmd:?} is not in the recorded corpus, so nothing compares it to Python",
        );
    }
}

/// The program is not token 0, and the three things in front of it are each a measured vault
/// read: an assignment, a redirection, and the wrapper run with its option grammar.
#[test]
fn the_wrapper_run_comes_off_before_the_program_is_named() {
    charter_core::unsteered!();
    let vault = ".charter/vaults/x.json";
    let argv = |cmd: &str| {
        shellseg::segment_argv(cmd)
            .into_iter()
            .next()
            .unwrap_or_default()
    };

    // `env cat <vault>` — verified live — printed a vault while token 0 said `env`.
    assert_eq!(
        shellwrap::split_env_chdir(&argv(&format!("env cat {vault}"))).prog,
        "cat"
    );
    // `env -i` takes NOTHING; a flat value table would consume the `cat` here.
    assert_eq!(
        shellwrap::split_env_chdir(&argv(&format!("env -i cat {vault}"))).prog,
        "cat"
    );
    // ...while `stdbuf -i` DOES take one, which is why the tables are per wrapper.
    assert_eq!(
        shellwrap::split_env_chdir(&argv(&format!("stdbuf -i0 cat {vault}"))).prog,
        "cat"
    );
    // A short option is its LETTER, not its position: `-iC<dir>` is `-i -C <dir>`.
    let bundled = shellwrap::split_env_chdir(&argv("env -iC.charter/vaults cat x.json"));
    assert_eq!(bundled.prog, "cat");
    assert_eq!(
        bundled.chdir, ".charter/vaults",
        "the chdir VALUE comes back out"
    );
    // The glued short form is read before the long form's `=`, or `-Sfoo=1` splits at the
    // packed value's own `=` and `1` becomes the program.
    assert_eq!(
        shellwrap::split_env_chdir(&argv(&format!("env -Sfoo=1 cat {vault}"))).prog,
        "cat"
    );
    // A letter this grammar cannot place leaves the program UNTRUSTED and reports the rest of
    // the segment as files this command may open — the rule that keeps a short table a false
    // negative instead of a bypass.
    let unplaced = shellwrap::split_env_chdir(&argv(&format!("env -q cat {vault}")));
    assert!(
        unplaced.reads.contains(&vault.to_string()),
        "an unplaceable option makes the rest of the segment reachable, got {:?}",
        unplaced.reads,
    );
    // `env`'s operand scan is `strchr(arg, '=')`, not the shell's identifier rule.
    assert_eq!(
        shellwrap::split_env_chdir(&argv(&format!("env a-b=1 cat {vault}"))).prog,
        "cat"
    );
    // ...and `doas`'s is not, because its usage carries no assignment operand.
    assert_eq!(
        shellwrap::split_env_chdir(&argv(&format!("doas a-b=1 cat {vault}"))).prog,
        "a-b=1"
    );
    // A redirection in front of the command is neither the program nor its operand, and the
    // SHELL is what opens the path.
    let redirected = shellwrap::split_env_chdir(&argv(&format!("< {vault} cat")));
    assert_eq!(redirected.prog, "cat");
    assert_eq!(redirected.reads, vec![vault.to_string()]);
    // An assignment keeps flowing across a wrapper — the form that walked past the
    // one-credential guard while the unwrapped one was denied.
    assert_eq!(
        shellwrap::split_env_chdir(&argv("env GIT_SSH_COMMAND=/tmp/k git push")).env,
        vec!["GIT_SSH_COMMAND=/tmp/k".to_string()],
    );
}
