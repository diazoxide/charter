//! `charter.toml`, as `init` writes it and as `init` and `reinit` read it.
//!
//! A port of `charter/instance.py`'s `load`, `default_persona_of`, `default_workspace_of`,
//! `_set_key`, and `commands._render_charter_toml`.

use std::path::Path;

use super::text;

/// The format version `init` writes into a new project (`instance.SCHEMA`). It stays 1: a new
/// project requires no feature, and `schema = 2` is declared only beside `requires` (FR-24, V37a).
pub const WRITTEN: i64 = 1;
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
    let path = crate::names::manifest(root);
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
    match crate::compat::schema(&table).refusal(&shown.to_string()) {
        None => Read::Config(table),
        Some(refusal) => Read::Refused(refusal),
    }
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

/// `commands._render_charter_toml`: what `init` writes into a fresh plane, and then the
/// `[sandbox]` block the Python charter never wrote: every project charter makes runs its chats
/// sandboxed (ADR 0067 §1, ruling V21).
pub fn render(forge: &str, owner: &str, host: Option<&str>) -> String {
    let mut lines = vec![
        format!("schema = {WRITTEN}"),
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
    lines.push(crate::sandbox::on_block());
    lines.join("\n")
}

/// `instance._set_key(root, section, key, value)`: set `key = "value"` inside `[section]`,
/// as a TEXT edit confined to that section's lines, so every comment in the hand-edited file
/// survives.
///
/// **Every byte the operator wrote is kept**, line endings included. Python reads the file
/// with universal newlines and writes it back, so a CRLF `charter.toml` comes back LF from
/// its `persona default`; this keeps the file's own endings and adds its lines with `\n`.
///
/// Read, edited and replaced under [`crate::rewrite::update`]'s lock and rename (#357).
pub fn set_key(root: &Path, section: &str, key: &str, value: &str) -> std::io::Result<()> {
    // A link `init`'s gate let through stays inside the plane and is followed, as every other
    // file `init` writes is; the file it lands on is the one replaced.
    let path = super::linked_to(&crate::names::manifest(root));
    let dir = path.parent().unwrap_or(root);
    crate::rewrite::update(dir, &path, |body| match body {
        Some(body) => {
            let next = edited(body, section, key, value);
            said_as_set(&next, section, key, value).map_err(|why| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: {why}, so it was left as it is", path.display()),
                )
            })?;
            Ok(Some(next))
        }
        None => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} is not there", path.display()),
        )),
    })
    .map(|_| ())
}

/// The table and key a project's stable id is kept under: `[project] id = "<ULID>"` (V76).
pub const PROJECT_ID: (&str, &str) = ("project", "id");

/// The project's stable id: the ULID in its `charter.toml`'s `[project] id`, in its canonical
/// spelling. `None` when there is none yet, or when what is there is not a ULID.
///
/// It is what the session protocol names a project by (ADR 0068, amended by FD-26). A path
/// differs from clone to clone and machine to machine; the id travels with the project, since
/// `charter.toml` is committed.
pub fn project_id(root: &Path) -> Option<String> {
    let Read::Config(table) = load(root) else {
        return None;
    };
    id_in(&table).ok().flatten()
}

/// [`project_id`], minting it first when the project has none: a fresh ULID, written into
/// `charter.toml` as `[project] id` by the same text edit as [`set_key`], so every byte the
/// operator wrote is kept. Read, decided and written under one lock, so two processes asking
/// at once get one id.
///
/// An id that is there and is not a ULID is an error, and the file is left as it is: an id is
/// never written over. So is a directory with no `charter.toml`, which is not a project.
pub fn ensure_project_id(root: &Path) -> std::io::Result<String> {
    let path = super::linked_to(&crate::names::manifest(root));
    let dir = path.parent().unwrap_or(root);
    let mut id = None;
    crate::rewrite::update(dir, &path, |body| {
        let Some(body) = body else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} is not there, so this is not a project", path.display()),
            ));
        };
        let table: toml::Table = body.parse().map_err(|e: toml::de::Error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "{} is not valid TOML: {}",
                    path.display(),
                    e.message().trim()
                ),
            )
        })?;
        match id_in(&table) {
            Ok(Some(held)) => {
                id = Some(held);
                Ok(None)
            }
            Ok(None) => {
                let minted = crate::reopen::mint();
                let next = with_project_id(body, &minted).map_err(|why| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{}: {why}", path.display()),
                    )
                })?;
                id = Some(minted);
                Ok(Some(next))
            }
            Err(why) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: {why}", path.display()),
            )),
        }
    })?;
    id.ok_or_else(|| std::io::Error::other("no project id was read or minted"))
}

