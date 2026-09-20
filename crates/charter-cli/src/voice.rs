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

/// `tui.column("", cells)`: the widest cell, plus the two-space gap to the next column.
///
/// **Measured in COLUMNS, by [`charter_core::tui`], and no longer in characters.** This
/// module used to carry its own `sanitize`/`width`/`pad` — a hand-rolled second copy of
/// `charter/tui.py` that counted one column per character, with the precondition written
/// beside it that "charter's own labels are ASCII". Half the cells here are not charter's: a
/// recall hit's label is a memory's heading, which is whatever an operator wrote. A CJK
/// heading declares two columns per glyph and a combining mark declares none, so the count
/// under-padded one and over-padded the other and the row stopped lining up with its
/// neighbours — against a Python that has measured east-asian width since it was written.
///
/// So there is one width in this repo now, and this is a call into it.
pub fn column<'a>(cells: impl IntoIterator<Item = &'a str>) -> usize {
    charter_core::tui::column("", cells, 2, None)
}

/// `tui.pad(text, w)` for a column [`column`] sized: sanitized, then fitted to `w`.
pub fn pad(text: &str, w: usize) -> String {
    charter_core::tui::pad(text, w, charter_core::tui::Align::Left)
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
    fn a_column_is_its_widest_cell_and_a_gap() {
        assert_eq!(column(["2026-03-02", "—"]), 12);
        assert_eq!(pad("—", 12), "—           ");
        assert_eq!(pad("\x1b[32mok\x1b[0m", 4), "\x1b[32mok\x1b[0m  ");
    }
}
