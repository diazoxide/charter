//! A value as one line a reader can read the value back off.
//!
//! This is `charter/contain.py`'s `readable(value, limit)` — the DISPLAY question, not the
//! containment one. It is a module of its own because [`crate::contain::readable`] already
//! answers something else entirely: whether a path may be read. Python tells the two apart
//! by their arity; in Rust only the arguments would, and a reader skimming a call site
//! would not.
//!
//! **The rule is a complement, and that is the whole point** (`charter/contain.py:267`).
//! Everything else in a display layer names a class of *bad* characters, and a list of
//! categories is a list of spellings the next codepoint is one step outside of. This says
//! instead what a line MAY hold — U+0020..U+007E — and escapes everything else, whatever
//! category, plane or script it belongs to. Nothing is added to it when Unicode grows, and
//! no Unicode table is needed to implement it — which is why this port is exact rather than
//! approximate.

/// How much of any one value a report line repeats back (`charter/contain.py:170`).
pub const DISPLAY_LIMIT: usize = 160;

/// What stands in for a value that renders as nothing (`charter/contain.py:239`), so
/// `'': no profile` reads as a blank name rather than as a missing word.
pub const BLANK: &str = "\"\"";

/// One codepoint as an escape no other codepoint can also spell
/// (`charter/contain.py:242`).
///
/// Astral planes get the eight-digit `\U` form rather than a long `\u`, because `ὠ0`
/// is five hex digits: U+1F600 and the two characters U+1F60 + `0` would render the same,
/// and two values that read identically on a report line is the homoglyph finding with a
/// different alphabet. Every form is fixed-width and injective.
pub fn escape_char(ch: char) -> String {
    let cp = ch as u32;
    if cp <= 0xFFFF {
        format!("\\u{cp:04x}")
    } else {
        format!("\\U{cp:08x}")
    }
}

/// `text` as printable ASCII — every other codepoint shown as its escape, reversibly.
///
/// **Reversible.** `\\` for a real backslash, an escape for everything outside printable
/// ASCII, and itself for the rest, so the characters printed for a value determine that
/// value. The backslash doubling is what makes that true and is not decoration: without it
/// a name holding the six characters `ㅤ` reads exactly like one holding U+3164, which
/// is one more pair of different values a reader cannot tell apart.
pub fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ => out.push_str(&escape_char(c)),
        }
    }
    out
}

/// `value` as one line, never blank, clipped to `limit` characters of escape.
///
/// **Blankness is decided, not enumerated.** After [`escaped`] the string holds only
/// U+0020..U+007E, and the ASCII space is the only member of that range that renders as
/// nothing. So "this renders as nothing" is exactly "this is spaces" — a question about the
/// whole class rather than a sample of it. The escape comes first and the emptiness test
/// second; the other order is the growing list this exists to avoid.
///
/// The clip sits BETWEEN them, and that placement is behaviour rather than detail: a value
/// of nothing but spaces that reaches the clip comes back as spaces and dots, because the
/// dots are visible and the value therefore no longer renders as nothing.
///
/// Clipped with ASCII dots rather than `…`, so the promise "what comes back is printable
/// ASCII" holds for a clipped value too and a caller never has to special-case the marker.
pub fn readable(value: &str, limit: usize) -> String {
    let mut shown = escaped(value);
    // Characters, as Python slices them — and after `escaped` every one is one ASCII byte,
    // so this byte truncation is the same cut and can never land inside a codepoint.
    if shown.len() > limit {
        shown.truncate(limit);
        shown.push_str("...");
    }
    if shown.chars().all(|c| c == ' ') {
        BLANK.to_owned()
    } else {
        shown
    }
}

/// [`readable`] at the ordinary budget, which is what nearly every caller wants.
pub fn short(value: &str) -> String {
    readable(value, DISPLAY_LIMIT)
}

// ------------------------------------------------------------------------------------------
// One LINE of a report, and the sentence it is assembled into
// ------------------------------------------------------------------------------------------
//
// `charter/contain.py`'s `one_line` and `sentence`, which answer a different question from
// [`readable`] above and are a different port for it.
//
// `readable` asks *can a reader read this value back off the line* — the identifier question,
// answered by a complement rule (printable ASCII, everything else escaped). `one_line` asks
// only *can this value forge a second LINE*, and it is deliberately narrower: its ninety
// Python callers print workspace names, persona roles and forge text into a TUI, where an
// em-dash is content and has to reach the screen as itself. Folding one into the other would
// escape every one of those.

