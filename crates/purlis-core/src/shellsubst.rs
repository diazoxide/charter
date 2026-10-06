//! Where a command line's command substitutions open and close, read with enough of the shell's
//! grammar that a quote, a comment, a heredoc body or a `case` pattern inside one does not end
//! it early (#1412).
//!
//! [`crate::shellseg`] reads a command line into the segments a shell runs, and it takes a
//! `"$( … )"` for one quoted word. The command inside still runs. So a guard that must see every
//! command a shell runs asks this module for the substitutions' text and reads each one again:
//! the leak guard reads every one ([`Scan::substitutions`]) from each directory the shell could
//! stand in, and the consent and operator-rule backstops read every one at every depth
//! ([`every_substitution`], #1417), so the grammar is fixed in one place.
//!
//! One pass over the text with a stack of the contexts it is in, so the cost grows with the text
//! and never with how deep the substitutions nest or how many are left open.

use crate::guardcaps;
use crate::shellseg;

/// What [`every_substitution`] read: the text of every command substitution a shell runs, at
/// every depth, and whether it stopped at a bound.
pub(crate) struct Inward {
    /// Each substitution's text as the shell runs it ([`Scan::substitutions`]), and read as
    /// commands ([`Scan::as_commands`]: a `case` pattern's `)` ends a command), outermost
    /// first; one nested in another is listed after it, apart. Each distinct text is scanned
    /// and listed once.
    pub(crate) texts: Vec<String>,
    /// The walk reached past a bound ([`guardcaps::MAX_NESTING`] deep, or more text than
    /// [`read_budget`] allows), so it is not a reading anyone may decide on. It has been
    /// reported to [`shellseg::too_deep_within`] as well.
    pub(crate) too_big: bool,
}

/// How many characters of substitutions a guard reads as commands, per character of the
/// command: a substitution nested in another is read once at each depth.
const READ_PER_CHARACTER: usize = 4;

/// …and at least, so a short command is read all the way in.
const READ_AT_LEAST: usize = 64 * 1024;

/// How much substitution text a guard reads for a command `len` characters long.
pub(crate) fn read_budget(len: usize) -> usize {
    len.saturating_mul(READ_PER_CHARACTER).max(READ_AT_LEAST)
}

/// Every command substitution a shell runs in `text`, at every depth, wherever it opens
/// ([`Scan::substitutions`]: unquoted, in double quotes, in a `${ … }`, a backtick or an
/// expanding heredoc body), the one reading the consent and operator-rule backstops share with
/// the leak guard (#1417).
///
/// **Bounded, and fails closed past a bound**: more than [`guardcaps::MAX_NESTING`] deep, or
/// more text than [`read_budget`] of `text`'s length, and the walk stops with
/// [`Inward::too_big`] set and the bound reported to [`shellseg::too_deep_within`], so the
/// caller refuses the call as too big to check.
pub(crate) fn every_substitution(text: &str) -> Inward {
    let mut budget = read_budget(text.len());
    let mut out = Inward {
        texts: Vec::new(),
        too_big: false,
    };
    // Depth first, each text with how deep it sits. A text read once is not read again: its
    // reading, and the substitutions in it, are the same wherever it sits.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut todo: Vec<(String, usize)> = vec![(text.to_owned(), 0)];
    while let Some((text, depth)) = todo.pop() {
        if depth > 0 && !seen.insert(text.clone()) {
            continue;
        }
        let scanned = scan(&text, true);
        if depth > 0 {
            out.texts.push(match &scanned {
                Some(scan) => scan.as_commands(false),
                None => text,
            });
        }
        let Some(scan) = scanned else {
            continue;
        };
        let inner = scan.substitutions();
        if inner.is_empty() {
            continue;
        }
        let cost: usize = inner.iter().map(String::len).sum();
        if depth >= guardcaps::MAX_NESTING || cost > budget {
            shellseg::met_a_string_too_deep();
            out.too_big = true;
            return out;
        }
        budget -= cost;
        for body in inner.into_iter().rev() {
            todo.push((body, depth + 1));
        }
    }
    out
}

/// `text` read once, as a shell reads it, for what a guard needs beside its segments
/// ([`scan`]).
pub(crate) struct Scan {
    chars: Vec<char>,
    found: Closes,
}

