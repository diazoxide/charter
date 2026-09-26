//! Which lines of a command are a heredoc **body**, which of those are stdin DATA the guard may
//! drop, and which of them a shell would RUN.
//!
//! A port of `charter/hooks.py`'s `_strip_reader_heredocs` and its whole transitive closure —
//! `_heredoc_layout`, `_heredoc_strip_plan`, `_line_pipelines`, `_heredoc_openers`,
//! `_heredoc_opener_words`, `_heredoc_header`, `_heredoc_could_run`, `_crowded_substitutions`,
//! `_pipeline_slice`, `_line_runs_text`, `_brief_heredocs`, `_opener_program`, `_is_executor`,
//! `_desugar_ansi_c`, `_compound_holds_executor`, `_commit_message_on_stdin`,
//! `_gh_body_on_stdin`, `_ends_in_line_continuation`, `_comment_index`, `_pipeline_continues`,
//! `_segments_of` and their tables. Measured with the same AST walk #92 used: 44 definitions and
//! 1,445 lines of Python on top of stage 1, `_split_env_chdir` ([`crate::shellwrap`]) included,
//! because `_line_pipelines` names each command's program through `_split_env`.
//!
//! # Where this is called from
//!
//! [`crate::toolgate`] — `charter/hooks.py:pretooluse`'s eight refusals, in its order — and
//! through it `charter hook pretooluse` (M3.1 stage 6).
//!
//! Until that stage the switch was closed: `main.rs`'s `is_a_tool_hook` answered every word in
//! the `pretooluse`/`posttooluse` namespace with exit 2 — *block* — because a PARTIAL guard on
//! the switch turns fail-closed into allow-everything-except-the-arm-that-is-ported. It moved
//! once all eight arms were standing, and never earlier. The other eight words in that
//! namespace still block, for the same reason they always did.
//! # Why a body has to be judged at all
//!
//! `_segment_argv` shlex-splits the whole command string, so `cat > file <<'DOC' … DOC` hands the
//! body to the leak check as `cat`'s argv, and a document *describing* charter's own layout is
//! refused as a *read* of it — and documentation about charter is exactly the text most likely to
//! name these paths. So a body that is stdin DATA comes out.
//!
//! Dropping the wrong one is the expensive direction, and it has three shapes, each of which
//! shipped:
//!
//! - **a body a shell RUNS.** `cat <<'A' | bash` feeds the body to `bash`; dropping it hides the
//!   very commands the guard exists to read. So `has_executor` is per PIPELINE and not per
//!   command, and a pipeline can span physical lines.
//! - **a body that ends where the guard thinks it does and not where BASH does.** For `<<EO'F'`
//!   bash's terminator is `EOF`, while the Python's opener pattern read `EO`; dropping on that
//!   reading found no terminator, ran to the end of the input, and took real commands with it.
//!   So there is one reading of a heredoc, bash's ([`heredoc_openers`], [`heredoc_header`],
//!   [`body_extent`]), and every guard asks it (#359).
//! - **a line the whole-line pass cannot attribute at all** — a `<<` in a comment, in a group, in
//!   a substitution. One default for every body on such a line is what let a canonical handoff
//!   inside `bash <<'EOF'` reach the host with no prompt, so the fallback is **per heredoc**.
//!
//! Every ambiguity therefore resolves toward KEEPING the body visible. The guard above reads more
//! text than it strictly must; that costs a refusal, never a leak.
//!
//! # One line, read once
//!
//! The oracle's `_quote_map` is `@functools.lru_cache(maxsize=512)`, and every function below asks
//! it per POSITION. Uncached that is O(n²) per line, three times over. [`Line`] is the
//! memoisation, structurally: a line's characters and its quote map are built once and handed to
//! everything that reads them.
//!
//! # The evidence
//!
//! Stage 1's differential harness, extended rather than replaced, put the same question to this
//! module and to the frozen Python over hundreds of thousands of generated command lines,
//! comparing each answer SEPARATELY so a divergence was attributed rather than merely seen. Its
//! answers are recorded — `fixtures/corpora/shellseg-oracle.jsonl` and a coverage-chosen subset of
//! the generated cases, `shellseg-generated.jsonl.gz` (ADR 0046) — and
//! `tests/the_shell_is_read_the_way_python_reads_it.rs` replays them with no Python present.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use crate::memstore::is_python_space;
use crate::shellseg::{self, Quoting, Tok};
use crate::shellwrap::{self, base_lower};

/// Programs that RUN a heredoc reaching their pipeline as CODE, so its body is never data —
/// `_EXECUTORS`.
///
/// Two shapes: a shell or interpreter whose stdin IS a script (`bash <<X`, `python3 <<X`), and a
/// program that turns the stream into a COMMAND it runs (`xargs`, `parallel`, `eval`).
/// `awk`/`sed`/`grep` are deliberately absent — their program is an ARGUMENT and the heredoc is a
/// data stream, which is exactly the body that must stay strippable. A wrapper (`env`, `sudo`,
/// `nohup`, `timeout`, …) is absent too: it does not run the body, the program it wraps does, and
/// [`shellwrap::split_env`] names that program.
///
/// This is a NAME list: a shell spelled a way this set misses is a missed *deny*, never a wrong
/// one, because the fallback is to keep the body VISIBLE rather than to strip it.
const EXECUTORS: [&str; 42] = [
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "mksh",
    "csh",
    "tcsh",
    "ash",
    "busybox",
    "python",
    "python2",
    "python3",
    "pypy",
    "pypy3",
    "ipython",
    "node",
    "nodejs",
    "deno",
    "bun",
    "ts-node",
    "tsx",
    "perl",
    "ruby",
    "irb",
    "php",
    "lua",
    "luajit",
    "tclsh",
    "osascript",
    "groovy",
    "scala",
    "jshell",
    "julia",
    "elixir",
    "iex",
    "erl",
    "escript",
    "xargs",
    "parallel",
    "eval",
];

/// Programs that run their input on ANOTHER machine, so the body is a script even though the
/// local process is not a shell — `_REMOTE_SHELLS`. Kept apart from [`EXECUTORS`], which the leak
/// guard also reads: what `ssh` does with a body is a question about the handoff gate, not about
/// a secret leaving this host.
const REMOTE_SHELLS: [&str; 1] = ["ssh"];

/// The programs whose heredoc body is stdin DATA — `_READERS`.
///
/// Stage 4 (`_leak_reason`) reads this same set; it is here because
/// [`heredoc_strip_plan`] is its first caller, and one constant is what keeps the two from
/// answering differently about the same program.
const READERS: [&str; 16] = [
    "cat", "less", "more", "head", "tail", "bat", "nl", "tac", "xxd", "od", "strings", "grep",
    "rg", "ag", "awk", "sed",
];

/// The reserved words that open a compound command whose body runs commands — `_COMPOUND_WORDS`.
const COMPOUND_WORDS: [&str; 6] = ["while", "until", "for", "select", "if", "case"];

/// The tokens a COMMAND POSITION follows — `_COMMAND_POSITION_AFTER`. A word is only reserved
/// where a command word is expected, which is exactly after one of these; elsewhere it is an
/// ordinary argument.
const COMMAND_POSITION_AFTER: [&str; 15] = [
    ";", ";;", "&&", "||", "|", "|&", "&", "\n", "do", "then", "else", "elif", "in", "{", "(",
];

/// Operators that START a new command where they appear unquoted — `_COMMAND_STARTS`. A closed
/// `$( … )`, `${ … }` or `( … )` is NOT one of them: it is an argument, and the command it sits
/// in goes on.
const COMMAND_STARTS: [&str; 8] = ["&&", "||", ";;", ";", "|&", "|", "&", "\n"];

/// Separators that END a pipeline — `_PIPELINE_BREAKS`. `|` is deliberately absent: it CONTINUES
/// one, handing the body onward.
const PIPELINE_BREAKS: [&str; 6] = ["&&", "||", ";;", ";", "&", "\n"];

/// A `#` may begin a comment only where a word begins — `_BEFORE_COMMENT`. `echo a#b` prints
/// `a#b`; `echo a #b` has a comment.
const BEFORE_COMMENT: &str = " \t\n;|&()<>";

