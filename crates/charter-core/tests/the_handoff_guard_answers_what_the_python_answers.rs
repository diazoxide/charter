//! Every handoff spelling `docs/handoff.md` records as having run with NO PROMPT, answered here
//! the way the frozen Python answers it — with no Python present.
//!
//! The answers are not written by hand. `tests/differential/shellseg.py --record` ran each of
//! these through the Python charter pinned at the commit the fixture planes come from;
//! `--check` fails if the file stops being what the oracle says. This replays the stage-6 half
//! of that recording — A7 and the seven readers under it — so the ordinary `cargo test` job
//! holds the line and the differential job, which fuzzes 200,000 generated cases against the
//! live oracle, is the wider net rather than the only one.
//!
//! # This one needs nothing on disk
//!
//! Unlike the golden rule's replay, which needs a `charter.toml` for the forge list, and unlike
//! the leak guard's, which needs a plane to walk. A7 asks no filesystem question at all: it
//! judges a SOURCE SPELLING and two fields of a payload. That is the same fact the module
//! header states as its reason for existing, and it is why this file can be a plain replay.

mod oracle_corpus;

use charter_core::handoffguard::{self, Caller};
use charter_core::{heredoc, shellseg, shellwrap};
use serde_json::{Value, json};

/// `PROBE_DISGUISES` in the harness.
const PROBE_DISGUISES: [&str; 47] = [
    "charter",
    "handoff",
    "$'handoff'",
    "ha$''ndoff",
    "${x:-handoff}",
    "{handoff,}",
    "hando?f",
    "handof[f]",
    "$h",
    "'charter'",
    "\"charter\"",
    "\\charter",
    "chart*",
    "[c]harter",
    "ch[a-z]rter",
    "handoff*",
    "*handoff*",
    "HANDOFF",
    "handoffx",
    "xhandoff",
    "$'\\x68andoff'",
    "{hand,}off",
    "\"handoff\"",
    "h?ndoff",
    "[!x]andoff",
    "$charter",
    "c'h'arter",
    "",
    "*",
    "?",
    "[",
    "[]",
    "[!]",
    "\\",
    "$",
    "{}",
    "{",
    "}",
    "handoff\n",
    "hand\\off",
    "hand$off",
    "ha*ff",
    "?",
    "**",
    "[a-",
    "charter*handoff",
    "İandoff",
];

/// `PROBE_DISGUISE_N` in the harness.
const PROBE_DISGUISE_N: usize = 4;

/// `PROBE_CALLERS` in the harness: `(agent_id, $CHARTER_HARNESS, permission_mode)`.
const PROBE_CALLERS: [(Option<&str>, Option<&str>, Option<&str>); 6] = [
    (None, Some("claude-code"), Some("default")),
    (None, Some("claude-code"), Some("bypassPermissions")),
    (Some("sub-1"), Some("claude-code"), Some("default")),
    (Some("sub-1"), Some("opencode"), Some("default")),
    (Some("sub-1"), None, Some("bypassPermissions")),
    (Some(""), Some("claude-code"), Some("default")),
];

/// `rotation` in the harness.
fn rotation<T: Clone>(cmd: &str, table: &[T], count: usize) -> Vec<T> {
    let n = cmd.chars().count();
    (0..count)
        .map(|k| table[(n + k) % table.len()].clone())
        .collect()
}

/// The curated rows alone — the command lines a docstring names. The wide replay below reads
/// the frozen fuzz subset as well (`oracle_corpus::shellseg`).
fn corpus() -> Vec<Value> {
    oracle_corpus::jsonl("shellseg-oracle.jsonl")
        .into_iter()
        .map(|r| r.row)
        .collect()
}

fn refusal(
    cmd: &str,
    caller: (Option<&str>, Option<&str>, Option<&str>),
) -> Option<(&'static str, String)> {
    let (agent_id, harness, permission_mode) = caller;
    handoffguard::handoff_refusal(
        cmd,
        Caller {
            agent_id,
            harness,
            permission_mode,
        },
    )
}