/// Has `c` no glyph of its own — `charter/contain.py`'s `_INVISIBLE`, plus the whitespace
/// clause beside it?
///
/// Python asks `category(ch) in {Cc, Cf, Cs, Zl, Zp} or (ch.isspace() and ch != ' ')`, and
/// [`crate::tui::tables::INVISIBLE`] is **that sentence, evaluated by CPython** for every
/// codepoint and written down as ranges (`tools/gen-unicode-tables.py`).
///
/// **Not decomposed into Rust's own `char` predicates any more, and that is the point.** It
/// used to be three clauses — `is_control` for `Cc`, `is_whitespace` for `Zl`/`Zp`/`Zs`, and a
/// hand-pasted `Cf` table — with a paragraph arguing that the three cover Python's set
/// exactly. The argument was correct and the table beside it still drifted: [`crate::pyrepr`]
/// derived its own list from the same `unicodedata` on a different day and came out nine
/// ranges short, so `shown` escaped U+0890 and `repr()` printed it. An argument a reader has
/// to re-check is not a rule; a table CPython wrote is. The whole file is regenerated from one
/// CPython, so the two answers cannot be of different Unicode versions either.
///
/// The residual risk is unchanged and still stated rather than hidden: the table is of ONE
/// Unicode version (named at the top of `tables.rs`). A codepoint whose category changes after
/// it escapes in charter and not here until the table moves — one line the two implementations
/// would render differently, named rather than left to be found.
fn invisible(c: char) -> bool {
    crate::tui::in_table(c, &crate::tui::tables::INVISIBLE)
}

/// `value` as one line of a report, with nothing in it that can forge another
/// (`charter/contain.py:195`).
///
/// **The property is line structure, not trustworthiness.** charter's reports are lines of
/// the form `  <name> → <command>`, and every field in one comes out of a committed file. A
/// newline in one of those fields writes a second line that looks exactly as much like
/// charter's own output as the first. So every character with no glyph is replaced by its own
/// escape, and the result is clipped.
///
/// What this does NOT do is make the value trustworthy to READ: `I` and `l`, a Cyrillic `а`
/// and a Latin `a`, come back unchanged and a reader cannot tell them apart. Those cannot
/// forge a line, which is the whole of what is claimed. A caller whose sentence has to NAME
/// something wants [`readable`] instead.
///
/// The escape is Python's, spelling for spelling: `\xNN` below U+0100 and `\uXXXX` above it,
/// where the four is a MINIMUM width — so an astral codepoint renders as five hex digits, not
/// four. Deliberately not [`escape_char`]'s fixed-width injective form; the two answer
/// different questions and charter writes both.
pub fn one_line(value: &str, limit: usize) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if invisible(c) {
            let cp = c as u32;
            if cp < 0x100 {
                out.push_str(&format!("\\x{cp:02x}"));
            } else {
                out.push_str(&format!("\\u{cp:04x}"));
            }
        } else {
            out.push(c);
        }
    }
    // Python clips on CHARACTERS, and what survives `invisible` is arbitrary Unicode, so this
    // counts codepoints rather than bytes.
    if out.chars().count() <= limit {
        return out;
    }
    let mut clipped: String = out.chars().take(limit).collect();
    clipped.push('…');
    clipped
}

/// [`one_line`] at the ordinary budget.
pub fn line(value: &str) -> String {
    one_line(value, DISPLAY_LIMIT)
}

/// What a caller passes as [`one_line`]'s `limit` to mean **do not clip**
/// (`charter/contain.py:184`) — for a field whose whole point is that the reader gets all of
/// it. Those want the escaping and not the budget above it.
pub const NO_CLIP: usize = usize::MAX;

/// charter's own separator, for a sentence naming several things (`charter/contain.py:385`).
///
/// charter's own, for the same reason the template is charter's own text: a line naming
/// several committed things is a line with structure, and a separator taken from one of the
/// things would let that thing restructure the line.
pub const SEQUENCE_SEPARATOR: &str = ", ";

/// One field of a [`sentence`]: one value, or several.
pub enum Slot {
    /// One value, contained whole.
    One(String),
    /// Several, contained **element by element** and then joined.
    ///
    /// Containing the elements and not the join, because the join IS the sentence: a sentence
    /// that names a list exists to name every entry in it, and clipping the joined string
    /// drops the last entries and leaves a line reading as though they were never there.
    Many(Vec<String>),
}

impl From<&str> for Slot {
    fn from(value: &str) -> Self {
        Slot::One(value.to_owned())
    }
}

impl From<String> for Slot {
    fn from(value: String) -> Self {
        Slot::One(value)
    }
}

impl From<usize> for Slot {
    fn from(value: usize) -> Self {
        Slot::One(value.to_string())
    }
}

