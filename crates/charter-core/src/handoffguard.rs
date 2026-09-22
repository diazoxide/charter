//! Arm A7: a handoff waits for a yes the operator can see (#1014, ADR 0014).
//!
//! A port of `charter/hooks.py`'s `_handoff_refusal` and the five readers under it —
//! `_disguised_as`, `_disguised_handoff`, `_shell_string`, `_as_the_shell_reads`,
//! `_shell_string_handoff`, `_handoff_segment`, `_handoff_line` — plus `_runs_handoff` and
//! `_is_handoff`, which the Python shares with the routing-mark clear.
//!
//! A handoff's brief becomes a new chat's first message, and a first message runs with the
//! operator's authority. **The consent is the harness's own permission prompt** — the `ask`
//! rule for `charter handoff *` that `charter init` writes, because a pattern belongs to the
//! host (ADR 0014). Each refusal here covers something that prompt cannot see: who is asking,
//! whether anybody is there to answer, a spelling the rule does not match, and a stdin the
//! prompt would not show.
//!
//! # Why the hook and not the command
//!
//! [`crate::handoff`]'s module header carries the table that divides them, and marks these
//! four rows as A7's. Each needs a fact no command can observe — the **source spelling** of
//! the command line, and the harness's own hook payload (`agent_id`, `permission_mode`) — so
//! a second permission rule could not express them and the command cannot reach them. By the
//! time `charter handoff` is running, the spelling that got it there is gone.
//!
//! # What A7 is, and is not
//!
//! It keeps a good-faith chat's permission prompt in front of its handoff by refusing the
//! spellings it can recognise. **It reads a command's words and is not a shell**: an
//! interpreter (`python3 -c`), a variable, a script file, a heredoc fed to a shell and an
//! expansion that does not leave a word whole (`{hand,}off`, `$'\x68andoff'`) all run a
//! handoff it never sees. Claude Code says the same of its own rule — "isn't a security
//! boundary around the program"
//! (<https://code.claude.com/docs/en/permissions.md>, *What a Bash rule doesn't match*).
//!
//! # The shared heredoc plan, and the trap under it
//!
//! Which heredoc bodies a shell RUNS comes from [`crate::leakguard::lines_a_command_could_run`],
//! and so from the one plan the leak guard uses ([`crate::heredoc::heredoc_strip_plan`]). A
//! body fed to a reader — `charter handoff`'s own brief, `git commit -F -` — is data and is not
//! searched; a body fed to a shell is searched and flagged, because the host's rule only ever
//! saw the `bash` that opened it.
//!
//! **An unknown plan has to fail the same way for both**, which is
//! [`crate::heredoc::heredoc_could_run`]'s whole subject. The leak guard's unknown is "keep
//! every body visible", which still denies. A7's *used to be* "drop every body", which is the
//! opposite — and a canonical handoff inside `bash <<'EOF'` then ran with no prompt in every
//! shape that loses the plan. The two now share one answer and this module adds no default of
//! its own; `heredoc.rs` is `pub` at that boundary for exactly this caller.
//!
//! # Nothing about this module is gated here
//!
//! A7 is PLANE-GATED in `pretooluse` — unlike A5 and A6 beside it, which refuse a fact about
//! the shell. A handoff opens a chat in one of this plane's workspaces, so outside a plane
//! there is no such chat to consent to. The gate is [`crate::toolgate`]'s, one level up, for
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

/// The programs that run a STRING as shell: `eval` runs its words, and these run `-c`'s.
/// `_STRING_SHELLS`.
pub const STRING_SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "ksh"];

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
// Spelled once, so a denial reads the same wherever it is quoted. Every one is
// `charter/hooks.py`'s constant verbatim, and the differential compares them as data as well
// as through the verdict, because a paraphrase here is a denial two charters word differently.

/// `_HANDOFF_SUBAGENT`.
pub const HANDOFF_SUBAGENT: &str = "`charter handoff` is refused from inside a sub-agent. A brief becomes a new chat's first \
     message and runs with the operator's authority, so only the chat the operator is talking \
     to may propose one — and whatever this sub-agent found goes back to that chat anyway. \
     Return it to the parent chat, and let the parent propose the handoff.";

