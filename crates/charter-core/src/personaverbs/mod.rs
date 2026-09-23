//! `charter persona list`, `use`, `sync-agents` and `stats` — the four persona verbs the
//! operator's planes call that the app's `charter` did not have (charter-app 0.1.0 audit,
//! 2026-09-24).
//!
//! A port of `commands_persona.cmd_persona_list`, `cmd_persona_use`,
//! `cmd_persona_sync_agents` and `cmd_persona_stats`, and of the parts of `persona.py`,
//! `mcpseen.py`, `dispatch.py` and `skilluse.py` beneath them that no earlier port needed.
//! Every sentence here is the Python charter's, byte for byte; the recorded scenarios named
//! `persona-list-…`, `persona-use-…`, `persona-sync-agents-…` and `persona-stats-…` hold them.
//!
//! # One divergence, declared: a registered vault's health
//!
//! `persona list`'s VAULT STATUS column asks a registered vault's PROVIDER how it is — a
//! plain-file vault counts its secrets, a 1Password one runs `op`. That is the secrets
//! registry, which is being ported on its own (`secret`, `persona secret`, `vault`), and this
//! module does not grow a second copy of it. So a vault the registry names is reported as
//! [`list::REGISTERED_UNCHECKED`] until that port lands and [`list::vault_status`] asks it.
//! "No vault" and "not set up (local)" are answered exactly as Python answers them, because
//! both are read off the registry's names alone.
//!
//! Every other answer — the roster, the selection, the generated sub-agents, the stats — is
//! Python's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub mod agents;
pub mod list;
pub mod mcp;
pub mod select;
pub mod stats;

/// `vault: none` — a persona that deliberately holds no credentials. `persona.NO_VAULT`.
pub const NO_VAULT: &str = "none";

/// A persona with its `extends:` chain applied: the merged frontmatter, the concatenated
/// charter and the chain — `persona.resolve`'s `{meta, charter, lineage}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub meta: BTreeMap<String, String>,
    pub charter: String,
    pub lineage: Vec<String>,
}

impl Resolved {
    /// One merged value, `None` when the chain does not set it. Empty values are never
    /// merged (`resolve` skips them), so a present value is a non-empty one.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.meta.get(key).map(String::as_str)
    }
}

/// `persona.resolve`: the frontmatter merge is [`crate::personagrant::resolve`]'s (one merge,
/// shared with the tool gate), and the charter is concatenated here, root first, each
/// descendant's own charter under a heading naming what it extends.
pub fn resolve(root: &Path, name: &str) -> Option<Resolved> {
    let merged = crate::personagrant::resolve(root, name)?;
    let mut charter = String::new();
    let mut prev: Option<&str> = None;
    for ancestor in merged.lineage.iter().rev() {
        let (_, body) = crate::personas::load_with_charter(root, ancestor)?;
        if !body.is_empty() {
            match prev {
                None => charter.push_str(&body),
                Some(parent) => charter.push_str(&format!(
                    "\n\n---\n\n### ⤷ `{ancestor}` extends `{parent}` — its own charter\n\n{body}"
                )),
            }
        }
        prev = Some(ancestor);
    }
    Some(Resolved {
        meta: merged.meta,
        charter,
        lineage: merged.lineage,
    })
}

/// A persona's OWN frontmatter as `dict(pairs)` would hold it: the last line for each key.
pub fn own_meta(root: &Path, name: &str) -> Option<BTreeMap<String, String>> {
    let pairs = crate::personas::load(root, name)?;
    Some(pairs.into_iter().collect())
}

/// Every persona this plane defines — `persona.list_personas`, by ITS rules: every
/// `personas/*.md` but a README, by stem, and every directory not starting `_` that holds a
/// `persona.md`, by its FULL name.
///
/// Not [`crate::personagrant::list_personas`], which takes a directory's `file_stem` and so
/// listed `personas/ops.v2/` as `ops`: `sync-agents` then generated nothing for it and pruned
/// its existing agent as stale. The names here decide what gets written and what gets
/// deleted, so they are Python's exactly.
pub fn names(root: &Path) -> Vec<String> {
    let Ok(reader) = std::fs::read_dir(root.join("personas")) else {
        return Vec::new();
    };
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in reader.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if stem.to_lowercase() != "readme" {
                out.insert(stem);
            }
        }
        if path.is_dir() && !name.starts_with('_') && path.join("persona.md").exists() {
            out.insert(name);
        }
    }
    out.into_iter().collect()
}

