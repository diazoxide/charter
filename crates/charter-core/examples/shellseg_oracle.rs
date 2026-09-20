//! The Rust side of the `shellseg` differential run: a command line in, its reading out.
//!
//! One JSON-encoded string per line of stdin, one JSON object per line of stdout. It exists so
//! `tests/differential/shellseg.py` can ask both implementations the same 200,000 questions
//! without anything being checked in, and it is an EXAMPLE rather than a binary because nothing
//! ships it: `cargo build -p charter-cli` does not build it, and the guard it reports on is not
//! wired to the hook yet either way.
//!
//! The comparison is over PARSED json on the Python side, so the spacing here means nothing.

use std::io::{BufWriter, Read, Write};

use charter_core::heredoc::{self, Header, Line};
use charter_core::shellseg::{self, LexError, Tok};
use charter_core::shellwrap;
use serde_json::{Value, json};

/// The wrappers whose option grammar is probed, plus one in no table at all. Must stay in step
/// with `PROBE_WRAPPERS` in `tests/differential/shellseg.py`.
const PROBE_WRAPPERS: [&str; 8] = [
    "env", "sudo", "xargs", "stdbuf", "timeout", "doas", "exec", "nonesuch",
];

/// The spellings `flag_name_value` is probed with — `PROBE_SPELLINGS` in the harness.
const PROBE_SPELLINGS: [&str; 5] = ["-S", "--split-string", "-C", "--chdir", "-u"];

/// How many words of a case the per-token probes run over — `PROBE_TOKENS` in the harness.
const PROBE_TOKENS: usize = 6;

/// `PROBE_OPTIONS` in the harness: `wrapper_option` and `flag_name_value` are reached only from
/// `split_env_chdir`'s option branch, so only option-shaped words are put to them.
const PROBE_OPTIONS: usize = 4;

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("stdin is readable");
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for line in input.lines() {
        if line.is_empty() {
            continue;
        }
        let cmd: String = serde_json::from_str(line).expect("each line is a JSON string");
        let answer = read_it(&cmd);
        writeln!(out, "{answer}").expect("stdout is writable");
    }
    out.flush().expect("stdout is writable");
}

fn shown(toks: &[Tok]) -> Vec<Value> {
    toks.iter()
        .map(|t| json!([t.text, t.bare, t.start, t.end]))
        .collect()
}

fn header_json(h: Option<Header>) -> Value {
    match h {
        None => Value::Null,
        Some(h) => json!([h.delim, h.expands, h.dash, h.end]),
    }
}

