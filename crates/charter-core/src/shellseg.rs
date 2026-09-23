//! A shell command line as **argv per separately-executed command** — the substrate every
//! `PreToolUse` Bash refusal stands on.
//!
//! This is a port of `charter/hooks.py`'s `_segment_argv_parsed` and its whole transitive
//! closure: `_unbacktick`, `_ShellLexer` (a subclass of CPython's `shlex.shlex`),
//! `_NewlineKeepingStream`, `_lex`, `_split_punctuation`, `_segment_tokens`, `_resegment`,
//! `_quote_map`, `_splice_continuations` and `_fallback_segments`. Twenty definitions and
//! 565 lines of Python, measured by an AST walk over the frozen oracle.
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
//! # Why this is the piece to get exactly right
//!
//! Every refusal above it decides on `prog` and `args` taken from these segments. A boundary
//! this module invents strands a reader's operand — `cat` in one segment and the vault path in
//! another, neither of which is a read — and the guard goes from deny to **allow**. The Python
//! docstrings record four separate rounds where exactly that shipped: a quoted `)`, the `&` of
//! `2>&1`, a `{` read as a reserved word mid-command, and `str.split` erasing the newlines that
//! separate the lines of a multi-line command. Each of those is a rule here, and each has a
//! test that fails when the rule is removed.
//!
//! # The answers are CPython's, not "a POSIX shell's"
//!
//! The oracle is `shlex.shlex(posix=True, punctuation_chars="();<>|&\n")` with
//! `whitespace_split=True` and `whitespace=" \t\r"`, driven through a stream whose `readline`
//! stops *before* the newline. That state machine — not the POSIX grammar — is what the Python
//! charter refuses on, so this reproduces the state machine. Three consequences are worth
//! naming because they are not what a shell does:
//!
//! - **A run of punctuation is one token.** `);` and `)&&` and `<(` come back glued, and
//!   [`split_punctuation`] breaks them apart with the same longest-first table the fallback
//!   path uses, so the two paths cannot come to disagree about what an operator is.
//! - **Offsets are CHARACTER offsets**, because CPython indexes a `str` by character. A7 reads
//!   [`Tok::start`] and [`Tok::end`] to judge the *spelling* a command was written with, so the
//!   whole module works over `Vec<char>` rather than bytes.
//! - **`str.strip()` and `str.split()` are Python's**, which count U+001C–U+001F as whitespace
//!   where Rust's `char::is_whitespace` does not. [`crate::memstore::is_python_space`] already
//!   existed for this and is reused rather than re-derived.
//!
//! # The evidence
//!
//! The differential harness ran the frozen Python and put the same question to this module, six
//! answers at a time, over 200,000 generated command lines. Its answers are recorded
//! (`fixtures/corpora/shellseg-oracle.jsonl`, and `shellseg-generated.jsonl.gz`, a subset of the
//! generated cases chosen to reach every branch the full run reached — ADR 0046), and
//! `tests/the_shell_is_read_the_way_python_reads_it.rs` replays them with no Python present.
//! Nothing in either is written by hand: a rule this module gets wrong changes an answer the
//! Python already gave.
//!
//! # What stage 2 added here
//!
//! [`Quoting`] (the memoisation the oracle gets from `lru_cache`), [`posix_split`] and
//! [`shell_quote`] (CPython's `shlex.split`/`shlex.quote`, which `_split_env_chdir` and
//! `_desugar_ansi_c` call), and [`Tok::is_control_op`]/[`Tok::is_grouping`]. Everything else in
//! this file is stage 1 unchanged: the lexer now reads its three tables off a `Cfg` so the two
//! `shlex` configurations the oracle uses are one state machine rather than two.

use crate::memstore::is_python_space;

/// The characters [`Lexer`] treats as punctuation, and which it emits as a glued RUN.
///
/// `\n` is here, and NOT in [`WHITESPACE`], because a newline separates two commands exactly
/// as `;` does. `shlex`'s default whitespace swallows it, and a multi-line Bash call — which is
/// most of them — then collapses into a single segment whose program is the first line's.
const PUNCTUATION_CHARS: &str = "();<>|&\n";

/// What the lexer drops between tokens. `\n` is deliberately absent; see [`PUNCTUATION_CHARS`].
const WHITESPACE: &str = " \t\r";

const QUOTES: &str = "'\"";
const ESCAPE: char = '\\';
/// The quotes inside which a backslash still escapes — `shlex.escapedquotes`.
const ESCAPED_QUOTES: &str = "\"";

/// `shlex`'s `wordchars` under `posix=True` with our `punctuation_chars`, reproduced exactly.
///
/// **It is inert here and is kept anyway.** Under `whitespace_split = True` both arms that
/// consult it reach the same state as the arm that would otherwise catch the character: in the
/// whitespace state a non-wordchar that is not punctuation, a quote or an escape falls to
/// `elif self.whitespace_split`, which sets the identical `token`/`state`; in the word state the
/// disjunct `whitespace_split and nextchar not in punctuation_chars` already covers every
/// wordchar, since `wordchars` and `punctuation_chars` are disjoint (CPython's `__init__`
/// translates the punctuation out of the wordchars). Kept because the port's claim is
/// "CPython's state machine", and a reader checking that claim against `shlex.py` should find
/// every table it looks for.
const WORDCHARS: &str = concat!(
    // CPython's own order, typo and all (`abcdfe…`); only membership is read.
    "abcdfeghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_",
    "ßàáâãäåæçèéêëìíîïðñòóôõöøùúûüýþÿÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖØÙÚÛÜÝÞ",
    "~-./*?=",
);

