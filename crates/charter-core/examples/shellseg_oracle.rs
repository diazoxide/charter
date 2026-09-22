//! The Rust side of the `shellseg` differential run: a command line in, its reading out.
//!
//! One JSON-encoded string per line of stdin, one JSON object per line of stdout. It exists so
//! `tests/differential/shellseg.py` can ask both implementations the same 200,000 questions
//! without anything being checked in, and it is an EXAMPLE rather than a binary because nothing
//! ships it: `cargo build -p charter-cli` does not build it, and the guard it reports on
//! reaches a chat through `charter hook pretooluse` rather than through anything here.
//!
//! The comparison is over PARSED json on the Python side, so the spacing here means nothing.

use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use charter_core::credguard;
use charter_core::floorguard;
use charter_core::forge;
use charter_core::handoffguard;
use charter_core::heredoc::{self, Header, Line};
use charter_core::leakguard;
use charter_core::livesub;
use charter_core::proseguard;
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

/// `PROBE_AT` in the harness: how many positions the quoting scanners are started at.
const PROBE_AT: usize = 4;

/// One row of `_CHARTER_PROSE` as the `tbl` answer carries it: `((noun, verb), (dest, file))`.
type ProseRow = (
    (&'static str, &'static str),
    (&'static str, Option<&'static str>),
);

/// `PROBE_PENDING` in the harness: the heredocs `heredoc_bodies` is asked to consume — one that
/// EXPANDS, one that does not, one with the `<<-` tab strip, and the empty delimiter `<<""`
/// names.
fn probe_pending() -> Vec<(String, bool, bool)> {
    vec![
        ("EOF".to_string(), true, false),
        ("EOF".to_string(), false, false),
        ("EOF".to_string(), true, true),
        (String::new(), true, false),
    ]
}

/// `PROBE_MODES` in the harness. `None` is the payload with no `permission_mode` at all, which
/// is what an attended host sends.
const PROBE_MODES: [Option<&str>; 5] = [
    None,
    Some("bypassPermissions"),
    Some("default"),
    Some("acceptEdits"),
    Some("BYPASSPERMISSIONS"),
];

/// `PROBE_DISGUISES` in the harness: the SOURCE spellings `disguised_as` is put, for both of
/// the words it is asked about.
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
    // The dotted capital I, whose `str.lower`/`to_lowercase` mappings differ: `disguised_as`
    // case-folds nothing, so this row is the one that says so.
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

/// `probe_positions` in the harness: 0, then just past the first three characters one of the
/// three scanners is really entered on. CHARACTER offsets, as every offset here is.
fn probe_positions(chars: &[char]) -> Vec<usize> {
    let mut out = vec![0usize];
    for (i, c) in chars.iter().enumerate() {
        if out.len() >= PROBE_AT {
            break;
        }
        if matches!(c, '\'' | '"' | '$' | '\n') {
            out.push(i + 1);
        }
    }
    while out.len() < PROBE_AT {
        out.push(chars.len());
    }
    out
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
    for (key, value) in stage4(cmd, fx, &chars, &segments) {
        obj.insert(key.to_string(), value);
    }
    for (key, value) in stage6(cmd) {
        obj.insert(key.to_string(), value);
    }
    answer
}

