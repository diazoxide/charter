//! JSON written the way Python's `json.dumps` writes it.
//!
//! charter's manifests are Python's output, and `workspace.json` carries a
//! `charter_generated` digest taken over a canonical re-serialisation of itself. A writer
//! that differs by one byte flips that digest, and charter then reads the file as the
//! operator's own and stops maintaining it. So this module reproduces `json.dumps`
//! exactly, rather than using `serde_json`'s own formatting, which differs on two counts:
//! it emits non-ASCII literally where Python escapes it, and it packs its separators.

/// `json.dumps(value, sort_keys=True)` — the form the `charter_generated` digest is taken over.
pub fn dumps_sorted(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_sorted(value, &mut out);
    out
}

fn write_sorted(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::String(s) => escape_into(s, out),
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_sorted(item, out);
            }
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            // Python's `sort_keys=True` orders by code point, which is what comparing
            // UTF-8 bytes gives: the encoding preserves code point order.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                escape_into(key, out);
                out.push_str(": ");
                write_sorted(&map[key.as_str()], out);
            }
            out.push('}');
        }
        serde_json::Value::Number(n) => out.push_str(&number(n)),
        other => out.push_str(&other.to_string()),
    }
}

/// One JSON string, escaped as `ensure_ascii=True` escapes it: nothing above US-ASCII
/// survives as itself.
fn escape_into(s: &str, out: &mut String) {
    escape_with(s, true, out);
}

/// One JSON string. `ascii` is `ensure_ascii`: with it off, Python escapes only the quote,
/// the backslash and what is below U+0020, and writes everything else as itself.
fn escape_with(s: &str, ascii: bool, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            // Python spells these five as shorthand and everything else below 0x20 as
            // `\u00XX`, so a writer that used `\u000a` for a newline would already differ.
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x7f && (c as u32) >= 0x20 => out.push(c),
            c if !ascii && (c as u32) >= 0x20 => out.push(c),
            c => {
                for unit in c.encode_utf16(&mut [0u16; 2]) {
                    out.push_str(&format!("\\u{:04x}", unit));
                }
            }
        }
    }
    out.push('"');
}

/// `json.dumps(value, indent=2) + "\n"` — the form a manifest takes on disk, in the key
/// order the document already has.
pub fn dumps_indent2(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_indented(value, 0, true, &mut out);
    out.push('\n');
    out
}

/// `json.dumps(value, indent=2, ensure_ascii=False) + "\n"` — how `inventory/repos.json` is
/// written: a repo's description keeps its own characters.
pub fn dumps_indent2_unicode(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_indented(value, 0, false, &mut out);
    out.push('\n');
    out
}