/// The `gh` commands whose `--body-file -` reads a body gh posts and never runs.
const GH_BODY_COMMANDS: [(&str, &str); 4] = [
    ("pr", "create"),
    ("pr", "comment"),
    ("issue", "create"),
    ("issue", "comment"),
];

/// One line, read ONCE: its characters and its quote map.
///
/// Everything below indexes by CHARACTER, because CPython indexes a `str` by character and every
/// offset here is compared against one the oracle produced. The quote map is built with the line
/// because the oracle's is memoised with `lru_cache` and the callers ask it per position — see
/// the module header.
#[derive(Clone, Debug)]
pub struct Line {
    chars: Vec<char>,
    quoting: Quoting,
}

impl Line {
    /// Read `text` once.
    pub fn of(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            quoting: Quoting::of(text),
        }
    }

    fn len(&self) -> usize {
        self.chars.len()
    }

    /// The line's CHARACTERS, for a walk that needs them but not the quote map —
    /// [`crate::livesub`]. `pub(crate)` rather than `pub`: it hands out this type's
    /// representation, and the differential compares answers rather than internals.
    pub(crate) fn chars(&self) -> &[char] {
        &self.chars
    }

    /// `_inside_quotes(line, at)`.
    fn quoted(&self, at: usize) -> bool {
        self.quoting.inside(at as isize)
    }

    /// `line.startswith(s, i)`.
    fn starts_with(&self, i: usize, s: &str) -> bool {
        let n = s.chars().count();
        i + n <= self.chars.len() && self.chars[i..i + n].iter().copied().eq(s.chars())
    }

    /// `line[a:b]`, with Python's clamping — `a` past `b` is the empty string.
    fn slice(&self, a: usize, b: usize) -> String {
        let hi = b.min(self.chars.len());
        let lo = a.min(hi);
        self.chars[lo..hi].iter().collect()
    }

    /// The line itself.
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// Whether a quote is still open at end of input — `_quote_map(line)[-1]`.
    pub fn quote_open_at_end(&self) -> bool {
        self.quoting.at_end()
    }
}

/// `_VERSIONED` from `charter/toolgate.py`: the interpreter names carrying a version suffix.
///
/// Asked of the tool gate's own pattern in the oracle rather than of a second spelling here, for
/// the reason the oracle gives: that pattern already answers "is this binary an interpreter that
/// runs text", and two regexes for one question drift apart. Its over-matching is load-bearing in
/// this direction too — a name it wrongly admits only keeps a body visible.
///
/// `\n?$` rather than `$` because CPython's `$` matches before a newline that ends the string and
/// the crate's does not.
fn versioned_re() -> &'static Regex {
    static ONCE: OnceLock<Regex> = OnceLock::new();
    ONCE.get_or_init(|| {
        Regex::new(
            r"^(?:python|pypy|node|deno|bun|perl|ruby|php|lua|bash|sh|zsh|ksh|tclsh|julia|scala|pip|uv)[0-9]+(?:[._-][0-9]+)*\n?$",
        )
        .expect("a pattern this module wrote")
    })
}

/// `_opener_program`'s shape test: a program name is `[a-z0-9_.+-]+` and nothing else.
fn plain_name_re() -> &'static Regex {
    static ONCE: OnceLock<Regex> = OnceLock::new();
    ONCE.get_or_init(|| Regex::new(r"^[a-z0-9_.+-]+$").expect("a pattern this module wrote"))
}

/// Whether `name` is a program in [`EXECUTORS`], version suffix included — `_is_executor`.
pub fn is_executor(name: &str) -> bool {
    let base = base_lower(name);
    EXECUTORS.contains(&base.as_str()) || versioned_re().is_match(&base)
}

/// One heredoc a line opens: where its `<<` stands, and bash's reading of the header after it.
///
/// **The one reading of a heredoc.** Every guard that asks which heredocs a line opens, where a
/// body ends or whether it expands asks [`heredoc_openers`], [`heredoc_header`] and
/// [`body_extent`], and nothing else. The frozen Python had a second reading, the `_HEREDOC_RE`
/// pattern, which knew only a delimiter spelled as one identifier inside one pair of quotes.
/// Where the two disagreed about one line, the strip plan paired one reading's opener with the
/// other's body, and a real command after a heredoc was dropped as body text (#359).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opener {
    /// The CHARACTER offset of the `<`.
    pub start: usize,
    /// The header bash reads after it.
    pub header: Header,
}

/// The heredocs `line` opens, left to right — `_heredoc_openers`, read the way bash reads them.
///
/// An opener is an unquoted `<<` that is not part of a here-string's `<<<` and has a delimiter
/// word after it ([`heredoc_header`]). A `<<` inside quotes opens nothing: `echo "use <<EOF for
/// heredocs"` is a sentence and `rg -n '<<\w' docs/` is a pattern. Counting one costs twice —
/// the count stops agreeing with the lexer's, which makes every plan on the line unknown, and
/// the phantom becomes a heredoc whose terminator never arrives, so its "body" swallows the rest
/// of the input and the REAL opener after it is never consulted. A `<<<` is stepped over whole,
/// because the last two of its characters are no opener.
///
/// A `<<` in a `#` comment or in `$(( … ))` arithmetic is still counted. The lexer counts
/// neither, so the counts disagree, the strip plan is unknown, and every body on the line stays
/// visible, which only ever shows a guard more text.
pub fn heredoc_openers(line: &Line) -> Vec<Opener> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < line.len() {
        if line.starts_with(i, "<<<") {
            i += 3;
        } else if line.starts_with(i, "<<") && !line.quoted(i) {
            match heredoc_header(line, i) {
                Some(header) => {
                    let start = i;
                    i = header.end;
                    out.push(Opener { start, header });
                }
                None => i += 2,
            }
        } else {
            i += 1;
        }
    }
    out
}

/// What [`heredoc_header`] read: `(delimiter, it expands, `<<-` strips tabs, index past the
/// header)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// The delimiter after BASH's quote removal — `EOF` for `<<EO'F'`.
    pub delim: String,
    /// Whether the body expands. **Any quoting anywhere in the delimiter makes the whole body
    /// literal** — `<<'EOF'`, `<<"EOF"`, `<<\EOF` and even `<<EO'F'` all stop expansion, verified
    /// against bash.
    pub expands: bool,
    /// The `<<-` flag.
    pub dash: bool,
    /// One past the header.
    pub end: usize,
    /// The shells end this body at different lines — a `$"…"` in the delimiter, which bash reads
    /// as the double-quoted word and zsh as a `$` before it. `delim` is bash's reading; a caller
    /// must not let either reading hide a line the other runs.
    pub shells_disagree: bool,
}

