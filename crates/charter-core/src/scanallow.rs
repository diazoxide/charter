//! The allowlist of a chat's commit scan (SQ-17, ADR 0074 as amended): the findings
//! [`crate::diffscan`] lets through, and why.
//!
//! # Where an entry comes from
//!
//! - **charter's own**, the same in every repository ([`Allowlist::builtin`]): an email address
//!   in a manifest's author field (`Cargo.toml`, `package.json`), `.mailmap`, `AUTHORS`,
//!   `CONTRIBUTORS` and a changelog, which publish names on purpose; and, in a plane, an email
//!   address in a memory file, which the plane's own save already scans by its rules.
//! - **The repository's [`FILE`]**, committed and reviewed like code. Each entry names a rule
//!   (by its id, [`crate::secretshape::LEAK_RULES`]) and the paths it covers, or one value by its
//!   fingerprint ([`fingerprint`]), and always says why.
//!
//! # An agent cannot allow its own finding (V16)
//!
//! **The file is read as it is at `HEAD`**, never from the working tree or the index, so an
//! entry a chat writes and has not committed allows nothing. And **a chat cannot commit it**:
//! charter's `pre-commit` refuses a commit that changes [`FILE`] ([`crate::diffscan`]). The
//! operator's own terminal is not armed, so a change to the allowlist is a commit the operator
//! makes. An agent may write an entry for the operator to read and commit; it takes effect only
//! when the operator does.

use std::path::Path;

use sha2::{Digest, Sha256};

/// The allowlist's file, at the top of the repository.
pub const FILE: &str = ".charter-scan-allow.toml";

/// Where an entry came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// charter's own, the same in every repository.
    Builtin,
    /// The repository's [`FILE`], its Nth `[[allow]]` (1-based).
    File(usize),
}

/// One entry: a rule in some paths, or one value, and why.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The rule id it covers, or `None` for a fingerprint entry, which covers one value under
    /// whatever rule found it.
    pub rule: Option<String>,
    /// The paths it covers, from the top of the repository; empty is every path.
    pub paths: Vec<glob::Pattern>,
    /// One value, by [`fingerprint`].
    pub fingerprint: Option<String>,
    /// Why, in the words of whoever wrote it.
    pub reason: String,
    pub origin: Origin,
}

/// What a finding is, for [`Allowlist::allowing`]: the rule, where, and the value's fingerprint.
#[derive(Debug, Clone, Copy)]
pub struct Seen<'a> {
    pub rule: &'a str,
    pub path: &'a str,
    pub fingerprint: &'a str,
}

/// The entries a repository's scan lets through, and what was wrong with its file.
#[derive(Debug, Clone, Default)]
pub struct Allowlist {
    pub entries: Vec<Entry>,
    /// Each entry of [`FILE`] that was left out, and why. An entry that cannot be read allows
    /// nothing, so what it was meant to allow is still refused.
    pub problems: Vec<String>,
}

/// A value's fingerprint, as an entry names it: `sha256:` and the hex of its SHA-256. The value
/// itself is never written down.
pub fn fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("sha256:{hex}")
}

fn pattern(glob: &str) -> Option<glob::Pattern> {
    glob::Pattern::new(glob).ok()
}

fn options() -> glob::MatchOptions {
    glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    }
}

/// Whether `glob` names `path`. `*` stays within one directory and `**` crosses them, so
/// `CHANGELOG*` covers `CHANGELOG.md` at the top and `**/Cargo.toml` a manifest anywhere.
fn covers(glob: &glob::Pattern, path: &str) -> bool {
    glob.matches_with(path, options())
}

impl Allowlist {
    /// charter's own entries. `plane` is whether the repository is a charter plane (a
    /// `charter.toml` at its top), whose memory files are allowed email addresses.
    pub fn builtin(plane: bool) -> Self {
        let manifests = [
            "Cargo.toml",
            "**/Cargo.toml",
            "package.json",
            "**/package.json",
            ".mailmap",
            "AUTHORS*",
            "**/AUTHORS*",
            "CONTRIBUTORS*",
            "**/CONTRIBUTORS*",
            "CHANGELOG*",
            "**/CHANGELOG*",
        ];
        let mut entries = vec![Entry {
            rule: Some("email".to_owned()),
            paths: manifests.iter().filter_map(|g| pattern(g)).collect(),
            fingerprint: None,
            reason: "authors and changelogs publish names and addresses on purpose".to_owned(),
            origin: Origin::Builtin,
        }];
        if plane {
            entries.push(Entry {
                rule: Some("email".to_owned()),
                paths: [
                    "workspaces/*/memory/**",
                    "personas/*/memory/**",
                    "memory/**",
                ]
                .iter()
                .filter_map(|g| pattern(g))
                .collect(),
                fingerprint: None,
                reason: "a plane's memory is scanned by the plane's own save, by its rules"
                    .to_owned(),
                origin: Origin::Builtin,
            });
        }
        Self {
            entries,
            problems: Vec::new(),
        }
    }

