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
    let body = crate::memstore::py_strip(body);
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
    let mut result = out
        .join("\n")
        .trim_end_matches(crate::memstore::is_python_space)
        .to_string();
    result.push('\n');
    if !replaced {
        result.push_str(&format!("\n## {header}\n\n{body}\n"));
    }
    result
}

/// Add one entry to the end of the body under `## <header>`, keeping everything already
/// there: the typed `workspace_section` write (#1384). A line that is exactly `placeholder`
/// (the template's `_Nothing yet._`) goes, since the section now has something in it; a
/// guidance comment stays. A section that is not there is appended, as [`replace`] appends one.
///
/// The section is found as [`replace`] finds it, header case-insensitive and ended by the next
/// `"## "` line, so the two never disagree about what is in it. The entry is one list item:
/// `- ` before its first line unless it is already one, and every further line indented under
/// it, so nothing in it can start a section of its own.
pub fn add_entry(text: &str, header: &str, entry: &str, placeholder: &str) -> String {
    let item = list_item(entry);
    let lines = split_lines(text);
    let Some(at) = lines.iter().position(|line| matches_header(line, header)) else {
        return replace(text, header, &item.join("\n"));
    };
    let end = (at + 1..lines.len())
        .find(|&i| lines[i].starts_with("## "))
        .unwrap_or(lines.len());
    let mut body: Vec<&str> = lines[at + 1..end]
        .iter()
        .copied()
        .filter(|line| crate::memstore::py_strip(line) != placeholder)
        .collect();
    while body.last().is_some_and(|line| line.trim().is_empty()) {
        body.pop();
    }
    let in_a_list = body.last().is_some_and(|line| {
        line.starts_with("- ") || line.starts_with("* ") || line.starts_with("  ")
    });
    if !in_a_list {
        body.push("");
    }
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + item.len() + 2);
    out.extend_from_slice(&lines[..=at]);
    out.extend(body);
    out.extend(item.iter().map(String::as_str));
    if end < lines.len() {
        out.push("");
        out.extend_from_slice(&lines[end..]);
    }
    let mut result = out
        .join("\n")
        .trim_end_matches(crate::memstore::is_python_space)
        .to_string();
    result.push('\n');
    result
}

/// `entry` as the lines of one Markdown list item.
fn list_item(entry: &str) -> Vec<String> {
    split_lines(crate::memstore::py_strip(entry))
        .into_iter()
        .enumerate()
        .map(|(i, line)| match i {
            0 if line.starts_with("- ") || line.starts_with("* ") => line.to_owned(),
            0 => format!("- {line}"),
            _ if line.trim().is_empty() => String::new(),
            _ => format!("  {line}"),
        })
        .collect()
}

/// `^##\s+<header>\s*$`, case-insensitive.
fn matches_header(line: &str, header: &str) -> bool {
    let Some(rest) = line.strip_prefix("##") else {
        return false;
    };
    // Python's `\s` in a unicode pattern matches U+001C–U+001F as well.
    let trimmed = rest.trim_start_matches(crate::memstore::is_python_space);
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
    tail.chars().all(crate::memstore::is_python_space)
}

/// The body under `## <header>`, trimmed, or `""` when there is no such section.
///
/// **This is the READER, and it is not the writer's mirror.** charter reads a vision with
/// `re.search(r"^##\s+Vision\s*$(.*?)(?=^##\s|\Z)", text, MULTILINE | DOTALL)` — no
/// `IGNORECASE`, unlike `_replace_md_section`, and a regex whose `^`/`$` break on `\n`
/// alone. So a `## vision` in lower case is NOT this section, and a `## Next` after a
/// U+2028 does NOT end it, both the opposite of what `replace` does. The asymmetry is
/// charter's; reproducing it is the point.
pub fn section_body(text: &str, header: &str) -> String {
    let lines: Vec<&str> = newline_lines(text);
    let mut i = 0;
    while i < lines.len() {
        if matches_header_exactly(lines[i], header) {
            let start = i + 1;
            let mut end = start;
            // charter's reader stops at `^##\s`, which is not the `"## "` its writer scans
            // for: a bare `##` on its own line ends the section for one and not the other.
            // The last line is newline-terminated unless the text does not end in one, and
            // that matters for a BARE `##`: the newline is the `\s` in `^##\s`, so a `##`
            // at the very end of a file with no trailing newline is not a heading.
            let terminated = |i: usize| i + 1 < lines.len() || text.ends_with('\n');
            while end < lines.len() && !starts_a_heading(lines[end], terminated(end)) {
                end += 1;
            }
            return crate::memstore::py_strip(&lines[start..end].join("\n")).to_string();
        }
        i += 1;
    }
    String::new()
}