/// [`Scan`] of `text` as written, heredoc bodies included: a body whose delimiter is quoted is
/// text, and one whose delimiter is not runs its substitutions, whichever program reads it.
/// `None` for a text with no substitution, no `case` and no heredoc in it, which a [`Scan`]
/// has nothing to say about: most commands, read at no cost. `heredocs` says whether a heredoc
/// alone is reason to read the text.
pub(crate) fn scan(text: &str, heredocs: bool) -> Option<Scan> {
    // A backslash-newline is read first: the shell takes it out before it looks for a `$(`, a
    // `case` or a `<<`, so `$\` and a newline and `(` open a substitution as `$(` does.
    let worth = text.contains("$(")
        || text.contains("\\\n")
        || text.contains('`')
        || text.contains("case")
        || (heredocs && text.contains("<<"));
    if !worth {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let found = closes(&chars);
    Some(Scan { chars, found })
}

impl Scan {
    /// The text of every command substitution a shell runs, outermost first, wherever it
    /// opens: unquoted, inside double quotes, inside a `${ … }`, a backtick's body or a heredoc
    /// body whose delimiter is unquoted. A substitution inside another is not listed apart,
    /// since it is in the outer one's text. A backtick's body is given as the shell runs it,
    /// with its escaping backslashes taken off ([`Opened::Tick`]). One left open runs to the
    /// end of the text, which a shell refuses to run at all.
    pub(crate) fn substitutions(&self) -> Vec<String> {
        let chars = &self.chars;
        let mut out = Vec::new();
        let mut taken_to = 0usize;
        for &(from, opened) in &self.found.all {
            if from < taken_to {
                continue; // inside a body already taken
            }
            let to = self
                .found
                .closes
                .get(&from)
                .copied()
                .unwrap_or(chars.len())
                .max(from);
            // As the shell runs it: its line continuations taken out.
            let body: Vec<char> = (from..to)
                .filter(|&j| !self.found.gone[j])
                .map(|j| chars[j])
                .collect();
            out.push(match opened {
                Opened::Dollar => body.iter().collect(),
                Opened::Tick { quoted } => untick(&body, quoted),
            });
            taken_to = to + 1;
        }
        out
    }

    /// The text with its line continuations taken out where the shell takes them out, and each
    /// `case` pattern's closing `)` read as the `;` that ends a command, so
    /// a reader that knows no `case` sees the command after the pattern as a command of its
    /// own. With `blank_bodies`, every heredoc body and its delimiter's line are emptied too,
    /// their newlines kept: what a shell parses as commands around the bodies, whoever reads
    /// them, so a body's apostrophe cannot read as a quote that runs on over the commands after.
    pub(crate) fn as_commands(&self, blank_bodies: bool) -> String {
        let mut chars = self.chars.clone();
        for &k in &self.found.case_patterns {
            chars[k] = ';';
        }
        for &k in &self.found.case_opens {
            chars[k] = ' ';
        }
        if blank_bodies {
            for &(from, to) in &self.found.bodies {
                for c in &mut chars[from..to.min(self.chars.len())] {
                    if *c != '\n' {
                        *c = ' ';
                    }
                }
            }
        }
        // And the line continuations the shell takes out, taken out.
        chars
            .into_iter()
            .zip(&self.found.gone)
            .filter(|(_, gone)| !**gone)
            .map(|(c, _)| c)
            .collect()
    }

    /// Whether the text has a `case` pattern.
    pub(crate) fn has_case_patterns(&self) -> bool {
        !self.found.case_patterns.is_empty()
    }

    /// Whether the text has a heredoc body.
    pub(crate) fn has_heredoc_bodies(&self) -> bool {
        !self.found.bodies.is_empty()
    }
}

/// A backtick's body as the shell runs it: the backslash taken off `\$`, `` \` `` and `\\`,
/// and off `\"` inside double quotes, and kept everywhere else.
fn untick(body: &[char], quoted: bool) -> String {
    let mut out = String::with_capacity(body.len());
    let mut k = 0;
    while k < body.len() {
        let c = body[k];
        let next = body.get(k + 1).copied();
        let escaped = matches!(next, Some('$' | '`' | '\\')) || (quoted && next == Some('"'));
        if c == '\\' && escaped {
            out.push(body[k + 1]);
            k += 2;
        } else {
            out.push(c);
            k += 1;
        }
    }
    out
}

/// What [`closes`] finds: where each substitution closes, keyed by where its body starts.
#[derive(Default)]
struct Closes {
    closes: std::collections::HashMap<usize, usize>,
    /// Every substitution's body start in order, in whatever context it opens, with what kind
    /// of substitution it is.
    all: Vec<(usize, Opened)>,
    /// Where each `case` pattern's closing `)` stands.
    case_patterns: Vec<usize>,
    /// Where each `case` pattern's optional opening `(` stands.
    case_opens: Vec<usize>,
    /// Each heredoc body read natively, from its first line to the end of its delimiter's line.
    bodies: Vec<(usize, usize)>,
    /// The characters of each line continuation the shell takes out (a backslash and its
    /// newline), outside single quotes, comments and a quoted heredoc's body.
    gone: Vec<bool>,
}

/// How a substitution [`closes`] found was opened.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opened {
    /// `$( … )`.
    Dollar,
    /// A backtick, and whether it stands inside double quotes: a shell takes the backslash off
    /// `\$`, `` \` `` and `\\` in its body before it runs it, and off `\"` too in quotes.
    Tick { quoted: bool },
}