    /// The entries of a [`FILE`]'s text. An entry with no `reason`, with neither a `rule` nor a
    /// `fingerprint`, naming a rule charter does not have, or with a path that is not a glob,
    /// is left out and said in [`Allowlist::problems`].
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let table: toml::Table = match text.parse() {
            Ok(table) => table,
            Err(why) => {
                out.problems
                    .push(format!("{FILE} is not TOML, so it allows nothing: {why}"));
                return out;
            }
        };
        let Some(allow) = table.get("allow") else {
            return out;
        };
        let Some(allow) = allow.as_array() else {
            out.problems
                .push(format!("{FILE}: `allow` is not a list of [[allow]] tables"));
            return out;
        };
        for (at, item) in allow.iter().enumerate() {
            let n = at + 1;
            match entry(item, n) {
                Ok(entry) => out.entries.push(entry),
                Err(why) => out
                    .problems
                    .push(format!("{FILE}: entry {n} allows nothing: {why}")),
            }
        }
        out
    }

    /// charter's own entries and the repository's file together.
    pub fn with(mut self, other: Allowlist) -> Self {
        self.entries.extend(other.entries);
        self.problems.extend(other.problems);
        self
    }

    /// The entry that lets `seen` through, or `None`.
    pub fn allowing(&self, seen: Seen<'_>) -> Option<&Entry> {
        self.entries.iter().find(|entry| {
            let rule = entry.rule.as_deref().is_none_or(|rule| rule == seen.rule);
            let path =
                entry.paths.is_empty() || entry.paths.iter().any(|glob| covers(glob, seen.path));
            let value = entry
                .fingerprint
                .as_deref()
                .is_none_or(|print| print == seen.fingerprint);
            rule && path && value
        })
    }
}

