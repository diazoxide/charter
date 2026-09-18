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