/// The `a7tbl` answer, built from the module's own constants rather than from a copy.
fn tables(cmd: &str) -> Value {
    let texts = [
        handoffguard::HANDOFF_SUBAGENT,
        handoffguard::HANDOFF_UNATTENDED,
        handoffguard::HANDOFF_SPELLING,
        handoffguard::HANDOFF_SHELL_STRING,
        handoffguard::HANDOFF_SOURCE,
    ];
    let mut marks: Vec<char> = handoffguard::SPELLING_MARKS.chars().collect();
    marks.sort_unstable();
    let mut shells: Vec<&str> = handoffguard::STRING_SHELLS.to_vec();
    shells.sort_unstable();
    let mut reads: Vec<&str> = handoffguard::REDIRECT_READS.to_vec();
    reads.sort_unstable();
    let mut measured: Vec<&str> = handoffguard::MEASURED_SUBAGENT_HARNESSES.to_vec();
    measured.sort_unstable();
    json!([
        texts.len(),
        rotation(cmd, &texts, 2),
        marks.iter().collect::<String>(),
        shells,
        reads,
        measured,
    ])
}

#[test]
fn the_recorded_python_answer_is_the_answer_this_guard_gives() {
    let (ats, rows): (Vec<String>, Vec<Value>) = oracle_corpus::shellseg()
        .into_iter()
        .map(|r| (r.at, r.row))
        .unzip();
    assert!(
        rows.len() >= 500,
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
        check("a7tbl", &row["a7tbl"], tables(cmd));
        check(
            "atsr",
            &row["atsr"],
            json!(handoffguard::as_the_shell_reads(cmd)),
        );
        let raws = rotation(cmd, &PROBE_DISGUISES, PROBE_DISGUISE_N);
        check(
            "da",
            &row["da"],
            json!(
                raws.iter()
                    .flat_map(
                        |raw| ["charter", "handoff"].into_iter().map(move |w| json!([
                            raw,
                            w,
                            handoffguard::disguised_as(raw, w)
                        ]))
                    )
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "dh",
            &row["dh"],
            json!(handoffguard::disguised_handoff(cmd)),
        );
        check("ih", &row["ih"], json!(handoffguard::is_handoff(cmd)));
        check(
            "ssh7",
            &row["ssh7"],
            json!(handoffguard::shell_string_handoff(cmd)),
        );
        let (a7seg, hs7) = match shellseg::lex(cmd) {
            Err(_) => (Value::Null, Value::Null),
            Ok(toks) => {
                let split = shellseg::split_punctuation(toks);
                let per: Vec<Value> = heredoc::segments_of(&split)
                    .into_iter()
                    .map(|(seg, _before)| {
                        let texts: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
                        let (prog, _env, argv) = shellwrap::split_env(&texts);
                        json!([
                            handoffguard::runs_handoff(&prog, &argv),
                            handoffguard::shell_string(&seg),
                        ])
                    })
                    .collect();
                let (seg, piped) = handoffguard::handoff_segment(&split);
                let shown = match seg {
                    None => Value::Null,
                    Some(seg) => json!(
                        seg.iter()
                            .map(|t| json!([t.text, t.bare, t.start, t.end]))
                            .collect::<Vec<_>>()
                    ),
                };
                (Value::Array(per), json!([shown, piped]))
            }
        };
        check("a7seg", &row["a7seg"], a7seg);
        check("hs7", &row["hs7"], hs7);
        check(
            "hl7",
            &row["hl7"],
            match handoffguard::handoff_line(cmd) {
                None => Value::Null,
                Some((line, in_body)) => json!([line, in_body]),
            },
        );
        check(
            "hr",
            &row["hr"],
            json!(
                PROBE_CALLERS
                    .iter()
                    .map(|c| refusal(cmd, *c).map(|(reason, _)| reason))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "hrd",
            &row["hrd"],
            match refusal(cmd, PROBE_CALLERS[0]) {
                None => Value::Null,
                Some((reason, denial)) => json!([reason, denial]),
            },
        );
    }
    assert!(
        wrong.is_empty(),
        "{} of {} recorded cases are answered differently here:\n{}",
        wrong.len(),
        rows.len(),
        wrong.join("\n")
    );
}

/// The second half, and the one that goes stale silently: **does the recording still COVER the
/// rules?**
///
/// A recording can fail in one direction the replay above cannot see — if the rows stop covering
/// the rules, it passes while proving nothing. So each spelling `docs/handoff.md` records as
/// having run with no prompt is asserted BY NAME, the way stages 1 to 4 did it.
#[test]
fn the_recording_still_covers_the_rules() {
    let rows = corpus();
    let recorded: Vec<String> = rows
        .iter()
        .map(|row| row["cmd"].as_str().unwrap_or_default().to_owned())
        .collect();
    let has = |cmd: &str| {
        assert!(
            recorded.iter().any(|r| r == cmd),
            "{cmd:?} is not in the recording, so the wide test above is not running it",
        );
    };

    // **The one shape that must still go THROUGH.** A guard that refused every handoff would
    // pass every other assertion in this file, so this is the first one.
    has("charter handoff beta <<'BRIEF'\nship it\nBRIEF");
    let allowed: Vec<&str> = rows
        .iter()
        .filter(|r| r["hrd"].is_null() && !r["hl7"].is_null())
        .filter_map(|r| r["cmd"].as_str())
        .collect();
    assert!(
        allowed.len() >= 4,
        "only {} recorded handoffs are ALLOWED; the guard's pass is what nothing else covers",
        allowed.len()
    );

    // The four measured spellings the host's `Bash(charter handoff *)` rule does not match,
    // each of which ran with no prompt on Claude Code 2.1.268.
    for cmd in [
        "python3 -m charter handoff beta <<'BRIEF'\nx\nBRIEF",
        "/usr/local/bin/charter handoff beta <<'BRIEF'\nx\nBRIEF",
        "charter 'handoff' beta <<'BRIEF'\nx\nBRIEF",
        "charter $'handoff' beta <<'BRIEF'\nx\nBRIEF",
    ] {
        has(cmd);
        assert_eq!(
            refusal(cmd, PROBE_CALLERS[0]).map(|(r, _)| r),
            Some(handoffguard::REASON_SPELLING),
            "{cmd:?}",
        );
    }

    // A string a shell runs, and a heredoc body it runs — the two halves of the same fact, and
    // the second is the one whose default used to be the OPPOSITE of the leak guard's.
    for cmd in [
        "eval 'charter handoff beta'",
        "bash -c 'charter handoff beta'",
        "sh -lc 'charter handoff beta'",
        "bash <<'EOF'\ncharter handoff beta <<'BRIEF'\nx\nBRIEF\nEOF",
    ] {
        has(cmd);
        assert_eq!(
            refusal(cmd, PROBE_CALLERS[0]).map(|(r, _)| r),
            Some(handoffguard::REASON_SHELL_STRING),
            "{cmd:?}",
        );
    }

    // ...against the bodies a READER swallows, which are data. Both directions, so the plan the
    // leak guard and this guard share cannot drift in either.
    for cmd in [
        "git commit -F - <<'MSG'\ncharter handoff beta\nMSG",
        "cat <<'DOC'\ncharter handoff beta\nDOC",
    ] {
        has(cmd);
        assert_eq!(refusal(cmd, PROBE_CALLERS[0]), None, "{cmd:?}");
    }

    // Every sentence `HANDOFF_SOURCE` can be filled with is really recorded, which is what makes
    // `handoff_source` a `pub` item with evidence rather than one with a `COVERED_ELSEWHERE` row
    // and nothing behind it.
    let sources: Vec<&str> = rows
        .iter()
        .filter(|r| r["hrd"][0] == json!(handoffguard::REASON_BRIEF_SOURCE))
        .filter_map(|r| r["hrd"][1].as_str())
        .collect();
    for what in [
        "a here-string (<<<)",
        "a file (<), which the prompt shows as a path rather than as the brief",
        "a pipe",
        "no heredoc at all",
        "more than one heredoc, and the shell sends charter only the last",
        "an unquoted heredoc, which expands $… and `…` in the brief",
        "a live command substitution",
        "a quote or an escape left open on its line, so no heredoc can be seen feeding it",
    ] {
        assert!(
            sources.iter().any(|d| d.contains(what)),
            "no recorded brief-source denial says {what:?}, so nothing compares that sentence",
        );
    }

    // The three arms in front of the spelling, and the ORDER they are asked in. `hr`'s six
    // callers are what make that visible: the same command, six payloads, six answers.
    let canonical = rows
        .iter()
        .find(|r| r["cmd"] == json!("charter handoff beta <<'BRIEF'\nship it\nBRIEF"))
        .expect("the canonical row");
    assert_eq!(
        canonical["hr"],
        json!([
            // attended, main conversation: through.
            Value::Null,
            // unattended: refused, and nothing about the spelling was asked.
            handoffguard::REASON_UNATTENDED,
            // a sub-agent on a MEASURED harness: refused, before the unattended check.
            handoffguard::REASON_SUBAGENT,
            // ...and on one nobody measured, an `agent_id` means nothing.
            Value::Null,
            // no harness word at all: the same, and the unattended arm then answers.
            handoffguard::REASON_UNATTENDED,
            // an EMPTY `agent_id` is the main conversation, not a sub-agent.
            Value::Null,
        ]),
        "the order of A7's first three arms is not what the oracle recorded",
    );
}