/// How a glued run breaks into the SHELL'S OWN tokens — `_OPERATOR_SPLIT_RE`, in the
/// alternation's own order, because a regex alternation takes the FIRST alternative that matches
/// and not the longest.
///
/// **What keeps `cat 2>&1 <vault>` in one segment is that `>&` is IN this table**, and that it
/// precedes the `>` that is a prefix of it. Cutting a punctuation run into operator *characters*
/// splits that command at the `&` and strands the vault path in a segment of its own: the reader
/// loses its operand, and the guard answers allow. Removing `>&` from the table, or putting the
/// bare `<`/`>` in front of the two- and three-character spellings, was each measured to change
/// thousands of answers in the differential fuzz.
///
/// The redirections stand ahead of the control operators because the Python regex is built that
/// way. That much is inert and is written down as inert: no control operator is a prefix of any
/// redirection — every redirection begins `<` or `>` — so moving the two groups past each other
/// changes no answer, measured over 20,000 fuzz cases. The ordering that is load-bearing is
/// longest-first *within* a prefix.
///
/// One table, read by [`split_punctuation`] on a lexed run and by [`resegment`] on a string that
/// never reached the tokenizer, so the parsed and the unparseable paths cannot disagree about
/// what a boundary is. The regex ends in the character class `[(){}]`, which is
/// [`GROUPING_SPLIT`] here.
const OPERATOR_ALTERNATION: [&str; 17] = [
    "<<<", "<<", "<>", "<&", ">>", ">&", ">|", "<", ">", // the redirections, first
    "||", "&&", ";;", ";", "|&", "|", "&", "\n", // the control operators
];

/// The regex's trailing `[(){}]`, tried after every alternative in [`OPERATOR_ALTERNATION`].
const GROUPING_SPLIT: [char; 4] = ['(', ')', '{', '}'];

/// The CONTROL operators: they end one separately-executed command and begin the next wherever
/// they stand. Membership is necessary and **not sufficient** — a token is a boundary only
/// where the shell would INTERPRET it, which [`Tok::bare`] records.
const CONTROL_OPERATORS: [&str; 8] = [";", ";;", "&&", "||", "|", "|&", "&", "\n"];

/// The GROUPING tokens, a boundary only where a shell RECOGNISES one — see [`segment_tokens`],
/// which is where position is decided.
const GROUPING: [&str; 4] = ["(", ")", "{", "}"];

/// Tokens that turn the `(` after them into a SUBSTITUTION rather than a subshell: `<(` and
/// `>(` process substitution. `$` is matched as a SUFFIX instead, because `x$(…)` lexes as `x$`.
const PROCSUB_LEAD: [&str; 2] = ["<", ">"];

/// The quoting contexts a position can sit in. `(` and a backtick are SUBSTITUTIONS — inside
/// them quoting starts again from nothing — so they are on the stack but are not "quoted".
const QUOTED_CONTEXTS: [&str; 4] = ["'", "\"", "$'", "$\""];

/// What may stand in front of the `#` that begins a comment.
const BEFORE_COMMENT: &str = " \t\n;|&()<>";

/// One token, plus whether the shell would INTERPRET its text as punctuation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tok {
    pub text: String,
    /// True when the token was produced without the lexer ever entering a quote state or an
    /// escape state — that is, every character of it stood unquoted and unescaped in the
    /// source. Only a bare token can be an operator: `\)`, `')'`, `"("`, `'&&'` and a
    /// single-quoted newline are ordinary words to a shell, and a word inside a reader's argv
    /// is an operand, not a boundary.
    pub bare: bool,
    /// Where the token's first character sits in the source, in CHARACTERS, or -1 where nothing
    /// measured it. Read by A7 alone, which judges the SPELLING a command was written with:
    /// `'charter' handoff` and `charter  handoff` lex to the same two texts as the exact form,
    /// and `charter $'handoff'` to a text no reader of words recognises at all — only the
    /// source tells any of them apart.
    pub start: isize,
    /// One past the last source character of the token, or -1.
    pub end: isize,
}

impl Tok {
    fn new(text: impl Into<String>, bare: bool, start: isize, end: isize) -> Self {
        Self {
            text: text.into(),
            bare,
            start,
            end,
        }
    }

    /// True when the shell would interpret this token as one of `texts`.
    pub fn is_op(&self, texts: &[&str]) -> bool {
        self.bare && texts.contains(&self.text.as_str())
    }

    /// True when the shell would interpret this token as any operator at all.
    pub fn is_any_op(&self) -> bool {
        self.bare
            && (CONTROL_OPERATORS.contains(&self.text.as_str())
                || GROUPING.contains(&self.text.as_str()))
    }

    /// True when the shell would interpret this token as a CONTROL operator — one that ends a
    /// separately-executed command. `_Tok.is_op(*_CONTROL_OPERATORS)` in the oracle.
    pub fn is_control_op(&self) -> bool {
        self.is_op(&CONTROL_OPERATORS)
    }

