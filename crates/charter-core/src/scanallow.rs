//! The allowlist of a chat's commit scan (SQ-17, ADR 0074 as amended): the findings
//! [`crate::diffscan`] lets through, and why.
//!
//! # Where an entry comes from
//!
//! - **charter's own**, the same in every repository ([`Allowlist::builtin`]): an email address
//!   in a file named `Cargo.toml`, `package.json`, `.mailmap`, `AUTHORS`, `CONTRIBUTORS` or
//!   `CHANGELOG` (the last three also as `.md` and `.txt`), which publish names on purpose. A
//!   repository turns them off with `[builtin] enabled = false` in its [`FILE`].
//! - **The repository's [`FILE`]**, committed and reviewed like code. Each entry names a rule
//!   (by its id, [`crate::secretshape::leak_rules`]) and the paths it covers, or one key by its
//!   fingerprint ([`fingerprint`]), and always says why.
//!
//! # An agent's own finding: a mistake guard, not a boundary
//!
//! **The file is read as it is at `HEAD`**, never from the working tree or the index, so an
//! entry a chat has written and not committed allows nothing. **A chat's `pre-commit` refuses a
//! commit that changes it** ([`crate::diffscan`]), and the Bash guard refuses the two ways past
//! `pre-commit` it can see cheaply (`commitguard`). The operator's own terminal is not armed,
//! so the ordinary way an entry takes effect is the operator's commit.
//!
//! **That holds against a mistake, not against an agent that means to get round it.** git moves
//! `HEAD` without a `pre-commit` in several ways: a fast-forward to a commit made elsewhere, a
//! revert, a reset or checkout to another commit, `commit-tree` with `update-ref`, and `am`,
//! which ADR 0074's V26b already records. Holding a chat to the allowlist its operator committed
//! is the sandbox's work (ADR 0067), and a follow-up there.

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

/// One entry: a rule in some paths, or one key, and why.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The rule id it covers, or `None` for a fingerprint entry, which covers one key under
    /// whatever key rule found it.
    pub rule: Option<String>,
    /// The paths it covers, from the top of the repository. A rule entry always has at least
    /// one; a fingerprint entry may have none, which is every path.
    pub paths: Vec<glob::Pattern>,
    /// One key, by [`fingerprint`].
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
#[derive(Debug, Clone)]
pub struct Allowlist {
    pub entries: Vec<Entry>,
    /// Each entry of [`FILE`] that was left out, and why. An entry that cannot be read allows
    /// nothing, so what it was meant to allow is still refused.
    pub problems: Vec<String>,
    /// Whether charter's own entries apply: `[builtin] enabled`, true unless the file says not.
    pub builtin: bool,
}

impl Default for Allowlist {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            problems: Vec::new(),
            builtin: true,
        }
    }
}

/// A value's fingerprint, as an entry names a key: `sha256:` and the hex of its SHA-256.
///
/// **Only a key is named this way.** A key is a long random run, and its SHA-256 says nothing
/// about it. Personal data is not: an email address or a card number can be recovered from its
/// hash by guessing, and an allowlist is committed, so a personal rule is let through by path,
/// never by fingerprint ([`crate::secretshape::is_personal`]).
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

/// Whether `glob` names `path`. `*` stays within one directory and `**` crosses them.
fn covers(glob: &glob::Pattern, path: &str) -> bool {
    glob.matches_with(path, options())
}

/// The file names charter's own entry covers, at any depth.
pub const BUILTIN_NAMES: [&str; 11] = [
    "Cargo.toml",
    "package.json",
    ".mailmap",
    "AUTHORS",
    "AUTHORS.md",
    "AUTHORS.txt",
    "CONTRIBUTORS",
    "CONTRIBUTORS.md",
    "CONTRIBUTORS.txt",
    "CHANGELOG",
    "CHANGELOG.md",
];