/// `_heredoc_header`: bash's own reading of the `<<` at `i`, or `None` when no delimiter word
/// follows.
///
/// The ONLY source for where a body ENDS. The Python's other reading, `_HEREDOC_RE`, stopped at
/// the first quote (`<<'EO'F` read as `EO`), so its terminator was never found, the "body" ran to
/// the end of the input, and the commands after the heredoc went with it.
///
/// Quote removal is bash's, checked against GNU bash 3.2.57 and zsh 5.9, which agree on every
/// form here but one:
///
/// - `'…'` is literal, and `\x` outside quotes is `x`;
/// - inside `"…"` a backslash is removed before `$`, `` ` ``, `"` and `\`, and a
///   backslash-newline goes altogether (`<<"EO\<newline>F"` is `EOF`); before anything else the
///   backslash stays (`"E\xF"` is `E\xF`);
/// - `$'…'` is ANSI-C quoting and is decoded (`<<$'E\x4fF'` is `EOF`), by the same
///   [`shellseg::ansi_c_decode`] every guard's word reader uses;
/// - `$"…"` is read as bash reads it, the double-quoted word (`<<$"EOF"` is `EOF`), which is the
///   reading the word reader takes for `$"…"` everywhere else. zsh 5.9 reads `$EOF` there, so
///   the two shells end the body at different lines, and [`Header::shells_disagree`] says so:
///   the layout then drops nothing and reads the body as a shell would run it, and A5 reads it
///   as commands.
/// - a backslash-newline outside quotes is removed before the header is read, so it is neither
///   a blank nor quoting: `<<E\<newline>OF` is the unquoted delimiter `EOF`, whose body expands.
pub fn heredoc_header(line: &Line, i: usize) -> Option<Header> {
    let chars = &line.chars;
    let n = chars.len();
    let mut j = i + 2;
    let mut dash = false;
    if j < n && chars[j] == '-' {
        dash = true;
        j += 1;
    }
    // `" \t"`, not `\s`: bash's header skips a blank but not a newline.
    let continuation = |j: usize| j + 1 < n && chars[j] == '\\' && chars[j + 1] == '\n';
    while j < n && (chars[j] == ' ' || chars[j] == '\t' || continuation(j)) {
        j += if continuation(j) { 2 } else { 1 };
    }
    let mut parts = String::new();
    let mut quoted = false;
    let mut shells_disagree = false;
    while j < n {
        let c = chars[j];
        let next = chars.get(j + 1).copied();
        if " \t\n;&|<>()".contains(c) {
            break;
        }
        if continuation(j) {
            j += 2;
        } else if c == '\\' {
            quoted = true;
            if let Some(x) = next {
                parts.push(x);
            }
            j += 2;
        } else if c == '$' && next == Some('$') {
            // The PID, never the `$` of a quotation after it: `$$'x'` is `$$` and then `'x'`.
            parts.push_str("$$");
            j += 2;
        } else if c == '$' && next == Some('\'') {
            // ANSI-C: the quotation ends at the first `'` no backslash escapes.
            let mut k = j + 2;
            let mut raw: Vec<char> = Vec::new();
            while k < n && chars[k] != '\'' {
                if chars[k] == '\\' && k + 1 < n {
                    raw.extend([chars[k], chars[k + 1]]);
                    k += 2;
                } else {
                    raw.push(chars[k]);
                    k += 1;
                }
            }
            if k >= n {
                return None; // unterminated: a shell errors, nothing runs
            }
            quoted = true;
            parts.push_str(&shellseg::ansi_c_decode(&raw));
            j = k + 1;
        } else if c == '$' && next == Some('"') {
            shells_disagree = true;
            j += 1; // `$"…"` is the double-quoted word; the `"` is read next
        } else if c == '\'' {
            let Some(off) = chars[j + 1..].iter().position(|&x| x == '\'') else {
                return None; // unterminated: a shell errors, nothing runs
            };
            let end = j + 1 + off;
            quoted = true;
            parts.extend(chars[j + 1..end].iter());
            j = end + 1;
        } else if c == '"' {
            let mut k = j + 1;
            loop {
                let Some(&x) = chars.get(k) else {
                    return None; // unterminated: a shell errors, nothing runs
                };
                if x == '"' {
                    break;
                }
                match (x, chars.get(k + 1).copied()) {
                    ('\\', Some('\n')) => k += 2,
                    ('\\', Some(e)) if "$`\"\\".contains(e) => {
                        parts.push(e);
                        k += 2;
                    }
                    _ => {
                        parts.push(x);
                        k += 1;
                    }
                }
            }
            quoted = true;
            j = k + 1;
        } else {
            parts.push(c);
            j += 1;
        }
    }
    // `delim or quoted` — an EMPTY delimiter is a real heredoc when it was written as one.
    // `<<""` and `<<''` name the empty string, do not expand, and end at the first empty line;
    // `<<` with no word after it is not a heredoc at all. Testing the delimiter alone collapsed
    // the two, and charter refused `<<""` bodies that bash does not expand.
    if parts.is_empty() && !quoted {
        return None;
    }
    Some(Header {
        delim: parts,
        expands: !quoted,
        dash,
        end: j,
        shells_disagree,
    })
}

/// Where the body of a heredoc ends in `lines`, the lines that follow its command line:
/// `(how many of them are body, whether the terminator line comes next)`.
///
/// Bash ends a body on a line EQUAL to the delimiter — a `<<-` ignoring only leading TABS. Not
/// `.strip()`: a lenient match ends a KEPT (shell) body early, and the real read that spills
/// past it is then read as the next heredoc's body and, if that one is a reader's, stripped
/// away. In a body that EXPANDS, a trailing backslash splices the next line onto it, so that
/// line cannot be the terminator and bash reads on past it (GNU bash 3.2.57 and zsh 5.9 both
/// run the `$(…)` in `cat <<EOF`, `a\`, `EOF`, `$(…)`, `EOF`).
///
/// The one body walk: [`heredoc_layout`] (the leak guard and A7) and
/// [`crate::livesub::heredoc_bodies`] (A5 and A6) both end a body here.
pub fn body_extent(lines: &[&str], delim: &str, expands: bool, dash: bool) -> (usize, bool) {
    let mut spliced = false;
    for (k, line) in lines.iter().enumerate() {
        let compared = if dash {
            line.trim_start_matches('\t')
        } else {
            line
        };
        if !spliced && compared == delim {
            return (k, true);
        }
        spliced = expands && ends_in_line_continuation(line);
    }
    (lines.len(), false)
}

