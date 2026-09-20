//! `charter.toml`, as `init` writes it and as `init` and `reinit` read it.
//!
//! A port of `charter/instance.py`'s `load`, `default_persona_of`, `default_workspace_of`,
//! `_set_key`, and `commands._render_charter_toml`.

use std::path::Path;

use super::text;

/// The plane format this charter reads and writes (`instance.SCHEMA`).
pub const SCHEMA: i64 = 1;

/// What reading `charter.toml` found.
#[derive(Debug, Clone, PartialEq)]
pub enum Read {
    /// No file, or one that could not be read — an empty configuration, as Python's `load`
    /// answers `{}` for any `OSError`.
    Absent,
    /// Parsed, and a format this charter understands.
    Config(toml::Table),
    /// Not TOML. Python's `load` raises, and a caller decides what that costs.
    Malformed(String),
    /// A format version this charter cannot place: the message `cli.main` refuses with.
    Refused(String),
}

/// `instance.load(root)`.
pub fn load(root: &Path) -> Read {
    let path = root.join(crate::plane::MANIFEST);
    let Ok(bytes) = std::fs::read(&path) else {
        return Read::Absent;
    };
    let shown = path.display();
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => return Read::Malformed(format!("{shown} is not valid TOML: {e}")),
    };
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(e) => {
            return Read::Malformed(format!("{shown} is not valid TOML: {}", e.message().trim()));
        }
    };
    match table.get("schema") {
        None => Read::Config(table),
        Some(toml::Value::Integer(found)) if *found > SCHEMA => Read::Refused(format!(
            "{shown} declares schema {found}, but this charter understands {SCHEMA}. Upgrade \
             charter: `uv tool install charter-cp --force --refresh`."
        )),
        Some(toml::Value::Integer(_)) => Read::Config(table),
        Some(other) => Read::Refused(format!(
            "{shown} declares schema {}, which is not a plane format version this charter can \
             compare against {SCHEMA}. charter will not operate on a plane whose format it \
             cannot place. Fix the `schema` line, or upgrade charter: `uv tool install \
             charter-cp --force --refresh`.",
            py_value(other)
        )),
    }
}

/// A TOML value as Python's `repr` prints what `tomllib` made of it —
/// [`crate::pyrepr::repr_toml`], the crate's one answer.
///
/// This had a body of its own, and `profiles` had a second one for the same question. They
/// disagreed about a float: this one asked [`crate::pyjson::float_repr`] and the other let
/// `toml`'s own `Display` write it, so `1e300` was quoted back as `1e+300` by one refusal and
/// as `1e300` by the next. `repr_toml` is this body, moved.
fn py_value(value: &toml::Value) -> String {
    crate::pyrepr::repr_toml(value)
}

/// `str(value)` for what a `[section] default` holds — [`crate::pyrepr::str_toml`].
fn py_str(value: &toml::Value) -> String {
    crate::pyrepr::str_toml(value)
}

/// `instance.default_persona_of(cfg)` — whether the plane declares a front door.
///
/// A blank value is absence. Anything else Python would turn into a non-blank string with
/// `str()`, so it counts as declared: `init` then leaves the question alone, which is the
/// point of asking it.
pub fn declares_default_persona(cfg: &toml::Table) -> bool {
    let Some(section) = cfg.get("persona") else {
        return false;
    };
    let Some(section) = section.as_table() else {
        // Python reads a FALSY section as an empty one and calls `.get` on anything else,
        // which raises. A section that is not a table is somebody's own structure, and
        // inventing a front door over it is not additive.
        return !matches!(section,
            toml::Value::String(s) if s.is_empty())
            && !matches!(
                section,
                toml::Value::Integer(0) | toml::Value::Boolean(false)
            )
            && !matches!(section, toml::Value::Array(a) if a.is_empty());
    };
    section
        .get("default")
        .is_some_and(|v| !crate::memstore::py_strip(&py_str(v)).is_empty())
}

/// `instance.default_workspace_of(cfg, "default")`: `[workspace] default` when it names a
/// workspace, else `default`.
pub fn default_workspace(cfg: &toml::Table) -> String {
    let named = cfg
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|section| section.get("default"))
        .filter(|v| {
            matches!(
                v,
                toml::Value::String(_)
                    | toml::Value::Integer(_)
                    | toml::Value::Float(_)
                    | toml::Value::Boolean(_)
            )
        })
        .map(|v| crate::memstore::py_strip(&py_str(v)).to_owned())
        .unwrap_or_default();
    if crate::contain::workspace_name_ok(&named) {
        named
    } else {
        "default".to_owned()
    }
}

/// `commands._toml_str`: a TOML basic string, through JSON's escaping as Python does it.
fn toml_str(s: &str) -> String {
    crate::pyjson::dumps(&serde_json::Value::String(s.to_owned()), None, ",", ":")
}