/// `_HANDOFF_UNATTENDED`.
pub const HANDOFF_UNATTENDED: &str = "`charter handoff` is refused in an unattended run (`permission_mode: bypassPermissions`). \
     A handoff opens a chat that starts working on its brief with the operator's authority, \
     and its consent is a permission prompt nobody is here to answer. Record the work \
     instead — charter ws todo --workspace <workspace> \"<what>\" — and hand it off from a \
     chat someone is attending.";

/// `_HANDOFF_SPELLING`.
pub const HANDOFF_SPELLING: &str = "`charter handoff` must be spelled exactly that, at the start of its command: the two \
     words unquoted, unescaped and unexpanded, one space apart, and one space before what \
     follows. The permission rule that asks the operator first is `Bash(charter handoff *)`, \
     and a spelling it does not match gets no prompt — on Claude Code 2.1.268, \
     `python3 -m charter handoff`, a path to charter, `charter 'handoff'` and \
     `charter $'handoff'` all ran with none. This guard refuses the spellings of a handoff it \
     can recognise; it reads a command's words and is not a shell. Spell it exactly \
     `charter handoff <workspace> <<'BRIEF'`.";

/// `_HANDOFF_SHELL_STRING`.
pub const HANDOFF_SHELL_STRING: &str = "`charter handoff` is refused inside a string or a heredoc a shell runs (`eval`, \
     `bash -c '…'`, `bash <<'EOF'`). The permission rule that asks the operator first is \
     `Bash(charter handoff *)`, and it reads the outer command — on Claude Code 2.1.268 a \
     handoff inside `eval` or `bash -c` ran with no prompt. This guard looks one level in and \
     no deeper. Run it directly instead, spelled exactly \
     `charter handoff <workspace> <<'BRIEF'`.";

/// `_HANDOFF_SOURCE`, with `{what}` still to fill — [`handoff_source`] fills it.
pub const HANDOFF_SOURCE: &str = "`charter handoff` takes its brief from a QUOTED heredoc in the same call — <<'BRIEF' — \
     so the permission prompt shows exactly the text the new chat is sent. This call feeds it \
     {what}, which the shell would change or hide before charter reads it. Write: \
     charter handoff <workspace> <<'BRIEF' … BRIEF";

/// [`HANDOFF_SOURCE`] with `{what}` filled — Python's `_HANDOFF_SOURCE.format(what=…)`.
pub fn handoff_source(what: &str) -> String {
    HANDOFF_SOURCE.replace("{what}", what)
}

/// The trace reason each refusal is tallied under. The words are the Python's, and they are
/// the stable key a tally reader already has.
pub const REASON_SUBAGENT: &str = "handoff-subagent";
/// See [`REASON_SUBAGENT`].
pub const REASON_UNATTENDED: &str = "handoff-unattended";
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

/// `line.startswith(prefix, start)` as CPython reads it: a CHARACTER offset, negative from the
/// end, clamped at zero, and `false` once the offset is past the end of a non-empty prefix.
fn py_starts_with_at(chars: &[char], prefix: &str, start: isize) -> bool {
    let n = chars.len() as isize;
    let at = if start < 0 { start + n } else { start };
    let at = at.clamp(0, n) as usize;
    let want: Vec<char> = prefix.chars().collect();
    chars.len() >= at + want.len() && chars[at..at + want.len()] == want[..]
}

// ----------------------------------------------------------------------------------------
// is a handoff about to run
// ----------------------------------------------------------------------------------------

/// Whether this ONE segment runs `charter handoff`, in any spelling charter recognises — the
/// plain one, `python3 -m charter`, a path to charter, behind a prefix or a wrapper.
/// `_runs_handoff`.
///
/// **Every spelling, on purpose.** Which spellings the harness's permission prompt covers is a
/// separate question with a measured answer, and [`handoff_refusal`] asks it; this answers only
/// "is a handoff about to run", which is what a guard has to know before it can say anything
/// about one.
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
        if (first.as_str(), second.as_str()) != ("charter", "handoff")
            && (first == "charter" || disguised_as(&first, "charter"))
            && (second == "handoff" || disguised_as(&second, "handoff"))
        {
            return true;
        }
    }
    false
}

// ----------------------------------------------------------------------------------------
// a string a shell runs
// ----------------------------------------------------------------------------------------

