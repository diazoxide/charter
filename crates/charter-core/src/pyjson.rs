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
        other => out.push_str(&other.to_string()),
    }
}

/// One JSON string, escaped as `ensure_ascii=True` escapes it: nothing above US-ASCII
/// survives as itself.
fn escape_into(s: &str, out: &mut String) {
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
    write_indented(value, 0, &mut out);
    out.push('\n');
    out
}

fn write_indented(value: &serde_json::Value, depth: usize, out: &mut String) {
    let pad = |n: usize, out: &mut String| out.push_str(&"  ".repeat(n));
    match value {
        serde_json::Value::String(s) => escape_into(s, out),
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
                write_indented(item, depth + 1, out);
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
                escape_into(key, out);
                out.push_str(": ");
                write_indented(item, depth + 1, out);
            }
            out.push('\n');
            pad(depth, out);
            out.push('}');
        }
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
    fn a_character_outside_the_basic_plane_is_written_as_a_surrogate_pair() {
        // Verified against CPython: json.dumps({"k": "ship \U0001F680"}, sort_keys=True)
        let doc = json!({"k": "ship 🚀"});

        assert_eq!(dumps_sorted(&doc), r#"{"k": "ship \ud83d\ude80"}"#);
    }
}
