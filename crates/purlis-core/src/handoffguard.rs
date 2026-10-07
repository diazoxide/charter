//! Arm A7: a handoff purlis can read, from the chat itself (#1014, #1444).
//!
//! A port of `charter/hooks.py`'s `_handoff_refusal` and the readers under it —
//! `_disguised_as`, `_disguised_handoff`, `_shell_string`, `_as_the_shell_reads`,
//! `_shell_string_handoff`, `_handoff_segment`, `_handoff_line` — plus `_runs_handoff` and
//! `_is_handoff`, which the Python shares with the routing-mark clear. The readers are the
//! Python's still; the verdict is not, since a handoff became a dispatch.
//!
//! # What consents to a handoff, and what this guard is for
//!
//! **Consent is the dispatch grant** (spec #1434, decisions 4 and 5; #1444). A handoff is a
//! dispatch in handoff mode: the app decides it from its own record of the asking chat, asks
//! the person once for a pair of personas with the brief in front of them, and starts nothing
//! until they allow it. That is the same on every harness. The harness's own permission prompt
//! is no part of it, so nothing here protects that prompt any more: the exact spelling its
//! rule matched and the refusal of an unattended run, which stood in for a prompt nobody was
//! there to answer, are gone. An unattended chat is answered by the app, under a grant that
//! already stands or not at all (`dispatchunattended`).
//!
//! What is left is what only a tool hook can know or read:
//!
//! - **who is asking**: a helper sub-agent's handoff reaches the app looking exactly like its
//!   chat's own, and only the hook's payload tells them apart ([`Caller::from_a_subagent`]);
//! - **a shape purlis cannot read**: a handoff inside a string or a heredoc a shell runs, or
//!   one whose words the shell rewrites, is one whose brief this cannot see and whose asker it
//!   cannot vouch for;
//! - **a brief the shell would change**: the new chat's first message and the dispatch record
//!   are the text written in the call, so a brief fed by a pipe, a file, an unquoted heredoc
//!   or beside a live substitution is refused, as it was.
//!
//! # Why the hook and not the command
//!
//! [`crate::handoff`]'s module header carries the table that divides them. Each refusal here
//! needs a fact no command can observe — the **source spelling** of the command line, and the
//! harness's own hook payload (`agent_id`) — so the command cannot reach them. By the time
//! `purlis handoff` is running, the spelling that got it there is gone.
//!
//! # What A7 is, and is not
//!
//! **It reads a command's words and is not a shell**: an interpreter (`python3 -c`), a
//! variable, a script file and an expansion that does not leave a word whole (`{hand,}off`)
//! all run a handoff it never sees. That is advice a sub-agent meets, not a boundary it cannot
//! cross, as [`crate::dispatchguard`] says of itself: a handoff that gets past it reaches the
//! app's decision like any other, and starts only what the grants in force allow.
//!
//! # The shared heredoc plan, and the trap under it
//!
//! Which heredoc bodies a shell RUNS comes from [`crate::leakguard::lines_a_command_could_run`],
//! and so from the one plan the leak guard uses ([`crate::heredoc::heredoc_strip_plan`]). A
//! body fed to a reader — `purlis handoff`'s own brief, `git commit -F -` — is data and is not
//! searched; a body fed to a shell is searched and flagged.
//!
//! **An unknown plan has to fail the same way for both**, which is
//! [`crate::heredoc::heredoc_could_run`]'s whole subject. The leak guard's unknown is "keep
//! every body visible", which still denies. A7's *used to be* "drop every body", which is the
//! opposite. The two now share one answer and this module adds no default of its own;
//! `heredoc.rs` is `pub` at that boundary for exactly this caller.
//!
//! # Nothing about this module is gated here
//!
//! A7 is PLANE-GATED in `pretooluse` — unlike A5 and A6 beside it, which refuse a fact about
//! the shell. A handoff opens a chat in one of this plane's workspaces, so outside a plane
//! there is no such chat to start. The gate is [`crate::toolgate`]'s, one level up, for
//! the reason that module's header gives: reading it once is what keeps five decisions from
//! drifting apart.

use crate::heredoc;
use crate::leakguard;
use crate::livesub;
use crate::memstore::is_python_space;
use crate::proseguard::charter_words;
use crate::pypath;
use crate::shellseg::{self, Tok};
use crate::shellwrap;

/// The `agent_id` on a payload means "a sub-agent" only on a harness where that was MEASURED.
///
/// Claude Code 2.1.268 and codex-cli 0.147.0 (`codex exec --enable multi_agent_v2`) both sent
/// none in the main conversation and one on a sub-agent's Bash call. opencode's plugin builds a
/// payload with no such field, and **a harness nobody measured is not read as one** — reading
/// an absent field as a sub-agent would refuse every handoff on it.
///
/// `charter/harness/claude_code.py:NAME` and `charter/harness/codex.py:NAME`.
pub const MEASURED_SUBAGENT_HARNESSES: [&str; 2] = ["claude-code", "codex"];

/// The characters that make a word something the shell rewrites before a program sees it:
/// quoting, an escape, `$` (parameter, ANSI-C and locale expansion), braces and globs.
/// `_SPELLING_MARKS`.
pub const SPELLING_MARKS: &str = "$'\"\\{}?*[]";

/// The redirection operators that READ — `hooks.py`'s `_REDIRECT_READS`.
///
/// Spelled here rather than borrowed from [`crate::shellwrap`], whose copy is private and is
/// asked a different question (which operand a redirection consumes). A7 asks only whether one
/// stands in the handoff's own segment at all.
pub const REDIRECT_READS: [&str; 2] = ["<>", "<"];

// ----------------------------------------------------------------------------------------
// what each refusal says
// ----------------------------------------------------------------------------------------
//
// Spelled once, so a denial reads the same wherever it is quoted. They were
// `charter/hooks.py`'s constants verbatim until a handoff became a dispatch (#1444); since then
// they are this guard's own, and the recording in `fixtures/corpora` holds them as rewritten.

/// What a helper sub-agent that hands off is told.
pub const HANDOFF_SUBAGENT: &str = "`purlis handoff` is refused from inside a sub-agent. A handoff starts a chat the person \
     can see, open and stop, for the chat that asks, and a sub-agent is not a chat. Return \
     what you found to your chat, and let that chat hand it off.";

/// What a handoff purlis cannot read as a command of its own is told.
pub const HANDOFF_SPELLING: &str = "`purlis handoff` is refused where purlis cannot read it as a command of its own: the \
     shell rewrites one of its two words (a brace, a glob or a parameter in \
     `purlis` or `handoff`), or it stands inside a command substitution. purlis reads a \
     command's words and is not a shell, so it cannot see which brief this hands off. Write \
     it at the start of its own command: \
     purlis handoff --name \"<task>\" <workspace> <<'BRIEF'";

/// What a handoff inside a string or a heredoc a shell runs is told.
pub const HANDOFF_SHELL_STRING: &str = "`purlis handoff` is refused inside a string or a heredoc a shell runs (`eval`, \
     `bash -c '…'`, `bash <<'EOF'`). purlis looks one level in and no deeper, so it cannot \
     read the brief there or tell a helper sub-agent's handoff from this chat's own. Run it \
     directly instead: purlis handoff --name \"<task>\" <workspace> <<'BRIEF'";

/// What a handoff inside a substitution, a `case` branch or a function body is told
/// ([`crate::consentspelling`] finds those). `where_` is where it sits, as a refusal says it.
pub fn handoff_placed(where_: &str) -> String {
    format!(
        "`purlis handoff` is refused {where_}. purlis cannot read the brief there or tell a helper \
         sub-agent's handoff from this chat's own. Run it as a command of its own: \
         purlis handoff --name \"<task>\" <workspace> <<'BRIEF'"
    )
}

/// What a brief the shell would change is told, with `{what}` still to fill —
/// [`handoff_source`] fills it.
pub const HANDOFF_SOURCE: &str = "`purlis handoff` takes its brief from a QUOTED heredoc in the same call — <<'BRIEF' — \
     so the new chat is sent exactly the text written here, and the dispatch record keeps the \
     same. This call feeds it {what}, which the shell would change or hide before purlis \
     reads it. Write: purlis handoff --name \"<task>\" <workspace> <<'BRIEF' … BRIEF";

/// [`HANDOFF_SOURCE`] with `{what}` filled.
pub fn handoff_source(what: &str) -> String {
    HANDOFF_SOURCE.replace("{what}", what)
}

