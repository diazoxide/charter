//! A persona with its inheritance applied: the merged frontmatter, and the tools it may run
//! without a prompt — `charter/persona.py`'s `resolve`, `tools_of`, `uses_of`, `borrows_of`,
//! `effective_tools`, `bin_scripts` and `list_personas`.
//!
//! [`crate::personas`] loads ONE definition and walks the `extends:` chain; this is what the
//! chain adds up to. Two readers need it and both are hooks: the session briefing reads the
//! merged `role:` and `delegate-when:`, and the persona tool gate ([`crate::personagate`]) reads
//! the merged `tools:`, `uses:` and `borrows:`.
//!
//! # Merging, as charter merges
//!
//! The chain is walked ROOT ancestor first, so a child's value for a plain key replaces its
//! parent's. Four keys are not plain: `tools`, `agent-tools` and `uses` ACCUMULATE, in that
//! ancestor-to-child order and without repeats, and `extends` is the chain itself and is never
//! merged. A key whose value is empty does not override — `persona.resolve`'s `or not v`.
//!
//! # `borrows:` fails closed
//!
//! `borrows` is read off the key's PRESENCE, so a declaration charter could not read would
//! otherwise look like no declaration — and no declaration means "borrow every `uses:`
//! persona's tools". charter#575 is that hole. So a grant-deciding key (`borrows`, `extends`)
//! that was misspelled (`Borrows:`) or written twice in any definition of the chain borrows
//! NOTHING, and the author's intent is not guessed in the wide direction.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::memstore::py_strip;
use crate::personas;

/// The sentinel `borrows: none` — "borrow from nobody", as opposed to not saying.
pub const BORROWS_NONE: &str = "none";

/// Where a persona keeps executables of its own — `persona.BIN_DIR`.
pub const BIN_DIR: &str = "bin";

/// The keys whose unreadability makes an absent `borrows:` a lie — `_GRANT_DECIDING_KEYS`.
const GRANT_DECIDING_KEYS: [&str; 2] = ["borrows", "extends"];

/// The whole frontmatter vocabulary — `persona.KNOWN_KEYS`. Only its case-folded spellings are
/// used here, to recognise a MISSPELLED grant-deciding key.
const KNOWN_KEYS: [&str; 21] = [
    "model",
    "color",
    "memory",
    "name",
    "role",
    "vault",
    "extends",
    "uses",
    "delegate-when",
    "description",
    "agent-description",
    "agent-tools",
    "tools",
    "activity",
    "dispatch-isolation",
    "draft",
    "skills",
    "disallowed-tools",
    "routing",
    "routes-to",
    "borrows",
];

/// The keys that accumulate down the chain rather than being replaced.
const LIST_KEYS: [&str; 3] = ["tools", "agent-tools", "uses"];

/// A persona with its chain applied — `persona.resolve`'s return.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    /// The merged frontmatter. `name` is always the persona asked about.
    pub meta: BTreeMap<String, String>,
    /// The chain, child first.
    pub lineage: Vec<String>,
}

impl Resolved {
    /// One merged value, or `None`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.meta.get(key).map(String::as_str)
    }
}

