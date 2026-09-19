//! How charter talks: `charter/util.py`'s four loggers and `charter/tui.py`'s column
//! arithmetic, byte for byte.
//!
//! Every line a person or an agent reads from a ported command goes through here, because
//! the words ARE the interface: an agent that reads `charter recall` at session start
//! parses what it prints, and a harness that runs both charters must not be able to tell
//! them apart. The glyph is coloured only when stderr is a terminal — the message never is.

use std::io::IsTerminal;
use std::path::Path;

fn glyph(code: &str, mark: &str) -> String {
    if std::io::stderr().is_terminal() {
        format!("\x1b[{code}m{mark}\x1b[0m")
    } else {
        mark.to_string()
    }
}

/// `util.info`: `• <msg>` on stderr.
pub fn info(msg: &str) {
    eprintln!("{} {msg}", glyph("36", "•"));
}

/// `util.ok`: `✓ <msg>` on stderr.
pub fn ok(msg: &str) {
    eprintln!("{} {msg}", glyph("32", "✓"));
}

/// `util.warn`: `! <msg>` on stderr.
pub fn warn(msg: &str) {
    eprintln!("{} {msg}", glyph("33", "!"));
}

/// `util.err`: `✗ <msg>` on stderr.
pub fn err(msg: &str) {
    eprintln!("{} {msg}", glyph("31", "✗"));
}

/// `str.rstrip()`, with Python's idea of whitespace.
pub fn py_rstrip(text: &str) -> &str {
    text.trim_end_matches(charter_core::memstore::is_python_space)
}

/// `tui.sanitize`: everything that is not charter's own colour markup removed.
///
/// SGR (`ESC [ … m`) is kept; every other escape goes whole — a string sequence (OSC, DCS,
/// APC, PM, SOS) to its terminator or the end, any other CSI, any other two-character
/// escape, a lone ESC. Tab, newline, CR, VT and FF become one space each, so the columns
/// below do not shear; every other C0 control, DEL and C1 is dropped.
pub fn sanitize(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\x1b' {
            i = escape(&chars, i, &mut out);
            continue;
        }
        match c {
            '\t' | '\n' | '\r' | '\x0b' | '\x0c' => out.push(' '),
            '\0'..='\x1f' | '\x7f'..='\u{9f}' => {}
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// One escape starting at `chars[at]`, kept when it is SGR; the index after it.
fn escape(chars: &[char], at: usize, out: &mut String) -> usize {
    let next = chars.get(at + 1).copied();
    if next == Some('[') {
        // SGR: `[0-9;]*m`, kept whole.
        let mut j = at + 2;
        while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == ';') {
            j += 1;
        }
        if chars.get(j) == Some(&'m') {
            out.extend(&chars[at..=j]);
            return j + 1;
        }
        // Any other CSI: `[0-?]*[ -/]*[@-~]`.
        let mut j = at + 2;
        while j < chars.len() && ('0'..='?').contains(&chars[j]) {
            j += 1;
        }
        while j < chars.len() && (' '..='/').contains(&chars[j]) {
            j += 1;
        }
        if chars.get(j).is_some_and(|c| ('@'..='~').contains(c)) {
            return j + 1;
        }
        // Not a complete CSI: the two-character escape it starts with.
        return at + 2;
    }
    if let Some(kind) = next
        && matches!(kind, ']' | 'P' | '^' | '_' | 'X')
    {
        // A string sequence runs to BEL, to ESC \, or to the end.
        let mut j = at + 2;
        while j < chars.len() && chars[j] != '\x1b' && chars[j] != '\x07' {
            j += 1;
        }
        if j == chars.len() {
            return j;
        }
        if chars[j] == '\x07' {
            return j + 1;
        }
        if chars.get(j + 1) == Some(&'\\') {
            return j + 2;
        }
        // Unterminated before another escape: only the two-character escape goes.
        return at + 2;
    }
    match next {
        Some(c) if c != '\x1b' => at + 2,
        _ => at + 1,
    }
}