/// A heredoc whose `<<` has been read and whose body starts on the next line.
#[derive(Clone)]
struct Pending {
    delim: Vec<char>,
    /// The delimiter carried no quoting, so the body's substitutions run.
    expands: bool,
    /// `<<-`: leading tabs are taken off each line, the delimiter's included.
    dash: bool,
}

/// A context the pass is inside.
enum Ctx {
    Single,
    /// `$' … '`, where a backslash escapes the next character, a `'` included.
    AnsiC,
    Double,
    /// A `${ … }`, and whether it stands inside double quotes, where a `'` is a plain character.
    Brace {
        quoted: bool,
    },
    /// A `$(` whose body starts here.
    Sub(usize),
    /// A plain `(` inside a command context.
    Group,
    /// A backtick whose body starts here.
    Tick(usize),
    /// A `case … esac`, where a pattern's `)` closes nothing, and where in it the pass is.
    Case(CasePart),
    /// A `#` comment, to the end of its line.
    Comment,
    /// A heredoc's body, and where it starts.
    Body(Pending, usize),
}

/// Where in a `case … esac` the pass is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CasePart {
    /// The word before `in`.
    Head,
    /// Where a pattern stands: after `in`, `;;`, `;&` or `;;&`. A `(` here opens the pattern
    /// (POSIX's optional leading paren) and pushes nothing.
    Pattern,
    /// A branch's commands, after the pattern's `)`.
    Branch,
}

/// The shell's reserved words after which a command may follow at once.
const LEADS_A_COMMAND: [&str; 10] = [
    "then", "do", "else", "elif", "if", "while", "until", "!", "{", "time",
];

