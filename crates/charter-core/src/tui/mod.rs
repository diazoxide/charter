//! charter's terminal-layout algebra: a port of `charter/tui.py`.
//!
//! The status line re-renders on every prompt, so column arithmetic lives in one tested
//! place instead of ad-hoc padding at call sites. The one hard guarantee, which every node
//! here enforces: **no rendered line ever exceeds the requested width** (visible columns;
//! ANSI SGR escapes count as zero). Overflow is truncated with an ellipsis, never wrapped —
//! a single wrapped line shears every column below it. Rendered lines also never carry
//! trailing whitespace, even when it hides behind trailing colour escapes.
//!
//! That guarantee is about the terminal, so it cannot be computed from a string the terminal
//! reads differently: **SGR is the only escape this module passes through**, and [`sanitize`]
//! drops everything else — a cursor move, an erase, an OSC title string, a bare control
//! character — before anything is measured or clamped (charter #326).
//!
//! # A column is what Python says it is, character for character
//!
//! `charter/tui.py:_char_width` is three questions, two of them Unicode table lookups:
//!
//! ```text
//! if ch < "\x80":                 return 1
//! if unicodedata.combining(ch):   return 0
//! return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
//! ```
//!
//! **This port answers them identically, and that is a decision rather than an accident.**
//! Two earlier ports in this repo (`charter-cli`'s `voice::width`, `doctor::width`) count
//! *characters*, which is right only where every value is ASCII — and a workspace
//! *directory* name is not validated by charter, so a CJK or combining-mark name reaches a
//! column. Counting characters there under-pads by one cell per CJK glyph and over-pads by
//! one per combining mark, and the row silently stops lining up with its neighbours. So
//! `voice` now calls this module and the character count is gone from it.
//!
//! The tables are **generated from CPython's own `unicodedata`** (`tools/gen-width-tables.py`
//! → [`tables`]) rather than taken from a crate: `unicode-width` deviates from
//! `east_asian_width` deliberately — it widens emoji presentation sequences and zeroes
//! default-ignorables — and a deliberate deviation is exactly what a byte-for-byte
//! differential cannot carry. The one residual risk is stated rather than hidden: the tables
//! are of one Unicode version (the header of `tables.rs` names it), and a CI Python built
//! against a different one could disagree about a codepoint whose class changed. The
//! differential renders a CJK workspace name, so such a drift fails a test instead of
//! shifting a column nobody is looking at.
//!
//! What this module still does not answer is whether a cell is **legible**: [`width`]
//! reports the cells a value *declares*, which is what alignment is made of; three U+3164
//! HANGUL FILLERs declare six and render as nothing. [`crate::shown::readable`] is the place
//! that question is asked.

pub mod tables;

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// SGR "everything off", appended by [`truncate`] so an open style cannot bleed past a cut.
pub const RESET: &str = "\x1b[0m";

/// What marks a cut. U+2026, one column (its east-asian width is Ambiguous, which is 1).
pub const ELLIPSIS: &str = "…";

/// The environment variables that describe **the terminal a process was launched from**, as
/// opposed to the one it is drawing into. [`term_width`] reads the first of them.
///
/// `LINES` is here although nothing reads it yet, for the reason `charter/tui.py` gives:
/// "deliberately not read" is a decision that can change, and naming the pair costs nothing.
pub const TERMINAL_SIZE_VARS: [&str; 2] = ["COLUMNS", "LINES"];

/// `\x1b[…m` — charter's own markup, and the only escape that survives [`sanitize`].
static SGR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