/// `v.strip('[]').split(',')`, each part stripped, empties dropped — `persona._csv_list`.
pub fn csv_list(value: Option<&str>) -> Vec<String> {
    let raw = value.unwrap_or_default();
    raw.trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|part| py_strip(part).to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// The last value a definition gives `key` — `dict(pairs)`, where a later line wins.
fn last<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// `name` with its chain applied, or `None` when `name` does not load.
pub fn resolve(root: &Path, name: &str) -> Option<Resolved> {
    let chain = personas::lineage(root, name);
    if chain.is_empty() {
        return None;
    }
    let mut meta: BTreeMap<String, String> = BTreeMap::new();
    let mut lists: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for ancestor in chain.iter().rev() {
        let pairs = personas::load(root, ancestor)?;
        // `dict(pairs)`: the last line for each key, and the keys the file wrote.
        let mut keys: Vec<&str> = Vec::new();
        for (key, _) in &pairs {
            if !keys.contains(&key.as_str()) {
                keys.push(key);
            }
        }
        for key in keys {
            let value = last(&pairs, key).unwrap_or_default();
            if LIST_KEYS.contains(&key) || key == "extends" || value.is_empty() {
                continue;
            }
            meta.insert(key.to_string(), value.to_string());
        }
        // `load` defaults `name` to the persona's own name when the file does not say.
        if last(&pairs, "name").is_none_or(str::is_empty) {
            meta.insert("name".into(), ancestor.clone());
        }
        for key in LIST_KEYS {
            let values = csv_list(last(&pairs, key));
            let into = lists.entry(key).or_default();
            for value in values {
                if key == "uses" && value == name {
                    continue;
                }
                if !into.contains(&value) {
                    into.push(value);
                }
            }
        }
    }
    meta.insert("name".into(), name.to_string());
    for (key, values) in lists {
        if !values.is_empty() {
            meta.insert(key.to_string(), values.join(", "));
        }
    }
    Some(Resolved {
        meta,
        lineage: chain,
    })
}

/// The KNOWN key `key` is a case-variant of, when it is not itself known —
/// `persona.misspelled_key`.
pub(crate) fn misspelled_key(key: &str) -> Option<&'static str> {
    if KNOWN_KEYS.contains(&key) {
        return None;
    }
    // `str.casefold`, for the one case where it differs from lowering on the way to a key
    // charter knows: U+017F LONG S folds to `s`, so `ſkills:`/`borrowſ:` is a misspelling of
    // a known key and is reported (and a `borrows` fails closed) as Python reports it.
    let folded = key.to_lowercase().replace('\u{17f}', "s");
    KNOWN_KEYS.iter().copied().find(|k| *k == folded)
}

/// Did the author write a grant-deciding declaration charter could not read? —
/// `persona._borrows_unreadable`.
fn borrows_unreadable(root: &Path, resolved: &Resolved) -> bool {
    if resolved
        .meta
        .keys()
        .any(|k| misspelled_key(k).is_some_and(|m| GRANT_DECIDING_KEYS.contains(&m)))
    {
        return true;
    }
    for ancestor in &resolved.lineage {
        let Some(pairs) = personas::load(root, ancestor) else {
            continue;
        };
        for key in GRANT_DECIDING_KEYS {
            if pairs.iter().filter(|(k, _)| k == key).count() > 1 {
                return true;
            }
        }
    }
    false
}

/// `tools:` with the chain applied — `persona.tools_of`.
pub fn tools_of(root: &Path, name: &str) -> BTreeSet<String> {
    resolve(root, name)
        .map(|r| csv_list(r.get("tools")).into_iter().collect())
        .unwrap_or_default()
}

/// `uses:` with the chain applied — `persona.uses_of`.
pub fn uses_of(root: &Path, name: &str) -> Vec<String> {
    resolve(root, name)
        .map(|r| csv_list(r.get("uses")))
        .unwrap_or_default()
}

/// Whose tools `name` borrows: `None` when it does not say (every `uses:` persona's), else the
/// named ones — `persona.borrows_of`.
pub fn borrows_of(root: &Path, name: &str) -> Option<Vec<String>> {
    let resolved = resolve(root, name)?;
    if borrows_unreadable(root, &resolved) {
        return Some(Vec::new());
    }
    let declared = resolved.get("borrows")?;
    Some(
        csv_list(Some(declared))
            .into_iter()
            .filter(|v| v != BORROWS_NONE)
            .collect(),
    )
}

/// Every tool `name` may run without a prompt: its own, and those of the personas it borrows
/// from — `persona.effective_tools`.
pub fn effective_tools(root: &Path, name: &str) -> BTreeSet<String> {
    let mut tools = tools_of(root, name);
    let lenders = borrows_of(root, name).unwrap_or_else(|| uses_of(root, name));
    for other in lenders {
        tools.extend(tools_of(root, &other));
    }
    tools
}

/// The executables a persona's chain ships, by file name, a child's shadowing its parent's —
/// `persona.bin_scripts`.
pub fn bin_scripts(root: &Path, name: &str) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    for ancestor in personas::lineage(root, name).iter().rev() {
        let dir = root.join("personas").join(ancestor).join(BIN_DIR);
        let Ok(reader) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = reader.flatten().map(|e| e.path()).collect();
        files.sort();
        for file in files {
            if is_executable_file(&file)
                && let Some(base) = file.file_name().and_then(|n| n.to_str())
            {
                out.insert(base.to_string(), file.clone());
            }
        }
    }
    out
}