fn write_indented(value: &serde_json::Value, depth: usize, ascii: bool, out: &mut String) {
    let pad = |n: usize, out: &mut String| out.push_str(&"  ".repeat(n));
    match value {
        serde_json::Value::String(s) => escape_with(s, ascii, out),
        // An empty container stays on one line: Python writes no newline it would then
        // have to strip.
        serde_json::Value::Array(items) if items.is_empty() => out.push_str("[]"),
        serde_json::Value::Object(map) if map.is_empty() => out.push_str("{}"),
        serde_json::Value::Array(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                pad(depth + 1, out);
                write_indented(item, depth + 1, ascii, out);
            }
            out.push('\n');
            pad(depth, out);
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            // Insertion order, not sorted: charter rewrites the document it read, and the
            // operator's key order is part of the file.
            out.push_str("{\n");
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                pad(depth + 1, out);
                escape_with(key, ascii, out);
                out.push_str(": ");
                write_indented(item, depth + 1, ascii, out);
            }
            out.push('\n');
            pad(depth, out);
            out.push('}');
        }
        serde_json::Value::Number(n) => out.push_str(&number(n)),
        other => out.push_str(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_digest_form_escapes_non_ascii_and_spaces_its_separators_like_python() {
        // Verified against CPython: json.dumps({...}, sort_keys=True).
        let doc = json!({"name": "alpha", "description": "Ship the widget — fast"});

        assert_eq!(
            dumps_sorted(&doc),
            r#"{"description": "Ship the widget \u2014 fast", "name": "alpha"}"#
        );
    }

    #[test]
    fn a_control_character_uses_pythons_shorthand_escape_where_it_has_one() {
        // Verified against CPython:
        // json.dumps({"k": "a\nb\tc\rd\x07e\x7ff\bg\x0ch"}, sort_keys=True)
        let doc = json!({"k": "a\nb\tc\rd\u{7}e\u{7f}f\u{8}g\u{c}h"});

        assert_eq!(
            dumps_sorted(&doc),
            r#"{"k": "a\nb\tc\rd\u0007e\u007ff\bg\fh"}"#
        );
    }

    #[test]
    fn a_trace_record_is_one_line_in_the_documents_own_key_order() {
        // `contain.json_line`: json.dumps(rec) with Python's default separators. Verified
        // against CPython for this record.
        let doc: serde_json::Value = serde_json::from_str(
            r#"{"ts": "2026-05-04T11:32:17", "event": "memory", "title": "é"}"#,
        )
        .unwrap();

        assert_eq!(
            dumps(&doc, None, ", ", ": "),
            r#"{"ts": "2026-05-04T11:32:17", "event": "memory", "title": "\u00e9"}"#
        );
    }

    #[test]
    fn a_character_outside_the_basic_plane_is_written_as_a_surrogate_pair() {
        // Verified against CPython: json.dumps({"k": "ship \U0001F680"}, sort_keys=True)
        let doc = json!({"k": "ship 🚀"});

        assert_eq!(dumps_sorted(&doc), r#"{"k": "ship \ud83d\ude80"}"#);
    }
}

/// A number, as Python's `json.dumps(json.loads(literal))` writes it back.
///
/// `arbitrary_precision` is why an integer is exact: without it `serde_json` turns any
/// integer outside `i64`/`u64` into an `f64` **at parse time**, so `18446744073709551616`
/// came back as `1.8446744073709552e19` and the digest was taken over a number the file did
/// not hold. Python reads an integer literal as an `int` of any size and writes its digits
/// back, so the literal is the answer — except `-0`, which is the int `0`.
///
/// A literal with a fraction or an exponent is a Python `float`, and Python writes a float
/// back as its `repr`, NOT as the literal it read: a hand-written `1e-7` comes back `1e-07`,
/// `1E5` comes back `100000.0` and `1e400` comes back `Infinity`. `charter init` rewrites the
/// operator's `.claude/settings.json` when it adds a key, so a float somebody wrote by hand
/// is on this path, and [`float_repr`] is CPython's rule for it.
pub(crate) fn number(n: &serde_json::Number) -> String {
    let literal = n.as_str();
    if !literal.contains(['.', 'e', 'E']) {
        return if literal == "-0" {
            "0".to_owned()
        } else {
            literal.to_owned()
        };
    }
    match literal.parse::<f64>() {
        Ok(value) => float_repr(value),
        // Unreachable for anything serde_json accepted as a number; the literal is the most
        // honest thing left to write.
        Err(_) => literal.to_owned(),
    }
}

/// CPython's `float.__repr__` as `json.dumps` uses it: the shortest digits that read back
/// as the same double, in fixed notation when the decimal exponent is in `-4..16` and in
/// scientific notation (`1e-05`, `1.5e+20`) otherwise — and `Infinity` for an overflow,
/// because `allow_nan` is on by default.
///
/// Rust's `{:e}` gives the same shortest round-trip digits (both are the shortest string
/// that reads back exactly), so only the notation is decided here.
pub(crate) fn float_repr(value: f64) -> String {
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if value.is_nan() {
        return "NaN".to_owned();
    }
    let sci = format!("{value:e}");
    let (sign, sci) = match sci.strip_prefix('-') {
        Some(rest) => ("-", rest.to_owned()),
        None => ("", sci),
    };
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let body = if (-4..16).contains(&exp) {
        if exp >= 0 {
            let whole = (exp + 1) as usize;
            if digits.len() <= whole {
                format!("{digits}{}.0", "0".repeat(whole - digits.len()))
            } else {
                format!("{}.{}", &digits[..whole], &digits[whole..])
            }
        } else {
            format!("0.{}{digits}", "0".repeat((-exp - 1) as usize))
        }
    } else {
        let lead = &digits[..1];
        let rest = &digits[1..];
        let mant = if rest.is_empty() {
            lead.to_owned()
        } else {
            format!("{lead}.{rest}")
        };
        let esign = if exp < 0 { '-' } else { '+' };
        format!("{mant}e{esign}{:02}", exp.abs())
    };
    format!("{sign}{body}")
}

/// `json.dumps(value, indent=indent, separators=(item_sep, key_sep))` — Python's writer with
/// the two knobs charter's settings writers pass it, in the key order the document has.
///
/// `indent` is the string Python repeats per level, or `None` for one line. With an indent,
/// Python puts a newline and the indent after `item_sep`, so `item_sep` is `,` there; on one
/// line it is whatever the file used (`,` or `, `). An empty container is `[]`/`{}` either
/// way. Non-ASCII is escaped (`ensure_ascii`), which is Python's default.
pub fn dumps(
    value: &serde_json::Value,
    indent: Option<&str>,
    item_sep: &str,
    key_sep: &str,
) -> String {
    let mut out = String::new();
    write_styled(value, indent, item_sep, key_sep, 0, &mut out);
    out
}

fn write_styled(
    value: &serde_json::Value,
    indent: Option<&str>,
    item_sep: &str,
    key_sep: &str,
    depth: usize,
    out: &mut String,
) {
    let newline = |level: usize, out: &mut String| {
        if let Some(pad) = indent {
            out.push('\n');
            out.push_str(&pad.repeat(level));
        }
    };
    match value {
        serde_json::Value::String(s) => escape_into(s, out),
        serde_json::Value::Array(items) if items.is_empty() => out.push_str("[]"),
        serde_json::Value::Object(map) if map.is_empty() => out.push_str("{}"),
        serde_json::Value::Array(items) => {
            out.push('[');
            newline(depth + 1, out);
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                    newline(depth + 1, out);
                }
                write_styled(item, indent, item_sep, key_sep, depth + 1, out);
            }
            newline(depth, out);
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            out.push('{');
            newline(depth + 1, out);
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                    newline(depth + 1, out);
                }
                escape_into(key, out);
                out.push_str(key_sep);
                write_styled(item, indent, item_sep, key_sep, depth + 1, out);
            }
            newline(depth, out);
            out.push('}');
        }
        serde_json::Value::Number(n) => out.push_str(&number(n)),
        other => out.push_str(&other.to_string()),
    }
}