/// `persona.is_draft`: the RESOLVED `draft:` is one of `true`, `yes`, `1`, `on`, so a child
/// of a draft is a draft unless it says otherwise.
pub fn is_draft(root: &Path, name: &str) -> bool {
    resolve(root, name)
        .and_then(|r| {
            r.get("draft")
                .map(|d| crate::memstore::py_strip(d).to_lowercase())
        })
        .is_some_and(|d| matches!(d.as_str(), "true" | "yes" | "1" | "on"))
}

/// `persona.declared_skills`: the persona's OWN `skills:`, comma-separated, in order.
pub fn declared_skills(root: &Path, name: &str) -> Vec<String> {
    let Some(meta) = own_meta(root, name) else {
        return Vec::new();
    };
    let raw = meta
        .get("skills")
        .map(|s| crate::memstore::py_strip(s))
        .unwrap_or("");
    raw.split(',')
        .map(|t| crate::memstore::py_strip(t).to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// The vault registry's entries by name, the committed half (`<root>/vaults.json`) overlaid
/// by this machine's (`<state>/vaults.json`) — `registry.load_registry`'s merged view, read
/// for NAMES and the `persona` tag only.
///
/// This is the one piece of the secrets registry these commands need, and only its shape:
/// no provider is built and no vault file is opened. The secrets port owns the rest (see the
/// module header). A half that is missing, unreadable or not JSON contributes nothing, and an
/// entry that is not an object is dropped as `usable_vaults` drops it.
pub fn registered_vaults(
    root: &Path,
    state: &Path,
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    // A half that is absent is empty; one that is there and is not a JSON object is the
    // whole registry failing, as `registry.load_registry` raises for it.
    fn half(path: &Path) -> Result<serde_json::Map<String, serde_json::Value>, String> {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Ok(Default::default());
        };
        let corrupt = || format!("vault registry {} is corrupt", path.display());
        let doc: serde_json::Value = serde_json::from_str(&text).map_err(|_| corrupt())?;
        let doc = doc.as_object().ok_or_else(corrupt)?;
        Ok(doc
            .get("vaults")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default())
    }
    let shared = half(&root.join("vaults.json"))?;
    let local = half(&state.join("vaults.json"))?;
    let mut merged: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for (name, entry) in shared {
        if entry.is_object() {
            merged.insert(name, entry);
        }
    }
    for (name, entry) in local {
        let Some(local) = entry.as_object() else {
            continue;
        };
        match merged.get_mut(&name).and_then(|b| b.as_object_mut()) {
            None => {
                merged.insert(name, entry.clone());
            }
            Some(base) => {
                for (k, v) in local {
                    if k == "config" {
                        let cfg = base
                            .entry("config")
                            .or_insert_with(|| serde_json::Value::Object(Default::default()));
                        if let (Some(cfg), Some(add)) = (cfg.as_object_mut(), v.as_object()) {
                            for (ck, cv) in add {
                                cfg.insert(ck.clone(), cv.clone());
                            }
                        }
                    } else if !v.is_null() {
                        base.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    Ok(merged)
}

/// `persona.vault_of`: the resolved `vault:` (`None` for [`NO_VAULT`]), else the first vault
/// the registry tags with this persona, by name.
pub fn vault_of(root: &Path, state: &Path, name: &str) -> Option<String> {
    if let Some(r) = resolve(root, name)
        && let Some(v) = r.get("vault")
    {
        let v = crate::memstore::py_strip(v);
        return (v != NO_VAULT && !v.is_empty()).then(|| v.to_string());
    }
    // A registry that cannot be read names no vault: `vault_of` catches the error.
    registered_vaults(root, state)
        .ok()?
        .into_iter()
        .find(|(_, entry)| entry.get("persona").and_then(|p| p.as_str()) == Some(name))
        .map(|(vault, _)| vault)
}

/// Where a persona's definition sits, relative to the plane — `def_path(name).relative_to(ROOT)`.
pub fn def_rel(root: &Path, name: &str) -> String {
    rel(root, &crate::personas::def_path(root, name))
}

/// `path` relative to `root` when it is inside it, else as it is — `commands_persona._rel`.
pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Python's `str.title()`: the first cased character after an uncased one is upper-cased and
/// every other cased character lower-cased — `ops-lite` is `Ops-Lite`.
pub fn py_title(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_cased = false;
    for c in text.chars() {
        let cased = c.is_lowercase() || c.is_uppercase();
        if cased {
            if after_cased {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
        } else {
            out.push(c);
        }
        after_cased = cased;
    }
    out
}

/// The plane's state directory, where the MCP approvals and this machine's vault registry
/// live — `config.STATE_DIR`, `$CHARTER_HOME` included.
pub fn state_dir(plane: &Path) -> PathBuf {
    crate::plane::state_dir(plane)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(personas: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        for (name, text) in personas {
            let d = dir.path().join("personas").join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("persona.md"), text).unwrap();
        }
        dir
    }

    #[test]
    fn a_child_charter_follows_its_parents_under_a_heading_naming_both() {
        let dir = plane(&[
            ("base", "---\nrole: Base\n---\n\n# Base\n\nFirst.\n"),
            ("mid", "---\nextends: base\n---\n"),
            ("kid", "---\nextends: mid\nrole: Kid\n---\n\nKid's own.\n"),
        ]);
        let r = resolve(dir.path(), "kid").unwrap();
        assert_eq!(
            r.charter,
            "# Base\n\nFirst.\n\n---\n\n### ⤷ `kid` extends `mid` — its own charter\n\nKid's own."
        );
        assert_eq!(r.get("role"), Some("Kid"));
    }

    #[test]
    fn a_persona_directory_is_named_in_full_and_a_flat_file_by_its_stem() {
        let dir = plane(&[("ops.v2", "---\nrole: x\n---\n"), ("_shared", "")]);
        std::fs::write(dir.path().join("personas/legacy.md"), "---\n---\n").unwrap();
        std::fs::write(dir.path().join("personas/README.md"), "").unwrap();
        assert_eq!(names(dir.path()), vec!["legacy", "ops.v2"]);
    }

    #[test]
    fn a_registry_that_does_not_read_names_no_vault() {
        let dir = plane(&[("solo", "---\nname: solo\n---\n")]);
        let state = dir.path().join(".charter");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(dir.path().join("vaults.json"), "{bad").unwrap();
        std::fs::write(
            state.join("vaults.json"),
            r#"{"vaults": {"a": {"persona": "solo"}}}"#,
        )
        .unwrap();
        assert_eq!(vault_of(dir.path(), &state, "solo"), None);
        assert!(registered_vaults(dir.path(), &state).is_err());
    }

    #[test]
    fn title_case_is_pythons() {
        assert_eq!(py_title("ops-lite"), "Ops-Lite");
        assert_eq!(py_title("steward"), "Steward");
        assert_eq!(py_title("a1b c.d"), "A1B C.D");
    }

    #[test]
    fn a_draft_is_read_off_the_resolved_chain() {
        let dir = plane(&[
            ("base", "---\ndraft: Yes\n---\n"),
            ("kid", "---\nextends: base\n---\n"),
            ("done", "---\nextends: base\ndraft: false\n---\n"),
        ]);
        assert!(is_draft(dir.path(), "kid"));
        assert!(!is_draft(dir.path(), "done"));
    }

    #[test]
    fn a_persona_without_a_vault_key_uses_the_vault_the_registry_tags_it_with() {
        let dir = plane(&[
            ("solo", "---\nname: solo\n---\n"),
            ("none", "---\nvault: none\n---\n"),
        ]);
        let state = dir.path().join(".charter");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join("vaults.json"),
            r#"{"vaults": {"b": {"persona": "solo"}, "a": {"persona": "solo"}, "c": {"persona": "none"}}}"#,
        )
        .unwrap();
        assert_eq!(vault_of(dir.path(), &state, "solo").as_deref(), Some("a"));
        assert_eq!(vault_of(dir.path(), &state, "none"), None);
    }
}