/// `f.is_file() and os.access(f, os.X_OK)`.
pub fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_ok()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// The plane's personas — `persona.list_personas`: a flat `personas/<name>.md` other than a
/// README, and a directory not starting `_` that holds `persona.md`. Sorted.
pub fn list_personas(root: &Path) -> Vec<String> {
    crate::workspaces::Plane::open(root)
        .personas()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(personas: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        for (name, front) in personas {
            let d = dir.path().join("personas").join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("persona.md"),
                format!("---\n{front}\n---\n\n# {name}\n"),
            )
            .unwrap();
        }
        dir
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_child_accumulates_its_parents_tools_and_replaces_its_role() {
        let dir = plane(&[
            ("base", "role: Base\ntools: gh, git"),
            ("kid", "extends: base\nrole: Kid\ntools: kubectl, gh"),
        ]);
        let r = resolve(dir.path(), "kid").unwrap();
        assert_eq!(r.get("role"), Some("Kid"));
        assert_eq!(r.get("tools"), Some("gh, git, kubectl"));
        assert_eq!(r.lineage, vec!["kid".to_string(), "base".to_string()]);
        assert_eq!(tools_of(dir.path(), "kid"), set(&["gh", "git", "kubectl"]));
    }

    #[test]
    fn an_empty_value_does_not_override_the_parents() {
        let dir = plane(&[("base", "role: Base"), ("kid", "extends: base\nrole:")]);
        assert_eq!(
            resolve(dir.path(), "kid").unwrap().get("role"),
            Some("Base")
        );
    }

    #[test]
    fn uses_lends_every_tool_unless_borrows_narrows_it() {
        let dir = plane(&[
            ("a", "tools: gh"),
            ("b", "tools: glab"),
            ("wide", "uses: a, b"),
            ("narrow", "uses: a, b\nborrows: b"),
            ("nobody", "uses: a, b\nborrows: none"),
        ]);
        assert_eq!(effective_tools(dir.path(), "wide"), set(&["gh", "glab"]));
        assert_eq!(effective_tools(dir.path(), "narrow"), set(&["glab"]));
        assert!(effective_tools(dir.path(), "nobody").is_empty());
    }

    #[test]
    fn a_borrows_charter_cannot_read_lends_nothing() {
        // charter#575: a misspelled or doubled `borrows:` must not read as "not declared".
        let dir = plane(&[
            ("a", "tools: gh"),
            ("typo", "uses: a\nBorrows: a"),
            ("twice", "uses: a\nborrows: a\nborrows: none"),
        ]);
        assert!(effective_tools(dir.path(), "typo").is_empty());
        assert!(effective_tools(dir.path(), "twice").is_empty());
    }

    #[test]
    fn a_persona_does_not_use_itself() {
        let dir = plane(&[("me", "uses: me\ntools: gh")]);
        assert_eq!(uses_of(dir.path(), "me"), Vec::<String>::new());
    }

    #[test]
    fn a_persona_that_does_not_load_resolves_to_nothing() {
        let dir = plane(&[]);
        assert!(resolve(dir.path(), "ghost").is_none());
        assert!(effective_tools(dir.path(), "ghost").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn bin_scripts_are_the_executables_a_child_shadowing_its_parent() {
        use std::os::unix::fs::PermissionsExt;
        let dir = plane(&[("base", "role: b"), ("kid", "extends: base")]);
        for (who, file, mode) in [
            ("base", "deploy", 0o755),
            ("base", "notes", 0o644),
            ("kid", "deploy", 0o755),
        ] {
            let d = dir.path().join("personas").join(who).join(BIN_DIR);
            std::fs::create_dir_all(&d).unwrap();
            let p = d.join(file);
            std::fs::write(&p, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let scripts = bin_scripts(dir.path(), "kid");
        assert_eq!(scripts.keys().collect::<Vec<_>>(), vec!["deploy"]);
        assert!(scripts["deploy"].starts_with(dir.path().join("personas/kid")));
    }
}
