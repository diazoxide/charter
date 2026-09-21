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
use std::path::{Path, PathBuf};

use charter_core::heredoc::{self, Header, Line};
use charter_core::leakguard;
use charter_core::pypath;
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

/// `PROBE_OPERANDS` in the harness. The operands `names_a_vault_path`, `normpath`, `realpath`,
/// `join` and `gh_at_path` are put, rotated per case.
const PROBE_OPERANDS: [&str; 46] = [
    ".charter/vaults/x.json",
    ".charter",
    ".charter/",
    ".charterx",
    ".edm/vaults",
    ".",
    "..",
    "/",
    "//",
    "///x",
    "sub",
    "docs",
    "here",
    "up",
    "tostate",
    "hop1",
    "dangling",
    "",
    "x",
    "a/b/../vaults",
    ".charter//vaults",
    ".charter/./vaults",
    ".charter/vaults/../..",
    ".CHARTER/VAULTS/x",
    ".charter/actİve-persona",
    ".charter/fıngerprint.key",
    "body=@notes.md",
    "body=@-",
    "body=x=@notes.md",
    "body",
    "=@x",
    "sub/../..",
    "/a/b/../..",
    "../..",
    "İ",
    "\u{a0}",
    "x\n",
    ".charter/vaults\n",
    "-",
    "./",
    ".charter\n",
    "x/.charter\n",
    ".edm\n",
    "absvaults",
    "absvaults/db.json",
    "here/absvaults",
];

/// `PROBE_PATTERNS` in the harness — every shape `fnmatch._translate` answers differently.
const PROBE_PATTERNS: [&str; 37] = [
    "*", "?", ".charter", ".char*", "[a-z]*", "[!a-z]", "[]]", "[]a]", "[b-a]", "[b-a-c]", "[a-]",
    "[-a]", "[^a]", "[[a]", "[", "[!]", "*.json", "**/x", "a**b", "[a\\-c]", "[&&]", "[|~]",
    "x[0-9]y", "*/*", "İ*", "[iı]", "**", "***", "[a-c-e]", "[!b-a]", "db.json", "*.py", "[\\]",
    "a[b", "[z-a]x", "[--0]", "[+--]",
];

/// `PROBE_NAMES` in the harness.
const PROBE_NAMES: [&str; 29] = [
    "vaults",
    ".charter",
    "db.json",
    "x",
    "",
    "-",
    "]",
    "^",
    "[",
    "a-c",
    "b",
    "İ",
    "ı",
    "browser",
    "active-persona",
    "a\\b",
    "&",
    "|",
    "~",
    "0",
    "a.json",
    "prefs",
    "ab",
    "aXb",
    "+",
    ".",
    "*",
    "a/b",
    "x/y/z",
];

/// `PROBE_PATHS` and `PROBE_GLOBS` in the harness.
const PROBE_PATHS: usize = 4;
const PROBE_GLOBS: usize = 3;

/// The fixture paths, which the harness makes and passes over the environment: the core holds
/// no globals, so where the Python reads `config.STATE_DIR` this takes the directory as an
/// argument and the example is what knows which one.
struct Fixture {
    root: PathBuf,
    state: PathBuf,
    rich: PathBuf,
    missing: PathBuf,
}

impl Fixture {
    fn from_env() -> Self {
        let get = |k: &str| PathBuf::from(std::env::var(k).unwrap_or_default());
        Self {
            root: get("SHELLSEG_FIXTURE_ROOT"),
            state: get("CHARTER_HOME"),
            rich: get("SHELLSEG_FIXTURE_RICH"),
            missing: get("SHELLSEG_FIXTURE_MISSING"),
        }
    }

    /// `rel` in the harness: a path as the corpus can carry it.
    fn rel(&self, p: Option<&Path>) -> Value {
        let Some(p) = p else { return Value::Null };
        let p = p.to_string_lossy().into_owned();
        let root = self.root.to_string_lossy().into_owned();
        if p == root {
            return Value::String(".".into());
        }
        let with_sep = format!("{root}/");
        match p.strip_prefix(&with_sep) {
            Some(rest) => Value::String(rest.to_string()),
            None => Value::String("<outside>".into()),
        }
    }