/// Lines as a regex with `MULTILINE` sees them: broken on `\n` only, with `$` also matching
/// before a trailing one.
fn newline_lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    // A trailing newline does not make a final empty line for the scan.
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines
}

/// `^##\s+<header>\s*$`, case-SENSITIVE — what the reader's regex matches.
fn matches_header_exactly(line: &str, header: &str) -> bool {
    // `\r` is not `\n`, so a CRLF file leaves it on the line and `\s*$` absorbs it.
    let Some(rest) = line.strip_prefix("##") else {
        return false;
    };
    let trimmed = rest.trim_start_matches(crate::memstore::is_python_space);
    if trimmed.len() == rest.len() {
        return false;
    }
    trimmed
        .strip_prefix(header)
        .is_some_and(|tail| tail.chars().all(crate::memstore::is_python_space))
}

/// `^##\s` — the lookahead charter's vision regex ends a section on.
fn starts_a_heading(line: &str, terminated: bool) -> bool {
    // `^##\s` — and a bare `##` line counts, because `split_lines` has already taken the
    // newline that WAS the `\s`. Without this a `##` on its own line does not end a section
    // for the reader, though it does for Python's regex.
    line.strip_prefix("##").is_some_and(|rest| {
        if rest.is_empty() {
            terminated
        } else {
            rest.starts_with(crate::memstore::is_python_space)
        }
    })
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

#[cfg(test)]
mod reader_tests {
    use super::*;

    // Every expectation verified against `charter.workspace.read_vision`'s regex.

    #[test]
    fn the_reader_is_case_sensitive_where_the_writer_is_not() {
        // `read_vision` passes no IGNORECASE; `_replace_md_section` does.
        assert_eq!(section_body("# t\n\n## vISion   \nold\n", "Vision"), "");
        assert_eq!(
            replace("# t\n\n## vISion\nold\n", "Vision", "new"),
            "# t\n\n## vISion\n\nnew\n",
            "the writer still matches it"
        );
    }

    #[test]
    fn the_reader_breaks_on_newlines_only_so_a_line_separator_does_not_end_a_section() {
        // The mirror of the writer's rule, and deliberately not the same.
        assert_eq!(
            section_body("## Vision\nold\u{2028}## Next\nkeep\n", "Vision"),
            "old\u{2028}## Next\nkeep"
        );
        assert_eq!(
            section_body("## Vision\nold\u{b}## Next\nkeep\n", "Vision"),
            "old\u{b}## Next\nkeep"
        );
    }

    #[test]
    fn a_bare_hash_hash_line_ends_a_section_for_the_reader() {
        assert_eq!(
            section_body(
                "## Vision\nold\n##\nnot a heading for the writer\n",
                "Vision"
            ),
            "old"
        );
    }

    #[test]
    fn the_reader_finds_an_ordinary_section() {
        assert_eq!(
            section_body(
                "# t\n\n## Vision\n\nShip the widget\n\n## Log\nx\n",
                "Vision"
            ),
            "Ship the widget"
        );
    }
}

#[cfg(test)]
mod eof_tests {
    use super::*;

    // Verified against `read_vision`'s regex directly:
    //   '## Vision\nbody\n##'      -> 'body\n##'   (no newline after `##`, so no heading)
    //   '## Vision\nbody\n##\n'    -> 'body'
    #[test]
    fn a_bare_hash_hash_ends_a_section_only_when_a_newline_follows_it() {
        assert_eq!(
            section_body("## Vision\nbody\n##", "Vision"),
            "body\n##",
            "at EOF with no trailing newline there is no whitespace to match `^##\\\\s`"
        );
        assert_eq!(section_body("## Vision\nbody\n##\n", "Vision"), "body");
        assert_eq!(
            section_body("## Vision\nbody\n##\nmore\n", "Vision"),
            "body"
        );
    }

    #[test]
    fn a_writer_needs_whitespace_after_the_hashes_or_it_appends_a_new_section() {
        // `^##\s+Vision` — `##Vision` is not the header, so charter APPENDS rather than
        // replacing. Dropping that requirement silently destroyed the old body.
        assert_eq!(
            replace("##Vision\nold\n", "Vision", "new"),
            "##Vision\nold\n\n## Vision\n\nnew\n"
        );
    }

    // ---- add_entry: one more entry under a section (#1384) ------------------------------------

    const NOTHING: &str = "_Nothing yet._";

    #[test]
    fn an_entry_takes_the_place_of_the_placeholder_and_keeps_the_guidance_comment() {
        let text = "# a\n\n## Glossary\n\n<!-- `term` — definition -->\n\n_Nothing yet._\n\n## Log\n\nkept\n";
        assert_eq!(
            add_entry(text, "Glossary", "`svc` — the service", NOTHING),
            "# a\n\n## Glossary\n\n<!-- `term` — definition -->\n\n- `svc` — the service\n\n## Log\n\nkept\n"
        );
    }

    #[test]
    fn a_second_entry_follows_the_first_in_the_same_list() {
        let once = add_entry(
            "## Glossary\n\n_Nothing yet._\n",
            "Glossary",
            "one",
            NOTHING,
        );
        assert_eq!(
            add_entry(&once, "Glossary", "two", NOTHING),
            "## Glossary\n\n- one\n- two\n"
        );
    }

    #[test]
    fn an_entry_under_prose_is_set_apart_from_it_by_a_blank_line() {
        assert_eq!(
            add_entry(
                "## Context & decisions\n\nWe ship on Fridays.\n",
                "Context & decisions",
                "No deploys on Mondays",
                NOTHING
            ),
            "## Context & decisions\n\nWe ship on Fridays.\n\n- No deploys on Mondays\n"
        );
    }

    #[test]
    fn a_multi_line_entry_is_one_list_item_and_never_starts_a_section() {
        assert_eq!(
            add_entry(
                "## Glossary\n",
                "Glossary",
                "term\n## Vision\nnot a heading",
                NOTHING
            ),
            "## Glossary\n\n- term\n  ## Vision\n  not a heading\n"
        );
        // A break Python splits on, which `replace` would turn into a newline, is indented too.
        assert_eq!(
            add_entry(
                "## Glossary\n",
                "Glossary",
                "term\u{2028}## Vision",
                NOTHING
            ),
            "## Glossary\n\n- term\n  ## Vision\n"
        );
    }

    #[test]
    fn an_entry_already_written_as_a_list_item_is_not_bulleted_twice() {
        assert_eq!(
            add_entry(
                "## Glossary\n",
                "Glossary",
                "- `api` — the service",
                NOTHING
            ),
            "## Glossary\n\n- `api` — the service\n"
        );
    }

    #[test]
    fn an_entry_finds_the_section_the_writer_finds_whatever_its_case() {
        // `replace` matches the header case-insensitively, so must this: a reader that did not
        // would hand back an empty body, and the hand-written one would be lost.
        assert_eq!(
            add_entry("## glossary\n\n- old\n", "Glossary", "new", NOTHING),
            "## glossary\n\n- old\n- new\n"
        );
    }

    #[test]
    fn a_section_that_is_not_there_is_appended_with_the_entry() {
        assert_eq!(
            add_entry("# a\n\n## Vision\n\nShip.\n", "Glossary", "term", NOTHING),
            "# a\n\n## Vision\n\nShip.\n\n## Glossary\n\n- term\n"
        );
    }
}