    /// True when the shell would interpret this token as a grouping token.
    pub fn is_grouping(&self) -> bool {
        self.is_op(&GROUPING)
    }
}

/// `_quote_map`'s answer for one line, computed ONCE.
///
/// The Python is `@functools.lru_cache(maxsize=512)`, and that cache is not decoration: the
/// callers above this module ask `_inside_quotes(line, i)` **per position**, and each of those
/// calls is an O(n) walk of the whole line. Uncached that is O(n²) per line, and
/// `_heredoc_opener_words`, `_crowded_substitutions` and `_pipeline_slice` each run one of those
/// walks. Rust has no decorator to inherit, so the memoisation is structural instead: the map is
/// built once per line and handed to everything that reads it, which is a cache that cannot
/// miss and needs no size.
#[derive(Clone, Debug)]
pub struct Quoting {
    flags: Vec<bool>,
}

impl Quoting {
    /// [`quote_map`] for `line`, kept for as long as the caller reads it.
    pub fn of(line: &str) -> Self {
        Self {
            flags: quote_map(line),
        }
    }

    /// `_inside_quotes`: whether offset `at` sits inside quotes.
    ///
    /// Python's `flags[at] if 0 <= at < len(flags) else flags[-1]`, and the `else` is reachable
    /// in both directions: a negative offset and an offset past the end both answer with the
    /// state at END of input. The map has one more entry than the line has characters, so
    /// `at == line.chars().count()` is an ordinary in-range read of that same last entry.
    pub fn inside(&self, at: isize) -> bool {
        // `(0..n).contains(&at)` rather than `at >= 0 && at < n`: the same test, written the way
        // clippy asks for it.
        let n = self.flags.len() as isize;
        if (0..n).contains(&at) {
            self.flags[at as usize]
        } else {
            self.at_end()
        }
    }

    /// The state at end of input — whether a quote is still open.
    pub fn at_end(&self) -> bool {
        // `quote_map` always returns `n + 1` entries, so there is always a last one.
        *self.flags.last().unwrap_or(&false)
    }

    /// The raw flags, one per character plus the end.
    pub fn flags(&self) -> &[bool] {
        &self.flags
    }
}

/// What `shlex` raises, and the only thing [`lex`] can fail with. `_segment_argv_parsed` owns
/// the fallback; nothing above it may read an error as "no command here".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexError {
    /// `ValueError("No closing quotation")`.
    NoClosingQuotation,
    /// `ValueError("No escaped character")`.
    NoEscapedCharacter,
}

/// `shlex`'s state, as the state machine spells it: `' '`, `None`, `'a'`, `'c'`, a quote
/// character, or the escape character.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum St {
    Space,
    Eof,
    Word,
    Punct,
    Quote(char),
    Esc(char),
}

/// The character source the lexer reads, whose `readline` stops BEFORE the newline instead of
/// consuming it.
///
/// `shlex` ends a comment by calling `instream.readline()`. With `\n` as an operator that would
/// swallow the separator too, and `echo a # note` + newline + `cat <vault>` would go back to
/// being one segment whose program is `echo` — the comment bypass in a second spelling.
struct NewlineKeepingStream {
    s: Vec<char>,
    i: usize,
}

impl NewlineKeepingStream {
    fn read1(&mut self) -> Option<char> {
        let c = self.s.get(self.i).copied();
        if c.is_some() {
            self.i += 1;
        }
        c
    }

    /// Everything up to the next `\n`, leaving the `\n` itself in the stream.
    fn readline(&mut self) {
        let end = self.s[self.i..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(self.s.len(), |off| self.i + off);
        self.i = end;
    }
}

/// The three `shlex.shlex` tables this module instantiates in two different ways.
///
/// One state machine, two configurations, because the oracle uses two: `_ShellLexer` for a
/// command line, and CPython's own `shlex.split` inside `_split_env_chdir` for `env -S`'s packed
/// string. Writing the second as a second machine is how two readings of one construct come to
/// disagree — the mistake this whole area keeps paying for — so it is the same code with a
/// different table.
#[derive(Clone, Copy)]
struct Cfg {
    /// What is dropped between tokens.
    whitespace: &'static str,
    /// What is emitted as a glued RUN. Empty for `shlex.split`, which has no punctuation
    /// characters at all, so its lexer never reaches the punctuation state.
    punctuation: &'static str,
    /// Whether `#` begins a comment. `shlex.split(s)` is `comments=False`, so its `commenters`
    /// is empty and its `readline` is never reached.
    comments: bool,
}

/// `_ShellLexer`: what a charter command line is read with.
const SHELL_CFG: Cfg = Cfg {
    whitespace: WHITESPACE,
    punctuation: PUNCTUATION_CHARS,
    comments: true,
};

/// CPython's `shlex.split(s)`: `posix=True`, `whitespace_split=True`, `comments=False`, and no
/// `punctuation_chars`, which leaves `whitespace` at `shlex`'s default — the newline INCLUDED,
/// unlike the command-line reading above.
///
/// `wordchars` differs too (CPython only adds `~-./*?=` and removes the punctuation when
/// `punctuation_chars` is truthy), and the difference is inert for the same reason
/// [`WORDCHARS`] is inert here at all: under `whitespace_split` both arms that consult it reach
/// the same state as the arm that would otherwise catch the character, and with no punctuation
/// characters the disjunct `whitespace_split and nextchar not in punctuation_chars` is simply
/// true. So one table serves both.
const SPLIT_CFG: Cfg = Cfg {
    whitespace: " \t\r\n",
    punctuation: "",
    comments: false,
};

/// `shlex.shlex` in posix mode, plus the three things `_ShellLexer` changes about it.
struct Lexer {
    cfg: Cfg,
    instream: NewlineKeepingStream,
    state: St,
    token: Vec<char>,
    pushback_chars: Vec<char>,
    bare: bool,
    start: isize,
    end: isize,
}

impl Lexer {
    fn new(cmd: &str, cfg: Cfg) -> Self {
        Self {
            cfg,
            instream: NewlineKeepingStream {
                s: cmd.chars().collect(),
                i: 0,
            },
            state: St::Space,
            token: Vec::new(),
            pushback_chars: Vec::new(),
            bare: true,
            start: -1,
            end: -1,
        }
    }