/// How an existing JSON file is laid out, as `charter/commands.py:_json_style` guesses it:
/// `(indent, item_sep, key_sep)` to hand [`dumps`] so that adding one key does not reformat
/// the whole file.
///
/// The indent is the whitespace before the first line that starts with a quoted key; with
/// one, the separators are `,` and `: `. Without one the file is one line, and each separator
/// carries a trailing space only if the file already has `":<space>` or `,<space>` somewhere —
/// "space" being Python's `\s`, which is wider than Rust's whitespace.
pub fn json_style(text: &str) -> (Option<String>, String, String) {
    let space = crate::memstore::is_python_space;
    // Python's `re.search(r'\n([ \t]+)"', text)`: every line but the first follows a `\n`.
    // Split rather than walked by index, so the scan is bounded by the text and no offset in
    // it can be off by one into a loop that never ends (the nightly's `at + 1` TIMEOUTs).
    for line in text.split('\n').skip(1) {
        let pad: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if !pad.is_empty() && line[pad.len()..].starts_with('"') {
            return (Some(pad), ",".to_owned(), ": ".to_owned());
        }
    }
    let follows = |mark: &str| {
        text.match_indices(mark)
            .any(|(at, _)| text[at + mark.len()..].chars().next().is_some_and(space))
    };
    let item = if follows(",") { ", " } else { "," };
    let key = if follows("\":") { ": " } else { ":" };
    (None, item.to_owned(), key.to_owned())
}