fn read_it(cmd: &str) -> Value {
    // `lex` and `sp` are reported apart because the offsets a run's PIECES carry are visible in
    // neither the token list (which is pre-split) nor the segments (which are text only), and
    // A7 is the caller that reads them.
    let (lex, sp, che, so) = match shellseg::lex(cmd) {
        Ok(toks) => {
            let split = shellseg::split_punctuation(toks.clone());
            let segs: Vec<Value> = heredoc::segments_of(&split)
                .into_iter()
                .map(|(seg, before)| {
                    json!([
                        seg.iter().map(|t| t.text.clone()).collect::<Vec<_>>(),
                        before
                    ])
                })
                .collect();
            (
                json!({ "toks": shown(&toks) }),
                json!({ "toks": shown(&split) }),
                Value::Bool(heredoc::compound_holds_executor(&split)),
                Value::Array(segs),
            )
        }
        Err(err) => {
            let said = match err {
                LexError::NoClosingQuotation => "No closing quotation",
                LexError::NoEscapedCharacter => "No escaped character",
            };
            (
                json!({ "err": said }),
                json!({ "err": said }),
                Value::Null,
                Value::Null,
            )
        }
    };
    let psp = match shellseg::posix_split(cmd) {
        Ok(words) => json!({ "toks": words }),
        Err(err) => json!({ "err": match err {
            LexError::NoClosingQuotation => "No closing quotation",
            LexError::NoEscapedCharacter => "No escaped character",
        } }),
    };

    let (segments, parsed) = shellseg::segment_argv_parsed(cmd);
    let line = Line::of(cmd);
    let openers = heredoc::heredoc_openers(&line);
    let starts: Vec<usize> = openers.iter().map(|m| m.start).collect();
    let words: Vec<Option<Vec<String>>> = starts
        .iter()
        .map(|&s| heredoc::heredoc_opener_words(&line, s))
        .collect();
    // Every `<<` in the source, quoted or not: `heredoc_header` is asked about positions the
    // opener filter throws away too, and its answer there is what a KEPT body's end depends on.
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

    json!({
        "ub": shellseg::unbacktick(cmd),
        "qm": shellseg::quote_map(cmd)
            .iter()
            .map(|&b| if b { '1' } else { '0' })
            .collect::<String>(),
        "sc": shellseg::splice_continuations(cmd),
        "lex": lex,
        "sp": sp,
        "seg": [segments.clone(), Value::Bool(parsed)],
        // ---- stage 2: the heredoc layout
        "dac": heredoc::desugar_ansi_c(&line),
        "psp": psp,
        "sq": shellseg::shell_quote(cmd),
        "ho": openers.iter().map(|m| json!([m.start, m.delim, m.dash, m.backslash, m.quote]))
            .collect::<Vec<_>>(),
        "hh": at_shift.iter()
            .map(|&i| json!([i, header_json(heredoc::heredoc_header(&line, i))]))
            .collect::<Vec<_>>(),
        "lp": match heredoc::line_pipelines(&line) {
            None => Value::Null,
            Some(ps) => Value::Array(
                ps.into_iter().map(|p| json!([p.argvs, p.executor, p.hcounts])).collect()),
        },
        "che": che,
        "so": so,
        "hsp": match heredoc::heredoc_strip_plan(&line) {
            None => Value::Null,
            Some(plan) => Value::Array(plan.into_iter()
                .map(|e| json!([e.delim, e.drop, header_json(e.header), e.executor]))
                .collect()),
        },
        "bh": sorted(heredoc::brief_heredocs(&line)),
        "cs": sorted(heredoc::crowded_substitutions(&line)),
        "ci": heredoc::comment_index(&line),
        "elc": heredoc::ends_in_line_continuation(cmd),
        "pc": heredoc::pipeline_continues(cmd),
        "lrt": heredoc::line_runs_text(cmd),
        "how": starts.iter().zip(words.iter()).map(|(s, w)| json!([s, w]))
            .collect::<Vec<_>>(),
        "op": starts.iter().zip(words.iter())
            .map(|(s, w)| json!([s, heredoc::opener_program(w.as_deref())]))
            .collect::<Vec<_>>(),
        "ps": starts.iter().map(|&s| json!([s, heredoc::pipeline_slice(&line, s)]))
            .collect::<Vec<_>>(),
        "hcr": starts.iter().map(|&s| json!([s, heredoc::heredoc_could_run(&line, s)]))
            .collect::<Vec<_>>(),
        "hl": heredoc::heredoc_layout(cmd).into_iter()
            .map(|l| json!([l.text, l.body, l.drop, l.executed])).collect::<Vec<_>>(),
        "srh": heredoc::strip_reader_heredocs(cmd),
        // ---- stage 2: the wrapper/env split, per segment of the parsed command
        "sec": segments.iter().map(|seg| {
            let it = shellwrap::split_env_chdir(seg.as_slice());
            json!([it.prog, it.env, it.argv, it.chdir, it.reads])
        }).collect::<Vec<_>>(),
        "cms": segments.iter().map(|s| heredoc::commit_message_on_stdin(s.as_slice())).collect::<Vec<_>>(),
        "ghb": segments.iter().map(|s| heredoc::gh_body_on_stdin(s.as_slice())).collect::<Vec<_>>(),
        "rr": segments.iter().map(|s| shellwrap::redirect_reads(s.as_slice())).collect::<Vec<_>>(),
        "gg": segments.iter().map(|s| {
            let (globals, rest) = shellwrap::git_globals(s.as_slice());
            json!([globals, rest])
        }).collect::<Vec<_>>(),
        "wo": PROBE_WRAPPERS.iter().flat_map(|base| opts.iter().map(move |tok| {
            let o = shellwrap::wrapper_option(base, tok);
            json!([base, tok, o.name, o.value, o.wants_next, o.placed])
        })).collect::<Vec<_>>(),
        "fnv": opts.iter().map(|tok| {
            let (name, value) = shellwrap::flag_name_value(tok, &PROBE_SPELLINGS);
            json!([tok, name, value])
        }).collect::<Vec<_>>(),
        "ie": probe.iter().map(|tok| json!([tok, heredoc::is_executor(tok)]))
            .collect::<Vec<_>>(),
    })
}

/// Python's `sorted(set)`, which is what the harness emits for a set-valued answer.
fn sorted(set: std::collections::HashSet<usize>) -> Vec<usize> {
    let mut v: Vec<usize> = set.into_iter().collect();
    v.sort_unstable();
    v
}