impl Allowlist {
    /// charter's own entry: an email address in a file named one of [`BUILTIN_NAMES`], at the
    /// top or in any directory. Never a key.
    pub fn builtin() -> Self {
        let paths = BUILTIN_NAMES
            .iter()
            .flat_map(|name| [(*name).to_owned(), format!("**/{name}")])
            .filter_map(|glob| pattern(&glob))
            .collect();
        Self {
            entries: vec![Entry {
                rule: Some("email".to_owned()),
                paths,
                fingerprint: None,
                reason: "authors and changelogs publish names and addresses on purpose".to_owned(),
                origin: Origin::Builtin,
            }],
            ..Self::default()
        }
    }

    /// The entries of a [`FILE`]'s text. An entry is left out, and said in
    /// [`Allowlist::problems`], when it has no `reason`; when it names neither a `rule` nor a
    /// `fingerprint`; when it names a rule charter does not have; when it has a `rule` and no
    /// `paths` (a repository-wide entry writes `paths = ["**"]`, which a reviewer sees); when it
    /// names a personal rule by fingerprint; or when a path is not a glob.
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
        for key in table.keys() {
            if !["allow", "builtin"].contains(&key.as_str()) {
                out.problems
                    .push(format!("{FILE}: `{key}` is not a table charter reads"));
            }
        }
        match table.get("builtin").map(|b| b.get("enabled")) {
            None => {}
            Some(Some(toml::Value::Boolean(enabled))) => out.builtin = *enabled,
            Some(_) => out.problems.push(format!(
                "{FILE}: `[builtin]` holds one key, `enabled = true|false`"
            )),
        }
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

    /// A repository's allowlist: its file's entries, after charter's own unless the file turns
    /// them off.
    pub fn of(file: Allowlist) -> Self {
        if !file.builtin {
            return file;
        }
        let mut out = Self::builtin();
        out.entries.extend(file.entries);
        out.problems.extend(file.problems);
        out
    }