/// The columns `text` takes once sanitized, SGR costing none. One column a character: a
/// cell here is a date, a base's label or a heading, and charter's own labels are ASCII.
pub fn width(text: &str) -> usize {
    let clean = sanitize(text);
    let mut n = 0;
    let chars: Vec<char> = clean.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\x1b' {
            // Only SGR survives sanitizing.
            while i < chars.len() && chars[i] != 'm' {
                i += 1;
            }
            i += 1;
            continue;
        }
        n += 1;
        i += 1;
    }
    n
}

/// `tui.column("", cells)`: the widest cell, plus the two-space gap to the next column.
pub fn column<'a>(cells: impl IntoIterator<Item = &'a str>) -> usize {
    cells.into_iter().map(width).max().unwrap_or(0) + 2
}

/// `tui.pad(text, w)` for a column [`column`] sized: sanitized, then padded to `w`. The
/// column is never narrower than its widest cell, so nothing is ever cut.
pub fn pad(text: &str, w: usize) -> String {
    let clean = sanitize(text);
    let fill = w.saturating_sub(width(&clean));
    format!("{clean}{}", " ".repeat(fill))
}

/// A path as charter prints it: relative to the plane where it is inside, else as is.
pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// `workspace.say_unread`: one sentence on stderr for each path charter could not look at.
///
/// Named from the plane's root and made readable first — the path is often a filename a
/// chat wrote, and a newline in one would otherwise write a line of its own into whatever
/// reads this (#1084).
pub fn unread(root: &Path, unread: &charter_core::memstore::Unread) {
    for (path, code) in unread {
        let shown = charter_core::shown::readable(&path.to_string_lossy(), PATH_LIMIT);
        let named = charter_core::shown::readable(&rel(root, path), PATH_LIMIT);
        err(&format!(
            "{named} cannot be checked — {}.",
            uncheckable_fix(*code, &shown, "it")
        ));
    }
}

/// How much of a path a sentence repeats back — `contain.PATH_DISPLAY_LIMIT`. A clipped
/// path is one a reader cannot go to.
pub const PATH_LIMIT: usize = 1024;

/// What clears a path charter could not check — `workspace.uncheckable_fix`: a symlink
/// loop names the link; anything else is read as a refusal, which read access clears.
pub fn uncheckable_fix(code: Option<i32>, path: &str, place: &str) -> String {
    if charter_core::recall::is_loop(code) {
        format!("fix the symlink loop at {path}")
    } else {
        format!("restoring read access to {place} clears this")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_markup_is_kept_and_every_other_control_goes() {
        assert_eq!(sanitize("a\x1b[32mb\x1b[0m"), "a\x1b[32mb\x1b[0m");
        assert_eq!(sanitize("a\x1b[2Jb"), "ab", "a CSI that is not SGR");
        assert_eq!(
            sanitize("a\x1b]0;pwned\x07b"),
            "ab",
            "an OSC, payload and all"
        );
        assert_eq!(sanitize("a\x1b]0;pwned"), "a", "unterminated, to the end");
        assert_eq!(sanitize("a\x1bcb"), "ab", "a two-character escape");
        assert_eq!(
            sanitize("a\tb\nc"),
            "a b c",
            "whitespace controls keep a space"
        );
        assert_eq!(sanitize("a\x07b\x7fc\u{85}d"), "abcd");
        assert_eq!(sanitize("a\x1b"), "a", "a trailing ESC");
    }

    #[test]
    fn a_column_is_its_widest_cell_and_a_gap() {
        assert_eq!(column(["2026-03-02", "—"]), 12);
        assert_eq!(pad("—", 12), "—           ");
        assert_eq!(pad("\x1b[32mok\x1b[0m", 4), "\x1b[32mok\x1b[0m  ");
    }
}