/// One line of charter's own report, with **every** field in it bounded to one line
/// (`charter/contain.py:420`).
///
/// `template` is a literal in charter's own source — charter's sentence, with `{}` slots.
/// Everything substituted into it is treated as a value out of a committed file, and goes
/// through [`one_line`].
///
/// **The containment is at the assembly, not at the slots.** `news.entry_errors` contained
/// the ordering *value* and interpolated the committed *filename* three inches away raw, so
/// an entry named `0.60.0-a\nEVIL: charter says nothing is wrong.md` printed two lines where
/// charter emitted one — the second being the author's sentence in charter's voice (#502).
/// The value was contained because the value was what that commit was about, not because the
/// filename had been judged safe. A field added to a template tomorrow is contained by having
/// been passed here, which is a property a reviewer checks by reading the call site.
///
/// Python spells the fields as keyword arguments and this spells them as pairs, which is the
/// one thing that could not be carried across. What IS carried across is that there is no way
/// to reach a template's slots without passing through here.
pub fn sentence(template: &str, fields: &[(&str, Slot)]) -> String {
    sentence_at(template, fields, DISPLAY_LIMIT)
}

/// [`sentence`] at a caller's own budget — `charter/contain.py`'s `path_sentence` is this
/// function with a longer one, for a refusal naming a path the reader has to act on.
pub fn sentence_at(template: &str, fields: &[(&str, Slot)], limit: usize) -> String {
    let shown: Vec<(&str, String)> = fields
        .iter()
        .map(|(key, slot)| {
            let value = match slot {
                Slot::One(v) => one_line(v, limit),
                Slot::Many(items) => items
                    .iter()
                    .map(|v| one_line(v, limit))
                    .collect::<Vec<_>>()
                    .join(SEQUENCE_SEPARATOR),
            };
            (*key, value)
        })
        .collect();
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(offset) = rest[open..].find('}') else {
            break;
        };
        let close = open + offset;
        let name = &rest[open + 1..close];
        out.push_str(&rest[..open]);
        match shown.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            // A slot no caller filled is left standing as it was written. Python raises
            // `KeyError` here, and raising is what `contain.py`'s own rule forbids; a visible
            // `{version}` in a report is the loudest thing that is still safe.
            None => out.push_str(&rest[open..=close]),
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod report_line_tests {
    use super::*;

    #[test]
    fn a_newline_cannot_forge_a_second_line() {
        assert_eq!(line("a\nEVIL: fine"), "a\\x0aEVIL: fine");
        assert_eq!(line("a\r\nb"), "a\\x0d\\x0ab");
    }

    #[test]
    fn a_glyph_is_left_alone_and_a_format_character_is_not() {
        // The characters charter's own entries carry: content, not structure.
        assert_eq!(
            line("Ship it — properly · no shortcuts ✗"),
            "Ship it — properly · no shortcuts ✗"
        );
        // U+200B ZERO WIDTH SPACE is `Cf`; U+180E is `Cf` and NOT `White_Space`.
        assert_eq!(line("a\u{200b}b"), "a\\u200bb");
        assert_eq!(line("a\u{180e}b"), "a\\u180eb");
        // U+00A0 is `Zs`: Python's `isspace()` clause, not the category set.
        assert_eq!(line("a\u{a0}b"), "a\\xa0b");
        // An astral escape is FIVE hex digits, because Python's width is a minimum.
        assert_eq!(line("a\u{e0020}b"), "a\\ue0020b");
    }

    #[test]
    fn the_four_separator_controls_are_escaped_where_rust_calls_them_printable() {
        assert_eq!(line("a\u{1f}b"), "a\\x1fb");
    }

    #[test]
    fn a_long_value_is_clipped_with_an_ellipsis() {
        let long = "x".repeat(DISPLAY_LIMIT + 5);
        let shown = line(&long);
        assert_eq!(shown.chars().count(), DISPLAY_LIMIT + 1);
        assert!(shown.ends_with('…'));
        assert_eq!(
            line(&"x".repeat(DISPLAY_LIMIT)).chars().count(),
            DISPLAY_LIMIT
        );
    }

    #[test]
    fn every_field_of_a_sentence_is_contained() {
        let said = sentence(
            "{name}: `{field}: {raw}` is not a value charter reads.",
            &[
                ("name", "0.60.0-a\nEVIL: nothing is wrong.md".into()),
                ("field", "security".into()),
                ("raw", "yes".into()),
            ],
        );
        assert_eq!(said.lines().count(), 1);
        assert!(said.contains("0.60.0-a\\x0aEVIL: nothing is wrong.md"));
    }

    #[test]
    fn a_list_is_contained_element_by_element_and_then_joined() {
        let said = sentence(
            "{version}: {count} entries ({names})",
            &[
                ("version", "0.52.0".into()),
                ("count", 2usize.into()),
                (
                    "names",
                    Slot::Many(vec!["a\nb.md".to_owned(), "c.md".to_owned()]),
                ),
            ],
        );
        assert_eq!(said, "0.52.0: 2 entries (a\\x0ab.md, c.md)");
    }

    #[test]
    fn an_unfilled_slot_is_left_standing_rather_than_raising() {
        assert_eq!(sentence("a {nope} b", &[]), "a {nope} b");
    }
}