/// `cmd` with each unquoted ANSI-C `$'…'` rewritten as an ordinary shell-quoted token —
/// `_desugar_ansi_c`, decoded by [`shellseg::ansi_c_decode`].
///
/// `shlex` does not know `$'…'`: it reads `$` as a bare character and the following `'…'` as a
/// plain single-quoted string, so `$'\''` (one apostrophe) leaves a dangling quote that swallows
/// the rest of the line, and a `| sh -s` after it stopped being a token. The executor behind it
/// was never seen and a heredoc body it runs was stripped as data. [`shellseg::lex`] now reads
/// `$'…'` itself, so this rewrite and a plain lex give the same tokens; it stays because the
/// recorded corpus pins its output (`dac`).
///
/// Only an UNQUOTED `$'` is a real ANSI-C string; inside `'…'` or `"…"` the `$` is literal, so
/// the quote map gates it. The terminator is the first unescaped `'`, exactly as bash ends it.
pub fn desugar_ansi_c(line: &Line) -> String {
    let chars = &line.chars;
    let n = chars.len();
    if !chars.windows(2).any(|w| w == ['$', '\'']) {
        return line.text();
    }
    let mut out = String::new();
    let mut i = 0usize;
    while i < n {
        if line.starts_with(i, "$'") && !line.quoted(i) {
            let mut j = i + 2;
            let mut raw: Vec<char> = Vec::new();
            while j < n && chars[j] != '\'' {
                if chars[j] == '\\' && j + 1 < n {
                    raw.extend([chars[j], chars[j + 1]]);
                    j += 2;
                } else {
                    raw.push(chars[j]);
                    j += 1;
                }
            }
            // The one decoder the command-line reader uses too, so `$'\x67it'` is `git` here as
            // it is to every guard.
            let buf = shellseg::ansi_c_decode(&raw);
            out.push_str(&shellseg::shell_quote(&buf));
            i = if j < n { j + 1 } else { n }; // step over the closing quote
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// `_LIVE_BACKTICK_RE.search`: whether any backtick could open or close a substitution.
///
/// The Python is `(?<!\\)(?:\\\\)*` + a backtick, and that is exactly "a backtick behind an EVEN
/// run of backslashes": with an odd run, every start the pairs could take is preceded by a
/// backslash the lookbehind refuses. Written as the parity test it is, because the regex form
/// invites the reading "a backtick not preceded by a backslash", which is a different and wrong
/// predicate.
fn has_live_backtick(chars: &[char]) -> bool {
    for (i, &c) in chars.iter().enumerate() {
        if c != '`' {
            continue;
        }
        let mut run = 0usize;
        while run < i && chars[i - 1 - run] == '\\' {
            run += 1;
        }
        if run.is_multiple_of(2) {
            return true;
        }
    }
    false
}

/// Whether `toks` contain a compound command whose body runs an executor in command position —
/// `_compound_holds_executor`.
///
/// The pipeline splitter treats the `;`/`&&` inside `while … do … done` as pipeline boundaries,
/// so an `eval`/`sh` in the body is attributed to a different pipeline than the heredoc it
/// consumes. Rather than model the compound's structure, [`line_pipelines`] bails to `None` when
/// this is true — the safe answer that keeps every body visible. The executor must stand where a
/// command begins, so `grep eval f` does not count.
pub fn compound_holds_executor(toks: &[Tok]) -> bool {
    let mut seen_compound = false;
    let mut prev = "\n".to_string(); // start of line is a command position
    for t in toks {
        if t.bare
            && COMPOUND_WORDS.contains(&t.text.as_str())
            && COMMAND_POSITION_AFTER.contains(&prev.as_str())
        {
            seen_compound = true;
        } else if seen_compound
            && t.bare
            && COMMAND_POSITION_AFTER.contains(&prev.as_str())
            && is_executor(&t.text)
        {
            return true;
        }
        if t.bare {
            prev = t.text.clone();
        }
    }
    false
}

/// One pipeline of a line: the commands in it, whether anything in it runs text, and how many
/// `<<` each command opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline {
    /// `argvs[k]` is command `k`'s argv, program first ([`shellwrap::split_env`]).
    pub argvs: Vec<Vec<String>>,
    /// Per PIPELINE, not per command: a body a reader opens is run all the same if any command
    /// downstream of it in the pipe is a shell.
    pub executor: bool,
    /// `argvs[k]` opened `hcounts[k]` of the `<<` on this line.
    pub hcounts: Vec<usize>,
}

/// `line` as its pipelines, or `None` when the line cannot be attributed safely —
/// `_line_pipelines`.
///
/// Split on the control operators that separate pipelines (`;`, `&&`, `||`, `&`, `;;`) but NOT on
/// `|`, because a pipeline shares one execution fate: in `zsh`'s `MULTIOS` even a body a single
/// pipe appears to discard (`cat <<A | bash <<B`) reaches both.
///
/// `None` rather than a guess for four shapes, and each is a measured defect:
///
/// - a **group, subshell or substitution** (`{ … }`, `( … )`, `$( … )`), whose output can be
///   re-piped (`{ cat <<X; } | bash`);
/// - a **compound command** — `while`/`until`/`for`/`select`, `if`/`case` — whose own `;` is not
///   a pipeline boundary, so `cat <<'EOF' | while read l; do eval "$l"; done` attributed the
///   `eval` to a pipeline the heredoc was not in;
/// - a **live backtick**, which needs saying separately because the lexer does not treat it as
///   punctuation: it folds `` x=`bash `` into one word, so the answer that comes back is WRONG
///   rather than absent and a caller whose fallback fires only on `None` never fires;
/// - **unbalanced quoting**, which the lexer refuses outright.
///
/// An ESCAPED backtick is the one exception, because it starts no substitution in any quoting
/// context — a PR title such as ``--title "keeps its \`~/\`"`` used to leave its line
/// unattributable.
pub fn line_pipelines(line: &Line) -> Option<Vec<Pipeline>> {
    if has_live_backtick(&line.chars) {
        return None;
    }
    let toks = shellseg::split_punctuation(shellseg::lex(&desugar_ansi_c(line)).ok()?);
    if compound_holds_executor(&toks) {
        // A loop or conditional whose body runs the piped heredoc: keep every body visible
        // rather than split it away at the compound's own separators.
        return None;
    }
    let mut pipelines: Vec<Vec<Vec<String>>> = Vec::new();
    let mut current: Vec<Vec<String>> = vec![Vec::new()];
    for t in toks {
        if !t.bare {
            current.last_mut().expect("never empty").push(t.text);
        } else if t.is_grouping() {
            return None; // a group/subshell/substitution — do not guess
        } else if t.text == "|" || t.text == "|&" {
            current.push(Vec::new());
        } else if t.is_control_op() {
            pipelines.push(std::mem::replace(&mut current, vec![Vec::new()]));
        } else {
            current.last_mut().expect("never empty").push(t.text);
        }
    }
    pipelines.push(current);
    Some(
        pipelines
            .into_iter()
            .map(|cmds| {
                let split: Vec<(String, Vec<String>, Vec<String>)> = cmds
                    .iter()
                    .map(|c| shellwrap::split_env(c.as_slice()))
                    .collect();
                // The count is over token TEXT, quoted or not, because it is compared against
                // the openers' count and a disagreement is what makes the plan unknown.
                let hcounts = cmds
                    .iter()
                    .map(|c| c.iter().filter(|tk| tk.as_str() == "<<").count())
                    .collect();
                let executor = cmds.iter().zip(split.iter()).any(|(c, (prog, _, _))| {
                    !c.is_empty() && (is_executor(&c[0]) || is_executor(prog))
                });
                Pipeline {
                    argvs: split.into_iter().map(|(_, _, argv)| argv).collect(),
                    executor,
                    hcounts,
                }
            })
            .collect(),
    )
}

/// The words of the command that opens the `<<` at `start` — its program first — or `None` when
/// the source does not settle what that program is. `_heredoc_opener_words`.
///
/// Read off the SOURCE rather than off the token stream, because this is only reached when the
/// line is one [`line_pipelines`] would not attribute, which is exactly when the token stream is
/// not a reliable map of it.
///
/// The command begins after the last thing that STARTS one: an unquoted control operator, or a
/// group/substitution that is still OPEN at the `<<`. A substitution that has already CLOSED is
/// an argument — a redirect target (`cat > "$(date +%F).md" <<'EOF'`), a flag value — and so is a
/// `${…}`; cutting at either lost a program that was plainly nameable.
///
/// `None` when the program word is not a name: `${RUNNER} <<'EOF'` is decided at runtime, and in
/// `$(which bash) <<'EOF'` the substitution IS the program. Both make [`heredoc_could_run`]
/// search the body.
pub fn heredoc_opener_words(line: &Line, start: usize) -> Option<Vec<String>> {
    let chars = &line.chars;
    let mut stack: Vec<usize> = Vec::new(); // command starts saved across open groups
    let mut btick = false; // inside a backtick substitution?
    let mut cmd = 0usize;
    let mut i = 0usize;
    let n = start.min(chars.len());
    while i < n {
        if chars[i] == '\\' {
            i += 2; // an escaped character is never a delimiter
            continue;
        }
        // A backtick is asked about BEFORE the quoted-skip, because the quote map marks the
        // OPENING backtick of a pair inside `"…"` as quoted and its closing partner as not —
        // correct for that map, and half a pair here. Seeing one half toggled `btick` the wrong
        // way and left `cmd` past the closing backtick.
        if chars[i] != '`' && line.quoted(i) {
            i += 1;
            continue;
        }
        // `${…}` needs no case of its own: `{` saves the command start and `}` restores it, so a
        // parameter expansion leaves it exactly where it was.
        if line.starts_with(i, "$(") {
            stack.push(cmd);
            cmd = i + 2;
            i += 2;
            continue;
        }
        let c = chars[i];
        if c == '`' {
            // A backtick substitution is a group like `$( … )`, and needs saying separately
            // because one character both opens and closes it. Without this, a separator INSIDE
            // the backticks moved the command start past the real program.
            if btick {
                cmd = stack.pop().unwrap_or(0);
            } else {
                stack.push(cmd);
                cmd = i + 1;
            }
            btick = !btick;
        } else if c == '(' || c == '{' {
            stack.push(cmd);
            cmd = i + 1;
        } else if (c == ')' || c == '}') && !stack.is_empty() {
            cmd = stack.pop().expect("just checked"); // the group was an argument
        } else {
            match COMMAND_STARTS
                .iter()
                .find(|op| line.starts_with(i, op))
                .copied()
            {
                Some(op) => {
                    cmd = i + op.chars().count();
                    i += op.chars().count();
                }
                None => i += 1,
            }
            continue;
        }
        i += 1;
    }
    let words = shellseg::py_split(&line.slice(cmd, n));
    if words.is_empty() { None } else { Some(words) }
}

/// The program `words` names, lowercased and without its directory — or `None` when the source
/// does not name one at all. `_opener_program`.
///
/// `None` is the honest answer for `"$PROG" <<'EOF'` or `${RUNNER} <<'EOF'`: the program is
/// decided at runtime, and guessing either way would be a guess.
pub fn opener_program(words: Option<&[String]>) -> Option<String> {
    let words = words?;
    if words.is_empty() {
        return None;
    }
    let (prog, _, _) = shellwrap::split_env(words);
    let prog = base_lower(&prog);
    plain_name_re().is_match(&prog).then_some(prog)
}

/// Whether any word of `line` names a program that runs text it is given — `_line_runs_text`.
///
/// Read off the source, word by word, because this is reached only for a line the lexer could not
/// take apart — `git commit -m "$(cat <<'EOF'` leaves a quote open until after the body, so the
/// one shape that most needs an answer is the one that raises.
///
/// Over-matching is the safe direction and is deliberate: an argument that happens to read `bash`
/// costs one over-refusal on a line nothing could attribute anyway, while a missed executor would
/// call a body data that a shell runs. The one over-match worth removing is the heredoc's own
/// DELIMITER — `cat <<'PYTHON'` opens no interpreter — so the headers are cut out first, as
/// [`heredoc_openers`] reads them. A `<<` inside quotes is no header and is not cut: in
/// `echo "<<PYTHON"` the word `PYTHON` is read, which errs toward the over-match.
pub fn line_runs_text(line: &str) -> bool {
    // Each heredoc header, `<<` to the end of its delimiter word, becomes one blank.
    let line = Line::of(line);
    let mut blanked = String::new();
    let mut i = 0usize;
    for m in heredoc_openers(&line) {
        blanked.push_str(&line.slice(i, m.start));
        blanked.push(' ');
        i = m.header.end;
    }
    blanked.push_str(&line.slice(i, line.len()));
    for word in blanked.split(|c: char| is_python_space(c) || ";&|()<>{}\"'`".contains(c)) {
        let word = word.trim_matches(|c| c == '$' || c == '\\');
        // The oracle asks `_is_executor(word) or _is_executor(basename(word).lower())`, and the
        // second is INERT: `_is_executor` basenames and lowercases its argument itself, and both
        // are idempotent. Kept as one call and written down as inert rather than transcribed as
        // two, because a reader checking the port against the Python will look for the pair.
        if !word.is_empty() && is_executor(word) {
            return true;
        }
    }
    false
}

/// The text of the pipeline containing the `<<` at `start` — `_pipeline_slice`.
///
/// The unit a shell actually uses. `cat <<'A' | bash` feeds the body to `bash`, so the body is a
/// script; `cat <<'A'; bash` and `cat <<'A' && bash` do not, so it is data. Asking the whole LINE
/// over-refused 22 good-faith commands whose shell sat after a `;`; asking the opener's word
/// alone let `cat <<'A' | bash` through.
pub fn pipeline_slice(line: &Line, start: usize) -> String {
    let n = line.len();
    let mut lo = 0usize;
    let mut hi = n;
    let mut i = 0usize;
    while i < n {
        if line.quoted(i) {
            i += 1;
            continue;
        }
        if let Some(br) = PIPELINE_BREAKS
            .iter()
            .find(|br| line.starts_with(i, br))
            .copied()
        {
            let len = br.chars().count();
            if i < start {
                lo = i + len;
                // Python advances by `len - 1` inside the `for`, then by 1 after it.
                i += len;
            } else {
                hi = i;
                return line.slice(lo, hi);
            }
            continue;
        }
        i += 1;
    }
    line.slice(lo, hi)
}

/// Indices into [`heredoc_openers`] whose body sits in a `$( … )` that holds TWO or more
/// heredocs, one of which an executor opened — `_crowded_substitutions`.
///
/// Bash's own ordering there is not what charter's attribution assumes: in
/// `x=$( cat <<'A' > n.md; bash <<'B' )` bash hands the FIRST body to `bash` and leaves `n.md`
/// empty, so the handoff in the "cat" body runs. Getting that ordering right is not work this
/// guard should carry, so the conservative answer is taken for the whole substitution. The cost is
/// an over-refusal only where a reader heredoc and a shell heredoc share one `$( … )`, which
/// nobody writes by accident.
pub fn crowded_substitutions(line: &Line) -> HashSet<usize> {
    let mut forced = HashSet::new();
    let openers = heredoc_openers(line);
    let n = line.len();
    let mut depth = 0usize;
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut open_at = 0usize;
    let mut i = 0usize;
    while i < n {
        if !line.quoted(i) {
            if line.starts_with(i, "$(") {
                if depth == 0 {
                    open_at = i;
                }
                depth += 1;
                i += 2;
                continue;
            }
            if line.chars[i] == ')' && depth > 0 {
                depth -= 1;
                if depth == 0 {
                    spans.push((open_at, i));
                }
            }
        }
        i += 1;
    }
    for (lo, hi) in spans {
        let inside: Vec<usize> = openers
            .iter()
            .enumerate()
            .filter(|(_, m)| lo < m.start && m.start < hi)
            .map(|(k, _)| k)
            .collect();
        if inside.len() < 2 {
            continue;
        }
        if inside.iter().any(|&k| {
            let words = heredoc_opener_words(line, openers[k].start);
            let prog = opener_program(words.as_deref()).unwrap_or_default();
            is_executor(&prog)
        }) {
            forced.extend(inside);
        }
    }
    forced
}

/// Whether a shell RUNS the body of the `<<` at `start`, for a line whose whole-line plan
/// [`heredoc_strip_plan`] returned `None` for. **Errs toward yes.** `_heredoc_could_run`.
///
/// A7 and the leak guard share one plan, and an unknown plan has to fail the same way for both or
/// the pair contradicts itself. The leak guard's unknown is "keep every body visible", which still
/// denies; A7's used to be "drop every body", which is the opposite, and a canonical handoff
/// inside `bash <<'EOF'` then ran with no prompt in every shape that loses the plan.
///
/// Flipping that default to "everything could run" is NOT the answer either: measured, it costs
/// two over-refusals, both `git commit -m "$(cat <<'EOF'…)"`. So the fallback is per heredoc, and
/// asks the only question that decides it — does anything RUN this body? Three answers are
/// possible:
///
/// 1. a **shell or interpreter**, including one reached through `env`/`nohup` and `ssh`, where
///    the remote shell runs the body — **searches** the body;
/// 2. a program that **cannot be resolved to a name** — `${RUNNER} <<'EOF'`, `$(which bash)
///    <<'EOF'` — **searches** it too. A name charter does not have is not a name charter may
///    assume is harmless;
/// 3. **any other program that can be named** — `git`, `tee`, `mail`, `wc`, every reader — hands
///    the body on as DATA.
///
/// And whatever the opener, the body is searched when an executor stands **downstream of it in
/// the same PIPELINE**. An UNQUOTED body is searched whatever opened it: it expands before the
/// program sees it, so a `$( … )` in it runs.
pub fn heredoc_could_run(line: &Line, start: usize) -> bool {
    let words = heredoc_opener_words(line, start);
    let Some(prog) = opener_program(words.as_deref()) else {
        return true;
    };
    if is_executor(&prog) || REMOTE_SHELLS.contains(&prog.as_str()) {
        return true;
    }
    if line_runs_text(&pipeline_slice(line, start)) {
        return true;
    }
    match heredoc_header(line, start) {
        None => true,
        Some(h) => h.expands, // unquoted: a `$( … )` in the body RUNS
    }
}

/// `toks` as `(segment, the control operator in front of it)` pairs — `_segments_of`.
///
/// Split wherever the shell would interpret a control operator, `""` in front of the first.
/// Shared surface: stage 6 (A7) reads it too, which is why it is `pub` and why the differential
/// compares it.
pub fn segments_of(toks: &[Tok]) -> Vec<(Vec<Tok>, String)> {
    let mut segments = Vec::new();
    let mut seg: Vec<Tok> = Vec::new();
    let mut before = String::new();
    for tok in toks {
        if tok.is_control_op() {
            segments.push((std::mem::take(&mut seg), std::mem::take(&mut before)));
            before = tok.text.clone();
        } else {
            seg.push(tok.clone());
        }
    }
    segments.push((seg, before));
    segments
}

/// The heredocs on `line`, by position in [`heredoc_openers`]'s order, whose opener command is one
/// bare `charter handoff` — the ones whose body is a BRIEF. `_brief_heredocs`.
///
/// A brief is charter's stdin: the text it sends a new chat as a first message, which no shell
/// ever runs. Read as commands, every line of it reaches the leak guard, and a brief that names a
/// vault path in prose — or carries a single apostrophe — is refused as a read of it.
///
/// **Two bare words, and that command alone.** `charter handoff b && bash <<'EOF'` opens
/// `bash`'s heredoc, not a brief, and `python3 -m charter handoff` is a spelling A7 refuses
/// outright. Both fall through to "not a brief", which only ever shows the guard more text.
///
/// **In any case.** `CHARTER handoff` runs charter on a filesystem that folds case, and A7
/// reads it as the handoff it is and refuses its spelling; `charter HANDOFF` names a
/// subcommand charter does not have, so charter exits before it reads stdin. Neither body is
/// ever run, so neither is a command for the leak guard to read.
pub fn brief_heredocs(line: &Line) -> HashSet<usize> {
    let headers = heredoc_openers(line);
    if headers.is_empty() {
        return HashSet::new();
    }
    let Ok(toks) = shellseg::lex(&line.text()) else {
        return HashSet::new();
    };
    let toks = shellseg::split_punctuation(toks);
    let opens = |ts: &[Tok]| ts.iter().filter(|t| t.is_op(&["<<"])).count();
    // The bail is `heredoc_strip_plan`'s, for its reason: an index space the lexer and the
    // header reader disagree about is one no caller can act on.
    if !lexer_agrees(&toks, &headers) {
        return HashSet::new();
    }
    let mut briefs = HashSet::new();
    let mut k = 0usize;
    for (seg, _before) in segments_of(&toks) {
        let opened = opens(&seg);
        if opened > 0
            && seg.len() >= 2
            && seg[0].bare
            && seg[1].bare
            && seg[0].text.eq_ignore_ascii_case("charter")
            && seg[1].text.eq_ignore_ascii_case("handoff")
        {
            briefs.extend(k..k + opened);
        }
        k += opened;
    }
    briefs
}

/// Whether `argv` is a `git commit` that takes its message from stdin and opens no editor on it —
/// so a heredoc it opens is a MESSAGE, which git stores and never runs. `_commit_message_on_stdin`.
///
/// Without this, `git commit -F - <<'MSG'` had its body read as commands, because `git` is not a
/// reader: a message holding one apostrophe beside a vault path stopped the call lexing and the
/// raw scan refused it. The same body fed to `cat` passed.
///
/// **Narrow, and every miss keeps the body visible.** The spellings read are `-F -`, `-F-`,
/// `--file=-` and `--file -` after a literal `commit`, with git's globals before it skipped. Not
/// read: another subcommand that takes `-F -`, an alias, a short cluster (`-aF -`), an
/// abbreviation, a redirection between `git` and `commit`.
///
/// **An editor is the one way git runs a message**, so any spelling of `--edit` refuses the
/// answer: git writes the message to `COMMIT_EDITMSG` and hands that file to the editor, and with
/// `core.editor=sh` the message runs. **Redirections come out first, targets with them**, because
/// a target is not an option: `git commit > -F- <<'MSG'` writes to a file named `-F-`.
pub fn commit_message_on_stdin(argv: &[String]) -> bool {
    if argv.is_empty() || base_lower(&argv[0]) != "git" {
        return false;
    }
    let words = without_redirections(argv);
    let (_globals, rest) = shellwrap::git_globals(&words);
    if rest.first().map(String::as_str) != Some("commit") {
        return false;
    }
    let opts = &rest[1..];
    let mut stdin = false;
    for (k, w) in opts.iter().enumerate() {
        if w == "--" {
            break; // pathspecs from here: `-F -` is two paths
        }
        if w == "-F-"
            || w == "--file=-"
            || ((w == "-F" || w == "--file") && opts.get(k + 1).map(String::as_str) == Some("-"))
        {
            stdin = true;
        } else if
        // Any spelling of `--edit`, including the abbreviations git accepts (`--e`, `--edi`)…
        (w.chars().count() >= 3 && "--edit".starts_with(w.as_str()))
            // …and a short cluster holding `e` (`-ae`). One arm because they answer the same
            // way and clippy refuses two blocks that do; two conditions because they are two
            // separate spellings and the oracle keeps them apart. A cluster whose `e` is a
            // VALUE (`-mfixe`) is refused along with them, which is a missed allow.
            || (w.starts_with('-') && !w.starts_with("--") && w.contains('e'))
        {
            return false;
        }
    }
    stdin
}

/// Whether `argv` is a `gh pr|issue create|comment` that takes its body from stdin, so a heredoc
/// it opens is a BODY, which gh posts and never runs. `_gh_body_on_stdin`.
///
/// The same fact as [`commit_message_on_stdin`], for the other message an agent writes in the same
/// call. `gh pr create --body-file - <<'EOF' | tail -1` kept its body visible because `gh` is not
/// a reader; one apostrophe in the prose stopped the call lexing, every word of the body became an
/// operand of `tail -1`, and the call was refused as a vault read.
///
/// An alias cannot shadow `pr` or `issue` — measured on gh 2.83.2, `gh alias set pr …` fails with
/// "already a gh command". Measured there too: gh opens no editor when the body is on stdin, but
/// `-e`, `--editor[=…]` and a short cluster holding `e` refuse the answer anyway, which costs a
/// missed allow.
pub fn gh_body_on_stdin(argv: &[String]) -> bool {
    if argv.is_empty() || base_lower(&argv[0]) != "gh" {
        return false;
    }
    let words = without_redirections(&argv[1..]);
    if words.len() < 2
        || !GH_BODY_COMMANDS
            .iter()
            .any(|(a, b)| words[0] == *a && words[1] == *b)
    {
        return false;
    }
    let opts = &words[2..];
    let mut stdin = false;
    for (k, w) in opts.iter().enumerate() {
        if w == "--" {
            break; // positionals from here: `-F -` is two args
        }
        if w == "-F-"
            || w == "--body-file=-"
            || ((w == "-F" || w == "--body-file")
                && opts.get(k + 1).map(String::as_str) == Some("-"))
        {
            stdin = true;
        } else if
        // `--editor`, with or without a value…
        (w == "--editor" || w.starts_with("--editor="))
            // …and a short cluster holding `e`. One arm for the reason git's is; gh opens no
            // editor when the body is on stdin anyway, so all of this costs a missed allow.
            || (w.starts_with('-') && !w.starts_with("--") && w.contains('e'))
        {
            return false;
        }
    }
    stdin
}

/// `argv` with every redirection token AND the token after it removed.
///
/// A target is not an option: `git commit > -F- <<'MSG'` writes to a file named `-F-`, reads no
/// message from stdin, and opens the editor with the heredoc as ITS stdin.
fn without_redirections(argv: &[String]) -> Vec<String> {
    let mut words = Vec::new();
    let mut skip = false;
    for w in argv {
        if skip {
            skip = false;
        } else if shellwrap::is_redirect_token(w) {
            skip = true;
        } else {
            words.push(w.clone());
        }
    }
    words
}

/// One heredoc's entry in [`heredoc_strip_plan`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanEntry {
    /// The heredoc's header, as [`heredoc_openers`] read it: where its body ends and whether it
    /// expands.
    pub header: Header,
    /// The verdict: this body is stdin DATA and may be dropped.
    pub drop: bool,
    /// Whether a program in this body's pipeline runs text — what A7 asks before it reads a body
    /// as commands.
    pub executor: bool,
}

/// Whether the lexer's bare `<<` tokens (`toks`, the line split by [`shellseg::split_punctuation`])
/// stand exactly where `openers` are — the same
/// heredocs, not merely as many of them.
fn lexer_agrees(toks: &[Tok], openers: &[Opener]) -> bool {
    let at: Vec<isize> = toks
        .iter()
        .filter(|t| t.is_op(&["<<"]))
        .map(|t| t.start)
        .collect();
    at.len() == openers.len() && at.iter().zip(openers).all(|(&a, m)| a == m.start as isize)
}

/// The heredocs opened on `line`, in the order their bodies follow — or `None` when nothing on
/// the line may be stripped. `_heredoc_strip_plan`.
///
/// A body is stdin DATA (drop) only when a **quoted** heredoc feeds a **reader** whose **pipeline
/// runs no executor**. Each clause earns its place:
///
/// - *quoted* — an unquoted `<<EOF` is expanded before the reader sees it, so a `$( … )` in the
///   body RUNS. A delimiter with any quoting in it (`<<'EOF'`, `<<"EOF"`, `<<\EOF`, `<<EO'F'`)
///   is inert, which is [`Header::expands`].
/// - *reader* — the program that OPENS the `<<` (its segment's, via [`shellwrap::split_env`]), so
///   `env cat <<'X'` is a reader behind a wrapper. A `git commit` taking its message from stdin
///   counts as one, and so does a `gh pr|issue create|comment` taking its body from stdin.
/// - *no executor in the pipeline* — the defect this replaces: the old pre-pass asked only
///   whether the LINE began with a reader, so `cat x && bash <<'EOF'`, `… | bash`, `… || bash`,
///   `… & bash` and `cat x; bash <<'EOF'` each dropped a body a shell ran.
///
/// The lexer's `<<` tokens must stand exactly where [`heredoc_openers`] found openers, or the
/// answer is `None`: a `<<` in a comment or in `$(( … ))`, which the header reader counts and the
/// lexer does not, makes a line this pre-pass cannot take apart, so it strips nothing and the
/// bodies stay visible. Positions and not only a count, because with a count alone one opener
/// only the lexer saw and one only the header reader saw paired each body with the wrong
/// program.
///
/// A **brief** is data by the other route ([`brief_heredocs`]): charter reads that body as stdin
/// rather than running it.
pub fn heredoc_strip_plan(line: &Line) -> Option<Vec<PlanEntry>> {
    let headers = heredoc_openers(line);
    if headers.is_empty() {
        return Some(Vec::new());
    }
    let pipelines = line_pipelines(line)?;
    let counted: usize = pipelines
        .iter()
        .flat_map(|p| p.hcounts.iter())
        .copied()
        .sum();
    let toks = shellseg::split_punctuation(shellseg::lex(&line.text()).ok()?);
    if counted != headers.len() || !lexer_agrees(&toks, &headers) {
        return None;
    }
    let briefs = brief_heredocs(line);
    let mut plan = Vec::new();
    let mut k = 0usize;
    for p in &pipelines {
        for (argv, hc) in p.argvs.iter().zip(p.hcounts.iter()) {
            let prog = argv.first().cloned().unwrap_or_default();
            // A commit message on stdin is a reader's body by another name: git stores it and
            // runs none of it, so it earns the same two clauses and nothing more. A gh PR or
            // issue body on stdin is the same, posted and never run.
            let reader = READERS.contains(&base_lower(&prog).as_str())
                || commit_message_on_stdin(argv)
                || gh_body_on_stdin(argv);
            for _ in 0..*hc {
                // In range because the count guard above proved it: the innermost loop runs
                // `sum(hcounts)` times in total, and that sum is `headers.len()`. Spelled as a
                // named expectation rather than an index, so a future edit that breaks the
                // invariant says which invariant it broke.
                let m = headers
                    .get(k)
                    .expect("the count guard above makes k < headers.len()");
                // A brief is data for the same reason a reader's body is, and by a different
                // route: nobody runs it, because it is charter's stdin.
                let data = (reader && !m.header.expands && !p.executor) || briefs.contains(&k);
                plan.push(PlanEntry {
                    header: m.header.clone(),
                    drop: data,
                    executor: p.executor,
                });
                k += 1;
            }
        }
    }
    Some(plan)
}

/// A bare trailing backslash — `_ends_in_line_continuation`.
///
/// bash splices this line with the next BEFORE tokenizing, so `cat <<'EOF' \` then `| bash` is the
/// one command `cat <<'EOF' | bash`, and the heredoc body follows the spliced whole. An even run
/// of trailing backslashes is a literal `\`, not a continuation.
///
/// This is the RAW parity test. A caller reading a COMMAND line must also ask [`comment_index`],
/// because bash does not splice a comment; a caller reading a heredoc BODY line must not, because
/// a `#` in a body is ordinary text.
pub fn ends_in_line_continuation(line: &str) -> bool {
    let run = line.chars().rev().take_while(|&c| c == '\\').count();
    line.ends_with('\\') && !run.is_multiple_of(2)
}

/// The offset of the `#` that begins an unquoted comment on `line`, or `-1` — `_comment_index`.
///
/// Read off the quote map so a `#` inside quotes (`grep '#'`) or in a substitution is not one, and
/// required at a WORD START so `a#b` is not. A comment runs to the end of the line, so a trailing
/// backslash after this index is inside the comment and splices nothing — the fact
/// [`heredoc_layout`] needs to stop folding a comment line into the next.
pub fn comment_index(line: &Line) -> isize {
    for (i, &c) in line.chars.iter().enumerate() {
        if c == '#'
            && !line.quoted(i)
            && (i == 0 || (BEFORE_COMMENT.contains(line.chars[i - 1]) && !line.quoted(i - 1)))
        {
            return i as isize;
        }
    }
    -1
}

/// A trailing bare `|` or `|&` — `_pipeline_continues`.
///
/// It pipes this command's output into the next command LINE, which, unlike a backslash splice,
/// sits AFTER the heredoc bodies this line opened. Measured on bash, zsh and dash: `cat <<'EOF' |`
/// then a body then `EOF` then `bash` feeds that body to `bash`, which runs it. A per-physical-line
/// plan cannot see that `bash`, so the pipeline is reassembled before attribution.
///
/// `&&`, `||`, `&`, `;` are NOT this: measured, the heredoc stays with its own segment's program
/// and is not piped downstream, so a per-line plan already attributes them.
pub fn pipeline_continues(line: &str) -> bool {
    let Ok(toks) = shellseg::lex(line) else {
        return false;
    };
    let toks = shellseg::split_punctuation(toks);
    toks.last()
        .is_some_and(|t| t.bare && (t.text == "|" || t.text == "|&"))
}

/// The `(open, close)` character spans of every `$( … )` and backtick substitution that opens
/// AND closes on `line`.
///
/// A heredoc opened inside one of those has no body inside it, and the shells disagree about
/// where its body is then. Measured with `x=$( cat <<'EOF' )`, a line, then `EOF`: GNU bash
/// 5.2.15 and 5.3 read the line as the heredoc's body, while GNU bash 3.2.57 and zsh 5.9 run it
/// as a command; with backticks all four run it. So such a body is read both ways: never
/// dropped, and searched as a shell would run it. A subshell `( … )` or group `{ …; }` is not
/// this: every one of those shells reads the body after the line.
///
/// Read with its own small walk, because a backtick's pairing is what matters here and the
/// quote map marks the two backticks of a pair differently inside `"…"`.
fn closed_substitutions(line: &Line) -> Vec<(usize, usize)> {
    #[derive(PartialEq)]
    enum Frame {
        Sub,
        Tick,
        Paren,
        DoubleQuoted,
    }
    let chars = &line.chars;
    let n = chars.len();
    let mut stack: Vec<(Frame, usize)> = Vec::new();
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < n {
        let c = chars[i];
        let in_dq = stack.last().is_some_and(|(f, _)| *f == Frame::DoubleQuoted);
        if c == '\\' {
            i += 2;
            continue;
        }
        if line.starts_with(i, "$(") {
            stack.push((Frame::Sub, i));
            i += 2;
            continue;
        }
        if !in_dq && line.starts_with(i, "$'") {
            // ANSI-C: ends at the first `'` no backslash escapes. This `$` is unescaped, since
            // an escaped one was stepped over with its backslash above.
            let mut k = i + 2;
            while k < n && chars[k] != '\'' {
                k += if chars[k] == '\\' { 2 } else { 1 };
            }
            i = k + 1;
            continue;
        }
        if c == '`' {
            if stack.last().is_some_and(|(f, _)| *f == Frame::Tick) {
                let (_, open) = stack.pop().expect("just checked");
                spans.push((open, i));
            } else {
                stack.push((Frame::Tick, i));
            }
        } else if in_dq {
            if c == '"' {
                stack.pop();
            }
        } else if c == '\'' {
            // Literal to the next `'`.
            let mut k = i + 1;
            while k < n && chars[k] != '\'' {
                k += 1;
            }
            i = k;
        } else if c == '"' {
            stack.push((Frame::DoubleQuoted, i));
        } else if c == '(' {
            stack.push((Frame::Paren, i));
        } else if c == ')' {
            match stack.pop() {
                Some((Frame::Sub, open)) => spans.push((open, i)),
                Some((Frame::Paren, _)) | None => {}
                Some(other) => stack.push(other), // a `)` inside a backtick is a word
            }
        }
        i += 1;
    }
    spans
}

/// One line of [`heredoc_layout`]'s answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutLine {
    /// The line's text, exactly as it stood.
    pub text: String,
    /// This line is a heredoc BODY (or its terminator), not command text.
    pub body: bool,
    /// The leak guard drops this line: it is stdin data nothing runs.
    pub drop: bool,
    /// An executor runs this body — what A7 reads.
    pub executed: bool,
}