/// The text a segment hands a shell to run — `eval`'s words, or the argument of a shell's `-c`,
/// alone or in a cluster such as `-lc` — or `None`. `_shell_string`.
///
/// `os.path.basename` and **not** `base_lower`: the Python compares the base as written, so
/// `BASH -c` is not a shell here. Kept as measured rather than tidied.
pub fn shell_string(seg: &[Tok]) -> Option<String> {
    let toks: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
    let (prog, _env, argv) = shellwrap::split_env(&toks);
    let base = shellwrap::basename(&prog);
    if base == "eval" {
        return Some(argv.iter().skip(1).cloned().collect::<Vec<_>>().join(" "));
    }
    if !STRING_SHELLS.contains(&base) {
        return None;
    }
    // Python's `enumerate(argv[1:-1], start=1)`: the LAST word can never be the flag, because
    // the string it introduces has to come after it.
    if argv.len() < 2 {
        return None;
    }
    for (k, arg) in argv[1..argv.len() - 1].iter().enumerate() {
        let k = k + 1;
        if is_dash_c_cluster(arg) {
            return Some(argv[k + 1].clone());
        }
    }
    None
}

/// `re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", arg)` — a `-c` alone or inside a letter cluster.
///
/// Hand-written rather than compiled: the class is ASCII by construction on both sides, so
/// there is no `\w`/`\d` disagreement to measure and a regex would only hide the shape.
fn is_dash_c_cluster(arg: &str) -> bool {
    let mut chars = arg.chars();
    if chars.next() != Some('-') {
        return false;
    }
    let rest: Vec<char> = chars.collect();
    rest.iter().all(|c| c.is_ascii_alphabetic()) && rest.contains(&'c')
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
        shell_string(seg).is_some_and(|inner| {
            is_handoff(&as_the_shell_reads(&inner)) || disguised_handoff(&inner)
        })
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
pub fn handoff_line(cmd: &str) -> Option<(String, bool)> {
    let rows = leakguard::lines_a_command_could_run(cmd);
    let mut i = 0usize;
    while i < rows.len() {
        let (mut line, in_a_shells_body) = rows[i].clone();
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

/// How many `\` a line ends with — `len(line) - len(line.rstrip("\\"))`.
fn trailing_backslashes(line: &str) -> usize {
    line.chars().rev().take_while(|c| *c == '\\').count()
}

// ----------------------------------------------------------------------------------------
// the verdict
// ----------------------------------------------------------------------------------------

/// What the hook knows about a tool call that no command can observe. `_handoff_refusal`'s
/// `data`, narrowed to the three fields it reads.
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
    /// The payload's `permission_mode` — [`crate::floorguard::unattended`]'s input.
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

/// `(trace reason, denial)` for a `charter handoff` the operator's permission prompt cannot
/// stand in front of — or `None`. `_handoff_refusal`.
///
/// **The first handoff INVOCATION line is the one judged** ([`handoff_line`]).
///
/// The refusals, in the order the Python asks them:
///
/// 1. `handoff-subagent` — the payload carries `agent_id` on a harness where that is measured
///    to mean a sub-agent ([`Caller::from_a_subagent`]).
/// 2. `handoff-unattended` — `permission_mode: bypassPermissions`.
/// 3. `handoff-shell-string` — a handoff inside a string a shell runs, one level deep
///    ([`shell_string_handoff`]), or inside a heredoc body a shell runs; the host's rule reads
///    only the outer command.
/// 4. `handoff-spelling` — a spelling of a handoff this guard can recognise that is not
///    `charter handoff` as the SOURCE spells it: two bare words (no quote or escape in either),
///    one ASCII space apart and one before whatever follows — **a backslash-newline there is
///    not that space** — and neither word one that reads `charter` or `handoff` behind quoting,
///    expansion or glob characters.
/// 5. `handoff-brief-source` — stdin that is not exactly one quoted heredoc on the handoff's
///    own segment, or a live substitution anywhere in the call
///    ([`livesub::live_substitution`], scoped to the WHOLE call like A5 and A6, for their
///    reason).
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
    if crate::floorguard::unattended(caller.permission_mode) {
        return Some((REASON_UNATTENDED, HANDOFF_UNATTENDED.to_string()));
    }
    // A handoff a shell runs — from a `-c` string or from a heredoc body it executes — is one
    // the host's rule never sees, because the command it matches is `bash`.
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
    let (seg, piped) = handoff_segment(&toks);
    if !spelled_exactly(&line, seg.as_deref()) {
        return Some((REASON_SPELLING, HANDOFF_SPELLING.to_string()));
    }
    let seg = seg.expect("`spelled_exactly` answers false for no segment");
    let what = brief_source(cmd, &seg, piped)?;
    Some((REASON_BRIEF_SOURCE, handoff_source(what)))
}

/// Whether the handoff on `line` is spelled the way the host's rule matches: the SOURCE, not
/// the words a shell makes of it.
///
/// `'charter' handoff`, `\charter handoff` and `charter  handoff` all lex to the same two texts
/// as the exact form. `bare` says no quote or escape touched either word; the offsets say what
/// stood between them.
fn spelled_exactly(line: &str, seg: Option<&[Tok]>) -> bool {
    let Some(seg) = seg else {
        return false;
    };
    // `_runs_handoff` cannot be true of a segment with fewer than two tokens — `_charter_words`
    // needs an argv of at least two, and `_split_env` only ever strips a PREFIX — so Python
    // indexes `seg[0]`/`seg[1]` unguarded and would raise `IndexError` if one ever arrived.
    //
    // **Measured as well as argued**, because "the oracle would crash" is a claim worth more
    // than a deduction: 200,529 cases through the differential's own generators produced 10,814
    // handoff segments, none shorter than two tokens and none raising out of
    // `_handoff_refusal`. Answering "not the exact spelling" for a shorter one is the same
    // verdict as the Python's for every input either implementation has been shown, without a
    // panic where the guard is the only thing standing in front of a tool call.
    let (Some(first), Some(second)) = (seg.first(), seg.get(1)) else {
        return false;
    };
    let chars: Vec<char> = line.chars().collect();
    if !(first.bare && second.bare) || first.text != "charter" || second.text != "handoff" {
        return false;
    }
    if py_slice(&chars, first.end, second.start) != " " {
        return false;
    }
    if let Some(third) = seg.get(2) {
        // A backslash-newline after `handoff` is not that one space, and the TOKENS do not say
        // so: the lexer folds the pair into the word that follows, leaving a gap that reads as
        // a single space. The source is where it shows.
        if py_slice(&chars, second.end, third.start) != " "
            || py_starts_with_at(&chars, "\\\n", third.start)
        {
            return false;
        }
    }
    !disguised_handoff(line)
}

/// What this call feeds the brief, for [`handoff_source`] — or `None`, which is the one way a
/// handoff passes A7.
///
/// The order is the Python's `if`/`elif` chain and it is a precedence: a here-string that is
/// also piped is named as a here-string, because that is the thing to fix first.
fn brief_source(cmd: &str, seg: &[Tok], piped: bool) -> Option<&'static str> {
    let heredocs: Vec<usize> = seg
        .iter()
        .enumerate()
        .filter(|(_, t)| t.is_op(&["<<"]))
        .map(|(i, _)| i)
        .collect();
    if seg.iter().any(|t| t.is_op(&["<<<"])) {
        return Some("a here-string (<<<)");
    }
    if seg.iter().any(|t| t.is_op(&REDIRECT_READS)) {
        return Some("a file (<), which the prompt shows as a path rather than as the brief");
    }
    if piped {
        return Some("a pipe");
    }
    let Some(&first) = heredocs.first() else {
        return Some("no heredoc at all");
    };
    if heredocs.len() > 1 {
        // bash reads every body and hands the command only the last one (GNU bash 3.2.57).
        return Some("more than one heredoc, and the shell sends charter only the last");
    }
    // Python's `all(t.bare for t in seg[i+1:i+2])`: a one-or-zero-element slice, so a `<<` that
    // is the segment's LAST token has no delimiter and `all` of nothing is true — an opener
    // with nothing after it reads as unquoted.
    if seg.get(first + 1).is_none_or(|t| t.bare) {
        return Some("an unquoted heredoc, which expands $… and `…` in the brief");
    }
    if livesub::live_substitution(cmd).is_some() {
        return Some("a live command substitution");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape the host's rule matches, and the only one A7 lets through.
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

    #[test]
    fn a_spelling_the_hosts_rule_does_not_match_is_refused() {
        for cmd in [
            "python3 -m charter handoff beta <<'BRIEF'\nx\nBRIEF",
            "/usr/local/bin/charter handoff beta <<'BRIEF'\nx\nBRIEF",
            "charter 'handoff' beta <<'BRIEF'\nx\nBRIEF",
            "charter $'handoff' beta <<'BRIEF'\nx\nBRIEF",
            "'charter' handoff beta <<'BRIEF'\nx\nBRIEF",
            "\\charter handoff beta <<'BRIEF'\nx\nBRIEF",
            "charter  handoff beta <<'BRIEF'\nx\nBRIEF",
            "CHARTER_ROOT=/x charter handoff beta <<'BRIEF'\nx\nBRIEF",
        ] {
            assert_eq!(reason(cmd), Some(REASON_SPELLING), "{cmd:?}");
        }
    }

    #[test]
    fn a_backslash_newline_is_not_the_one_space_after_handoff() {
        // The lexer folds the pair into the word that follows, so the tokens read as a single
        // space and only the source tells them apart.
        let cmd = "charter handoff \\\nbeta <<'BRIEF'\nx\nBRIEF";
        assert_eq!(reason(cmd), Some(REASON_SPELLING));
    }

    #[test]
    fn a_brief_the_shell_would_change_is_refused_and_the_denial_says_which() {
        for (cmd, what) in [
            ("charter handoff beta <<<'x'", "a here-string (<<<)"),
            (
                "charter handoff beta < brief.txt",
                "a file (<), which the prompt shows as a path rather than as the brief",
            ),
            ("cat brief.txt | charter handoff beta", "a pipe"),
            ("charter handoff beta", "no heredoc at all"),
            (
                "charter handoff beta <<'A' <<'B'\nx\nA\ny\nB",
                "more than one heredoc, and the shell sends charter only the last",
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
        // The spelling is exact and the heredoc is quoted; what is left is the substitution,
        // and it is looked for in the WHOLE call, exactly as A5 and A6 look for one.
        let (r, said) = refusal(cmd).expect("refused");
        assert_eq!(r, REASON_BRIEF_SOURCE);
        assert!(said.contains("a live command substitution"), "{said}");
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
    fn a_brief_a_reader_swallows_is_not_searched_for_commands() {
        // `git commit -F -`'s body is a MESSAGE. Reading it as commands is what refused real
        // work before the plan was shared, and it is the leak guard's fact as much as A7's.
        let cmd = "git commit -F - <<'MSG'\ncharter handoff beta\nMSG";
        assert_eq!(reason(cmd), None, "a commit message is not a handoff");
    }

    #[test]
    fn a_disguised_first_or_second_word_is_a_spelling_refusal() {
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
        // Both gates are open; the sub-agent one is asked first, and the Python's order is
        // what decides which sentence the chat reads.
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

    #[test]
    fn an_unattended_run_may_not_hand_off() {
        let caller = Caller {
            agent_id: None,
            harness: Some("claude-code"),
            permission_mode: Some(crate::floorguard::UNATTENDED_MODE),
        };
        assert_eq!(
            handoff_refusal(CANONICAL, caller).map(|(r, _)| r),
            Some(REASON_UNATTENDED)
        );
    }

    #[test]
    fn unattended_is_asked_before_the_shell_string() {
        let caller = Caller {
            agent_id: None,
            harness: Some("claude-code"),
            permission_mode: Some(crate::floorguard::UNATTENDED_MODE),
        };
        assert_eq!(
            handoff_refusal("eval 'charter handoff beta'", caller).map(|(r, _)| r),
            Some(REASON_UNATTENDED)
        );
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

    #[test]
    fn a_dash_c_cluster_is_a_shells_string_flag() {
        assert!(is_dash_c_cluster("-c"));
        assert!(is_dash_c_cluster("-lc"));
        assert!(is_dash_c_cluster("-cx"));
        assert!(!is_dash_c_cluster("-l"));
        assert!(!is_dash_c_cluster("c"));
        assert!(!is_dash_c_cluster("-c1"));
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
}