    /// `probe_cwds` in the harness.
    fn cwds(&self) -> [String; 6] {
        [
            self.root.to_string_lossy().into_owned(),
            self.root.join("sub").to_string_lossy().into_owned(),
            String::new(),
            ".".to_string(),
            self.state.to_string_lossy().into_owned(),
            "sub".to_string(),
        ]
    }

    /// `glob_entries` in the harness.
    fn glob_entries(&self) -> [PathBuf; 6] {
        [
            self.state.join("vaults"),
            self.rich.join("browser"),
            self.rich.join("active-persona"),
            self.state.join("vaults").join("db.json"),
            self.missing.clone(),
            self.root.join("sub"),
        ]
    }
}

/// `rotation` in the harness: `count` entries of a table, rotated by the case's CHARACTER
/// length. A character count, not a byte count — CPython's `len` is code points.
fn rotation<T: Clone>(cmd: &str, table: &[T], count: usize) -> Vec<T> {
    let n = cmd.chars().count();
    (0..count)
        .map(|k| table[(n + k) % table.len()].clone())
        .collect()
}

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("stdin is readable");
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let fixture = Fixture::from_env();
    for line in input.lines() {
        if line.is_empty() {
            continue;
        }
        let cmd: String = serde_json::from_str(line).expect("each line is a JSON string");
        let answer = read_it(&cmd, &fixture);
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

fn read_it(cmd: &str, fx: &Fixture) -> Value {
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

    // ---- stage 3's probes, chosen the way `tests/differential/shellseg.py` chooses them: a
    // window that ROTATES with the case's character length, so a 200,000-case run sweeps every
    // table without any one case carrying the whole of it, and nothing extra goes on the wire.
    let cwds = fx.cwds();
    let cwd = cwds[cmd.chars().count() % cwds.len()].clone();
    let ops: Vec<String> = rotation(cmd, &PROBE_OPERANDS, PROBE_PATHS)
        .into_iter()
        .map(str::to_string)
        .collect();
    let pats: Vec<String> = rotation(cmd, &PROBE_PATTERNS, PROBE_GLOBS)
        .into_iter()
        .map(str::to_string)
        .collect();
    let probe_names: Vec<String> = rotation(cmd, &PROBE_NAMES, PROBE_GLOBS)
        .into_iter()
        .map(str::to_string)
        .collect();
    let entries: Vec<PathBuf> = rotation(cmd, &fx.glob_entries(), 2);
    // The per-segment answers are over the STRIPPED command's segments, because that is what
    // `leak_reason` walks: a body a reader swallows is not a segment of anything.
    let (guard_segments, _guard_parsed) =
        shellseg::segment_argv_parsed(&heredoc::strip_reader_heredocs(cmd));

    let mut answer = json!({
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
        "ho": openers.iter().map(|m| json!([m.start, m.end, m.delim, m.dash, m.backslash, m.quote]))
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
    });
    // Stage 3's answers are inserted rather than written into the literal above: one `json!`
    // with every key of all three stages in it overruns rustc's macro recursion limit, and
    // raising that limit for a dev-only example is a worse trade than one `insert` loop.
    let obj = answer.as_object_mut().expect("the answer is an object");
    for (key, value) in stage3(
        cmd,
        fx,
        &cwd,
        &ops,
        &pats,
        &probe_names,
        &entries,
        &guard_segments,
    ) {
        obj.insert(key.to_string(), value);
    }
    answer
}

/// The leak guard's answers — `stage3` in `tests/differential/shellseg.py`.
#[allow(clippy::too_many_arguments)] // the probes are the harness's contract, not a struct
fn stage3(
    cmd: &str,
    fx: &Fixture,
    cwd: &str,
    ops: &[String],
    pats: &[String],
    names: &[String],
    entries: &[PathBuf],
    guard_segments: &[Vec<String>],
) -> Vec<(&'static str, Value)> {
    let gseg: Vec<Value> = guard_segments
        .iter()
        .map(|seg| {
            let it = shellwrap::split_env_chdir(seg.as_slice());
            let operands = leakguard::file_operands(&it.prog, &it.argv);
            let walked = leakguard::walks_into_guarded_state(&it.prog, &it.argv, cwd, &fx.state);
            Value::Array(vec![
                json!(leakguard::is_charter(&it.prog, &it.argv)),
                json!(operands),
                json!(leakguard::spliced_operands(&operands)),
                json!(leakguard::gh_file_operands(&it.argv)),
                json!(leakguard::excluded_names(&it.prog, &it.argv)),
                json!(leakguard::walks_directories(&it.prog, &it.argv)),
                fx.rel(walked.as_deref()),
            ])
        })
        .collect();
    let gse: Vec<Value> = [&fx.state, &fx.rich, &fx.missing]
        .into_iter()
        .map(|d| {
            let mut ns: Vec<String> = leakguard::guarded_state_entries(d)
                .iter()
                .map(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
                .collect();
            ns.sort();
            json!(ns)
        })
        .collect();
    vec![
        // The probe INPUTS, beside the answers: the harness and this file each build the
        // rotation from their own copy of the tables, so a drift between them shows up HERE, by
        // name, instead of as a mysterious divergence in whatever the probes fed.
        (
            "pr",
            json!([
                cmd.chars().count() % fx.cwds().len(),
                ops,
                pats,
                names,
                cmd.chars().count() % fx.glob_entries().len(),
            ]),
        ),
        ("lr", json!(leakguard::leak_reason(cmd, cwd, &fx.state))),
        (
            "lacr",
            json!(
                leakguard::lines_a_command_could_run(cmd)
                    .into_iter()
                    .map(|(text, ex)| json!([text, ex]))
                    .collect::<Vec<_>>()
            ),
        ),
        ("gseg", Value::Array(gseg)),
        (
            "nvp",
            json!(
                ops.iter()
                    .map(|o| leakguard::names_a_vault_path(o))
                    .chain(std::iter::once(leakguard::names_a_vault_path(cmd)))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "iab",
            json!(ops.iter().map(|o| pypath::is_abs(o)).collect::<Vec<_>>()),
        ),
        (
            "np",
            json!(ops.iter().map(|o| pypath::normpath(o)).collect::<Vec<_>>()),
        ),
        (
            "pj",
            json!(
                ops.iter()
                    .flat_map(|a| ops.iter().map(move |b| pypath::join(a, b)))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "rp",
            Value::Array(
                ops.iter()
                    .map(|o| fx.rel(Some(Path::new(&pypath::realpath(o)))))
                    .collect(),
            ),
        ),
        (
            "fnm",
            json!(
                names
                    .iter()
                    .flat_map(|n| pats
                        .iter()
                        .map(move |p| json!([n, p, pypath::fnmatch(n, p)])))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "gap",
            json!(
                ops.iter()
                    .map(|o| leakguard::gh_at_path(o))
                    .collect::<Vec<_>>()
            ),
        ),
        ("gse", Value::Array(gse)),
        (
            "gsi",
            json!(
                entries
                    .iter()
                    .flat_map(|e| pats.iter().flat_map(move |p| {
                        [0usize, 512]
                            .into_iter()
                            .map(move |lim| leakguard::glob_selects_inside(e, p, lim))
                    }))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "wigs0",
            fx.rel(leakguard::walk_into_guarded_state(cwd, ops, pats, &fx.state).as_deref()),
        ),
    ]
}

/// Python's `sorted(set)`, which is what the harness emits for a set-valued answer.
fn sorted(set: std::collections::HashSet<usize>) -> Vec<usize> {
    let mut v: Vec<usize> = set.into_iter().collect();
    v.sort_unstable();
    v
}