/// Trailing whitespace hiding *behind* trailing SGR escapes (`"a \x1b[0m"`).
static HIDDEN_TRAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]+((?:\x1b\[[0-9;]*m)+)$").unwrap());

/// Everything this module may be handed, split into "charter's own markup" and "not".
///
/// **Alternation order is the whole design**, and it is Python's, kept: SGR is matched FIRST
/// and handed back untouched, so the catch-all `\x1b` at the end can delete a stray
/// introducer without decapitating a colour span and spilling `[32m` onto the screen as text.
/// The `regex` crate resolves an alternation leftmost-first, as Python's `re` does, so the
/// order means the same thing on both sides.
///
/// The rest, in the order a terminal would parse it. The *string* sequences (OSC, DCS, APC,
/// PM, SOS) come before the single-character forms because they must go whole: dropping only
/// the introducer leaves `0;pwned` printing as ordinary text, which is worse than the escape
/// because it looks like data. An unterminated one is removed to the end of the string for
/// that same reason.
static MARKUP_OR_CONTROL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(\x1b\[[0-9;]*m)",                           // 1 — SGR: charter's own, kept
        r"|\x1b[\]P^_X][^\x1b\x07]*(?:\x07|\x1b\\|$)", // OSC/DCS/APC/PM/SOS … ST | BEL | end
        r"|\x1b\[[0-?]*[ -/]*[@-~]",                   // any other CSI (cursor, erase, …)
        r"|\x1b[^\x1b]",                               // other two-character escapes
        r"|\x1b",                                      // a lone/trailing ESC
        r"|([\t\n\r\x0b\x0c])",                        // 2 — whitespace controls → a space
        r"|[\x00-\x1f\x7f-\x9f]",                      // other C0, DEL, C1: removed
    ))
    .unwrap()
});

/// Return `s` with everything that is not charter's own colour markup removed.
///
/// This module measures and clamps *markup*, and its contract has always been that markup is
/// charter's: SGR escapes, which cost zero columns, and text, which costs its own columns.
/// Nothing enforced that. A forge's JSON, a directory name off the filesystem — anything that
/// reaches a cell — could carry an erase-in-display, an OSC title string or a bare BEL, and
/// the module would count it as visible columns, pad around a width the terminal disagreed
/// with, and copy it to a surface that repaints with no human in the loop (charter #326).
///
/// Deliberately a *narrow* removal rather than a whitelist of printable text. This module
/// exists to emit SGR — a blanket strip would fight the colour it is built to produce — so
/// only what a terminal would interpret as a command rather than as content is dropped.
///
/// **Python's `s.isprintable()` fast path is not reproduced, and does not need to be.** Every
/// codepoint the pattern matches is one `str.isprintable` calls unprintable, so a string that
/// takes Python's fast path is one this regex leaves unchanged. Reproducing the test would
/// mean carrying Unicode general categories for no behavioural difference at all.
pub fn sanitize(s: &str) -> Cow<'_, str> {
    MARKUP_OR_CONTROL.replace_all(s, |caps: &Captures| {
        if let Some(sgr) = caps.get(1) {
            return sgr.as_str().to_owned();
        }
        // A tab jumps to the next tab stop and a newline ends the line — both shear the
        // columns below, which is the one thing this module promises not to do. They keep
        // their separation as a single space rather than vanishing, so `a\tb` stays two words
        // instead of becoming one.
        if caps.get(2).is_some() {
            " ".to_owned()
        } else {
            String::new()
        }
    })
}

/// `s` as the plain text a terminal would show: SGR removed, and — since anything else that
/// claims to be an escape is not charter's markup either — [`sanitize`] applied first.
pub fn strip_ansi(s: &str) -> String {
    let s = sanitize(s);
    if s.contains('\x1b') {
        SGR.replace_all(&s, "").into_owned()
    } else {
        s.into_owned()
    }
}

/// Terminal cell width of one character: 0 combining, 2 east-asian wide, else 1.
pub fn char_width(ch: char) -> usize {
    if (ch as u32) < 0x80 {
        return 1;
    }
    if in_table(ch, &tables::COMBINING) {
        return 0;
    }
    if in_table(ch, &tables::WIDE) { 1 } else { 2 }
}