    /// Every `self.state = …` in `read_token` goes through `_ShellLexer`'s property setter, and
    /// this is that setter. It is the whole mechanism behind `bare`, `start` and `end`.
    fn set_state(&mut self, value: St) {
        // `shlex` sets `state` to the quote character it is inside, or to the escape character
        // it is honouring, and to nothing else that is not a plain word/punctuation marker — so
        // this catches quoting wherever it appears in the token, including a quote glued to the
        // middle of one.
        if matches!(value, St::Quote(_) | St::Esc(_)) {
            self.bare = false;
        }
        // The same transition says where the token STARTS. `shlex` leaves whitespace on the
        // token's first character, and that is the last character read from the stream: a
        // character `shlex` pushed back at the end of the previous token was read from the
        // stream and is not re-read, so the cursor is already one past it either way.
        if self.state == St::Space && value != St::Space && self.start < 0 {
            self.start = self.instream.i as isize - 1;
        }
        // And where it ENDS. Leaving a word or a punctuation run for whitespace, the lexer has
        // just read the one character that ended the token — a blank it drops, or a character it
        // pushes back — so the token ends one before the cursor; at the end of the input, at it.
        if matches!(self.state, St::Word | St::Punct) && matches!(value, St::Space | St::Eof) {
            self.end = self.instream.i as isize - if value == St::Space { 1 } else { 0 };
        }
        self.state = value;
    }

    /// A shell begins a comment at `#` only where a WORD BEGINS. `shlex` honours it mid-word
    /// too and swallows the rest of the line, so `echo hi#; cat <vault>` — which runs the `cat`
    /// in bash — lexed as a lone `echo hi` and every later command became invisible.
    fn commenters(&self) -> &'static str {
        if self.cfg.comments && self.state == St::Space {
            "#"
        } else {
            ""
        }
    }

    /// CPython's `shlex.read_token`, with `_ShellLexer`'s overrides folded in.
    fn read_token(&mut self) -> Result<Option<String>, LexError> {
        self.bare = true; // per token, not per lex
        self.start = -1;
        self.end = -1;
        let mut quoted = false;
        let mut escapedstate = St::Space;
        loop {
            let nextchar = match self.pushback_chars.pop() {
                Some(c) => Some(c),
                None => self.instream.read1(),
            };
            match self.state {
                St::Eof => {
                    self.token.clear();
                    break;
                }
                St::Space => {
                    let Some(c) = nextchar else {
                        self.set_state(St::Eof);
                        break;
                    };
                    if self.cfg.whitespace.contains(c) {
                        if !self.token.is_empty() || quoted {
                            break;
                        }
                        continue;
                    } else if self.commenters().contains(c) {
                        self.instream.readline();
                    } else if c == ESCAPE {
                        escapedstate = St::Word;
                        self.set_state(St::Esc(c));
                    } else if WORDCHARS.contains(c) {
                        self.token = vec![c];
                        self.set_state(St::Word);
                    } else if self.cfg.punctuation.contains(c) {
                        self.token = vec![c];
                        self.set_state(St::Punct);
                    } else if QUOTES.contains(c) {
                        self.set_state(St::Quote(c));
                    } else {
                        // `whitespace_split` is true, so this is the arm that catches
                        // everything else, and it is the same assignment `wordchars` makes.
                        self.token = vec![c];
                        self.set_state(St::Word);
                    }
                }
                St::Quote(q) => {
                    quoted = true;
                    let Some(c) = nextchar else {
                        return Err(LexError::NoClosingQuotation);
                    };
                    if c == q {
                        self.set_state(St::Word);
                    } else if c == ESCAPE && ESCAPED_QUOTES.contains(q) {
                        escapedstate = St::Quote(q);
                        self.set_state(St::Esc(c));
                    } else {
                        self.token.push(c);
                    }
                }
                St::Esc(e) => {
                    let Some(c) = nextchar else {
                        return Err(LexError::NoEscapedCharacter);
                    };
                    // In posix shells, only the quote itself or the escape character may be
                    // escaped within quotes.
                    if let St::Quote(q) = escapedstate
                        && c != e
                        && c != q
                    {
                        self.token.push(e);
                    }
                    self.token.push(c);
                    self.set_state(escapedstate);
                }
                St::Word | St::Punct => {
                    let Some(c) = nextchar else {
                        self.set_state(St::Eof);
                        break;
                    };
                    if self.cfg.whitespace.contains(c) {
                        self.set_state(St::Space);
                        if !self.token.is_empty() || quoted {
                            break;
                        }
                        continue;
                    } else if self.commenters().contains(c) {
                        // Unreachable: `commenters()` is empty outside the whitespace state,
                        // which is the narrowing `_ShellLexer` exists for. Kept so the shape of
                        // CPython's machine is visible where a reader looks for it.
                        self.instream.readline();
                        self.set_state(St::Space);
                        if !self.token.is_empty() || quoted {
                            break;
                        }
                        continue;
                    } else if self.state == St::Punct {
                        if self.cfg.punctuation.contains(c) {
                            self.token.push(c);
                        } else {
                            if !self.cfg.whitespace.contains(c) {
                                self.pushback_chars.push(c);
                            }
                            self.set_state(St::Space);
                            break;
                        }
                    } else if QUOTES.contains(c) {
                        self.set_state(St::Quote(c));
                    } else if c == ESCAPE {
                        escapedstate = St::Word;
                        self.set_state(St::Esc(c));
                    } else if WORDCHARS.contains(c)
                        || QUOTES.contains(c)
                        || !self.cfg.punctuation.contains(c)
                    {
                        self.token.push(c);
                    } else {
                        self.pushback_chars.push(c);
                        self.set_state(St::Space);
                        if !self.token.is_empty() || quoted {
                            break;
                        }
                        continue;
                    }
                }
            }
        }
        let result: String = std::mem::take(&mut self.token).into_iter().collect();
        // posix: an empty token that was never quoted is end of input, and `''` is a real token.
        if !quoted && result.is_empty() {
            return Ok(None);
        }
        Ok(Some(result))
    }
}