/// One `[[allow]]` table, read.
fn entry(item: &toml::Value, n: usize) -> Result<Entry, String> {
    let table = item.as_table().ok_or("it is not a table")?;
    let text = |key: &str| -> Result<Option<String>, String> {
        match table.get(key) {
            None => Ok(None),
            Some(toml::Value::String(value)) if !value.trim().is_empty() => {
                Ok(Some(value.trim().to_owned()))
            }
            Some(_) => Err(format!("`{key}` is not a word")),
        }
    };
    for key in table.keys() {
        if !["rule", "paths", "fingerprint", "reason"].contains(&key.as_str()) {
            return Err(format!("`{key}` is not a key an entry has"));
        }
    }
    let reason = text("reason")?.ok_or("it gives no `reason`")?;
    let rule = text("rule")?;
    let fingerprint = text("fingerprint")?;
    if rule.is_none() && fingerprint.is_none() {
        return Err("it names neither a `rule` nor a `fingerprint`".to_owned());
    }
    if let Some(rule) = &rule
        && !crate::secretshape::LEAK_RULES
            .iter()
            .any(|(id, _)| id == rule)
    {
        return Err(format!(
            "`{rule}` is not a rule charter has (`charter scan --explain` names them)"
        ));
    }
    if let Some(print) = &fingerprint
        && !(print.starts_with("sha256:") && print.len() == 7 + 64)
    {
        return Err("`fingerprint` is not one `charter scan --explain` printed".to_owned());
    }
    let paths = match table.get("paths") {
        None => Vec::new(),
        Some(toml::Value::Array(globs)) => globs
            .iter()
            .map(|glob| {
                glob.as_str()
                    .and_then(pattern)
                    .ok_or_else(|| format!("`paths` holds {glob}, which is not a glob"))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("`paths` is not a list".to_owned()),
    };
    Ok(Entry {
        rule,
        paths,
        fingerprint,
        reason,
        origin: Origin::File(n),
    })
}

/// Whether `path`, a file at the top of a repository, is the allowlist.
pub fn is_the_file(path: &str) -> bool {
    Path::new(path) == Path::new(FILE)
}

/// The entry `charter scan --explain` offers for a finding: its rule in its file.
pub fn suggested(rule: &str, path: &str) -> String {
    let quoted = glob::Pattern::escape(path)
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "[[allow]]\nrule = \"{rule}\"\npaths = [\"{quoted}\"]\nreason = \"<why this is not a leak>\"\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen<'a>(rule: &'a str, path: &'a str, print: &'a str) -> Seen<'a> {
        Seen {
            rule,
            path,
            fingerprint: print,
        }
    }

    #[test]
    fn an_author_email_in_a_manifest_mailmap_or_changelog_is_allowed_anywhere() {
        let builtin = Allowlist::builtin(false);
        for path in [
            "Cargo.toml",
            "crates/core/Cargo.toml",
            "package.json",
            "app/package.json",
            ".mailmap",
            "CHANGELOG.md",
            "docs/CHANGELOG",
            "AUTHORS",
        ] {
            assert!(
                builtin.allowing(seen("email", path, "x")).is_some(),
                "{path}"
            );
        }
        assert!(
            builtin
                .allowing(seen("email", "src/main.rs", "x"))
                .is_none()
        );
        assert!(
            builtin
                .allowing(seen("forge-token", "Cargo.toml", "x"))
                .is_none(),
            "only an address, never a key, in a manifest"
        );
    }

    #[test]
    fn a_planes_memory_is_allowed_an_email_and_any_other_repositorys_is_not() {
        let path = "workspaces/alpha/memory/20260930-note.md";
        assert!(
            Allowlist::builtin(true)
                .allowing(seen("email", path, "x"))
                .is_some()
        );
        assert!(
            Allowlist::builtin(false)
                .allowing(seen("email", path, "x"))
                .is_none()
        );
        assert!(
            Allowlist::builtin(true)
                .allowing(seen("forge-token", path, "x"))
                .is_none()
        );
    }

    #[test]
    fn a_file_entry_allows_its_rule_in_its_paths_or_one_value_by_its_fingerprint() {
        let print = fingerprint("ada@lovelace.dev");
        let file = Allowlist::parse(&format!(
            r#"
[[allow]]
rule = "email"
paths = ["docs/**"]
reason = "the docs name their authors"

[[allow]]
fingerprint = "{print}"
reason = "a published test card"
"#
        ));
        assert!(file.problems.is_empty(), "{:?}", file.problems);
        let by_path = file.allowing(seen("email", "docs/guide/intro.md", "other"));
        assert_eq!(by_path.map(|e| &e.origin), Some(&Origin::File(1)));
        assert!(
            file.allowing(seen("email", "src/lib.rs", "other"))
                .is_none()
        );
        let by_value = file.allowing(seen("card-number", "src/lib.rs", &print));
        assert_eq!(by_value.map(|e| &e.origin), Some(&Origin::File(2)));
    }

    #[test]
    fn an_entry_that_would_allow_everything_or_cannot_be_read_allows_nothing() {
        let file = Allowlist::parse(
            r#"
[[allow]]
paths = ["**"]
reason = "everything"

[[allow]]
rule = "email"

[[allow]]
rule = "no-such-rule"
reason = "x"

[[allow]]
fingerprint = "md5:abc"
reason = "x"

[[allow]]
rule = "email"
reason = "x"
secret = true
"#,
        );
        assert!(file.entries.is_empty(), "{:?}", file.entries);
        assert_eq!(file.problems.len(), 5, "{:?}", file.problems);
        assert!(
            Allowlist::parse("not = [toml")
                .problems
                .first()
                .is_some_and(|p| p.contains("not TOML"))
        );
    }

    #[test]
    fn a_fingerprint_names_a_value_without_holding_it() {
        let print = fingerprint("ghp_0123456789abcdefABCDEF");
        assert!(print.starts_with("sha256:") && print.len() == 71);
        assert!(!print.contains("0123456789abcdefABCDEF"));
        assert_eq!(print, fingerprint("ghp_0123456789abcdefABCDEF"));
    }
}