/// `body` with `[project] id = "<minted>"` added, through `toml_edit` as the Settings
/// tab's save edits (`settings::save`), so every comment, key order and spacing is kept and a
/// `project` table written any way TOML allows (a header with a comment, spaces or quotes, an
/// inline table, a dotted key, a subtable) is the one the id goes into.
///
/// The text is parsed again before it is handed back, and must read as holding exactly this
/// id; anything else is an error and nothing is written. A `project` that is not a table has no
/// room for an id, and is an error too.
fn with_project_id(body: &str, minted: &str) -> Result<String, String> {
    let (section, key) = PROJECT_ID;
    let mut doc: toml_edit::DocumentMut = body
        .parse()
        .map_err(|e: toml_edit::TomlError| format!("not valid TOML: {}", e.message().trim()))?;
    match doc.get_mut(section) {
        None => {
            let mut table = toml_edit::Table::new();
            table.insert(key, toml_edit::value(minted));
            doc.insert(section, toml_edit::Item::Table(table));
        }
        Some(item) => match item.as_table_like_mut() {
            Some(table) => {
                table.insert(key, toml_edit::value(minted));
            }
            None => {
                return Err(format!(
                    "`{section}` is not a table, so it has no room for an id"
                ));
            }
        },
    }
    let next = doc.to_string();
    let reread: toml::Table = next.parse().map_err(|e: toml::de::Error| {
        format!("the id would leave a file that does not parse: {e}")
    })?;
    match id_in(&reread) {
        Ok(Some(held)) if held == minted => Ok(next),
        other => Err(format!(
            "the id would not read back as written ({other:?}), so nothing was written"
        )),
    }
}

/// The id `table` holds: `Ok(None)` for none, an error for one that is not a ULID.
fn id_in(table: &toml::Table) -> Result<Option<String>, String> {
    let (section, key) = PROJECT_ID;
    match table.get(section).and_then(|s| s.get(key)) {
        None => Ok(None),
        Some(toml::Value::String(word)) => crate::reopen::a_ulid(word)
            .map(Some)
            .ok_or_else(|| format!("[{section}] {key} = \"{word}\" is not a ULID")),
        Some(other) => Err(format!(
            "[{section}] {key} is a {}, not a ULID",
            other.type_str()
        )),
    }
}

/// Whether `next`, what [`set_key`]'s text edit made, parses and says `[section] key = value`.
/// The edit finds a section only by a plain `[section]` header, so a table written any other
/// way (a header with a comment, an inline table, a dotted key) would get a second one, and the
/// file would not parse; a `section` that is not a table would be shadowed. Either is refused.
fn said_as_set(next: &str, section: &str, key: &str, value: &str) -> Result<(), String> {
    let table: toml::Table = next.parse().map_err(|e: toml::de::Error| {
        format!(
            "setting [{section}] {key} here would leave a file that does not parse ({})",
            e.message().trim()
        )
    })?;
    match table
        .get(section)
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_str())
    {
        Some(said) if said == value => Ok(()),
        _ => Err(format!(
            "setting [{section}] {key} here would not read back as \"{value}\""
        )),
    }
}

/// `body` with `key = "value"` set inside `[section]`, as [`set_key`] edits it: a text edit,
/// so a caller that must know the edit landed in the table re-reads the result.
pub(crate) fn edited(body: &str, section: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text::lines_with_ends(body)
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
    lines.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The block every new project ends with (ADR 0067 §1, ruling V21 5).
    const SANDBOX_ON: &str =
        "\n[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"forge\", \"toolchains\"]\n";

    /// Verified against CPython 3.14: `commands._render_charter_toml(...)`, and then the
    /// `[sandbox]` block the Python charter never wrote (ADR 0067 §1).
    #[test]
    fn a_fresh_charter_toml_is_the_one_python_writes() {
        assert_eq!(
            render("github", "acme", None),
            format!(
                "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n\n[memory]\nshare = \"local\"\n{SANDBOX_ON}"
            )
        );
        assert_eq!(
            render("gitlab", "", Some("git.example.com")),
            format!(
                "schema = 1\n\n[[forge]]\nkind = \"gitlab\"\nhost = \"git.example.com\"\n\n[memory]\nshare = \"local\"\n{SANDBOX_ON}"
            )
        );
        assert_eq!(
            render("github", "a\"c\u{e9}", None).lines().nth(4),
            Some("owner = \"a\\\"c\\u00e9\"")
        );
    }

    /// A project charter makes runs every chat sandboxed, with the default egress (ADR 0067
    /// §1, ruling V21 5), and the file says nothing charter would refuse.
    #[test]
    fn a_new_project_turns_the_sandbox_on_with_the_default_egress() {
        let said = crate::sandbox::Plane::of(Some(&render("github", "acme", None))).said();

        assert_eq!(
            said.policy,
            Some(crate::sandbox::Policy {
                egress: crate::sandbox::Preset::DEFAULT.to_vec(),
                hosts: vec![],
                certificate_checks: false,
            })
        );
        assert!(said.refused.is_empty(), "{:?}", said.refused);
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

    /// #357: `init`'s declaration of its front door goes through the same temp-and-rename,
    /// so a crash between the two leaves the charter.toml `init` had just written.
    #[test]
    fn init_s_declaration_replaces_charter_toml_whole_or_not_at_all() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
        let _hook = crate::rewrite::hook::set(|target, _| {
            assert_eq!(std::fs::read_to_string(target).unwrap(), "schema = 1\n");
            Err(std::io::Error::other("killed before the rename"))
        });

        assert!(set_key(dir.path(), "persona", "default", "steward").is_err());

        assert_eq!(
            std::fs::read_to_string(dir.path().join("charter.toml")).unwrap(),
            "schema = 1\n"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