/// Where each command substitution in `chars` closes, keyed by where its body starts: the `)`
/// that balances a `$(`, or the backtick that ends one. Quotes are read as quotes, `$' … '` with
/// its escapes, a `'` inside a double-quoted `${ … }` as a plain character, a `#` comment to its
/// line's end, a `case` pattern's `)` as no close, and a heredoc body as a shell reads one: from
/// the line after its `<<` to its delimiter's line, at any depth, with its substitutions live
/// where the delimiter is unquoted. One left open has no entry.
fn closes(chars: &[char]) -> Closes {
    let mut out = Closes::default();
    let mut stack: Vec<Ctx> = Vec::new();
    // Heredocs whose `<<` is read and whose bodies start after the next newline, in order.
    let mut pending: std::collections::VecDeque<Pending> = std::collections::VecDeque::new();
    // In a command context: whether the next word is in command position.
    let mut command_position = true;
    out.gone = vec![false; chars.len()];
    let mut k = 0;
    while k < chars.len() {
        if chars[k] == '\\' && chars.get(k + 1) == Some(&'\n') && splices(stack.last()) {
            out.gone[k] = true;
            out.gone[k + 1] = true;
            k += 2;
            continue;
        }
        let c = chars[k];
        // The character after this one, past any line continuation, and where it stands.
        let nj = past_continuations(chars, k + 1);
        let next = chars.get(nj).copied();
        match stack.last() {
            Some(Ctx::Single) => {
                if c == '\'' {
                    stack.pop();
                }
            }
            Some(Ctx::AnsiC) => match c {
                '\\' => k += 1,
                '\'' => {
                    stack.pop();
                }
                _ => {}
            },
            Some(Ctx::Comment) => {
                if c == '\n' {
                    // The newline ends the comment and is read again in the command context.
                    stack.pop();
                    continue;
                }
            }
            Some(Ctx::Tick(from)) => match c {
                '\\' => k += 1,
                '`' => {
                    out.closes.insert(*from, k);
                    stack.pop();
                }
                _ => {}
            },
            Some(Ctx::Body(body, started)) => {
                let started = *started;
                let line_end = (k..chars.len())
                    .find(|&j| chars[j] == '\n')
                    .unwrap_or(chars.len());
                if k == 0 || chars[k - 1] == '\n' {
                    let mut line = &chars[k..line_end];
                    if body.dash {
                        while line.first() == Some(&'\t') {
                            line = &line[1..];
                        }
                    }
                    if line == body.delim.as_slice() {
                        stack.pop();
                        out.bodies.push((started, line_end));
                        if let Some(more) = pending.pop_front() {
                            stack.push(Ctx::Body(more, line_end + 1));
                        }
                        k = line_end + 1;
                        continue;
                    }
                    if !body.expands {
                        k = line_end + 1;
                        continue;
                    }
                }
                match c {
                    '\\' => k += 1,
                    '`' => {
                        stack.push(Ctx::Tick(k + 1));
                        out.all.push((k + 1, Opened::Tick { quoted: false }));
                    }
                    '$' if next == Some('(') => {
                        stack.push(Ctx::Sub(nj + 1));
                        out.all.push((nj + 1, Opened::Dollar));
                        k = jump(&mut out.gone, chars, k, nj);
                    }
                    '$' if next == Some('{') => {
                        stack.push(Ctx::Brace { quoted: true });
                        k = jump(&mut out.gone, chars, k, nj);
                    }
                    _ => {}
                }
            }
            Some(Ctx::Double) => match c {
                '\\' => k += 1,
                '"' => {
                    stack.pop();
                }
                '$' if next == Some('(') => {
                    stack.push(Ctx::Sub(nj + 1));
                    command_position = true;
                    out.all.push((nj + 1, Opened::Dollar));
                    k = jump(&mut out.gone, chars, k, nj);
                }
                '$' if next == Some('{') => {
                    stack.push(Ctx::Brace { quoted: true });
                    k = jump(&mut out.gone, chars, k, nj);
                }
                '`' => {
                    stack.push(Ctx::Tick(k + 1));
                    out.all.push((k + 1, Opened::Tick { quoted: true }));
                }
                _ => {}
            },
            Some(Ctx::Brace { quoted }) => {
                let quoted = *quoted;
                match c {
                    '\\' => k += 1,
                    '}' => {
                        stack.pop();
                    }
                    '\'' if !quoted => stack.push(Ctx::Single),
                    '"' => stack.push(Ctx::Double),
                    '`' => {
                        stack.push(Ctx::Tick(k + 1));
                        out.all.push((k + 1, Opened::Tick { quoted }));
                    }
                    '$' if next == Some('(') => {
                        stack.push(Ctx::Sub(nj + 1));
                        command_position = true;
                        out.all.push((nj + 1, Opened::Dollar));
                        k = jump(&mut out.gone, chars, k, nj);
                    }
                    '$' if next == Some('{') => {
                        stack.push(Ctx::Brace { quoted });
                        k = jump(&mut out.gone, chars, k, nj);
                    }
                    '$' if next == Some('\'') && !quoted => {
                        stack.push(Ctx::AnsiC);
                        k = jump(&mut out.gone, chars, k, nj);
                    }
                    _ => {}
                }
            }
            // A command context: the top level, a substitution's body, a group or a `case`.
            _ => {
                let word_start = k == 0 || " \t\n;&|()".contains(chars[k - 1]);
                let part = match stack.last() {
                    Some(Ctx::Case(part)) => Some(*part),
                    _ => None,
                };
                // `in` ends a case's head, and `esac` may stand where a pattern would.
                if word_start
                    && is_word_char(c)
                    && matches!(part, Some(CasePart::Head | CasePart::Pattern))
                {
                    let mut word = String::new();
                    let mut end = k;
                    while let Some(&w) = chars.get(end).filter(|&&w| is_word_char(w)) {
                        word.push(w);
                        end = past_continuations(chars, end + 1);
                    }
                    let ends_here = chars.get(end).is_none_or(|&e| " \t\n;&|()<>".contains(e));
                    let next_part = match (part, word.as_str()) {
                        (Some(CasePart::Head), "in") => Some(true),
                        (Some(CasePart::Pattern), "esac") => Some(false),
                        _ => None,
                    };
                    if let (true, Some(opens)) = (ends_here, next_part) {
                        if opens {
                            if let Some(Ctx::Case(part)) = stack.last_mut() {
                                *part = CasePart::Pattern;
                            }
                        } else {
                            stack.pop();
                        }
                        command_position = false;
                        jump(&mut out.gone, chars, k, end);
                        k = end;
                        continue;
                    }
                }
                if command_position && word_start && is_word_char(c) {
                    // The word as the shell reads it, line continuations taken out.
                    let mut word = String::new();
                    let mut end = k;
                    while let Some(&w) = chars.get(end).filter(|&&w| is_word_char(w)) {
                        word.push(w);
                        end = past_continuations(chars, end + 1);
                    }
                    let ends_here = chars.get(end).is_none_or(|&e| " \t\n;&|()<>".contains(e));
                    let keyword = ends_here
                        && (word == "case"
                            || (word == "esac" && matches!(stack.last(), Some(Ctx::Case(_))))
                            || LEADS_A_COMMAND.contains(&word.as_str()));
                    if keyword {
                        if word == "case" {
                            stack.push(Ctx::Case(CasePart::Head));
                            command_position = false;
                        } else if word == "esac" {
                            stack.pop();
                            command_position = false;
                        }
                        jump(&mut out.gone, chars, k, end);
                        k = end;
                        continue;
                    }
                }
                match c {
                    ' ' | '\t' => {}
                    // `;;`, `;&` and `;;&` end a branch: a pattern stands next.
                    ';' if part == Some(CasePart::Branch) && matches!(next, Some(';' | '&')) => {
                        command_position = true;
                        if let Some(Ctx::Case(part)) = stack.last_mut() {
                            *part = CasePart::Pattern;
                        }
                        k = jump(&mut out.gone, chars, k, nj);
                        let after = past_continuations(chars, k + 1);
                        if chars.get(after) == Some(&'&') {
                            k = jump(&mut out.gone, chars, k, after);
                        }
                    }
                    ';' | '&' | '|' => command_position = true,
                    '\n' => {
                        command_position = true;
                        if let Some(first) = pending.pop_front() {
                            stack.push(Ctx::Body(first, k + 1));
                        }
                    }
                    '#' if word_start => stack.push(Ctx::Comment),
                    '\\' => {
                        command_position = false;
                        k += 1;
                    }
                    '\'' => {
                        command_position = false;
                        stack.push(Ctx::Single);
                    }
                    '"' => {
                        command_position = false;
                        stack.push(Ctx::Double);
                    }
                    '`' => {
                        command_position = false;
                        stack.push(Ctx::Tick(k + 1));
                        out.all.push((k + 1, Opened::Tick { quoted: false }));
                    }
                    '$' if next == Some('(') => {
                        stack.push(Ctx::Sub(nj + 1));
                        command_position = true;
                        out.all.push((nj + 1, Opened::Dollar));
                        k = jump(&mut out.gone, chars, k, nj);
                    }
                    '$' if next == Some('{') => {
                        command_position = false;
                        stack.push(Ctx::Brace { quoted: false });
                        k = jump(&mut out.gone, chars, k, nj);
                    }
                    '$' if next == Some('\'') => {
                        command_position = false;
                        stack.push(Ctx::AnsiC);
                        k = jump(&mut out.gone, chars, k, nj);
                    }
                    '<' if next == Some('<') => {
                        let third = past_continuations(chars, nj + 1);
                        if chars.get(third) == Some(&'<') {
                            k = jump(&mut out.gone, chars, k, third); // a here-string
                        } else if let Some((heredoc, end)) = heredoc_header(chars, nj + 1) {
                            jump(&mut out.gone, chars, k, nj);
                            pending.push_back(heredoc);
                            k = end;
                            continue;
                        } else {
                            k = jump(&mut out.gone, chars, k, nj);
                        }
                    }
                    // A pattern's optional leading `(` opens nothing.
                    '(' if part == Some(CasePart::Pattern) => out.case_opens.push(k),
                    '(' => {
                        command_position = true;
                        if !stack.is_empty() {
                            stack.push(Ctx::Group);
                        }
                    }
                    ')' => {
                        command_position = true;
                        match stack.last_mut() {
                            // A pattern's `)`: the command after it is in command position.
                            Some(Ctx::Case(part)) => {
                                out.case_patterns.push(k);
                                *part = CasePart::Branch;
                            }
                            // A group's `)`, or one at the top level that closes nothing,
                            // records none.
                            _ => {
                                if let Some(Ctx::Sub(from)) = stack.pop() {
                                    out.closes.insert(from, k);
                                }
                            }
                        }
                    }
                    _ => command_position = false,
                }
            }
        }
        k += 1;
    }
    // A body left open runs to the end of the text.
    for ctx in &stack {
        if let Ctx::Body(_, started) = ctx {
            out.bodies.push((*started, chars.len()));
        }
    }
    out
}

