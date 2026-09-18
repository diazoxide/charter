//! `workspace.md` section surgery, as `charter/workspace.py:_replace_md_section` does it.
//!
//! The living charter is a hand-edited file, so charter never rewrites it wholesale: it
//! keeps the `## <header>` line and replaces the body under it. Reproduced here down to the
//! line-splitting, because Python's `str.splitlines()` breaks on characters Rust's `lines()`
//! does not, and the difference decides where a section ends.

/// Split as Python's `str.splitlines()` does, dropping the separators.
///
/// Python breaks on far more than `\n`: `\r`, `\r\n`, vertical tab, form feed, the file/group/
/// record separators, NEL, and the Unicode line and paragraph separators. A `## ` after one of
/// those starts a new section to charter, and joining the parts back with `\n` is what turns
/// the separator into a newline on disk.
pub fn split_lines(text: &str) -> Vec<&str> {
    fn is_break(c: char) -> bool {
        matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        )
    }

    let mut lines = Vec::new();
    let mut start = 0;
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if !is_break(c) {
            continue;
        }
        lines.push(&text[start..i]);
        // `\r\n` is one break, not two.
        if c == '\r' && it.peek().is_some_and(|&(_, next)| next == '\n') {
            it.next();
            start = i + 2;
        } else {
            start = i + c.len_utf8();
        }
    }
    // Python yields no trailing empty line for a text that ends in a break.
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Replace the body under `## <header>` (to the next line starting `## `, or the end),
/// keeping the header line. A section that is not there is appended.
pub fn replace(text: &str, header: &str, body: &str) -> String {
    let lines = split_lines(text);
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + 3);
    let body = body.trim();
    let mut replaced = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        out.push(line);
        if matches_header(line, header) {
            i += 1;
            // The scan for the end of the section looks for a literal `"## "`, which is
            // not the same test as the header match above; charter uses both.
            while i < lines.len() && !lines[i].starts_with("## ") {
                i += 1;
            }
            out.push("");
            out.push(body);
            out.push("");
            replaced = true;
            continue;
        }
        i += 1;
    }
    let mut result = out.join("\n").trim_end().to_string();
    result.push('\n');
    if !replaced {
        result.push_str(&format!("\n## {header}\n\n{body}\n"));
    }
    result
}

/// `^##\s+<header>\s*$`, case-insensitive.
fn matches_header(line: &str, header: &str) -> bool {
    let Some(rest) = line.strip_prefix("##") else {
        return false;
    };
    let trimmed = rest.trim_start_matches(char::is_whitespace);
    // `\s+` — at least one space between `##` and the name.
    if trimmed.len() == rest.len() {
        return false;
    }
    let Some(tail) = trimmed
        .get(..header.len())
        .filter(|head| head.eq_ignore_ascii_case(header))
        .map(|_| &trimmed[header.len()..])
    else {
        return false;
    };
    tail.chars().all(char::is_whitespace)
}

/// The body under `## <header>`, trimmed, or `""` when there is no such section.
pub fn section_body(text: &str, header: &str) -> String {
    let lines = split_lines(text);
    let mut i = 0;
    while i < lines.len() {
        if matches_header(lines[i], header) {
            let start = i + 1;
            let mut end = start;
            // charter's reader stops at `^##\s`, which is not the `"## "` its writer scans
            // for: a bare `##` on its own line ends the section for one and not the other.
            while end < lines.len() && !starts_a_heading(lines[end]) {
                end += 1;
            }
            return lines[start..end].join("\n").trim().to_string();
        }
        i += 1;
    }
    String::new()
}

/// `^##\s` — the lookahead charter's vision regex ends a section on.
fn starts_a_heading(line: &str) -> bool {
    line.strip_prefix("##")
        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every expectation came from charter itself:
    // `from charter.workspace import _replace_md_section`.

    #[test]
    fn the_body_under_the_header_is_replaced_and_the_header_line_kept() {
        assert_eq!(
            replace(
                "## Vision\nold body\n\n## Next\nkeep\n",
                "Vision",
                "new body"
            ),
            "## Vision\n\nnew body\n\n## Next\nkeep\n"
        );
    }

    #[test]
    fn a_section_that_runs_to_the_end_of_the_file_is_replaced_to_the_end() {
        assert_eq!(
            replace("# t\n\n## Vision\nold\n", "Vision", "new"),
            "# t\n\n## Vision\n\nnew\n"
        );
    }

    #[test]
    fn a_missing_section_is_appended_with_its_body_stripped() {
        assert_eq!(
            replace("# t\n\nsome text\n", "Vision", "  Ship it  "),
            "# t\n\nsome text\n\n## Vision\n\nShip it\n"
        );
    }

    #[test]
    fn the_header_matches_whatever_its_case_and_spacing() {
        assert_eq!(
            replace("##   vISion\nold\n\n## Next\nkeep\n", "Vision", "new"),
            "##   vISion\n\nnew\n\n## Next\nkeep\n"
        );
    }

    #[test]
    fn a_unicode_line_separator_ends_a_line_the_way_python_ends_one() {
        // U+2028 is a line break to `str.splitlines()`, so `## Next` after one is a real
        // section header — and the separator itself comes back as a newline.
        assert_eq!(
            replace("## Vision\nold\u{2028}## Next\nkeep\n", "Vision", "new"),
            "## Vision\n\nnew\n\n## Next\nkeep\n"
        );
    }
}
