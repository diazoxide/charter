//! The few Python string rules `init` and `reinit` lean on, where Rust's own differ.
//!
//! Each of these decides whether a line the operator already has counts as present, so a
//! rule that differed from Python's by one separator would add a line Python leaves out —
//! or leave out one Python adds.

/// Whether `c` ends a line for `str.splitlines`: wider than `\n`, and the reason this exists.
fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r'
            | '\u{0b}'
            | '\u{0c}'
            | '\u{1c}'
            | '\u{1d}'
            | '\u{1e}'
            | '\u{85}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

/// `text.splitlines(keepends=True)`: every line with its own terminator, `\r\n` as one.
pub fn lines_with_ends(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if !is_line_break(c) {
            continue;
        }
        let mut end = at + c.len_utf8();
        if c == '\r'
            && let Some(&(next, '\n')) = chars.peek()
        {
            end = next + 1;
            chars.next();
        }
        out.push(&text[start..end]);
        start = end;
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// A line without its terminator — what `str.splitlines()` yields for it.
pub fn without_end(line: &str) -> &str {
    let mut end = line.len();
    for (at, c) in line.char_indices().rev() {
        if is_line_break(c) {
            end = at;
        } else {
            break;
        }
    }
    // `\r\n` is one terminator and every other is one character, so at most two come off:
    // a line is split at its FIRST break, and only `\r\n` puts a second one after it.
    &line[..end]
}

/// `{line.strip() for line in text.splitlines()}`, as a list.
pub fn stripped_lines(text: &str) -> Vec<&str> {
    lines_with_ends(text)
        .into_iter()
        .map(|line| crate::memstore::py_strip(without_end(line)))
        .collect()
}

/// `repr(s)` for a `str` — the quoting a Python f-string's `!r` puts round a value.
///
/// **[`crate::pyrepr::repr_str`], and not a second port of it.** This module had its own,
/// which escaped `char::is_control` plus U+2028 and U+2029 and printed every other
/// non-printable character as itself — so a name holding a no-break space came back
/// `'nb\u{a0}sp'` from `init` and `'nb\\xa0sp'` from every other refusal in the binary.
/// The name stays here because `init`'s call sites read better for it; the rule does not.
pub fn py_repr(s: &str) -> String {
    crate::pyrepr::repr_str(s)
}

/// `str.title()` over the ASCII a persona name can hold: a letter is upper-cased when the
/// character before it is not a letter, and lower-cased when it is. A digit or a `.` is not
/// a letter, so `bot2x` is `Bot2X` — which is what Python says.
pub fn py_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut after_letter = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            if after_letter {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            after_letter = true;
        } else {
            out.push(c);
            after_letter = false;
        }
    }
    out
}

/// Python's truthiness of a JSON value: `null`, `false`, `0`, `0.0`, `""`, `[]` and `{}` are
/// false and everything else is true.
pub fn truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_str().parse::<f64>().map(|f| f != 0.0).unwrap_or(true),
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verified against CPython 3.14: `"a\r\nb\rc\x0cd e".splitlines(keepends=True)`.
    #[test]
    fn lines_split_where_python_splits_them() {
        assert_eq!(
            lines_with_ends("a\r\nb\rc\u{0c}d\u{2028}e"),
            vec!["a\r\n", "b\r", "c\u{0c}", "d\u{2028}", "e"]
        );
        assert_eq!(lines_with_ends(""), Vec::<&str>::new());
        assert_eq!(lines_with_ends("\n\n"), vec!["\n", "\n"]);
        assert_eq!(
            stripped_lines(" /.charter/ \r\n\t!x\u{1f}\n"),
            vec!["/.charter/", "!x"]
        );
    }

    /// Verified against CPython 3.14: `repr(s)`.
    ///
    /// Kept although the rule now lives in `pyrepr`: these are `init`'s own values, and the
    /// day somebody gives this name a second body again, this is the test that says so.
    #[test]
    fn a_value_is_quoted_the_way_python_quotes_it() {
        assert_eq!(py_repr("Bad Name"), "'Bad Name'");
        assert_eq!(py_repr("it's"), "\"it's\"");
        assert_eq!(py_repr("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(py_repr("x\ny\u{7}"), "'x\\ny\\x07'");
        assert_eq!(py_repr("é"), "'é'");
        // The divergence that made this a delegation: a `Zs` the old body printed raw.
        assert_eq!(py_repr("nb\u{a0}sp"), "'nb\\xa0sp'");
    }

    /// Verified against CPython 3.14: `s.title()`.
    #[test]
    fn a_role_is_title_cased_the_way_python_cases_it() {
        assert_eq!(py_title("steward"), "Steward");
        assert_eq!(py_title("front door"), "Front Door");
        assert_eq!(py_title("bot2x"), "Bot2X");
        assert_eq!(py_title("a.b"), "A.B");
    }
}