/// The trace reason each refusal is tallied under. The words are the Python's, and they are
/// the stable key a tally reader already has. `handoff-spelling` is now the handoff purlis
/// cannot read as a command of its own, and `handoff-unattended` is no longer made (#1444).
pub const REASON_SUBAGENT: &str = "handoff-subagent";
/// See [`REASON_SUBAGENT`].
pub const REASON_SHELL_STRING: &str = "handoff-shell-string";
/// See [`REASON_SUBAGENT`].
pub const REASON_SPELLING: &str = "handoff-spelling";
/// See [`REASON_SUBAGENT`].
pub const REASON_BRIEF_SOURCE: &str = "handoff-brief-source";

// ----------------------------------------------------------------------------------------
// CPython's string arithmetic, where A7 does it on the SOURCE
// ----------------------------------------------------------------------------------------

/// `line[start:end]` as CPython slices a `str`: CHARACTERS, negative indices from the end,
/// both ends clamped, and an end before the start giving `""`.
///
/// **CHARACTERS is the load-bearing half**, and it is the half that fires: every offset A7 reads
/// is a `str` index on the oracle's side, the alphabet carries astral-plane characters, and a
/// byte index would read a different slice on any line holding one.
///
/// **The negative half is faithfulness, not a live path, and saying so is the point.**
/// [`Tok::start`] and [`Tok::end`] are `-1` where nothing measured a token, and `-1` is the LAST
/// character to Python rather than an error — so clamping it to zero would be a silent
/// disagreement with the oracle. Measured: the lexer sets both the moment a token has a first
/// character, so no token it EMITS carries `-1`; the only `-1`s made anywhere are
/// `shellseg::resegment`'s, on the lex-FAILURE path, which A7 never reaches because it returns
/// on `Err` before any of this. Zero of the 529 recorded corpus rows carry a `-1` offset in the
/// segment A7 judges. It is kept because the Python would do this if one ever arrived, and it is
/// documented as unreached because defensive code that reads as though it matters is worse than
/// none in a guard somebody has to re-derive.
/// A token's source text, as [`py_slice`] reads it: what the consent-spelling guard checks a
/// program word against (RN-7).
pub(crate) fn source_of(chars: &[char], start: isize, end: isize) -> String {
    py_slice(chars, start, end)
}

fn py_slice(chars: &[char], start: isize, end: isize) -> String {
    let n = chars.len() as isize;
    let fix = |i: isize| -> usize {
        let i = if i < 0 { i + n } else { i };
        i.clamp(0, n) as usize
    };
    let (a, b) = (fix(start), fix(end));
    if b <= a {
        return String::new();
    }
    chars[a..b].iter().collect()
}

// ----------------------------------------------------------------------------------------
// is a handoff about to run
// ----------------------------------------------------------------------------------------

/// Whether this ONE segment runs `charter handoff`, in any spelling charter recognises — the
/// plain one, `python3 -m charter`, a path to charter, behind a prefix or a wrapper.
/// `_runs_handoff`.
///
/// **Every spelling, on purpose.** This answers only "is a handoff about to run", which is
/// what a guard has to know before it can say anything about one.
pub fn runs_handoff(prog: &str, argv: &[String]) -> bool {
    charter_words(prog, argv).is_some_and(|w| w.first().is_some_and(|f| f == "handoff"))
}

/// Whether any segment of `cmd` runs `charter handoff` (chat handoff). `_is_handoff`.
///
/// Shared in the Python: A7 finds the handoff line with it, and the routing-mark clear that a
/// `charter handoff` brings reads it too, so the two cannot disagree about what a handoff is.
///
/// **Any segment, and newlines are segment boundaries here**, so a heredoc body line that
/// itself begins `charter handoff` counts as well. That over-match costs one cleared mark where
/// it is read on its own; A7 skips heredoc bodies before it judges anything
/// ([`crate::heredoc::strip_reader_heredocs`], through [`handoff_line`]).
pub fn is_handoff(cmd: &str) -> bool {
    shellseg::segment_argv(cmd).iter().any(|toks| {
        let (prog, _env, argv) = shellwrap::split_env(toks);
        runs_handoff(&prog, &argv)
    })
}

// ----------------------------------------------------------------------------------------
// the spelling
// ----------------------------------------------------------------------------------------

/// Whether `raw` — a word as the SOURCE spells it — is `word` behind a shell rewrite.
/// `_disguised_as`.
///
/// Only when `raw` carries a character the shell rewrites ([`SPELLING_MARKS`]), and either what
/// is left once those characters are dropped still spells `word` (`$'handoff'`, `ha$''ndoff`,
/// `${x:-handoff}`, `{handoff,}`) or `word` matches `raw` as a glob (`hando?f`).
///
/// **A recognition of the usual shapes, never a shell**: `$h` holding `handoff` is not seen,
/// and neither is anything an interpreter, a variable or a script file produces.
///
/// `word in kept` is a SUBSTRING test and not an equality — Python's `in` on a `str` — which is
/// what catches `${x:-handoff}`, where the braces come off and a `x:-` is still in front.
pub fn disguised_as(raw: &str, word: &str) -> bool {
    if !raw.chars().any(|c| SPELLING_MARKS.contains(c)) {
        return false;
    }
    let kept: String = raw
        .chars()
        .filter(|c| !SPELLING_MARKS.contains(*c))
        .collect();
    // `fnmatch.fnmatchcase(word, raw)`: the WORD is the name and the SOURCE spelling is the
    // pattern. On posix `os.path.normcase` is the identity, so `fnmatch` and `fnmatchcase` are
    // one function and [`pypath::fnmatch`] is both.
    kept.contains(word) || pypath::fnmatch(word, raw)
}

/// Whether a segment of `line` begins `charter handoff` with a shell rewrite in either word.
/// `_disguised_handoff`.
///
/// [`charter_words`] reads none of these as charter at all — `$'charter'` is the word
/// `$charter` to a tokenizer — and each ran a handoff with no prompt on Claude Code 2.1.268.
///
/// **The first two words of a segment and only those**, so `grep 'charter handoff' docs`,
/// which charter's own repository runs all day, is a search rather than a spelling of one.
pub fn disguised_handoff(line: &str) -> bool {
    disguised_handoff_spelling(line, &[crate::cliname::ALIAS])
}

/// [`disguised_handoff`], where `spelt` holds every program name the project's handoff rule is
/// spelt with: `charter`, and `purlis` too where the project carries that rule (RN-7). A bare
/// `<name> handoff` under one of those names is the exact spelling, and is not a disguise.
fn disguised_handoff_spelling(line: &str, spelt: &[&str]) -> bool {
    let Ok(toks) = shellseg::lex(line) else {
        return false;
    };
    let toks = shellseg::split_punctuation(toks);
    let chars: Vec<char> = line.chars().collect();
    for (seg, _before) in heredoc::segments_of(&toks) {
        if seg.len() < 2 {
            continue;
        }
        let first = py_slice(&chars, seg[0].start, seg[0].end);
        let second = py_slice(&chars, seg[1].start, seg[1].end);
        // Every name the command line has (RN-3). The spellings the host's rule matches are
        // `spelt`: `charter`, and `purlis` where the project carries that rule too (RN-7). Any
        // other name, bare, is refused here for its spelling.
        let named = crate::cliname::INSTALLED
            .iter()
            .any(|name| first == *name || disguised_as(&first, name));
        let exact = spelt.contains(&first.as_str()) && second == "handoff";
        if !exact && named && (second == "handoff" || disguised_as(&second, "handoff")) {
            return true;
        }
    }
    false
}

// ----------------------------------------------------------------------------------------
// a string a shell runs
// ----------------------------------------------------------------------------------------

/// The text a segment hands a shell to run, or `None` — [`shellwrap::shell_string`] over the
/// segment's words. `_shell_string`.
pub fn shell_string(seg: &[Tok]) -> Option<String> {
    let toks: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
    shellwrap::shell_string(&toks)
}