/// Whether a backslash-newline is a line continuation the shell takes out, in `ctx`: anywhere
/// but inside single quotes, `$' … '`, a comment and a quoted heredoc's body.
fn splices(ctx: Option<&Ctx>) -> bool {
    match ctx {
        Some(Ctx::Single | Ctx::AnsiC | Ctx::Comment) => false,
        Some(Ctx::Body(body, _)) => body.expands,
        _ => true,
    }
}

/// The first index from `at` on that does not start a line continuation.
fn past_continuations(chars: &[char], mut at: usize) -> usize {
    while chars.get(at) == Some(&'\\') && chars.get(at + 1) == Some(&'\n') {
        at += 2;
    }
    at
}

/// Moves from `k` to `to`, marking each line continuation stepped over as taken out, and
/// answers `to`.
fn jump(gone: &mut [bool], chars: &[char], k: usize, to: usize) -> usize {
    let mut j = k + 1;
    while j < to.min(chars.len()) {
        if chars[j] == '\\' && chars.get(j + 1) == Some(&'\n') {
            gone[j] = true;
            gone[j + 1] = true;
            j += 2;
        } else {
            j += 1;
        }
    }
    to
}

/// A character of a reserved word.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '!' | '{')
}

/// The heredoc whose header starts at `at`, just past its `<<`, and where the header ends: an
/// optional `-`, blanks, then the delimiter word, whose quoting is taken off and tells whether
/// the body expands. `None` when no word follows.
fn heredoc_header(chars: &[char], mut at: usize) -> Option<(Pending, usize)> {
    // A line continuation anywhere in the header is taken out first, as the shell does: it
    // neither quotes the delimiter nor ends it (`<<E\` newline `OF` is `<<EOF`, #1417).
    at = past_continuations(chars, at);
    let dash = chars.get(at) == Some(&'-');
    if dash {
        at = past_continuations(chars, at + 1);
    }
    while matches!(chars.get(at), Some(' ' | '\t')) {
        at += 1;
    }
    let mut delim = Vec::new();
    let mut expands = true;
    while let Some(&c) = chars.get(at) {
        match c {
            '\\' if chars.get(at + 1) == Some(&'\n') => at += 2,
            '\'' | '"' => {
                expands = false;
                let close = (at + 1..chars.len()).find(|&j| chars[j] == c)?;
                delim.extend_from_slice(&chars[at + 1..close]);
                at = close + 1;
            }
            '\\' => {
                expands = false;
                delim.extend(chars.get(at + 1).copied());
                at += 2;
            }
            c if c.is_whitespace() || ";&|()<>".contains(c) => break,
            _ => {
                delim.push(c);
                at += 1;
            }
        }
    }
    (!delim.is_empty() || !expands).then_some((
        Pending {
            delim,
            expands,
            dash,
        },
        at,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Vec<String> {
        scan(text, true)
            .map(|it| it.substitutions())
            .unwrap_or_default()
    }

    #[test]
    fn a_case_pattern_ends_its_command_and_a_body_is_blanked() {
        assert_eq!(
            scan("case x in x) cat y;; esac", false)
                .expect("a case")
                .as_commands(false),
            "case x in x; cat y;; esac"
        );
        // A line continuation in a heredoc's delimiter neither quotes nor ends it.
        assert_eq!(run("cat <<E\\\nOF\n$(cat x)\nEOF"), ["cat x"]);
        assert_eq!(run("cat <<\\\nEOF\n$(cat x)\nEOF"), ["cat x"]);
        assert_eq!(
            run("cat <<'E\\\nOF'\n$(cat x)\nE\\\nOF"),
            Vec::<String>::new()
        );
        // A pattern's optional leading paren, in every branch, and `;&` / `;;&` between them.
        assert_eq!(
            scan(
                "case x in (y) :;; (x) cat y;& (z|w) cat z;;& *) (cat q);; esac",
                false
            )
            .expect("a case")
            .as_commands(false),
            "case x in  y; :;;  x; cat y;&  z|w; cat z;;& *; (cat q);; esac"
        );
        assert_eq!(
            run("echo \"$(case x in (x) cat y;; esac)\""),
            ["case x in (x) cat y;; esac"]
        );
        assert_eq!(
            scan("cat <<EOF\nit's\n$(x)\nEOF\ncat y", true)
                .expect("a substitution")
                .as_commands(true),
            "cat <<EOF\n    \n    \n   \ncat y"
        );
    }

    #[test]
    fn a_line_continuation_is_taken_out_before_anything_is_opened() {
        assert_eq!(run("echo \"$\\\n(cat x)\""), ["cat x"]);
        assert_eq!(run("echo $\\\n{u:-$(cat x)}"), ["cat x"]);
        assert_eq!(run("echo `ca\\\nt x`"), ["cat x"]);
        assert_eq!(run("cat <\\\n<EOF\n$(cat x)\nEOF"), ["cat x"]);
        assert_eq!(
            scan("ca\\\nse x in x) ca\\\nt y;; esac", false)
                .expect("a continuation")
                .as_commands(false),
            "case x in x; cat y;; esac"
        );
        // Kept inside single quotes, a comment and a quoted heredoc's body.
        assert_eq!(run("echo '$\\\n(cat x)'"), Vec::<String>::new());
        assert_eq!(run("echo hi # \\\necho $(cat x)"), ["cat x"]);
        assert_eq!(run("cat <<'EOF'\n$\\\n(cat x)\nEOF"), Vec::<String>::new());
    }

    #[test]
    fn a_quote_in_ansi_c_quoting_is_escaped_and_does_not_hide_what_follows() {
        assert_eq!(run(r#"echo $'\'' "$(cat x)" $'\''"#), ["cat x"]);
    }

    #[test]
    fn a_single_quote_in_a_double_quoted_parameter_default_is_a_plain_character() {
        assert_eq!(run(r#"echo "${u:-'$(cat x)'}""#), ["cat x"]);
        assert_eq!(run(r#"echo "${u-'}$(cat x)${u-'}""#), ["cat x"]);
        // Unquoted, the same quotes do quote.
        assert_eq!(run("echo ${u:-'$(cat x)'}"), Vec::<String>::new());
    }

    #[test]
    fn a_paren_in_a_comment_does_not_close_the_substitution() {
        assert_eq!(
            run("echo \"$(echo hi # )\ncat x\n)\""),
            ["echo hi # )\ncat x\n"]
        );
        // A `#` inside a word is not a comment.
        assert_eq!(run("echo \"$(echo a#b)\" $(cat x)"), ["echo a#b", "cat x"]);
    }

    #[test]
    fn a_heredoc_body_inside_a_substitution_is_text_to_its_delimiter() {
        assert_eq!(
            run("echo \"$(cat <<'EOF'\n)\nEOF\ncat x)\""),
            ["cat <<'EOF'\n)\nEOF\ncat x"]
        );
        assert_eq!(
            run("echo \"$(cat <<-EOF\n)\n\tEOF\ncat x)\""),
            ["cat <<-EOF\n)\n\tEOF\ncat x"]
        );
    }

    #[test]
    fn an_unquoted_heredoc_body_runs_its_substitutions_and_a_quoted_one_does_not() {
        assert_eq!(run("cat <<EOF\nit's\n$(cat x)\nEOF"), ["cat x"]);
        assert_eq!(run("cat <<'EOF'\n$(cat x)\nEOF"), Vec::<String>::new());
        assert_eq!(run("cat <<\\EOF\n$(cat x)\nEOF\n$(cat y)"), ["cat y"]);
    }

    #[test]
    fn a_case_pattern_does_not_close_the_substitution() {
        assert_eq!(
            run("echo \"$(case x in x) cat y;; esac)\" $(cat z)"),
            ["case x in x) cat y;; esac", "cat z"]
        );
        // `case` as an argument is a word, not the keyword.
        assert_eq!(run("echo \"$(echo case x) cat y)\""), ["echo case x"]);
    }
}