/// `cmd` as [`Tok`]s, or the error `shlex` raises on genuinely unbalanced quoting.
pub fn lex(cmd: &str) -> Result<Vec<Tok>, LexError> {
    let mut lexer = Lexer::new(cmd, SHELL_CFG);
    let mut out = Vec::new();
    loop {
        match lexer.read_token()? {
            // `None`, not `""`: `cat ''` produces a real empty token.
            None => return Ok(out),
            Some(text) => out.push(Tok::new(text, lexer.bare, lexer.start, lexer.end)),
        }
    }
}

/// CPython's `shlex.split(s)` — **not** [`lex`].
///
/// `_split_env_chdir` calls it on the string `env -S` packs a whole command into, and it is a
/// different reading from a command line's: no punctuation characters (so `;` is an ordinary
/// word character rather than an operator), no comments, and a newline is plain whitespace.
/// Only the texts come back; `bare` and the offsets mean nothing on this path and the oracle
/// discards them too.
pub fn posix_split(s: &str) -> Result<Vec<String>, LexError> {
    let mut lexer = Lexer::new(s, SPLIT_CFG);
    let mut out = Vec::new();
    loop {
        match lexer.read_token()? {
            None => return Ok(out),
            Some(text) => out.push(text),
        }
    }
}

/// The characters `shlex.quote` leaves alone: `[\w@%+=:,./-]` under `re.ASCII`, so `\w` is
/// exactly ASCII alphanumerics and `_` — a Unicode letter is NOT safe and gets quoted.
fn quote_safe(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c)
}