/// `text` with bash's backslash-newlines removed and every other whitespace character read as a
/// space — **where A7 LOOKS for a handoff, never the text it judges**. `_as_the_shell_reads`.
///
/// `str.isspace()` and not `char::is_whitespace`: CPython counts U+001C–U+001F and Rust does
/// not, and a non-breaking space between `charter` and `handoff` is one word to the tokenizer
/// and to bash alike. [`crate::memstore::is_python_space`] is that predicate.
pub fn as_the_shell_reads(text: &str) -> String {
    text.replace("\\\n", "")
        .chars()
        .map(|c| {
            if is_python_space(c) && c != '\n' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// Whether a string this call hands a shell to run holds a handoff — ONE level in.
/// `_shell_string_handoff`.
///
/// The host's rule reads the outer command, so a handoff inside `eval '…'` or `bash -c '…'`
/// ran with no prompt on Claude Code 2.1.268. Looked for with the same two readers A7 uses on a
/// line ([`is_handoff`], [`disguised_handoff`]), in the string exactly as the shell would
/// receive it.
///
/// **Not recursive**: a string inside that string is not opened, and a heredoc fed to a shell
/// (`bash <<'EOF'`) is not either — that one is [`handoff_line`]'s, through the shared plan.
pub fn shell_string_handoff(cmd: &str) -> bool {
    // The STRIPPED text, not A7's line view: a quoted string that spans lines carries a `<<`
    // the header regex counts and the lexer cannot, so its lines are filed as a body nobody
    // executes and A7 skips them — which is right for a commit message and would hide the very
    // string this looks into (`eval "charter handoff b <<'BRIEF'` …). Nothing is dropped from
    // that text when the plan is unknown, so the whole call is here to lex.
    let Ok(toks) = shellseg::lex(&heredoc::strip_reader_heredocs(cmd)) else {
        return false;
    };
    let toks = shellseg::split_punctuation(toks);
    heredoc::segments_of(&toks).iter().any(|(seg, _before)| {
        let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
        shellwrap::shell_scripts(&words)
            .iter()
            .any(|inner| is_handoff(&as_the_shell_reads(inner)) || disguised_handoff(inner))
    })
}

// ----------------------------------------------------------------------------------------
// which line, and which segment of it
// ----------------------------------------------------------------------------------------

/// `(the first segment that runs charter handoff, whether a pipe feeds it)`.
/// `_handoff_segment`.
///
/// A walk of its own over the line's tokens rather than [`shellseg::segment_argv`], because the
/// two facts this guard needs are exactly the two that function discards: the operator in FRONT
/// of a segment (a `|` is stdin arriving from another program) and whether a word was quoted (a
/// heredoc delimiter's quoting decides whether its body expands).
///
/// Substitutions and groups are not unpicked: a handoff found only inside one is not at the
/// start of its own command, and the caller refuses that as a spelling.
pub fn handoff_segment(toks: &[Tok]) -> (Option<Vec<Tok>>, bool) {
    for (seg, before) in heredoc::segments_of(toks) {
        let texts: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
        let (prog, _env, argv) = shellwrap::split_env(&texts);
        if runs_handoff(&prog, &argv) {
            let piped = before == "|" || before == "|&";
            return (Some(seg), piped);
        }
    }
    (None, false)
}

/// The first line of `cmd` that runs `charter handoff`, as its SOURCE is written, paired with
/// whether a shell RUNS that line from a heredoc body — or `None`. `_handoff_line`.
///
/// Which bodies those are comes from [`leakguard::lines_a_command_could_run`], and so from the
/// one heredoc plan the leak guard uses; the module header argues why that sharing is the whole
/// point. A line ending in an ODD number of backslashes is joined to the next, because bash
/// removes that backslash-newline and runs the two as one command.
///
/// The handoff is LOOKED FOR with the continuation removed and every whitespace character read
/// as a space, so `charter` and `handoff` split across a continuation or joined by a
/// non-breaking space are still found. **What comes back is the line as written**, because A7
/// judges the spelling, and those are spellings.
///
/// The `in_a_shells_body` flag is the FIRST row's, not the joined tail's: a continuation that
/// starts outside a body is one command, and the shell that would run it is the one that
/// opened the line.
///
/// **A line a quoted string began on an earlier line starts inside that string** (M8.2), and
/// the part of it the string still holds is data: `git commit -m 'fix\n\ncharter handoff …'`
/// is a message, not a handoff. [`quoted_row_prefixes`] reads where each row's quoted head
/// ends off the WHOLE text, and only what follows it is looked at. A row that the string
/// covers entirely is skipped; a row where the string closes is judged from the close on.
pub fn handoff_line(cmd: &str) -> Option<(String, bool)> {
    let rows = leakguard::lines_a_command_could_run(cmd);
    let heads = quoted_row_prefixes(&rows);
    let live = live_text_in_heads(&rows, &heads);
    let mut i = 0usize;
    while i < rows.len() {
        let (row, in_a_shells_body) = rows[i].clone();
        let head = heads[i];
        // A shell that runs this row runs a `$( … )` or a backtick inside the string its head
        // sits in, as a `"…"` leaves them live. Read both ways, a body can be that string's
        // text in one shell and the shell's own in another (#780's review).
        if in_a_shells_body
            && let Some(run) = live[i]
                .iter()
                .find(|run| is_handoff(&as_the_shell_reads(run)) || disguised_handoff(run))
        {
            return Some((run.clone(), true));
        }
        let mut line: String = row.chars().skip(head).collect();
        if head > 0 && line.is_empty() {
            i += 1;
            continue;
        }
        // `len(line) - len(line.rstrip("\\"))` — trailing backslashes, in CHARACTERS.
        while trailing_backslashes(&line) % 2 == 1 && i + 1 < rows.len() {
            i += 1;
            line.push('\n');
            line.push_str(&rows[i].0);
        }
        if is_handoff(&as_the_shell_reads(&line)) || disguised_handoff(&line) {
            return Some((line, in_a_shells_body));
        }
        i += 1;
    }
    None
}

/// For each row, how many of its leading CHARACTERS still sit inside a quoted string that an
/// earlier row opened — 0 for a row that starts as a command.
///
/// Read off the rows joined back together, which is the text a shell reads once the reader
/// bodies are gone, by the two readers that already know it: the lexer, whose one token spans
/// the newline in front of the row (and which knows a `#` comment opens no quote), and
/// [`shellseg::quote_map`], which says that newline is QUOTED. Both are asked because each is
/// wrong alone in a direction that would hide a real handoff:
///
/// - a backslash-newline outside quotes is folded into the next word by the lexer, but is a
///   continuation, not a string — `quote_map` reads it unquoted;
/// - inside `"$( … )"` the lexer holds everything as one word, but the substitution RUNS its
///   lines — `quote_map` opens a command context there and reads them unquoted;
/// - an apostrophe in a comment opens a quote to `quote_map`, and no token to the lexer.
///
/// **Text the lexer cannot read gives no row a head**, which is the reading this guard had
/// before, so an unbalanced call is judged exactly as it was.
fn quoted_row_prefixes(rows: &[(String, bool)]) -> Vec<usize> {
    let mut heads = vec![0usize; rows.len()];
    let text = rows
        .iter()
        .map(|(r, _)| r.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(toks) = shellseg::lex(&text) else {
        return heads;
    };
    let quoted = shellseg::quote_map(&text);
    let mut start = 0usize;
    for (i, (row, _)) in rows.iter().enumerate() {
        let len = row.chars().count();
        if i > 0 && quoted.get(start - 1).copied().unwrap_or(false) {
            let at = start as isize;
            if let Some(t) = toks
                .iter()
                .find(|t| !t.bare && t.start >= 0 && t.start < at && t.end >= at)
            {
                heads[i] = ((t.end - at) as usize).min(len);
            }
        }
        start += len + 1;
    }
    heads
}

/// For each row, the stretches of its quoted head ([`quoted_row_prefixes`]) that are NOT quoted
/// after all: the inside of a `$( … )` or a backtick in a `"…"`, which the shell runs.
///
/// Read off the same joined text, with [`shellseg::quote_map`], which opens a command context
/// at a substitution inside double quotes and none inside single quotes.
fn live_text_in_heads(rows: &[(String, bool)], heads: &[usize]) -> Vec<Vec<String>> {
    let text = rows
        .iter()
        .map(|(r, _)| r.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let quoted = shellseg::quote_map(&text);
    let mut out = Vec::with_capacity(rows.len());
    let mut start = 0usize;
    for ((row, _), &head) in rows.iter().zip(heads) {
        let mut runs = Vec::new();
        let mut run = String::new();
        for (k, c) in row.chars().take(head).enumerate() {
            if quoted.get(start + k).copied().unwrap_or(true) {
                if !run.is_empty() {
                    runs.push(std::mem::take(&mut run));
                }
            } else {
                run.push(c);
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
        out.push(runs);
        start += row.chars().count() + 1;
    }
    out
}

/// How many `\` a line ends with — `len(line) - len(line.rstrip("\\"))`.
fn trailing_backslashes(line: &str) -> usize {
    line.chars().rev().take_while(|c| *c == '\\').count()
}

// ----------------------------------------------------------------------------------------
// the verdict
// ----------------------------------------------------------------------------------------

/// What the hook knows about a tool call that no command can observe. `_handoff_refusal`'s
/// `data`, narrowed to the three fields purlis's guards read.
///
/// A struct rather than three arguments: the two that gate the sub-agent refusal only mean
/// anything together, and a call site that passed them the other way round would compile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Caller<'a> {
    /// The payload's `agent_id`. Python's truth test is a string's, so an EMPTY one is no
    /// sub-agent — a harness that sends the field always, empty in the main conversation,
    /// is read the way it means it.
    pub agent_id: Option<&'a str>,
    /// `$CHARTER_HARNESS`. Read here rather than from the environment because the core holds
    /// no globals; the CLI passes what the process was given.
    pub harness: Option<&'a str>,
    /// The payload's `permission_mode` — [`crate::floorguard::unattended`]'s input. **A7 no
    /// longer reads it** (#1444): whether anybody answers a chat's prompts is the app's own
    /// mark on that chat, read where a handoff is decided. The release floor still does.
    pub permission_mode: Option<&'a str>,
}

impl Caller<'_> {
    /// Whether this call came from a sub-agent on a harness where `agent_id` was MEASURED to
    /// mean one. See [`MEASURED_SUBAGENT_HARNESSES`].
    pub fn from_a_subagent(&self) -> bool {
        self.agent_id.is_some_and(|id| !id.is_empty())
            && self
                .harness
                .is_some_and(|h| MEASURED_SUBAGENT_HARNESSES.contains(&h))
    }
}

/// `(trace reason, denial)` for a `purlis handoff` this guard refuses — or `None`.
///
/// **The first handoff INVOCATION line is the one judged** ([`handoff_line`]).
///
/// The refusals, in the order they are asked:
///
/// 1. `handoff-subagent` — the payload carries `agent_id` on a harness where that is measured
///    to mean a sub-agent ([`Caller::from_a_subagent`]).
/// 2. `handoff-shell-string` — a handoff inside a string a shell runs, one level deep
///    ([`shell_string_handoff`]), or inside a heredoc body a shell runs.
/// 3. `handoff-spelling` — a handoff this guard recognises and cannot read as a command of its
///    own: a word of it that reads `purlis` or `handoff` only behind an expansion or a glob, or
///    one inside a command substitution ([`handoff_segment`] finds no segment for it). A
///    group and a subshell are read through: the handoff in one is a segment like any other.
/// 4. `handoff-brief-source` — stdin that is not exactly one quoted heredoc on the handoff's
///    own segment, or a live substitution anywhere in the call
///    ([`livesub::live_substitution`], scoped to the WHOLE call like A5 and A6, for their
///    reason).
///
/// **What it no longer refuses** (#1444, where a handoff became a dispatch and its consent the
/// dispatch grant). Both stood in for the harness's own permission prompt, which consents to
/// nothing now:
///
/// - *an unattended run* (`permission_mode: bypassPermissions`). The app answers a chat nobody
///   is at, from its own mark on that chat: under a grant that already stands, or refused
///   (`dispatchunattended`). Codex reports that mode for every chat, so this arm refused every
///   handoff from one.
/// - *a spelling the harness's rule did not match*: a path to the binary, `python3 -m`, a
///   `VAR=` prefix or a wrapper, a quoted or escaped word that still reads plainly, more than
///   one space, a line continuation. Each is a handoff this guard reads, word for word, so the
///   brief's source is judged as for any other.
///
/// **Quoting is read off the delimiter token**, which is the answer
/// [`heredoc::heredoc_header`] gives — any quoting anywhere in the word makes the body literal
/// — taken from the tokenizer that already found this `<<`. A search of the raw line for it
/// would be misled by a quoted `"<<"` earlier on the line.
pub fn handoff_refusal(cmd: &str, caller: Caller<'_>) -> Option<(&'static str, String)> {
    let found = handoff_line(cmd);
    let in_a_string = shell_string_handoff(cmd);
    if found.is_none() && !in_a_string {
        return None;
    }
    if caller.from_a_subagent() {
        return Some((REASON_SUBAGENT, HANDOFF_SUBAGENT.to_string()));
    }
    // A handoff a shell runs — from a `-c` string or from a heredoc body it executes — is one
    // this guard looks one level into and no deeper.
    //
    // Python's `in_a_string or found[1]` short-circuits, which is what makes `found` safe to
    // unwrap below: the only way past this line with `found` unset is `in_a_string` being
    // true, and that returns here.
    if in_a_string || found.as_ref().is_some_and(|f| f.1) {
        return Some((REASON_SHELL_STRING, HANDOFF_SHELL_STRING.to_string()));
    }
    let line = found
        .expect("a handoff line, or `in_a_string` returned above")
        .0;
    let Ok(lexed) = shellseg::lex(&line) else {
        // The shell would still be reading that quote or escape past the end of the line, so
        // which heredoc (if any) feeds this command cannot be read off it. Refused, not guessed.
        return Some((
            REASON_BRIEF_SOURCE,
            handoff_source(
                "a quote or an escape left open on its line, so no heredoc can be seen \
                 feeding it",
            ),
        ));
    };
    let toks = shellseg::split_punctuation(lexed);
    // Found by its line and by no segment of it: the words are rewritten by the shell, or the
    // handoff is not at the start of a command of its own.
    let (Some(seg), piped) = handoff_segment(&toks) else {
        return Some((REASON_SPELLING, HANDOFF_SPELLING.to_string()));
    };
    // A report back carries no brief and opens no chat (charter-app#259): its text is the
    // command's own argument. Only a live substitution is still refused, because that text is
    // not the one written here.
    if is_a_report(&seg) {
        return livesub::live_substitution(cmd).map(|hit| {
            let said = if livesub::is_process_substitution(hit) {
                HANDOFF_REPORT_PROCESS_SOURCE
            } else {
                HANDOFF_REPORT_SOURCE
            };
            (REASON_BRIEF_SOURCE, said.to_string())
        });
    }
    let what = brief_source(cmd, &seg, piped)?;
    Some((REASON_BRIEF_SOURCE, handoff_source(what)))
}

/// Whether a handoff segment is `purlis handoff report <summary>` — a report back, which
/// carries its text as an argument and reads no brief (charter-app#259).
///
/// `report` bare, right after the `handoff` **the reader found**, and followed by at least one
/// word, and nothing read from stdin: `purlis handoff report <<'BRIEF'` is still a handoff INTO
/// a workspace called `report`, judged as one.
///
/// The word is found where [`charter_words`] says the command line's own words start, never by
/// looking for a token spelt `handoff`: a segment this guard reads may start with a prefix or a
/// wrapper, and a wrapper's own argument can be spelt anything (`sudo -u handoff …`). Where the
/// reader's words are not the segment's own tail, this answers no, and the brief is judged.
fn is_a_report(seg: &[Tok]) -> bool {
    let texts: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
    let (prog, _env, argv) = shellwrap::split_env(&texts);
    let Some(words) = charter_words(&prog, &argv) else {
        return false;
    };
    let Some(at) = texts.len().checked_sub(words.len()) else {
        return false;
    };
    if texts[at..] != words[..] || words.first().is_none_or(|word| word != "handoff") {
        return false;
    }
    seg.get(at + 1)
        .is_some_and(|word| word.bare && word.text == "report")
        && seg.len() > at + 2
        && !seg.iter().enumerate().any(|(i, t)| {
            t.is_op(&["<<", "<<<", "<", "<>"]) && !opens_a_process_substitution(seg, i)
        })
}

/// Whether the `<` at `seg[i]` is the first half of a `<(…)` process substitution rather than
/// a redirection that reads a file: the lexer hands the pair back as a `<` and a `(` with nothing
/// between them. A substitution is refused as the live substitution it is, and a denial that
/// called it "a file (<)" would name the wrong thing. `< (x)`, with a blank, stays a read: bash
/// refuses it, and zsh reads the file the glob `(x)` names. zsh's `<<(…)` is the same pair
/// behind a `<<`, and is not a heredoc.
fn opens_a_process_substitution(seg: &[Tok], i: usize) -> bool {
    let (Some(lt), Some(paren)) = (seg.get(i), seg.get(i + 1)) else {
        return false;
    };
    // A negative offset is one nothing measured, and two of them say nothing about adjacency.
    (lt.text == "<" || lt.text == "<<")
        && paren.is_op(&["("])
        && lt.end >= 0
        && paren.start == lt.end
}

/// What a report back with a live substitution in it is told.
pub const HANDOFF_REPORT_SOURCE: &str = "`purlis handoff report` sends its summary as it is written here, and this call has a \
     live command substitution in it, which the shell would replace before purlis reads it. \
     Write the summary out in plain words: purlis handoff report \"<summary>\"";

/// What a report back with a live PROCESS substitution in it is told: the shell runs that
/// command too, and hands purlis a path to its output where the words stood.
pub const HANDOFF_REPORT_PROCESS_SOURCE: &str = "`purlis handoff report` sends its summary as it is written here, and this call has a \
     live process substitution in it, which the shell would run and replace with a path \
     before purlis reads it. Write the summary out in plain words: \
     purlis handoff report \"<summary>\"";

/// What this call feeds the brief, for [`handoff_source`] — or `None`, which is the one way a
/// handoff passes A7.
///
/// The order is the Python's `if`/`elif` chain and it is a precedence: a here-string that is
/// also piped is named as a here-string, because that is the thing to fix first.
fn brief_source(cmd: &str, seg: &[Tok], piped: bool) -> Option<&'static str> {
    let heredocs: Vec<usize> = seg
        .iter()
        .enumerate()
        .filter(|&(i, t)| t.is_op(&["<<"]) && !opens_a_process_substitution(seg, i))
        .map(|(i, _)| i)
        .collect();
    if seg.iter().any(|t| t.is_op(&["<<<"])) {
        return Some("a here-string (<<<)");
    }
    if seg
        .iter()
        .enumerate()
        .any(|(i, t)| t.is_op(&REDIRECT_READS) && !opens_a_process_substitution(seg, i))
    {
        return Some("a file (<), whose text this call does not show");
    }
    if piped {
        return Some("a pipe");
    }
    let Some(&first) = heredocs.first() else {
        return Some("no heredoc at all");
    };
    if heredocs.len() > 1 {
        // bash reads every body and hands the command only the last one (GNU bash 3.2.57).
        return Some("more than one heredoc, and the shell sends purlis only the last");
    }
    // Python's `all(t.bare for t in seg[i+1:i+2])`: a one-or-zero-element slice, so a `<<` that
    // is the segment's LAST token has no delimiter and `all` of nothing is true — an opener
    // with nothing after it reads as unquoted.
    if seg.get(first + 1).is_none_or(|t| t.bare) {
        return Some("an unquoted heredoc, which expands $… and `…` in the brief");
    }
    if let Some(hit) = livesub::live_substitution(cmd) {
        return Some(if livesub::is_process_substitution(hit) {
            "a live process substitution"
        } else {
            "a live command substitution"
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape the handoff skill writes: two plain words and a quoted heredoc.
    const CANONICAL: &str = "charter handoff beta <<'BRIEF'\nship it\nBRIEF";

    fn attended() -> Caller<'static> {
        Caller {
            agent_id: None,
            harness: Some("claude-code"),
            permission_mode: Some("default"),
        }
    }

    fn refusal(cmd: &str) -> Option<(&'static str, String)> {
        handoff_refusal(cmd, attended())
    }

    fn reason(cmd: &str) -> Option<&'static str> {
        refusal(cmd).map(|(r, _)| r)
    }

    #[test]
    fn the_canonical_spelling_passes() {
        assert_eq!(refusal(CANONICAL), None);
    }

    #[test]
    fn a_command_that_is_no_handoff_is_not_this_guards_business() {
        assert_eq!(reason("git status"), None);
        assert_eq!(reason("echo charter handoff"), None);
        // The first two words of a SEGMENT, so a search for the phrase is a search.
        assert_eq!(reason("grep -rn 'charter handoff' docs"), None);
    }

    /// A handoff is a dispatch, and its consent is the dispatch grant (#1444): no rule of the
    /// harness has to match its spelling, so every spelling this guard reads word for word
    /// goes on to the app, which decides it.
    #[test]
    fn a_handoff_purlis_reads_passes_however_its_two_words_are_written() {
        for cmd in [
            "purlis handoff beta <<'BRIEF'\nx\nBRIEF",
            "python3 -m charter handoff beta <<'BRIEF'\nx\nBRIEF",
            "/usr/local/bin/charter handoff beta <<'BRIEF'\nx\nBRIEF",
            "charter 'handoff' beta <<'BRIEF'\nx\nBRIEF",
            "charter $'handoff' beta <<'BRIEF'\nx\nBRIEF",
            "'charter' handoff beta <<'BRIEF'\nx\nBRIEF",
            "\\charter handoff beta <<'BRIEF'\nx\nBRIEF",
            "charter  handoff beta <<'BRIEF'\nx\nBRIEF",
            "CHARTER_ROOT=/x charter handoff beta <<'BRIEF'\nx\nBRIEF",
            // A backslash-newline is gone before the shell reads the words.
            "charter handoff \\\nbeta <<'BRIEF'\nx\nBRIEF",
        ] {
            assert_eq!(refusal(cmd), None, "{cmd:?}");
        }
    }

    /// What this guard recognises as a handoff and cannot read as a command of its own is
    /// still refused: it cannot see which brief that hands off.
    #[test]
    fn a_handoff_purlis_cannot_read_as_a_command_of_its_own_is_refused() {
        for cmd in [
            "charter hando?f beta <<'BRIEF'\nx\nBRIEF",
            "charter {handoff,} beta <<'BRIEF'\nx\nBRIEF",
            "purlis ${x:-handoff} beta <<'BRIEF'\nx\nBRIEF",
        ] {
            let (why, said) = refusal(cmd).unwrap_or_else(|| panic!("{cmd:?} is refused"));
            assert_eq!(why, REASON_SPELLING, "{cmd:?}");
            assert_eq!(said, HANDOFF_SPELLING, "{cmd:?}");
        }
    }

    /// The brief's source is judged for every spelling this guard reads, not only the plain
    /// one: a prefix or a path in front does not let a brief through that the shell changes.
    #[test]
    fn a_brief_the_shell_would_change_is_refused_under_every_spelling_that_is_read() {
        for cmd in [
            "FOO=1 purlis handoff beta < brief.txt",
            "/usr/local/bin/purlis handoff beta",
            "python3 -m purlis handoff beta <<BRIEF\nx\nBRIEF",
            "cat brief.txt | 'purlis' handoff beta",
            "charter  handoff \"$(cat name)\" <<'BRIEF'\nx\nBRIEF",
        ] {
            assert_eq!(reason(cmd), Some(REASON_BRIEF_SOURCE), "{cmd:?}");
        }
    }

    #[test]
    fn a_brief_the_shell_would_change_is_refused_and_the_denial_says_which() {
        for (cmd, what) in [
            ("charter handoff beta <<<'x'", "a here-string (<<<)"),
            (
                "charter handoff beta < brief.txt",
                "a file (<), whose text this call does not show",
            ),
            ("cat brief.txt | charter handoff beta", "a pipe"),
            ("charter handoff beta", "no heredoc at all"),
            (
                "charter handoff beta <<'A' <<'B'\nx\nA\ny\nB",
                "more than one heredoc, and the shell sends purlis only the last",
            ),
            (
                "charter handoff beta <<BRIEF\nx\nBRIEF",
                "an unquoted heredoc, which expands $… and `…` in the brief",
            ),
        ] {
            let (r, said) = refusal(cmd).unwrap_or_else(|| panic!("{cmd:?} is refused"));
            assert_eq!(r, REASON_BRIEF_SOURCE, "{cmd:?}");
            assert!(said.contains(what), "{cmd:?} does not say {what:?}: {said}");
        }
    }

    #[test]
    fn a_live_substitution_beside_a_quoted_heredoc_is_refused() {
        let cmd = "charter handoff \"$(cat name)\" <<'BRIEF'\nx\nBRIEF";
        // The heredoc is quoted; what is left is the substitution,
        // and it is looked for in the WHOLE call, exactly as A5 and A6 look for one.
        let (r, said) = refusal(cmd).expect("refused");
        assert_eq!(r, REASON_BRIEF_SOURCE);
        assert!(said.contains("a live command substitution"), "{said}");
    }

    /// A process substitution anywhere in the call is refused as one, and named as one.
    #[test]
    fn a_live_process_substitution_is_refused_and_named() {
        let cmd = "charter handoff beta <<'BRIEF' && cat <(env)\nx\nBRIEF";
        let (r, said) = refusal(cmd).expect("refused");
        assert_eq!(r, REASON_BRIEF_SOURCE);
        assert!(said.contains("a live process substitution"), "{said}");
        let (r, said) = refusal("charter handoff report done <(env)").expect("refused");
        assert_eq!(r, REASON_BRIEF_SOURCE);
        assert!(said.contains("process substitution"), "{said}");
        assert!(!said.contains("command substitution"), "{said}");
        let (_, said) = refusal("charter handoff beta <(env) <<'B'\nx\nB").expect("refused");
        assert!(said.contains("a live process substitution"), "{said}");
        // zsh's `<<(…)` is a redirection of one, not a heredoc.
        let (_, said) = refusal("charter handoff report done <<(env)").expect("refused");
        assert!(said.contains("process substitution"), "{said}");
        let (_, said) = refusal("charter handoff beta <<(env) <<'B'\nx\nB").expect("refused");
        assert!(said.contains("a live process substitution"), "{said}");
        // With a blank before the `(` it is a redirection that reads a file, as it was.
        let (_, said) = refusal("charter handoff beta < (env) <<'B'\nx\nB").expect("refused");
        assert!(said.contains("a file (<)"), "{said}");
        // Quoted, it is prose.
        assert_eq!(refusal("charter handoff report 'done <(x)'"), None);
    }

    #[test]
    fn a_handoff_a_shell_runs_is_refused_whichever_way_the_shell_got_it() {
        for cmd in [
            "eval 'charter handoff beta'",
            "bash -c \"charter handoff beta\"",
            "sh -lc 'charter handoff beta'",
            "bash <<'EOF'\ncharter handoff beta <<'BRIEF'\nx\nBRIEF\nEOF",
        ] {
            assert_eq!(reason(cmd), Some(REASON_SHELL_STRING), "{cmd:?}");
        }
    }

    #[test]
    fn text_that_only_mentions_a_handoff_is_data_wherever_it_sits() {
        // M8.2: a chat writing a test, a commit message or a doc holds the words `charter
        // handoff` as TEXT. A heredoc body a reader takes, a quoted argument and an `echo`'s
        // words are none of them a command, and neither is a line that a quoted string began
        // on an earlier line — the shell is still inside that string when the line starts.
        for cmd in [
            "cat > f.txt <<'EOF'\ncharter handoff beta\nEOF",
            "cat > f.txt <<EOF\ncharter handoff beta\nEOF",
            "cat > t.rs <<'EOF'\n    let hand = \"charter handoff beta\";\nEOF",
            "cat > t.sh <<'EOF'\ncharter handoff beta <<'BRIEF'\nship\nBRIEF\nEOF",
            "echo charter handoff beta > f.txt",
            "echo \"charter handoff beta\"",
            "printf '%s\\n' 'charter handoff beta' > f",
            "git commit -m \"$(cat <<'EOF'\nfix the guard\n\ncharter handoff beta now asks\nEOF\n)\"",
            // A quoted string that spans lines: its second line starts inside the quote.
            "echo \"x\ncharter handoff beta\"",
            "git commit -m 'fix the guard\n\ncharter handoff beta now asks first'",
            "python3 -c \"\nimport sys\ncharter handoff beta\n\"",
            "cat > f <<'EOF'\nx\nEOF\necho 'one\ncharter handoff beta'",
            // #488: a quoted heredoc a reader takes inside a substitution whose value is only
            // assigned. Every shell reads the body as `cat`'s stdin, so it is data.
            "x=\"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            // A BALANCED `( … )` in that body ends nothing: bash keeps reading the heredoc.
            "x=\"$(cat <<'EOF'\nfix (a)\ncharter handoff beta\nEOF\n)\"",
        ] {
            assert_eq!(refusal(cmd), None, "{cmd:?} is text, not a handoff");
        }
    }

    /// #488's other side: reading the program inside `"$( … )"` must not hide a handoff. A
    /// body that program RUNS, a program nobody can name, a shell downstream of it, and a
    /// handoff on a line after the heredoc are all still refused.
    #[test]
    fn a_handoff_is_still_caught_around_a_quoted_substitution() {
        // The command AROUND the substitution runs its output too: a runner nobody can name, a
        // remote shell, a shell or `source`, or the substitution standing where the program
        // goes. Any of them runs the heredoc's text, whatever program printed it.
        for cmd in [
            "$SHELL -c \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "$0 -c \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "\"$RUNNER\" -c \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "${X} -c \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "ssh host \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "fish -c \"$(true; cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "busybox sh -c \"$(true; cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "$(cat <<'EOF'\ncharter handoff beta\nEOF\n)",
            "source /dev/stdin <<< \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            ". /dev/stdin <<< \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "\"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            // A substitution that starts its own word, behind an assignment, a wrapper or a
            // redirection, is still where the program goes.
            "FOO=1 $(cat <<'EOF'\ncharter handoff beta\nEOF\n)",
            "x= $(cat <<'EOF'\ncharter handoff beta\nEOF\n)",
            "exec $(cat <<'EOF'\ncharter handoff beta\nEOF\n)",
            "command $(cat <<'EOF'\ncharter handoff beta\nEOF\n)",
            "nice $(cat <<'EOF'\ncharter handoff beta\nEOF\n)",
            "env $(cat <<'EOF'\ncharter handoff beta\nEOF\n)",
            "2>/dev/null $(cat <<'EOF'\ncharter handoff beta\nEOF\n)",
            // A plainly named program that runs its arguments as a command, or hands them to a
            // shell further along its argv.
            "su root -c \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "runuser -u op -- sh -c \"$(true; cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "script -qc \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\" /dev/null",
            "docker exec c sh -c \"$(true; cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "kubectl exec p -- sh -c \"$(true; cat <<'EOF'\ncharter handoff beta\nEOF\n)\"",
            "find . -exec sh -c \"$(true; cat <<'EOF'\ncharter handoff beta\nEOF\n)\" \\;",
        ] {
            assert_eq!(reason(cmd), Some(REASON_SHELL_STRING), "{cmd:?}");
        }
        for (cmd, want) in [
            // #450's two-way reading: GNU bash 3.2.57 ends the substitution at an UNBALANCED `)`
            // in the body and runs what follows. Unquoted, the next line is a command; inside
            // `"…"` a `$( … )` or a backtick still runs, and a `"` ends the string.
            (
                "x=$(cat <<'EOF'\nfix a)\ncharter handoff beta\nEOF\n)",
                REASON_SHELL_STRING,
            ),
            (
                "x=\"$(cat <<'EOF'\nfix a)\n$(charter handoff beta)\nEOF\n)\"",
                REASON_SHELL_STRING,
            ),
            (
                "x=\"$(cat <<'EOF'\nfix a)\n`charter handoff beta`\nEOF\n)\"",
                REASON_SHELL_STRING,
            ),
            (
                "x=\"$(cat <<'EOF'\nfix a)\"\ncharter handoff beta\nEOF\n)\"",
                REASON_SHELL_STRING,
            ),
            (
                "x=\"$(bash <<'EOF'\ncharter handoff beta\nEOF\n)\"",
                REASON_SHELL_STRING,
            ),
            (
                "x=\"$(python3 - <<'EOF'\ncharter handoff beta\nEOF\n)\"",
                REASON_SHELL_STRING,
            ),
            (
                "x=\"$($RUNNER <<'EOF'\ncharter handoff beta\nEOF\n)\"",
                REASON_SHELL_STRING,
            ),
            (
                "x=\"$(cat <<'EOF' | sh\ncharter handoff beta\nEOF\n)\"",
                REASON_SHELL_STRING,
            ),
            (
                "x=\"$(cat <<'EOF'\nhello\nEOF\n)\"\ncharter handoff beta",
                REASON_BRIEF_SOURCE,
            ),
        ] {
            assert_eq!(reason(cmd), Some(want), "{cmd:?}");
        }
        // And a sub-agent is refused for a real one, while the data shape stays data.
        let sub = Caller {
            agent_id: Some("a1"),
            harness: Some("claude-code"),
            permission_mode: None,
        };
        assert_eq!(
            handoff_refusal("x=\"$(bash <<'EOF'\ncharter handoff beta\nEOF\n)\"", sub).map(|r| r.0),
            Some(REASON_SUBAGENT)
        );
        assert_eq!(
            handoff_refusal("x=\"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"", sub),
            None
        );
    }

    #[test]
    fn a_handoff_after_a_quoted_string_closes_is_still_judged() {
        // The other side of the one above: a quote that CLOSES on a line leaves the rest of
        // that line a command, and the lines after it are commands too.
        for (cmd, want) in [
            ("echo \"a\nb\"; charter handoff beta", REASON_BRIEF_SOURCE),
            ("echo \"a\nb\"\ncharter handoff beta", REASON_BRIEF_SOURCE),
            (
                "echo 'a\nb' && charter hando?f beta <<'BRIEF'\nx\nBRIEF",
                REASON_SPELLING,
            ),
            // An apostrophe in a COMMENT opens no quote, so the next line is a command.
            ("# don't forget\ncharter handoff beta", REASON_BRIEF_SOURCE),
            // A substitution inside double quotes RUNS its lines; the lexer holds them in one
            // word, and `quote_map` is what says they are not a string.
            ("echo \"$(\ncharter handoff beta\n)\"", REASON_BRIEF_SOURCE),
            // A backslash-newline is a continuation, not a quote, though the lexer folds it.
            ("true \\\n&& charter 'handoff' beta", REASON_BRIEF_SOURCE),
            ("bash -c \"\ncharter handoff beta\n\"", REASON_SHELL_STRING),
            (
                "echo \"a\nb\"\nbash <<'EOF'\ncharter handoff beta <<'BRIEF'\nx\nBRIEF\nEOF",
                REASON_SHELL_STRING,
            ),
        ] {
            assert_eq!(reason(cmd), Some(want), "{cmd:?}");
        }
        // ...and the canonical handoff after a closed multi-line quote still passes.
        assert_eq!(
            refusal("echo \"a\nb\"\ncharter handoff beta <<'BRIEF'\nship it\nBRIEF"),
            None
        );
    }

    #[test]
    fn a_brief_a_reader_swallows_is_not_searched_for_commands() {
        // `git commit -F -`'s body is a MESSAGE. Reading it as commands is what refused real
        // work before the plan was shared, and it is the leak guard's fact as much as A7's.
        let cmd = "git commit -F - <<'MSG'\ncharter handoff beta\nMSG";
        assert_eq!(reason(cmd), None, "a commit message is not a handoff");
    }

    /// A heredoc opened in a substitution that closes on its own line has no body there, and
    /// the shells disagree about the lines after it: GNU bash 3.2.57 and zsh 5.9 run them as
    /// commands, GNU bash 5.2.15 and 5.3 read them as the body, and with backticks all four run
    /// them. So they are searched as lines a shell runs (#359).
    #[test]
    fn a_heredoc_in_a_substitution_closed_on_its_line_does_not_hide_the_next_lines() {
        for cmd in [
            "x=$( cat <<'EOF' )\ncharter handoff beta\nEOF",
            "x=`cat <<EOF`\ncharter handoff beta\nEOF",
            "echo \"$(cat <<EOF)\"\ncharter handoff beta\nEOF",
        ] {
            assert_eq!(reason(cmd), Some(REASON_SHELL_STRING), "{cmd:?}");
        }
        // `<<$"EOF"` ends at `$EOF` in zsh 5.9, so the handoff after it runs there.
        let cmd = "cat <<$\"EOF\"\n$EOF\ncharter handoff beta\nEOF";
        assert!(reason(cmd).is_some(), "{cmd:?}");
        // Closed on a later line, the body is inside the substitution, as every shell reads it.
        let cmd = "git commit -m \"$(cat <<'EOF'\ncharter handoff beta\nEOF\n)\"";
        assert_eq!(reason(cmd), None, "a commit message is not a handoff");
    }

    #[test]
    fn a_word_that_reads_as_one_of_the_two_behind_an_expansion_is_a_disguise() {
        for cmd in [
            "$'charter' handoff beta <<'BRIEF'\nx\nBRIEF",
            "charter hando?f beta <<'BRIEF'\nx\nBRIEF",
            "charter {handoff,} beta <<'BRIEF'\nx\nBRIEF",
        ] {
            assert!(disguised_handoff(cmd), "{cmd:?} reads as a disguise");
        }
    }

    #[test]
    fn a_sub_agent_is_refused_before_everything_else() {
        let caller = Caller {
            agent_id: Some("sub-1"),
            harness: Some("claude-code"),
            permission_mode: Some(crate::floorguard::UNATTENDED_MODE),
        };
        // Whatever its harness says of its prompts, a sub-agent is refused, and first.
        assert_eq!(
            handoff_refusal(CANONICAL, caller).map(|(r, _)| r),
            Some(REASON_SUBAGENT)
        );
    }

    #[test]
    fn a_harness_nobody_measured_does_not_read_agent_id_as_a_sub_agent() {
        let caller = Caller {
            agent_id: Some("sub-1"),
            harness: Some("opencode"),
            permission_mode: Some("default"),
        };
        // opencode's plugin builds a payload with no such field at all, so a field that
        // arrived anyway means nothing measured. The canonical spelling still passes.
        assert_eq!(handoff_refusal(CANONICAL, caller), None);
        // ...and so does no harness word at all.
        assert_eq!(
            handoff_refusal(
                CANONICAL,
                Caller {
                    harness: None,
                    ..caller
                }
            ),
            None
        );
    }

    #[test]
    fn an_empty_agent_id_is_the_main_conversation() {
        let caller = Caller {
            agent_id: Some(""),
            harness: Some("claude-code"),
            permission_mode: Some("default"),
        };
        assert_eq!(handoff_refusal(CANONICAL, caller), None);
    }

    /// Whether anybody answers a chat's prompts is the app's to weigh, from its own mark on
    /// the chat, where it decides the handoff (#1444, V98i). The harness's prompt consents to
    /// nothing, so this guard no longer refuses a run for having it switched off; Codex says
    /// so of every chat.
    #[test]
    fn an_unattended_run_is_not_this_guards_to_refuse() {
        for harness in ["claude-code", "codex"] {
            let caller = Caller {
                agent_id: None,
                harness: Some(harness),
                permission_mode: Some(crate::floorguard::UNATTENDED_MODE),
            };
            assert_eq!(handoff_refusal(CANONICAL, caller), None, "{harness}");
            // And everything it still refuses, it refuses there too.
            assert_eq!(
                handoff_refusal("eval 'charter handoff beta'", caller).map(|(r, _)| r),
                Some(REASON_SHELL_STRING),
                "{harness}"
            );
            assert_eq!(
                handoff_refusal("charter handoff beta < brief.txt", caller).map(|(r, _)| r),
                Some(REASON_BRIEF_SOURCE),
                "{harness}"
            );
            let helper = Caller {
                agent_id: Some("sub-1"),
                ..caller
            };
            assert_eq!(
                handoff_refusal(CANONICAL, helper).map(|(r, _)| r),
                Some(REASON_SUBAGENT),
                "{harness}"
            );
        }
    }

    #[test]
    fn a_quote_left_open_on_the_handoff_line_is_refused_rather_than_guessed() {
        // `_handoff_line` found it through `as_the_shell_reads`, and then the line will not
        // lex, so which heredoc feeds it cannot be read off it.
        let cmd = "charter handoff beta 'unclosed";
        let (r, said) = refusal(cmd).expect("refused");
        assert_eq!(r, REASON_BRIEF_SOURCE);
        assert!(said.contains("left open on its line"), "{said}");
    }

    #[test]
    fn py_slice_reads_a_negative_offset_the_way_python_does() {
        // **The only thing that exercises the negative branch** — no token A7 is handed carries
        // a `-1` offset, and [`py_slice`] says why at length. Kept as a unit test rather than
        // left to the differential for that reason: the fuzz cannot reach it either.
        let chars: Vec<char> = "abcde".chars().collect();
        assert_eq!(py_slice(&chars, 1, 3), "bc");
        assert_eq!(py_slice(&chars, -1, -1), "");
        assert_eq!(py_slice(&chars, -2, 5), "de");
        assert_eq!(py_slice(&chars, 3, 1), "");
        assert_eq!(py_slice(&chars, 0, 99), "abcde");
    }

    #[test]
    fn as_the_shell_reads_folds_every_python_space_but_the_newline() {
        // U+00A0 and U+001C are `str.isspace()` and `char::is_whitespace` disagrees on the
        // second, which is the whole reason this does not use Rust's predicate.
        assert_eq!(
            as_the_shell_reads("charter\u{a0}handoff"),
            "charter handoff"
        );
        assert_eq!(
            as_the_shell_reads("charter\u{1c}handoff"),
            "charter handoff"
        );
        assert_eq!(as_the_shell_reads("a\\\nb"), "ab");
        assert_eq!(as_the_shell_reads("a\nb"), "a\nb");
    }

    /// A handoff in a shell's `-c` string is found past the shell's options, whatever they are.
    #[test]
    fn a_handoff_in_a_shell_string_is_found_past_the_shells_options() {
        for cmd in [
            "bash -c 'charter handoff beta <<BRIEF\nx\nBRIEF'",
            "bash -euo pipefail -c 'charter handoff beta <<BRIEF\nx\nBRIEF'",
            "bash --rcfile rc -c 'charter handoff beta <<BRIEF\nx\nBRIEF'",
            "bash -c -- 'charter handoff beta <<BRIEF\nx\nBRIEF'",
            "bash -co pipefail 'charter handoff beta <<BRIEF\nx\nBRIEF'",
            "zsh --emulate sh -c 'charter handoff beta <<BRIEF\nx\nBRIEF'",
            "bash --some-new-option value -c 'charter handoff beta <<BRIEF\nx\nBRIEF'",
        ] {
            assert!(shell_string_handoff(cmd), "{cmd}");
        }
    }

    #[test]
    fn disguised_as_needs_a_mark_and_then_a_shape() {
        assert!(!disguised_as("handoff", "handoff"), "no mark, no disguise");
        assert!(disguised_as("$'handoff'", "handoff"));
        assert!(disguised_as("ha$''ndoff", "handoff"));
        assert!(disguised_as("${x:-handoff}", "handoff"));
        assert!(disguised_as("{handoff,}", "handoff"));
        assert!(disguised_as("hando?f", "handoff"), "a glob is read as one");
        assert!(!disguised_as("$h", "handoff"), "a variable is not seen");
    }

    // ----- a report back (charter-app#259) ------------------------------------------------

    #[test]
    fn a_report_back_carries_its_summary_as_an_argument_and_needs_no_heredoc() {
        assert_eq!(
            refusal("charter handoff report \"Dropped it. Two repos changed.\""),
            None
        );
        assert_eq!(refusal("charter handoff report 'done'"), None);
    }

    #[test]
    fn a_report_back_the_shell_would_rewrite_is_refused() {
        assert_eq!(
            reason("charter handoff report \"$(cat notes.md)\""),
            Some(REASON_BRIEF_SOURCE)
        );
    }

    #[test]
    fn a_report_back_is_read_under_every_spelling_and_still_asked_only_of_the_chat_itself() {
        assert_eq!(refusal("charter 'handoff' report \"done\""), None);
        assert_eq!(refusal("FOO=1 purlis handoff report \"done\""), None);
        // Still a report, so still no brief to ask for, and still no live substitution.
        assert_eq!(
            reason("FOO=1 purlis handoff report \"$(cat notes.md)\""),
            Some(REASON_BRIEF_SOURCE)
        );
        let subagent = Caller {
            agent_id: Some("a1"),
            ..attended()
        };
        assert_eq!(
            handoff_refusal("charter handoff report \"done\"", subagent).map(|(r, _)| r),
            Some(REASON_SUBAGENT)
        );
    }

    /// The report is the word after the `handoff` the reader found, not after the first
    /// token that happens to be spelt so: a wrapper's own argument is not the command.
    #[test]
    fn a_report_back_is_found_after_the_handoff_the_reader_read_and_no_other_word() {
        // `sudo -u handoff`: the user is called `handoff`, and the report is real.
        assert_eq!(
            refusal("sudo -u handoff purlis handoff report \"done\""),
            None
        );
        // The same wrapper in front of a handoff that is no report: its brief is judged.
        assert_eq!(
            reason("cat f | sudo -u handoff purlis handoff report-it"),
            Some(REASON_BRIEF_SOURCE)
        );
        assert_eq!(
            reason("cat f | env X=1 purlis handoff beta handoff report now"),
            Some(REASON_BRIEF_SOURCE)
        );
    }

    /// What the sentence says is refused is refused, and what it does not name is read:
    /// an ANSI-C word is the word the shell makes of it.
    #[test]
    fn the_spelling_sentence_names_only_shapes_that_are_refused() {
        assert!(!HANDOFF_SPELLING.contains("$'"), "{HANDOFF_SPELLING}");
        for refused in ["a brace", "a glob", "a parameter", "a command substitution"] {
            assert!(HANDOFF_SPELLING.contains(refused), "{refused}");
        }
        assert_eq!(refusal("charter $'handoff' beta <<'BRIEF'\nx\nBRIEF"), None);
    }

    #[test]
    fn a_handoff_into_a_workspace_called_report_is_still_judged_as_a_handoff() {
        assert_eq!(
            refusal("charter handoff report <<'BRIEF'\nship it\nBRIEF"),
            None
        );
        assert_eq!(
            reason("charter handoff report < brief.txt"),
            Some(REASON_BRIEF_SOURCE)
        );
        assert_eq!(reason("charter handoff report"), Some(REASON_BRIEF_SOURCE));
    }

    /// A row a quoted string covers from end to end is data and is stepped over, and the
    /// search goes on to the rows after it (#311): the handoff two rows below a message is
    /// found, and it is found on the row it is on.
    #[test]
    fn a_row_wholly_inside_a_string_is_skipped_and_the_rows_after_it_are_still_read() {
        let cmd = "git commit -m 'first\nsecond\nthird'\ncharter handoff beta < brief.txt";
        assert_eq!(
            handoff_line(cmd),
            Some(("charter handoff beta < brief.txt".to_owned(), false))
        );
        assert_eq!(reason(cmd), Some(REASON_BRIEF_SOURCE));
        // …and a message that only MENTIONS one, on a row of its own, is a message.
        assert_eq!(
            handoff_line("git commit -m 'first\ncharter handoff beta\nthird'"),
            None
        );
    }

    /// A continuation joins the next row to this one, from wherever this one is.
    #[test]
    fn a_continuation_after_the_first_row_joins_the_row_after_it() {
        let cmd = "git status\ncharter \\\nhandoff beta < brief.txt";
        assert_eq!(
            handoff_line(cmd),
            Some(("charter \\\nhandoff beta < brief.txt".to_owned(), false))
        );
    }

    /// An apostrophe in a `#` comment opens a quote to `quote_map` and no token to the lexer,
    /// so the row after it has no quoted head — even when that row BEGINS with a quoted word.
    /// Reading the quoted word as the head would cut `'charter'` off the handoff (#311).
    #[test]
    fn a_quoted_word_that_starts_a_row_is_not_a_string_an_earlier_row_opened() {
        let cmd = "# it's a comment\n'charter' handoff beta < brief.txt";
        assert_eq!(
            handoff_line(cmd),
            Some(("'charter' handoff beta < brief.txt".to_owned(), false))
        );
        assert!(
            reason(cmd).is_some(),
            "the handoff after the comment was missed"
        );
    }
    /// A shell named in capitals runs its `-c` string on a filesystem that folds case, so it is
    /// looked into like the lower-case one. `eval` is a builtin with no file behind it, so
    /// `EVAL` runs nothing and is not.
    #[test]
    fn a_shell_named_in_capitals_is_still_the_shell() {
        for cmd in [
            "BASH -c 'charter handoff beta'",
            "Bash -lc 'charter handoff beta'",
            "/bin/SH -c 'charter handoff beta'",
            "env ZSH -c 'charter handoff beta'",
        ] {
            assert_eq!(reason(cmd), Some(REASON_SHELL_STRING), "{cmd:?}");
        }
        let toks =
            shellseg::split_punctuation(shellseg::lex("EVAL 'charter handoff beta'").unwrap());
        assert_eq!(shell_string(&toks), None);
    }

    /// A word written with ANSI-C escapes is the word the shell makes, so `$'\x68'andoff` is
    /// `handoff`: this guard reads it as the handoff it is, and judges its brief.
    #[test]
    fn an_ansi_c_spelling_of_a_handoff_is_read_as_the_handoff_it_is() {
        for cmd in [
            "charter $'\\x68'andoff beta",
            "$'\\x63harter' handoff beta",
            "charter $'\\150andoff' beta",
        ] {
            assert_eq!(reason(cmd), Some(REASON_BRIEF_SOURCE), "{cmd:?}");
            let fed = format!("{cmd} <<'BRIEF'\nx\nBRIEF");
            assert_eq!(reason(&fed), None, "{fed:?}");
        }
        assert_eq!(
            reason("bash -c $'charter \\x68andoff beta'"),
            Some(REASON_SHELL_STRING)
        );
    }

    /// A backslash-newline inside `charter` or `handoff` is gone before the shell reads the
    /// word, so the word is the program, and the handoff is read as one.
    #[test]
    fn a_backslash_newline_inside_a_word_is_still_the_word() {
        for cmd in ["char\\\nter handoff beta", "charter hand\\\noff beta"] {
            assert_eq!(reason(cmd), Some(REASON_BRIEF_SOURCE), "{cmd:?}");
            let fed = format!("{cmd} <<'BRIEF'\nx\nBRIEF");
            assert_eq!(reason(&fed), None, "{fed:?}");
        }
    }

    /// `<<B\<newline>RIEF` is the unquoted delimiter `BRIEF` to the shell, whose body expands.
    #[test]
    fn a_delimiter_split_by_a_backslash_newline_is_unquoted() {
        let cmd = "charter handoff beta <<BR\\\nIEF\n$(x)\nBRIEF";
        assert_eq!(reason(cmd), Some(REASON_BRIEF_SOURCE));
    }
}