/// `text` parsed as `JSON.parse` would parse it, the rule charter's settings writers read by
/// (`charter/doctor.py:_json_as_claude_code_parses`): `NaN` and `Infinity` are not JSON.
/// `serde_json` refuses both already; this exists so the rule has one name at every caller.
///
/// **Where this is stricter than Python, it is on purpose.** A string holding a lone
/// surrogate escape (`"\ud800"`) is text Python reads and `serde_json` refuses; such a file is
/// reported as one charter will not rewrite, which is the direction that loses nothing.
pub fn loads_strict(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod number_tests {
    use super::*;

    /// Verified against CPython: `json.dumps(json.loads(src), sort_keys=True)`.
    fn round_trip(src: &str) -> String {
        dumps_sorted(&serde_json::from_str(src).expect("parses"))
    }

    #[test]
    fn an_integer_beyond_u64_keeps_every_digit() {
        // Without `arbitrary_precision` this came back as 1.8446744073709552e19 and the
        // digest was taken over a number the file never held.
        assert_eq!(
            round_trip(r#"{"n": 18446744073709551616}"#),
            r#"{"n": 18446744073709551616}"#
        );
        assert_eq!(
            round_trip(r#"{"n": 123456789012345678901234567890}"#),
            r#"{"n": 123456789012345678901234567890}"#
        );
        assert_eq!(
            round_trip(r#"{"n": 100000000000000000000}"#),
            r#"{"n": 100000000000000000000}"#
        );
    }

    #[test]
    fn an_ordinary_integer_is_written_as_itself() {
        assert_eq!(round_trip(r#"{"n": 0}"#), r#"{"n": 0}"#);
        assert_eq!(round_trip(r#"{"n": -7}"#), r#"{"n": -7}"#);
        assert_eq!(round_trip(r#"{"n": 42}"#), r#"{"n": 42}"#);
    }

    #[test]
    fn a_float_charter_wrote_round_trips_because_it_is_already_pythons_spelling() {
        for src in [
            r#"{"n": 1e-07}"#,
            r#"{"n": 1e-06}"#,
            r#"{"n": 0.5}"#,
            r#"{"n": 1.5e+20}"#,
        ] {
            assert_eq!(round_trip(src), src, "{src}");
        }
    }
}

#[cfg(test)]
mod styled_tests {
    use super::*;

    /// Verified against CPython 3.14: `json.dumps(json.loads(src))`.
    #[test]
    fn a_float_is_written_back_as_pythons_repr_and_not_as_its_literal() {
        for (src, want) in [
            ("1e-7", "1e-07"),
            ("1E5", "100000.0"),
            ("1e400", "Infinity"),
            ("-1e400", "-Infinity"),
            ("0.1", "0.1"),
            ("1.5e20", "1.5e+20"),
            ("123.45", "123.45"),
            ("1e16", "1e+16"),
            ("1e15", "1000000000000000.0"),
            ("-0", "0"),
            ("-0.0", "-0.0"),
            ("1e-4", "0.0001"),
            ("1e-5", "1e-05"),
            ("2.50", "2.5"),
            ("100", "100"),
            ("12345678901234567890.5", "1.2345678901234567e+19"),
        ] {
            let value: serde_json::Value = serde_json::from_str(src).expect("parses");
            assert_eq!(dumps(&value, None, ",", ":"), want, "{src}");
        }
    }

    /// Verified against CPython 3.14: `json.dumps(d, indent=…, separators=…)`.
    #[test]
    fn each_layout_python_writes_is_written_byte_for_byte() {
        let doc: serde_json::Value =
            serde_json::from_str(r#"{"a": [1, {"b": []}], "c": {}, "é": "ü"}"#).unwrap();
        assert_eq!(
            dumps(&doc, Some("\t"), ",", ": "),
            "{\n\t\"a\": [\n\t\t1,\n\t\t{\n\t\t\t\"b\": []\n\t\t}\n\t],\n\t\"c\": {},\n\t\"\\u00e9\": \"\\u00fc\"\n}"
        );
        assert_eq!(
            dumps(&doc, None, ",", ":"),
            r#"{"a":[1,{"b":[]}],"c":{},"\u00e9":"\u00fc"}"#
        );
        assert_eq!(
            dumps(&doc, None, ", ", ": "),
            r#"{"a": [1, {"b": []}], "c": {}, "\u00e9": "\u00fc"}"#
        );
        assert_eq!(
            dumps(&doc, Some("  "), ",", ": ") + "\n",
            dumps_indent2(&doc)
        );
    }

    #[test]
    fn a_files_layout_is_read_the_way_charter_reads_it() {
        let two = (Some("  ".to_owned()), ",".to_owned(), ": ".to_owned());
        assert_eq!(json_style("{\n  \"a\": 1\n}\n"), two);
        assert_eq!(
            json_style("{\n\t\"a\": 1}"),
            (Some("\t".to_owned()), ",".to_owned(), ": ".to_owned())
        );
        assert_eq!(
            json_style(r#"{"a":1,"b":2}"#),
            (None, ",".to_owned(), ":".to_owned())
        );
        assert_eq!(
            json_style(r#"{"a": 1, "b": 2}"#),
            (None, ", ".to_owned(), ": ".to_owned())
        );
        // A line that starts with whitespace and no quote is not an indented key.
        assert_eq!(
            json_style("{\"a\":[\n  1]}"),
            (None, ",".to_owned(), ":".to_owned())
        );
        // Python's `\s` holds U+001F; Rust's whitespace does not.
        assert_eq!(
            json_style("{\"a\":\u{1f}1}"),
            (None, ",".to_owned(), ": ".to_owned())
        );
        assert_eq!(json_style(""), (None, ",".to_owned(), ":".to_owned()));
    }

    /// Verified against CPython: `re.search(r'\n([ \t]+)"', text)` in
    /// `charter/commands.py:_json_style`.
    #[test]
    fn only_a_line_after_a_newline_is_read_for_the_indent() {
        // The FIRST line is not after a newline, so its indent is not the file's.
        assert_eq!(
            json_style("  \"a\": 1"),
            (None, ",".to_owned(), ": ".to_owned())
        );
        // The first indented key wins, however far down it is.
        assert_eq!(
            json_style("{\n\n[\n    \"a\": 1,\n  \"b\": 2}"),
            (Some("    ".to_owned()), ",".to_owned(), ": ".to_owned())
        );
        // A newline as the last byte leaves an empty line, which is no key.
        assert_eq!(json_style("{}\n"), (None, ",".to_owned(), ":".to_owned()));
    }

    /// `charter/doctor.py:_json_as_claude_code_parses`: JSON as `JSON.parse` reads it.
    #[test]
    fn strict_json_is_the_document_and_a_python_only_constant_is_none() {
        assert_eq!(
            loads_strict(r#"{"hooks": {"a": [1, "x"]}}"#),
            Some(serde_json::json!({"hooks": {"a": [1, "x"]}}))
        );
        assert_eq!(loads_strict("[]"), Some(serde_json::json!([])));
        // Python's `json.loads` reads these three; `JSON.parse` and this refuse them.
        for text in ["NaN", "Infinity", r#"{"a": -Infinity}"#, "", "{"] {
            assert_eq!(loads_strict(text), None, "{text:?}");
        }
    }
}