/// CPython's `shlex.quote`: `s` as one shell word.
///
/// `_desugar_ansi_c` rewrites a decoded `$'…'` through this, which is what restores the token
/// boundary bash sees. The empty string becomes `''`, a string of only safe characters is
/// returned untouched, and anything else is single-quoted with each `'` spelled `'"'"'`.
pub fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.chars().all(quote_safe) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\"'\"'");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// `cmd` with backtick substitutions rewritten as `$( … )` — the same construct.
///
/// Alternating: the first unescaped backtick opens, the next closes. An odd count leaves the
/// last one open, which [`segment_tokens`] closes at end of input — the same place a shell would
/// report the error, and the segment is still SEEN.
///
/// A backtick inside single quotes is literal to a shell and is rewritten here anyway. The cost
/// is a token whose text reads `$(x)` instead of `` `x` `` inside an argument that is never
/// split, which is why this is safe to do before quoting is known.
pub fn unbacktick(cmd: &str) -> String {
    let chars: Vec<char> = cmd.chars().collect();
    // `_BACKTICK_RE` is `(?<!\\)` + a backtick, and `re.split` on it yields the pieces between.
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '`' && (i == 0 || chars[i - 1] != '\\') {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    parts.push(cur);
    if parts.len() == 1 {
        return cmd.to_string();
    }
    let mut out = parts[0].clone();
    for (i, part) in parts[1..].iter().enumerate() {
        out.push_str(if i % 2 == 0 { "$(" } else { ")" });
        out.push_str(part);
    }
    out
}

/// `_OPERATOR_SPLIT_RE.split`: the pieces of `text`, separators included, longest-first,
/// redirections ahead of the control operator of the same spelling. Empty pieces are kept so a
/// caller tracking offsets can add their length; [`split_punctuation`] drops them.
fn operator_split(text: &[char]) -> Vec<Vec<char>> {
    /// The length of the alternative that matches at `text[i]`, in the alternation's order, or
    /// `None`. Leftmost-longest is NOT the rule — the regex takes the FIRST alternative that
    /// matches, and the table is ordered so that is also the longest.
    fn matches_at(text: &[char], i: usize) -> Option<usize> {
        for pat in OPERATOR_ALTERNATION {
            let len = pat.chars().count();
            if i + len <= text.len() && text[i..i + len].iter().copied().eq(pat.chars()) {
                return Some(len);
            }
        }
        GROUPING_SPLIT.contains(&text[i]).then_some(1)
    }

    let mut out = Vec::new();
    let mut last = 0usize;
    let mut i = 0usize;
    while i < text.len() {
        match matches_at(text, i) {
            Some(len) => {
                out.push(text[last..i].to_vec());
                out.push(text[i..i + len].to_vec());
                i += len;
                last = i;
            }
            None => i += 1,
        }
    }
    out.push(text[last..].to_vec());
    out
}

/// `toks` with the lexer's glued punctuation RUNS broken into the SHELL'S OWN tokens.
///
/// Not into individual operator *characters*: `>&` is one alternative of
/// [`OPERATOR_ALTERNATION`] and stands in front of the `>` that is its prefix, so the `&` stays
/// part of the redirection instead of becoming a boundary that strands `cat 2>&1 <vault>`'s
/// operand.
///
/// Only tokens made ENTIRELY of punctuation characters are touched, so a real argument is never
/// rewritten — and only **bare** runs. A quoted `'();'` is one word to a shell, and splitting it
/// into three boundaries is not a conservative error: it strands a reader's operand in a segment
/// of its own, which is a false ALLOW (`cat '()' <vault>` printed a vault). The pieces inherit
/// `bare`, since a run that was unquoted is unquoted character by character.
pub fn split_punctuation(toks: Vec<Tok>) -> Vec<Tok> {
    let mut out = Vec::new();
    for t in toks {
        let chars: Vec<char> = t.text.chars().collect();
        if t.bare && chars.len() > 1 && chars.iter().all(|c| PUNCTUATION_CHARS.contains(*c)) {
            // Each piece starts where it sits inside the run, so a split token's offset stays an
            // offset into the source. The advance is outside the `if` because Python's is, and
            // because the general shape is right; in a run it is also inert, since every
            // character of a run matches some alternative and the pieces BETWEEN the separators
            // are therefore all empty.
            let mut at = t.start;
            for p in operator_split(&chars) {
                if !p.is_empty() {
                    let text: String = p.iter().collect();
                    out.push(Tok::new(text, true, at, at + p.len() as isize));
                }
                at += p.len() as isize;
            }
        } else {
            out.push(t);
        }
    }
    out
}

/// Tokens as **argv per separately-executed command**, parentheses understood.
///
/// Shared by both of [`segment_argv_parsed`]'s paths, so the parsed and the unparseable answers
/// cannot disagree about what a boundary is.
///
/// Three kinds of token end a segment: the control operators, the braces of a group, and the
/// parenthesis of a SUBSHELL — each only where the shell would INTERPRET it, which
/// [`Tok::is_op`] answers and a quoted or escaped character never satisfies.
///
/// A parenthesis that opens a **substitution** — `$( … )`, `<( … )`, `>( … )` — is not a
/// boundary in the same sense, and treating it as one was a bypass rather than a gap:
/// `cat $(echo <vault>)` segments into `cat $` and `echo <vault>` under a plain boundary rule,
/// neither half of which is a read of the vault. So a substitution yields an **additional inner
/// segment** while the **enclosing segment keeps accumulating** the same tokens. Both readings
/// are needed: the inner one is what runs (`echo $(cat <vault>)`), and the outer one is where
/// its output lands (`cat $(echo <vault>)`).
///
/// **Being unquoted is not enough to make a token a boundary — POSITION decides too.** `{` and
/// `}` are RESERVED WORDS, recognised only where a command word is expected; everywhere else
/// bash passes them through as ordinary arguments, so `cat { <vault>` is ONE command that prints
/// the vault. A parenthesis is judged the same way — bash only closes a subshell that is open,
/// and `cat ( x` is a syntax error rather than a boundary. Keeping the segment whole is the
/// conservative direction: a reader holds on to its operand.
pub fn segment_tokens(toks: Vec<Tok>) -> Vec<Vec<String>> {
    /// What is open at this point, innermost last.
    enum Open {
        Subst,
        Subshell,
    }

    let mut out: Vec<Vec<Tok>> = Vec::new();
    let mut open_segs: Vec<Vec<Tok>> = vec![Vec::new()]; // outermost first
    let mut stack: Vec<Open> = Vec::new();
    for t in split_punctuation(toks) {
        if t.is_op(&["("]) {
            let prev = open_segs.last().and_then(|s| s.last());
            let substitution = prev.is_some_and(|p| {
                p.bare && (p.text.ends_with('$') || PROCSUB_LEAD.contains(&p.text.as_str()))
            });
            if substitution {
                open_segs.push(Vec::new());
                stack.push(Open::Subst);
                continue;
            }
            if open_segs.last().is_some_and(|s| s.is_empty()) {
                // command position: a subshell opens
                stack.push(Open::Subshell);
                continue;
            }
            // `cat ( x` — a shell does not start a subshell mid-command; it fails to parse.
            // Treat it as the word it is rather than stranding `cat`'s operand.
        } else if t.is_op(&[")"]) {
            match stack.last() {
                Some(Open::Subst) => {
                    out.push(open_segs.pop().unwrap_or_default());
                    stack.pop();
                    continue;
                }
                Some(Open::Subshell) => {
                    if let Some(seg) = open_segs.last_mut() {
                        out.push(std::mem::take(seg));
                    }
                    stack.pop();
                    continue;
                }
                // nothing is open for it to close: an ordinary word, as above.
                None => {}
            }
        } else if t.is_op(&["{", "}"]) {
            if open_segs.last().is_some_and(|s| s.is_empty()) {
                continue; // command position: the reserved word
            }
            // mid-command: an ordinary argument to the program already named.
        } else if t.is_any_op() {
            if let Some(seg) = open_segs.last_mut() {
                out.push(std::mem::take(seg));
            }
            continue;
        }
        for seg in open_segs.iter_mut() {
            // every open segment, the outer ones included
            seg.push(t.clone());
        }
    }
    out.extend(open_segs);
    out.into_iter()
        .filter(|c| !c.is_empty())
        .map(|c| c.into_iter().map(|t| t.text).collect())
        .collect()
}

/// Split an already-whitespace-split token list on shell operators — the unparseable path only,
/// where there is no tokenizer to lean on.
///
/// Operators are split out of the MIDDLE of a token as well (`a;b` is two commands to a shell,
/// and one token to `str.split`), because a fallback that only noticed free-standing operators
/// would be a rule an attacker satisfies by deleting a space.
///
/// Every piece is marked bare, because on this path nothing knows what was quoted — the quoting
/// is what failed to parse. That is a guess in BOTH directions, which is why
/// [`segment_argv_parsed`] reports the failure and the leak guard scans the raw string as well.
fn resegment(toks: &[String]) -> Vec<Vec<String>> {
    let mut pieces: Vec<Tok> = Vec::new();
    for tok in toks {
        let chars: Vec<char> = tok.chars().collect();
        for p in operator_split(&chars) {
            if !p.is_empty() {
                pieces.push(Tok::new(p.iter().collect::<String>(), true, -1, -1));
            }
        }
    }
    segment_tokens(pieces)
}

/// For each offset in `line`, whether it sits inside quotes — computed in ONE pass.
///
/// Read off the source rather than from the lexer, because this runs on lines the lexer could
/// not take apart. A backslash escapes the next character outside single quotes, where bash
/// takes it literally.
///
/// **A substitution inside double quotes is not quoted.** `"$(cat <<'EOF')"` runs a command and
/// that command's `<<` is a real opener, so `$(` and a backtick open a nested context.
///
/// **But `$'` opens nothing inside `"…"`**, where both shells read a bare `$` as a literal. The
/// `"` arm therefore comes BEFORE the `$'`/`$"` arm: with the order reversed, the `$` in an
/// ordinary regex anchor (`grep -v "^$" f`) swallows the closing quote and the rest of the line
/// reads as quoted.
///
/// The returned vector has one more entry than `line` has characters: the last is the state at
/// end of input.
pub fn quote_map(line: &str) -> Vec<bool> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut flags = vec![false; n + 1];
    let mut stack: Vec<&'static str> = Vec::new();
    fn quoted_now(stack: &[&'static str]) -> bool {
        stack.last().is_some_and(|t| QUOTED_CONTEXTS.contains(t))
    }
    fn starts_with(chars: &[char], i: usize, s: &str) -> bool {
        let len = s.chars().count();
        i + len <= chars.len() && chars[i..i + len].iter().copied().eq(s.chars())
    }
    let mut i = 0usize;
    while i < n {
        let here = quoted_now(&stack);
        flags[i] = here;
        let c = chars[i];
        let top = stack.last().copied().unwrap_or("");
        if top != "'" && c == '\\' {
            if i + 1 < n {
                flags[i + 1] = here;
            }
            i += 2;
            continue;
        }
        if top == "'" {
            if c == '\'' {
                stack.pop();
            }
        } else if top == "$'" || top == "$\"" {
            // Closes on its own quote, and nothing else opens inside it: `$(` is literal there.
            if c == top.chars().nth(1).unwrap_or('\0') {
                stack.pop();
            }
        } else if starts_with(&chars, i, "$(") {
            stack.push("(");
            flags[i + 1] = here;
            i += 2;
            continue;
        } else if c == '`' {
            if top == "`" {
                stack.pop();
            } else {
                stack.push("`");
            }
        } else if top == "\"" {
            // Ahead of the `$'` arm on purpose — see above. Inside `"…"` only the closing quote
            // (and a substitution, handled above) means anything.
            if c == '"' {
                stack.pop();
            }
        } else if starts_with(&chars, i, "$'") {
            stack.push("$'");
            flags[i + 1] = here;
            i += 2;
            continue;
        } else if starts_with(&chars, i, "$\"") {
            stack.push("$\"");
            flags[i + 1] = here;
            i += 2;
            continue;
        } else if c == ')' && top == "(" {
            stack.pop();
        } else if c == '\'' || c == '"' {
            stack.push(if c == '\'' { "'" } else { "\"" });
        }
        i += 1;
    }
    flags[n] = quoted_now(&stack);
    flags
}

/// `cmd` with each ACTIVE backslash-newline removed — bash's first transformation, before tokens
/// or quotes are read.
///
/// A backslash-newline splices only where the backslash is live: it is literal inside single
/// quotes and inside a comment, so those newlines stay. Kept narrow — only an UNQUOTED backslash
/// is spliced; one inside double quotes is left in place, which can only keep more text on one
/// line (the fail-closed direction for a guard reading operands).
pub fn splice_continuations(cmd: &str) -> String {
    let chars: Vec<char> = cmd.chars().collect();
    let q = quote_map(cmd);
    let n = chars.len();
    let mut out = String::new();
    let mut in_comment = false;
    let mut i = 0usize;
    while i < n {
        let c = chars[i];
        if c == '\n' {
            in_comment = false;
            out.push(c);
            i += 1;
            continue;
        }
        if !in_comment
            && c == '#'
            && !q[i]
            && (i == 0 || (BEFORE_COMMENT.contains(chars[i - 1]) && !q[i - 1]))
        {
            in_comment = true;
        }
        if c == '\\' && i + 1 < n && chars[i + 1] == '\n' && !q[i] && !in_comment {
            i += 2; // drop the backslash and the newline it splices
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Segment an unparseable `cmd` keeping bash's LINE structure.
///
/// The old fallback was `_resegment(cmd.split())`, and `str.split` erases newlines. On the
/// parsed path a newline IS a command boundary, so a single broken quote anywhere folded every
/// following line into the segment before it — `cd .charter/vaults`, then `cat x.json`, then a
/// stray `echo "` read as ONE command whose program was `cd`, and the vault read vanished. It
/// cuts both ways: splitting on EVERY newline strands a reader's operand when a quoted string
/// spans lines, and not splicing is not an option either.
///
/// So a newline is a boundary exactly where bash makes one: not inside a quote it has not closed
/// ([`quote_map`], which reads unbalanced input without failing), and not one a live backslash
/// spliced away ([`splice_continuations`], run first as bash runs it). Each resulting logical
/// line is then lexed on its OWN, and only a line that still will not lex falls back to the
/// crude [`resegment`], whose stranding is confined to that one line.
fn fallback_segments(cmd: &str) -> Vec<Vec<String>> {
    let spliced = splice_continuations(cmd);
    let chars: Vec<char> = spliced.chars().collect();
    let q = quote_map(&spliced);
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut bounds: Vec<usize> = (0..chars.len())
        .filter(|&i| chars[i] == '\n' && !q[i])
        .collect();
    bounds.push(chars.len());
    let mut start = 0usize;
    for cut in bounds {
        // `start` can pass `cut` only for an empty trailing slice; Python's slice clamps.
        let line: String = if start <= cut {
            chars[start..cut].iter().collect()
        } else {
            String::new()
        };
        start = cut + 1;
        // Python's `str.strip()`, which counts U+001C–U+001F as blank where Rust's does not.
        if line.chars().all(is_python_space) {
            continue;
        }
        match lex(&line) {
            Ok(toks) => out.extend(segment_tokens(toks)),
            Err(_) => out.extend(resegment(&py_split(&line))),
        }
    }
    out
}

/// Python's `str.split()` with no argument: split on RUNS of Python whitespace, no empty pieces.
pub fn py_split(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if is_python_space(c) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A shell command as **argv per separately-executed segment**, quoting respected, plus whether
/// the command actually PARSED.
///
/// A caller that must fail closed needs to know that the answer it just got is best-effort. The
/// leak guard is the one caller that does: it adds a raw scan of the whole string when this
/// returns `false`.
///
/// **On a command that cannot be parsed at all** (genuinely unbalanced quoting) this falls back
/// to a per-line lex, still segmented on the operators. It used to return the whole string as
/// ONE segment, and the Python docstring claimed that kept the leak guard fail-closed. It did
/// the opposite: every guard reads token 0 as the program, so one segment means one program, and
/// every invocation AFTER the first became invisible — `echo $'it\'s fine' ; cat <vault>` is
/// valid bash, trips `shlex`, and printed a vault through the shipped hook. A fallback that
/// drops guards is worse than no fallback, because the guard still looks present.
pub fn segment_argv_parsed(cmd: &str) -> (Vec<Vec<String>>, bool) {
    let cmd = unbacktick(cmd);
    match lex(&cmd) {
        Ok(toks) => (segment_tokens(toks), true),
        // Tokenized, and segmented: the leak guard has to see `--reveal` among the arguments AND
        // has to see the second command at all. Quoting is not honoured on this path — it is
        // what failed to parse — so the boundaries here are a guess, and a guess is wrong in
        // both directions. Neither is survivable on its own, so this path is not relied on
        // alone: the flag is `false`, and the caller matches the raw string as well.
        Err(_) => (fallback_segments(&cmd), false),
    }
}

/// [`segment_argv_parsed`] without the flag.
pub fn segment_argv(cmd: &str) -> Vec<Vec<String>> {
    segment_argv_parsed(cmd).0
}