    /// The entry that lets `seen` through, or `None`. A fingerprint entry never lets a
    /// personal rule through, whatever its fingerprint.
    pub fn allowing(&self, seen: Seen<'_>) -> Option<&Entry> {
        self.entries.iter().find(|entry| {
            let rule = entry.rule.as_deref().is_none_or(|rule| rule == seen.rule);
            let path =
                entry.paths.is_empty() || entry.paths.iter().any(|glob| covers(glob, seen.path));
            let value = match entry.fingerprint.as_deref() {
                None => true,
                Some(print) => {
                    print == seen.fingerprint && !crate::secretshape::is_personal(seen.rule)
                }
            };
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
        && !crate::secretshape::leak_rules()
            .iter()
            .any(|(id, _)| id == rule)
    {
        return Err(format!(
            "`{rule}` is not a rule charter has (`charter scan --explain` names them)"
        ));
    }
    if let Some(print) = &fingerprint {
        if !(print.starts_with("sha256:") && print.len() == 7 + 64) {
            return Err("`fingerprint` is not one `charter scan --explain` printed".to_owned());
        }
        if let Some(rule) = rule
            .as_deref()
            .filter(|r| crate::secretshape::is_personal(r))
        {
            return Err(format!(
                "`{rule}` is personal data, which is let through by `paths`, never by \
                 fingerprint: a hash of it can be reversed by guessing"
            ));
        }
    }
    let paths: Vec<glob::Pattern> = match table.get("paths") {
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
    if rule.is_some() && paths.is_empty() {
        return Err(
            "a `rule` entry names its `paths`; for the whole repository, write `paths = [\"**\"]`"
                .to_owned(),
        );
    }
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
    path == FILE
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
        let builtin = Allowlist::builtin();
        for path in [
            "Cargo.toml",
            "crates/core/Cargo.toml",
            "package.json",
            "app/package.json",
            ".mailmap",
            "CHANGELOG.md",
            "docs/CHANGELOG",
            "AUTHORS",
            "CONTRIBUTORS.txt",
        ] {
            assert!(
                builtin.allowing(seen("email", path, "x")).is_some(),
                "{path}"
            );
        }
        assert!(
            builtin
                .allowing(seen("forge-token", "Cargo.toml", "x"))
                .is_none(),
            "only an address, never a key, in a manifest"
        );
    }

    #[test]
    fn the_builtin_names_are_exact_so_a_file_that_only_starts_like_one_is_not_covered() {
        let builtin = Allowlist::builtin();
        for path in [
            "src/main.rs",
            "CHANGELOG-draft.rs",
            "AUTHORSHIP.md",
            "Cargo.toml.bak",
            "workspaces/alpha/memory/note.md",
        ] {
            assert!(
                builtin.allowing(seen("email", path, "x")).is_none(),
                "{path}"
            );
        }
    }

    #[test]
    fn a_repository_can_turn_charters_own_entries_off() {
        let file = Allowlist::parse("[builtin]\nenabled = false\n");
        assert!(file.problems.is_empty(), "{:?}", file.problems);
        assert!(
            Allowlist::of(file)
                .allowing(seen("email", "Cargo.toml", "x"))
                .is_none()
        );
        assert!(
            Allowlist::of(Allowlist::parse(""))
                .allowing(seen("email", "Cargo.toml", "x"))
                .is_some()
        );
    }

    #[test]
    fn a_file_entry_allows_its_rule_in_its_paths_or_one_key_by_its_fingerprint() {
        let print = fingerprint("ghp_0123456789abcdefABCDEF");
        let file = Allowlist::parse(&format!(
            r#"
[[allow]]
rule = "email"
paths = ["docs/**"]
reason = "the docs name their authors"

[[allow]]
fingerprint = "{print}"
reason = "a revoked token in a fixture"
"#
        ));
        assert!(file.problems.is_empty(), "{:?}", file.problems);
        let by_path = file.allowing(seen("email", "docs/guide/intro.md", "other"));
        assert_eq!(by_path.map(|e| &e.origin), Some(&Origin::File(1)));
        assert!(
            file.allowing(seen("email", "src/lib.rs", "other"))
                .is_none()
        );
        let by_value = file.allowing(seen("forge-token", "src/lib.rs", &print));
        assert_eq!(by_value.map(|e| &e.origin), Some(&Origin::File(2)));
    }

    #[test]
    fn personal_data_is_never_let_through_by_fingerprint() {
        let print = fingerprint("4111 1111 1111 1111");
        let file = Allowlist::parse(&format!(
            "[[allow]]\nfingerprint = \"{print}\"\nreason = \"a test card\"\n"
        ));
        assert!(file.problems.is_empty(), "{:?}", file.problems);
        for rule in ["card-number", "email", "us-ssn"] {
            assert!(
                file.allowing(seen(rule, "src/lib.rs", &print)).is_none(),
                "{rule}"
            );
        }
        let named = Allowlist::parse(&format!(
            "[[allow]]\nrule = \"card-number\"\nfingerprint = \"{print}\"\nreason = \"x\"\n"
        ));
        assert!(named.entries.is_empty());
        assert!(
            named.problems[0].contains("personal data"),
            "{:?}",
            named.problems
        );
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
paths = ["**"]

[[allow]]
rule = "no-such-rule"
paths = ["**"]
reason = "x"

[[allow]]
fingerprint = "md5:abc"
reason = "x"

[[allow]]
rule = "email"
paths = ["**"]
reason = "x"
secret = true

[[allow]]
rule = "private-key"
reason = "a key rule with no paths"
"#,
        );
        assert!(file.entries.is_empty(), "{:?}", file.entries);
        assert_eq!(file.problems.len(), 6, "{:?}", file.problems);
        assert!(file.problems[5].contains("paths"), "{:?}", file.problems);
        assert!(
            Allowlist::parse("not = [toml")
                .problems
                .first()
                .is_some_and(|p| p.contains("not TOML"))
        );
    }

    #[test]
    fn a_fingerprint_is_the_sha256_of_the_value() {
        let print = fingerprint("ghp_0123456789abcdefABCDEF");
        assert!(print.starts_with("sha256:") && print.len() == 71);
        assert!(!print.contains("0123456789abcdefABCDEF"));
        assert_eq!(print, fingerprint("ghp_0123456789abcdefABCDEF"));
    }
}