/// The golden rule's, the floor's and the two prose guards' answers — `stage4` in
/// `tests/differential/shellseg.py`.
///
/// Every arm is asked of the RAW command, not the stripped one: A2, A4, A5 and A6 all walk
/// `segment_argv(cmd)` themselves, and A5/A6 deliberately read the whole string.
fn stage4(
    cmd: &str,
    fx: &Fixture,
    chars: &[char],
    segments: &[Vec<String>],
) -> Vec<(&'static str, Value)> {
    let forges = forge::known_ordered(&fx.root);
    let befores = shellwrap::exported_env(segments);
    let s4seg: Vec<Value> = segments
        .iter()
        .zip(befores.iter())
        .map(|(toks, before)| {
            let (prog, seg_env, argv) = shellwrap::split_env(toks);
            let args: Vec<String> = argv.iter().skip(1).cloned().collect();
            let mut env = before.clone();
            env.extend(seg_env);
            json!([
                credguard::git_subcommand(&args),
                credguard::has_ssh_command_config(&args),
                credguard::has_config_env_sshcommand(&args),
                credguard::has_git_config_env_sshcommand(&env),
                credguard::is_sshcommand_config_write(&args),
                credguard::url_args(&args),
                proseguard::charter_words(&prog, &argv),
            ])
        })
        .collect();

    // Python's `sorted(set)` / `sorted(dict.items())`: every entry here is ASCII, so the two
    // languages' string order is the same order.
    let mut forge_prose: Vec<[&str; 3]> = proseguard::FORGE_PROSE
        .iter()
        .map(|(a, b, c)| [*a, *b, *c])
        .collect();
    forge_prose.sort_unstable();
    let mut publish: Vec<[&str; 3]> = floorguard::PUBLISH_FORGE
        .iter()
        .map(|(a, b, c)| [*a, *b, *c])
        .collect();
    publish.sort_unstable();
    let mut tags: Vec<&str> = floorguard::TAG_HARMLESS.to_vec();
    tags.sort_unstable();
    // `_CHARTER_PROSE`, which is `CHARTER_PROSE_ROWS` widened by the noun aliases — built here
    // the same way the module builds it, because a hand-copied second half is a row that drifts.
    let mut charter_prose: Vec<ProseRow> = Vec::new();
    for (noun, verb, dest, from_file) in proseguard::CHARTER_PROSE_ROWS {
        charter_prose.push(((noun, verb), (dest, from_file)));
        if let Some(alias) = match noun {
            "workspace" => Some("ws"),
            "worktree" => Some("wt"),
            _ => None,
        } {
            charter_prose.push(((alias, verb), (dest, from_file)));
        }
    }
    charter_prose.sort_unstable_by_key(|((noun, verb), _)| (*noun, *verb));
    let charter_prose_rows: Vec<Value> = charter_prose
        .iter()
        .map(|((noun, verb), (dest, from_file))| json!([[noun, verb], [dest, from_file]]))
        .collect();

    let at = probe_positions(chars);
    let pending = probe_pending();
    let lsub = |v: Option<&'static str>| match v {
        None => Value::Null,
        Some(s) => Value::String(s.to_string()),
    };
    let pair = |v: Option<(&'static str, String)>| match v {
        None => Value::Null,
        Some((shape, said)) => json!([shape, said]),
    };

    vec![
        (
            "tbl",
            json!([
                [
                    json!(forge_prose.len()),
                    json!(rotation(cmd, &forge_prose, 3))
                ],
                json!(publish),
                json!(tags),
                json!(livesub::SUBSTITUTIONS),
                json!(floorguard::UNATTENDED_MODE),
                [
                    json!(charter_prose_rows.len()),
                    json!(rotation(cmd, &charter_prose_rows, 2))
                ],
            ]),
        ),
        (
            "kf",
            json!(
                forges
                    .iter()
                    .map(|f| json!([f.host, f.kind.cli()]))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "sph",
            json!(
                credguard::ssh_prefix_hosts(&forges)
                    .into_iter()
                    .map(|(p, h)| json!([p, h]))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "sch",
            match credguard::single_credential_hit(cmd, &forges) {
                None => Value::Null,
                Some((shape, detail)) => json!([shape, detail]),
            },
        ),
        (
            "scr",
            json!(credguard::single_credential_reason(cmd, &forges)),
        ),
        ("s4seg", Value::Array(s4seg)),
        ("ee", json!(befores)),
        ("rfr", json!(floorguard::release_floor_reason(cmd, true))),
        ("rfa", json!(floorguard::release_floor_reason(cmd, false))),
        (
            "unat",
            json!(
                PROBE_MODES
                    .iter()
                    .map(|m| floorguard::unattended(*m))
                    .collect::<Vec<_>>()
            ),
        ),
        ("ls", lsub(livesub::live_substitution(cmd))),
        (
            "ace",
            json!(
                at.iter()
                    .map(|&i| json!([i, livesub::ansi_c_end(chars, i)]))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "dqs",
            json!(
                at.iter()
                    .map(|&i| {
                        let (hit, next) = livesub::double_quoted_substitution(chars, i);
                        json!([i, [lsub(hit), json!(next)]])
                    })
                    .collect::<Vec<_>>()
            ),
        ),
        ("hsub", lsub(livesub::heredoc_substitution(cmd))),
        (
            "hb",
            json!(
                at.iter()
                    .map(|&i| {
                        let (hit, next) = livesub::heredoc_bodies(chars, i, &pending);
                        json!([i, [lsub(hit), json!(next)]])
                    })
                    .collect::<Vec<_>>()
            ),
        ),
        ("fpc", json!(proseguard::forge_prose_command(cmd))),
        ("fsh", pair(proseguard::forge_substitution_hit(cmd))),
        (
            "cpc",
            match proseguard::charter_prose_command(cmd) {
                None => Value::Null,
                Some((where_, dest, from_file)) => json!([where_, dest, from_file]),
            },
        ),
        ("csh", pair(proseguard::charter_substitution_hit(cmd))),
    ]
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

/// A7's answers — `stage6` in `tests/differential/shellseg.py`.
///
/// Asked of the RAW command, like stage 4's arms: `handoff_line` does its own stripping through
/// `lines_a_command_could_run` and `shell_string_handoff` does its own through
/// `strip_reader_heredocs`, so a pre-stripped string would measure a call the guard never makes.
fn stage6(cmd: &str) -> Vec<(&'static str, Value)> {
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
            let shown_seg = match seg {
                None => Value::Null,
                Some(seg) => json!(shown(&seg)),
            };
            (Value::Array(per), json!([shown_seg, piped]))
        }
    };
    let raws = rotation(cmd, &PROBE_DISGUISES, PROBE_DISGUISE_N);
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
    let refusal =
        |(agent_id, harness, permission_mode): (Option<&str>, Option<&str>, Option<&str>)| {
            handoffguard::handoff_refusal(
                cmd,
                handoffguard::Caller {
                    agent_id,
                    harness,
                    permission_mode,
                },
            )
        };
    vec![
        (
            "a7tbl",
            json!([
                texts.len(),
                rotation(cmd, &texts, 2),
                marks.iter().collect::<String>(),
                shells,
                reads,
                measured,
            ]),
        ),
        ("atsr", json!(handoffguard::as_the_shell_reads(cmd))),
        (
            "da",
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
        ),
        ("dh", json!(handoffguard::disguised_handoff(cmd))),
        ("ih", json!(handoffguard::is_handoff(cmd))),
        ("ssh7", json!(handoffguard::shell_string_handoff(cmd))),
        ("a7seg", a7seg),
        ("hs7", hs7),
        (
            "hl7",
            match handoffguard::handoff_line(cmd) {
                None => Value::Null,
                Some((line, in_body)) => json!([line, in_body]),
            },
        ),
        (
            "hr",
            json!(
                PROBE_CALLERS
                    .iter()
                    .map(|c| refusal(*c).map(|(reason, _)| reason))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "hrd",
            match refusal(PROBE_CALLERS[0]) {
                None => Value::Null,
                Some((reason, denial)) => json!([reason, denial]),
            },
        ),
    ]
}

/// Python's `sorted(set)`, which is what the harness emits for a set-valued answer.
fn sorted(set: std::collections::HashSet<usize>) -> Vec<usize> {
    let mut v: Vec<usize> = set.into_iter().collect();
    v.sort_unstable();
    v
}