fn in_table(ch: char, table: &[(u32, u32)]) -> bool {
    let cp = ch as u32;
    table
        .binary_search_by(|&(lo, hi)| {
            if cp < lo {
                std::cmp::Ordering::Greater
            } else if cp > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Visible terminal width of markup `s` (ANSI stripped, wide characters two).
pub fn width(s: &str) -> usize {
    let s = strip_ansi(s);
    if s.is_ascii() {
        // The fast path Python keeps, for the overwhelmingly common case. Identical by
        // construction: every ASCII character is one column.
        return s.len();
    }
    s.chars().map(char_width).sum()
}

/// How many bytes of `s`, starting at `at`, are one SGR escape — 0 when that is not what is
/// there.
///
/// The same grammar as [`SGR`], matched by hand because it has to be matched ANCHORED at a
/// position, which is what [`truncate`]'s walk needs and what Python's `_SGR.match(s, i)`
/// does. A `find` would happily skip forward to the next escape and copy the text between.
fn sgr_at(s: &str, at: usize) -> usize {
    let rest = &s.as_bytes()[at..];
    if rest.first() != Some(&0x1b) || rest.get(1) != Some(&b'[') {
        return 0;
    }
    let mut i = 2;
    while let Some(&b) = rest.get(i) {
        match b {
            b'0'..=b'9' | b';' => i += 1,
            b'm' => return i + 1,
            _ => return 0,
        }
    }
    0
}

/// Clamp markup `s` to at most `w` visible columns.
///
/// Returns `s` unchanged when it already fits. When cut, ANSI escapes are carried through
/// verbatim (so colour spans stay intact), [`ELLIPSIS`] marks the cut, and a reset is appended
/// so open styles never bleed into whatever is printed next.
pub fn truncate(s: &str, w: usize) -> String {
    if w == 0 {
        return String::new();
    }
    // Before the fits-already fast path, not after: returning `s` unchanged is exactly how an
    // escape reached the screen without anything ever having looked at it (charter #326).
    let s = sanitize(s);
    if width(&s) <= w {
        return s.into_owned();
    }
    let keep = w.saturating_sub(width(ELLIPSIS));
    let mut out = String::with_capacity(s.len());
    let mut vis = 0usize;
    let mut i = 0usize;
    let bytes = s.len();
    while i < bytes {
        let esc = sgr_at(&s, i);
        if esc > 0 {
            // The whole escape, verbatim and at zero visible width.
            out.push_str(&s[i..i + esc]);
            i += esc;
            continue;
        }
        let ch = s[i..]
            .chars()
            .next()
            .expect("byte index is a char boundary");
        let cw = char_width(ch);
        if vis + cw > keep {
            break;
        }
        out.push(ch);
        vis += cw;
        i += ch.len_utf8();
    }
    out.push_str(ELLIPSIS);
    if s.contains('\x1b') {
        out.push_str(RESET);
    }
    out
}

/// Which edge short markup is pushed against by [`pad`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

/// Fit markup `s` to exactly `w` visible columns (truncate long, pad short).
///
/// Padding is plain spaces appended outside any colour span, so a style such as underline
/// never leaks into the padding.
pub fn pad(s: &str, w: usize, align: Align) -> String {
    let s = truncate(s, w);
    let fill = w.saturating_sub(width(&s));
    if fill == 0 {
        return s;
    }
    match align {
        Align::Right => format!("{}{s}", " ".repeat(fill)),
        Align::Center => {
            let left = fill / 2;
            format!("{}{s}{}", " ".repeat(left), " ".repeat(fill - left))
        }
        Align::Left => format!("{s}{}", " ".repeat(fill)),
    }
}

/// Visible width for one table column: its `header`, its widest cell, and `gap`.
///
/// The width has to come from the values about to be printed — a hand-written `{name:<28}` is
/// a guess about content that pads a short value and pushes a long one, and that row's
/// remaining columns then land where no other row's do. `gap` is the separation from the next
/// column, counted inside the returned width so a caller pads once rather than padding and
/// then adding spaces. A `cap` bounds the content (never the gap) for a column whose values
/// are prose; leave it `None` where the reader has to be able to read the value back off the
/// row, because a clipped name is one they cannot go and act on.
pub fn column<'a>(
    header: &str,
    cells: impl IntoIterator<Item = &'a str>,
    gap: usize,
    cap: Option<usize>,
) -> usize {
    let w = cells
        .into_iter()
        .map(width)
        .fold(width(header), std::cmp::max);
    let content = match cap {
        Some(cap) => w.min(cap),
        None => w,
    };
    content + gap
}

/// `$COLUMNS` as a number, or `None` when it is unset or is not one.
///
/// Answers what the variable *says*, never whether it is usable — see [`term_width`] for why
/// that judgement is made in exactly one place.
fn env_columns(env: &dyn Fn(&str) -> Option<String>) -> Option<i64> {
    // `int(os.environ[...])` — Python tolerates surrounding whitespace and a leading sign,
    // and so does `i64::from_str` for the sign. The space is trimmed for the same reason.
    env(TERMINAL_SIZE_VARS[0])?.trim().parse::<i64>().ok()
}

/// The tty's own column count, or `None` when there is no tty to ask.
///
/// Python asks `os.get_terminal_size()`, which is `TIOCGWINSZ` on **stdout**; this asks the
/// same descriptor through `rustix`, because `unsafe_code = "forbid"` rules out calling libc.
#[cfg(unix)]
fn tty_columns() -> Option<i64> {
    let size = rustix::termios::tcgetwinsize(std::io::stdout()).ok()?;
    Some(i64::from(size.ws_col))
}

#[cfg(not(unix))]
fn tty_columns() -> Option<i64> {
    None
}

/// Terminal width: `$COLUMNS`, else the tty size, else `default`; clamped to at least `floor`.
///
/// Status-line style programs get their size via `$COLUMNS` because stdout is a pipe, hence
/// the env-first order.
///
/// **A width is a positive number, whichever source produced it — and that is asked once,
/// here, rather than by each source about itself.** charter's env branch used to carry its own
/// zero check, because a real environment exports `COLUMNS=0` and `int("0")` parses happily;
/// the guard was attached to a *spelling* rather than to the property, so one rung lower the
/// identical value walked straight through and a tty reporting zero columns drew every table
/// one column wide (charter #594). A pty created without a window size reports zero until
/// someone calls `TIOCSWINSZ`, which is ordinary rather than exotic.
///
/// So each source answers with a number or with nothing, the *answer* is what is judged, and a
/// source with no usable answer is indistinguishable from a source with no answer at all.
pub fn term_width(env: &dyn Fn(&str) -> Option<String>, default: usize, floor: usize) -> usize {
    judged([env_columns(env), tty_columns()], default, floor)
}

/// The half of [`term_width`] that judges the answers, apart from the half that asks.
///
/// Apart because the asking reaches this process's real stdout, and a test that could not
/// drive it would be a test of whatever terminal the suite happened to run under. `default`
/// is deliberately not one of the `asked`: it is the caller's own stated fallback for
/// "nothing could be measured", and `floor` is the last word on all three paths.
fn judged(asked: [Option<i64>; 2], default: usize, floor: usize) -> usize {
    for answer in asked {
        if let Some(w) = answer.filter(|w| *w >= 0) {
            return floor.max(w as usize);
        }
    }
    floor.max(default)
}

/// This process's own environment, for the callers that are not a test.
pub fn ambient(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Strip trailing whitespace, including any hiding behind trailing escapes.
pub fn finish(line: &str) -> String {
    let mut line = line.trim_end_matches([' ', '\t']).to_owned();
    loop {
        let cut = HIDDEN_TRAIL.replace(&line, "$1").into_owned();
        if cut == line {
            return line;
        }
        line = cut.trim_end_matches([' ', '\t']).to_owned();
    }
}

// --------------------------------------------------------------------------------------- //
// layout nodes                                                                              //
// --------------------------------------------------------------------------------------- //

/// A [`Row`](Node::Row) ingredient: markup with an optional fixed visible width.
///
/// `width: None` keeps the natural width (no padding or truncation — the row still clamps the
/// assembled line). A fixed width pads or truncates the cell to exactly that many columns.
#[derive(Debug, Clone)]
pub struct Cell {
    pub markup: String,
    pub width: Option<usize>,
    pub align: Align,
}

impl Cell {
    /// A natural-width cell, which is what a plain string is to a row.
    pub fn new(markup: impl Into<String>) -> Self {
        Self {
            markup: markup.into(),
            width: None,
            align: Align::Left,
        }
    }

    /// A cell padded or cut to exactly `w` columns, so a table's columns stay aligned across
    /// sibling rows without any call-site padding maths.
    pub fn fixed(markup: impl Into<String>, w: usize, align: Align) -> Self {
        Self {
            markup: markup.into(),
            width: Some(w),
            align,
        }
    }
}

/// One column of a [`Columns`](Node::Columns): either a node or plain markup lines.
#[derive(Debug, Clone)]
pub enum Block {
    Node(Box<Node>),
    Lines(Vec<String>),
}

impl Block {
    fn render(&self, w: usize) -> Vec<String> {
        match self {
            Block::Node(node) => node.render(w),
            Block::Lines(lines) => lines.iter().map(|ln| finish(&truncate(ln, w))).collect(),
        }
    }
}

/// A layout node. Every node renders to a list of finished lines: each is at most the
/// requested number of visible columns (truncated with `…`, never wrapped) and carries no
/// trailing whitespace.
#[derive(Debug, Clone)]
pub enum Node {
    /// One or more lines of raw markup, clamped at render time. Embedded newlines split into
    /// multiple lines; each is clamped separately.
    Text(String),
    /// A single line of cells joined by `gap`, clamped to the render width.
    Row { cells: Vec<Cell>, gap: String },
    /// Vertical composition: the children's lines, top to bottom.
    Stack(Vec<Node>),
    /// Side-by-side columns of **independent heights**. A `None` width is a flex column that
    /// shares whatever space remains after the fixed columns and the gaps. Shorter columns are
    /// blank-filled so rows stay aligned however tall each column is; every assembled row is
    /// clamped to the render width, so an over-budget spec degrades to truncated lines instead
    /// of wrapping.
    Columns {
        columns: Vec<(Block, Option<usize>)>,
        gap: String,
    },
}

impl Node {
    /// Markup lines stacked, which is what nearly every caller builds.
    pub fn stack(lines: impl IntoIterator<Item = String>) -> Self {
        Node::Stack(lines.into_iter().map(Node::Text).collect())
    }

    /// Render to lines, each clamped to `width` visible columns.
    pub fn render(&self, width: usize) -> Vec<String> {
        match self {
            Node::Text(markup) => markup
                .split('\n')
                .map(|ln| finish(&truncate(ln, width)))
                .collect(),
            Node::Row { cells, gap } => {
                let parts: Vec<String> = cells
                    .iter()
                    .map(|c| match c.width {
                        None => c.markup.clone(),
                        Some(w) => pad(&c.markup, w, c.align),
                    })
                    .collect();
                vec![finish(&truncate(&parts.join(gap), width))]
            }
            Node::Stack(children) => children.iter().flat_map(|c| c.render(width)).collect(),
            Node::Columns { columns, gap } => render_columns(columns, gap, width),
        }
    }
}

fn render_columns(columns: &[(Block, Option<usize>)], gap: &str, width: usize) -> Vec<String> {
    if columns.is_empty() {
        return Vec::new();
    }
    let gap_w = self::width(gap);
    let fixed: usize = columns.iter().filter_map(|(_, w)| *w).sum();
    let flexible = columns.iter().filter(|(_, w)| w.is_none()).count();
    let spare = width
        .saturating_sub(fixed)
        .saturating_sub(gap_w * (columns.len() - 1));
    // Python's `divmod`, and `extra` is an int that goes NEGATIVE once it has been spent —
    // every flex column decrements it, not only the ones that took a spare column.
    let (share, mut extra) = match (spare.checked_div(flexible), spare.checked_rem(flexible)) {
        (Some(share), Some(extra)) => (share as i64, extra as i64),
        // No flexible column at all: there is nothing to share and nothing left over, which
        // is Python's `(0, 0)` and not a division by zero.
        _ => (0, 0),
    };
    let widths: Vec<usize> = columns
        .iter()
        .map(|(_, w)| match w {
            Some(w) => *w,
            None => {
                let w = share + i64::from(extra > 0);
                extra -= 1;
                w.max(0) as usize
            }
        })
        .collect();

    let blocks: Vec<Vec<String>> = columns
        .iter()
        .zip(&widths)
        .map(|((content, _), w)| content.render(*w))
        .collect();
    let height = blocks.iter().map(Vec::len).max().unwrap_or(0);
    (0..height)
        .map(|i| {
            let row = blocks
                .iter()
                .zip(&widths)
                .map(|(block, w)| pad(block.get(i).map_or("", String::as_str), *w, Align::Left))
                .collect::<Vec<_>>()
                .join(gap);
            finish(&truncate(&row, width))
        })
        .collect()
}

#[cfg(test)]
mod tests;
