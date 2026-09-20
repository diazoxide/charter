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
///
/// In the core, because `charter status` trims its own table rows there and two spellings of
/// "what counts as trailing whitespace" is how one row keeps a space the other drops.
pub use charter_core::memstore::py_rstrip;

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
/// The sentence itself is [`charter_core::memstore::cannot_check`], because `docs generate`
/// says the same one about a persona's memory store from inside the core, and one unreadable
/// path worded two ways is two different repairs for one directory.
pub fn unread(root: &Path, unread: &charter_core::memstore::Unread) {
    for (path, code) in unread {
        err(&charter_core::memstore::cannot_check(root, path, *code));
    }
}

/// `charter/workspace.py`'s two helpers behind that sentence, from the core for the same
/// reason: the commands that run there and the commands that print here must not be able to
/// word one unreadable path two ways.
pub use charter_core::memstore::{PATH_LIMIT, uncheckable_fix};

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
