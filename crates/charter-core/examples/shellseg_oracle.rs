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

use charter_core::shellseg::{self, LexError};
use serde_json::{Value, json};

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

fn read_it(cmd: &str) -> Value {
    let shown = |toks: &[shellseg::Tok]| {
        toks.iter()
            .map(|t| json!([t.text, t.bare, t.start, t.end]))
            .collect::<Vec<_>>()
    };
    // `lex` and `sp` are reported apart because the offsets a run's PIECES carry are visible in
    // neither the token list (which is pre-split) nor the segments (which are text only), and
    // A7 is the caller that reads them.
    let (lex, sp) = match shellseg::lex(cmd) {
        Ok(toks) => (
            json!({ "toks": shown(&toks) }),
            json!({ "toks": shown(&shellseg::split_punctuation(toks)) }),
        ),
        Err(err) => {
            let said = match err {
                LexError::NoClosingQuotation => "No closing quotation",
                LexError::NoEscapedCharacter => "No escaped character",
            };
            (json!({ "err": said }), json!({ "err": said }))
        }
    };
    let (segments, parsed) = shellseg::segment_argv_parsed(cmd);
    json!({
        "ub": shellseg::unbacktick(cmd),
        "qm": shellseg::quote_map(cmd)
            .iter()
            .map(|&b| if b { '1' } else { '0' })
            .collect::<String>(),
        "sc": shellseg::splice_continuations(cmd),
        "lex": lex,
        "sp": sp,
        "seg": [segments, Value::Bool(parsed)],
    })
}