/// Every line of `cmd` as `(text, is a heredoc body, drop it, an executor runs it)` —
/// `_heredoc_layout`.
///
/// The walk [`strip_reader_heredocs`] used to be, answering both of its callers' questions in ONE
/// pass: the leak guard keeps every line that is not dropped, and A7 reads the lines a shell would
/// run. One walk, because two would come to disagree about where a body ends, which is the defect
/// this area keeps producing.
///
/// Bodies are buffered and judged once the logical command is assembled — a pipeline can span
/// physical lines, and the executor can be on the continuation — folding two kinds of continuation
/// in the order bash applies them: a **backslash-newline** splices before tokenizing, so it is
/// folded into the command line *before* that line's heredoc bodies are read; a trailing
/// **`|`/`|&`** continues the pipeline onto the next command line, which sits *after* the bodies.
///
/// Two things are about a body that gets DROPPED:
///
/// - it ends where BASH ends it ([`body_extent`], at [`heredoc_header`]'s delimiter). For
///   `<<BRIEF'X'` bash's terminator is `BRIEFX`; dropping on `BRIEF` finds no terminator, runs
///   to the end of the input, and takes the commands after the heredoc with it;
/// - a body whose terminator never arrives is **not dropped at all**, whoever opened it. Bash
///   reads an unterminated body to the end of the input, so a drop there would hide everything
///   after it if the delimiter was misread;
/// - a body opened in a substitution closed on its own line (`closed_substitutions`) is not
///   dropped either, and is read as lines a shell runs, because the shells disagree about
///   whether those lines are a body at all.
pub fn heredoc_layout(cmd: &str) -> Vec<LayoutLine> {
    let lines: Vec<&str> = cmd.split('\n').collect();
    let n = lines.len();
    let mut layout: Vec<LayoutLine> = Vec::new();
    let mut i = 0usize;
    while i < n {
        // One LOGICAL command: command-text lines folded for the plan, heredoc bodies buffered as
        // chunks so their fate is decided once the whole pipeline is assembled. A command line
        // carries `None`, which is never a key of `drop`.
        let mut chunks: Vec<(Option<usize>, String)> = Vec::new();
        let mut ended: HashMap<usize, bool> = HashMap::new();
        let mut fallback: HashMap<usize, bool> = HashMap::new();
        let mut body_count = 0usize;
        // Bodies whose opener sits in a substitution closed on its own line: which lines are
        // body there depends on the shell, so they are read both ways.
        let mut read_both_ways: HashSet<usize> = HashSet::new();
        let mut plan_text = String::new();
        let mut join = ""; // separator carried from the previous stage
        loop {
            // A command line, with any backslash-newline splices folded in first — bash does this
            // before it looks for heredoc bodies, so the bodies follow the folded whole. No bound
            // of its own: out of lines, the fold below yields "", which opens no heredoc and
            // continues no pipeline, so this breaks on the next test.
            let mut folded = String::new();
            while i < n {
                let line = lines[i];
                chunks.push((None, line.to_string()));
                i += 1;
                // A comment is not spliced — bash ends the line at it — so a trailing backslash
                // inside one continues nothing. `cat <<'EOF' # note\` opens its heredoc on THIS
                // line; folding the next line in moved the body a line late and a real read after
                // the terminator was stripped as body.
                if ends_in_line_continuation(line) && comment_index(&Line::of(line)) < 0 {
                    // Python's `line[:-1]`: the splicing backslash itself comes out.
                    folded.push_str(line.strip_suffix('\\').unwrap_or(line));
                    continue;
                }
                folded.push_str(line);
                // A quote bash has not closed swallows the newline: the next physical line is part
                // of the same quoted word, not a new command, and a `<<` inside it opens no
                // heredoc. `heredoc_openers` filters a `<<` quoted WITHIN a line; carrying the
                // open quote across the newline is what lets it filter a `<<` quoted by a string
                // that began on an earlier line. The newline is literal text inside the quote, so
                // it is kept in `folded`.
                if Line::of(&folded).quote_open_at_end() && i < n {
                    folded.push('\n');
                    continue;
                }
                break;
            }
            plan_text = format!("{plan_text}{join}{folded}"); // `join` is "" until a stage continues
            let folded_line = Line::of(&folded);
            // When this line is one the whole-line pass cannot attribute, each heredoc is judged
            // on its own rather than all of them sharing one default.
            let unknown = heredoc_strip_plan(&folded_line).is_none();
            let crowded = if unknown {
                crowded_substitutions(&folded_line)
            } else {
                HashSet::new()
            };
            let closed = closed_substitutions(&folded_line);
            for (h, m) in heredoc_openers(&folded_line).into_iter().enumerate() {
                let idx = body_count;
                body_count += 1;
                // Closed on the header's own line: a newline kept in `folded` for an open quote
                // puts the close on a later line, where the body is inside the substitution.
                if m.header.shells_disagree
                    || closed.iter().any(|&(lo, hi)| {
                        lo < m.start
                            && m.start < hi
                            && !folded_line.chars[m.start..hi].contains(&'\n')
                    })
                {
                    read_both_ways.insert(idx);
                }
                if unknown {
                    fallback.insert(
                        idx,
                        crowded.contains(&h) || heredoc_could_run(&folded_line, m.start),
                    );
                }
                // Where bash ends the body, from the one header reading and the one body walk.
                let h = &m.header;
                let (len, found) = body_extent(&lines[i..], &h.delim, h.expands, h.dash);
                for body_line in &lines[i..i + len] {
                    chunks.push((Some(idx), (*body_line).to_string()));
                }
                i += len;
                ended.insert(idx, found);
                if i < n {
                    // the terminator line itself
                    chunks.push((Some(idx), lines[i].to_string()));
                    i += 1;
                }
            }
            if !pipeline_continues(&folded) {
                break;
            }
            join = " "; // the next stage joins onto the same pipe
        }
        let plan = heredoc_strip_plan(&Line::of(&plan_text));
        let mut drop: HashMap<usize, bool> = HashMap::new();
        let executed: HashMap<usize, bool> = match plan {
            Some(entries) => {
                for (idx, e) in entries.iter().enumerate() {
                    drop.insert(idx, e.drop);
                }
                entries
                    .iter()
                    .enumerate()
                    .map(|(idx, e)| (idx, e.executor))
                    .collect()
            }
            // Nothing is DROPPED on an unattributable line — the leak guard's unknown keeps every
            // body visible, and that does not change. What the fallback supplies is the other
            // verdict: which of those bodies a shell would RUN, which is what A7 reads.
            None => fallback,
        };
        for (idx, text) in chunks {
            // A body whose terminator was never found is kept, whoever opened it: "not found" is
            // what every delimiter disagreement in this walk has looked like, and keeping it only
            // shows the guard more text.
            let unterminated = idx.is_some_and(|k| !ended.get(&k).copied().unwrap_or(true));
            layout.push(LayoutLine {
                text,
                body: idx.is_some(),
                drop: idx.is_some_and(|k| {
                    drop.get(&k).copied().unwrap_or(false) && !read_both_ways.contains(&k)
                }) && !unterminated,
                executed: idx.is_some_and(|k| {
                    executed.get(&k).copied().unwrap_or(false) || read_both_ways.contains(&k)
                }),
            });
        }
    }
    layout
}

/// Remove heredoc BODIES that are stdin DATA — never the ones a command runs.
/// `_strip_reader_heredocs`.
///
/// A body fed to `bash`/`python`, or to a reader whose output pipes into one, is a SCRIPT:
/// dropping it would hide the very commands the guard exists to read, so it is kept, and the
/// guard's newline-segmentation then reads each of its lines as the command it is.
pub fn strip_reader_heredocs(cmd: &str) -> String {
    if !cmd.contains("<<") {
        return cmd.to_string();
    }
    heredoc_layout(cmd)
        .into_iter()
        .filter(|l| !l.drop)
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join("\n")
}
