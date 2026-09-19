//! Python's `repr()` of a string — how charter quotes a name back in a refusal
//! (`f"{name!r}"`), so a sentence the two implementations print is one sentence.

/// `repr(s)`.
///
/// Single quotes unless the text holds one and no double quote. The backslash, the quote in
/// use, `\t`, `\n` and `\r` get their short escapes; any other character Python does not
/// count as printable gets `\xNN`, `\uNNNN` or `\UNNNNNNNN` by size. Letters of any script
/// stay themselves, as they do in Python.
pub fn repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if !printable(c) => {
                let cp = c as u32;
                if cp < 0x100 {
                    out.push_str(&format!("\\x{cp:02x}"));
                } else if cp < 0x10000 {
                    out.push_str(&format!("\\u{cp:04x}"));
                } else {
                    out.push_str(&format!("\\U{cp:08x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `str.isprintable()` for one character, as far as a name can reach it: controls, every
/// separator but the ASCII space, and the format characters that render as nothing.
///
/// Not a full Unicode table. A codepoint this calls printable and Python does not would be
/// quoted as itself where Python escapes it — a byte of difference in a refusal, never a
/// forged line, because every line-breaking character is a control or a separator and is
/// escaped here.
fn printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    if c.is_control() || c.is_whitespace() {
        return false;
    }
    !matches!(
        c,
        '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{61c}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{e000}'..='\u{f8ff}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_quoted_back_the_way_python_quotes_it() {
        // Each verified against CPython 3.14 `repr(...)`.
        assert_eq!(repr_str("widget"), "'widget'");
        assert_eq!(repr_str("../escape"), "'../escape'");
        assert_eq!(repr_str("it's"), "\"it's\"");
        assert_eq!(repr_str("both'\""), "'both\\'\"'");
        assert_eq!(repr_str("a\nb\tc\\d"), "'a\\nb\\tc\\\\d'");
        assert_eq!(repr_str("x\u{1b}[2J"), "'x\\x1b[2J'");
        assert_eq!(repr_str("zw\u{200b}j"), "'zw\\u200bj'");
        assert_eq!(repr_str("nb\u{a0}sp"), "'nb\\xa0sp'");
        assert_eq!(repr_str("line\u{2028}sep"), "'line\\u2028sep'");
        assert_eq!(repr_str("café"), "'café'");
    }
}