/// `commands._render_charter_toml`: what `init` writes into a fresh plane.
pub fn render(forge: &str, owner: &str, host: Option<&str>) -> String {
    let mut lines = vec![
        format!("schema = {SCHEMA}"),
        String::new(),
        "[[forge]]".to_owned(),
        format!("kind = {}", toml_str(forge)),
    ];
    if !owner.is_empty() {
        lines.push(format!("owner = {}", toml_str(owner)));
    }
    if let Some(host) = host.filter(|h| !h.is_empty()) {
        lines.push(format!("host = {}", toml_str(host)));
    }
    lines.extend([
        String::new(),
        "[memory]".to_owned(),
        "share = \"local\"".to_owned(),
        String::new(),
    ]);
    lines.join("\n")
}

/// `instance._set_key(root, section, key, value)`: set `key = "value"` inside `[section]`,
/// as a TEXT edit confined to that section's lines, so every comment in the hand-edited file
/// survives.
///
/// **Every byte the operator wrote is kept**, line endings included. Python reads the file
/// with universal newlines and writes it back, so a CRLF `charter.toml` comes back LF from
/// its `persona default`; this keeps the file's own endings and adds its lines with `\n`.
pub fn set_key(root: &Path, section: &str, key: &str, value: &str) -> std::io::Result<()> {
    let path = root.join(crate::plane::MANIFEST);
    let body = std::fs::read_to_string(&path)?;
    let mut lines: Vec<String> = text::lines_with_ends(&body)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let is_header = |line: &str, name: Option<&str>| {
        let line = text::without_end(line).trim_start_matches([' ', '\t']);
        match name {
            Some(name) => line
                .strip_prefix(&format!("[{name}]"))
                .is_some_and(|rest| rest.chars().all(|c| c == ' ' || c == '\t')),
            None => line.starts_with('['),
        }
    };
    // `^([ \t]*key[ \t]*=[ \t]*).*$` — the prefix up to the value, or `None`.
    let key_prefix = |line: &str| -> Option<usize> {
        let bare = text::without_end(line);
        let lead = bare.len() - bare.trim_start_matches([' ', '\t']).len();
        let rest = bare[lead..].strip_prefix(key)?;
        let pad = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let rest = rest[pad..].strip_prefix('=')?;
        let pad2 = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        Some(lead + key.len() + pad + 1 + pad2)
    };
    let setting = format!("{key} = \"{value}\"\n");
    match lines.iter().position(|l| is_header(l, Some(section))) {
        None => {
            if lines.last().is_some_and(|l| !l.ends_with('\n')) {
                lines.push("\n".to_owned());
            }
            lines.push("\n".to_owned());
            lines.push(format!("[{section}]\n"));
            lines.push(setting);
        }
        Some(start) => {
            let stop = (start + 1..lines.len())
                .find(|&i| is_header(&lines[i], None))
                .unwrap_or(lines.len());
            match (start + 1..stop).find_map(|i| key_prefix(&lines[i]).map(|at| (i, at))) {
                None => lines.insert(start + 1, setting),
                Some((i, at)) => {
                    let line = &lines[i];
                    let end = &line[text::without_end(line).len()..];
                    lines[i] = format!("{}\"{value}\"{end}", &line[..at]);
                }
            }
        }
    }
    std::fs::write(&path, lines.concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verified against CPython 3.14: `commands._render_charter_toml(...)`.
    #[test]
    fn a_fresh_charter_toml_is_the_one_python_writes() {
        assert_eq!(
            render("github", "acme", None),
            "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n\n[memory]\nshare = \"local\"\n"
        );
        assert_eq!(
            render("gitlab", "", Some("git.example.com")),
            "schema = 1\n\n[[forge]]\nkind = \"gitlab\"\nhost = \"git.example.com\"\n\n[memory]\nshare = \"local\"\n"
        );
        assert_eq!(
            render("github", "a\"c\u{e9}", None).lines().nth(4),
            Some("owner = \"a\\\"c\\u00e9\"")
        );
    }

    fn set(body: &str) -> String {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), body).unwrap();
        set_key(dir.path(), "persona", "default", "steward").unwrap();
        std::fs::read_to_string(dir.path().join("charter.toml")).unwrap()
    }

    /// Verified against CPython 3.14: `instance.set_default_persona(root, "steward")`.
    #[test]
    fn a_key_is_set_inside_its_own_section_and_nowhere_else() {
        assert_eq!(
            set("schema = 1\n"),
            "schema = 1\n\n[persona]\ndefault = \"steward\"\n"
        );
        assert_eq!(
            set("schema = 1"),
            "schema = 1\n\n[persona]\ndefault = \"steward\"\n"
        );
        assert_eq!(
            set("[workspace]\ndefault = \"w\"\n[persona]\n# who\nx = 1\n[memory]\n"),
            "[workspace]\ndefault = \"w\"\n[persona]\ndefault = \"steward\"\n# who\nx = 1\n[memory]\n"
        );
        assert_eq!(
            set("[persona]\n  default   =  \"old\" # note\n"),
            "[persona]\n  default   =  \"steward\"\n"
        );
    }

    #[test]
    fn the_operators_line_endings_are_kept() {
        assert_eq!(
            set("schema = 1\r\n# kept\r\n"),
            "schema = 1\r\n# kept\r\n\n[persona]\ndefault = \"steward\"\n"
        );
    }
}
