//! The vault registry — `charter/secrets/registry.py`.
//!
//! **Two files, one view.** `vaults.json` at the plane root is the SHARED half: committed,
//! carrying what is identical on every machine (provider, persona, op-vault, a `file` relative
//! to the plane). `.charter/vaults.json` (0600, gitignored) is the LOCAL half: this developer's
//! own vaults and per-machine overrides of shared ones. The merge is per FIELD, local winning,
//! so pinning `account` locally does not restate the provider, the file and the persona.
//!
//! Registering is local by default; `--share` publishes. A registry names which personas hold
//! credentials and where their files are, which is a map worth not publishing by accident.

use std::path::Path;

use serde_json::{Map, Value};

use super::{Ctx, VaultError};
use crate::rewrite::Mode;

/// Provider ids this charter implements — `registry.PROVIDERS`.
pub const PROVIDERS: [&str; 4] = ["1password", "keyring", "plain-file", "reference"];

/// Config keys that never travel — `registry.LOCAL_ONLY_KEYS`. `identity` is the moved-token
/// record: it names this machine's keyring item and the `op` it pins, and honouring a committed
/// one would let a commit steer a token (#271 review, U5), so it stays local like `account`.
pub const LOCAL_ONLY_KEYS: [&str; 2] = ["account", "identity"];

/// Whether `name` may name a vault: `[A-Za-z0-9][A-Za-z0-9._-]*`, and never `..`.
///
/// A vault name reaches a path (`.charter/vaults/<name>.json` by default), an `op` item title
/// and a tag, so one holding `/` or `..` would lead out of the vault directory and one opening
/// with `-` would be read as an option. Checked when a vault is registered AND when the
/// registry is read, because the committed half arrives by `git pull`.
pub fn name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !name.contains("..")
}

/// The refusal for a vault name [`name_ok`] rejects.
pub fn name_refusal(name: &str) -> String {
    format!(
        "'{}' is not a vault name charter accepts: letters, digits, '.', '_' and '-', starting \
         with a letter or digit, and never '..'.",
        crate::personas::one_line(name)
    )
}

/// One registered vault: its name, provider id, persona tag and provider config.
#[derive(Debug, Clone, PartialEq)]
pub struct Vault {
    pub name: String,
    pub provider: String,
    pub persona: Option<String>,
    pub config: Map<String, Value>,
}

/// `_read`: a missing file is an empty registry; one that is not JSON is refused by name.
fn read(path: &Path) -> Result<Map<String, Value>, VaultError> {
    let mut doc = Map::new();
    if !path.exists() {
        doc.insert("vaults".into(), Value::Object(Map::new()));
        return Ok(doc);
    }
    let text = std::fs::read_to_string(path).map_err(|e| {
        VaultError::new(format!("vault registry {} is corrupt: {e}", path.display()))
    })?;
    let parsed: Value = serde_json::from_str(&text).map_err(|e| {
        VaultError::new(format!(
            "vault registry {} is corrupt: {}",
            path.display(),
            py_json_error(&e)
        ))
    })?;
    match parsed {
        Value::Object(map) => doc = map,
        _ => {
            return Err(VaultError::new(format!(
                "vault registry {} is corrupt: not a JSON object",
                path.display()
            )));
        }
    }
    if !doc.contains_key("vaults") {
        doc.insert("vaults".into(), Value::Object(Map::new()));
    }
    Ok(doc)
}

/// A decode failure worded the way Python's `json.JSONDecodeError` is, as closely as a
/// different parser can say it: the reason, then where.
pub fn py_json_error(e: &serde_json::Error) -> String {
    let what = e.to_string();
    // serde's text ends " at line L column C"; Python's is "<reason>: line L column C (char N)".
    let reason = what.split(" at line ").next().unwrap_or(&what);
    format!("{reason}: line {} column {}", e.line(), e.column())
}

/// The shared (committed) half.
pub fn load_shared(ctx: &Ctx) -> Result<Map<String, Value>, VaultError> {
    read(&ctx.shared_registry())
}

/// The local (this machine's) half.
pub fn load_local(ctx: &Ctx) -> Result<Map<String, Value>, VaultError> {
    read(&ctx.local_registry())
}

/// `usable_vaults`: the entries of one half that are objects at all. A committed string where
/// a vault belongs is not a vault, and is dropped rather than crashing every reader.
pub fn usable_vaults(half: &Map<String, Value>) -> Map<String, Value> {
    half.get("vaults")
        .and_then(Value::as_object)
        .map(|v| {
            v.iter()
                .filter(|(n, e)| e.is_object() && name_ok(n))
                .map(|(n, e)| (n.clone(), e.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// `load_registry`: shared as the base, local layered over it per FIELD.
pub fn load_registry(ctx: &Ctx) -> Result<Map<String, Value>, VaultError> {
    let shared = load_shared(ctx)?;
    let local = load_local(ctx)?;
    let mut merged: Map<String, Value> = usable_vaults(&shared);
    for (name, entry) in usable_vaults(&local) {
        let Some(base) = merged.get_mut(&name).and_then(Value::as_object_mut) else {
            merged.insert(name, entry);
            continue;
        };
        for (k, v) in entry.as_object().into_iter().flatten() {
            if k == "config" {
                let cfg = base
                    .entry("config")
                    .or_insert_with(|| Value::Object(Map::new()));
                if let (Some(cfg), Some(over)) = (cfg.as_object_mut(), v.as_object()) {
                    for (ck, cv) in over {
                        cfg.insert(ck.clone(), cv.clone());
                    }
                }
            } else if !v.is_null() {
                base.insert(k.clone(), v.clone());
            }
        }
    }
    let mut doc = Map::new();
    doc.insert("vaults".into(), Value::Object(merged));
    Ok(doc)
}

/// `vaults(doc)`: the merged registry's vault entries.
pub fn vaults(doc: &Map<String, Value>) -> Map<String, Value> {
    doc.get("vaults")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// `get_vault_config` + `provider_for`: the vault registered as `name`, or why not.
pub fn vault(ctx: &Ctx, name: &str) -> Result<Vault, VaultError> {
    let doc = load_registry(ctx)?;
    vault_in(&doc, name)
}

/// [`vault`] against a registry already loaded.
pub fn vault_in(doc: &Map<String, Value>, name: &str) -> Result<Vault, VaultError> {
    if !name_ok(name) {
        return Err(VaultError::not_configured(name_refusal(name)));
    }
    let all = vaults(doc);
    // `if not vc`: an empty object is as unregistered as a missing one.
    let entry = match all.get(name).and_then(Value::as_object) {
        Some(e) if !e.is_empty() => e.clone(),
        _ => {
            return Err(VaultError::not_configured(format!(
                "no vault named '{name}'. Register one with `charter vault add {name}`."
            )));
        }
    };
    let provider = entry.get("provider");
    let pid = provider.and_then(Value::as_str).unwrap_or_default();
    if !PROVIDERS.contains(&pid) {
        let shown = match provider {
            None | Some(Value::Null) => "None".to_string(),
            Some(v) => super::py_str(v),
        };
        return Err(VaultError::new(format!(
            "vault '{name}' uses unknown provider '{shown}'"
        )));
    }
    Ok(Vault {
        name: name.to_string(),
        provider: pid.to_string(),
        persona: entry
            .get("persona")
            .and_then(Value::as_str)
            .map(str::to_owned),
        config: entry
            .get("config")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
    })
}

/// `vaults_for_persona`: the vaults tagged with `persona`, by name.
pub fn vaults_for_persona(doc: &Map<String, Value>, persona: &str) -> Vec<String> {
    let mut out: Vec<String> = vaults(doc)
        .iter()
        .filter(|(_, v)| v.get("persona").and_then(Value::as_str) == Some(persona))
        .map(|(n, _)| n.clone())
        .collect();
    out.sort();
    out
}

/// `scope_of`: `shared`, `local` or `both` — where `name` is registered.
pub fn scope_of(ctx: &Ctx, name: &str) -> String {
    let has = |half: Result<Map<String, Value>, VaultError>| {
        half.ok()
            .and_then(|h| {
                h.get("vaults")
                    .and_then(Value::as_object)
                    .map(|v| v.contains_key(name))
            })
            .unwrap_or(false)
    };
    let (s, l) = (has(load_shared(ctx)), has(load_local(ctx)));
    match (s, l) {
        (true, true) => "both",
        (true, false) => "shared",
        _ => "local",
    }
    .to_string()
}

/// `_write`: the document, replaced whole and never written through a link (#434).
///
/// [`crate::rewrite::replace`], gated from the half's own directory — the file alone, as a
/// vault is gated — so a link at the registry is refused and a crash leaves the previous
/// registry whole rather than truncated.
fn write(path: &Path, doc: &Map<String, Value>, mode: Mode) -> Result<(), VaultError> {
    let fail = |e: std::io::Error| VaultError::new(format!("cannot write {}: {e}", path.display()));
    let parent = path.parent().unwrap_or(Path::new("."));
    super::make_private_dir(parent).map_err(fail)?;
    let text = crate::pyjson::dumps_indent2_unicode(&Value::Object(doc.clone()));
    crate::rewrite::replace(parent, path, text.as_bytes(), mode).map_err(fail)
}

/// `save_registry`: the LOCAL half — which names every vault's file and account on this
/// machine — 0600 before a byte of it lands, or nothing written (`Mode::Secret`).
pub fn save_local(ctx: &Ctx, doc: &Map<String, Value>) -> Result<(), VaultError> {
    write(&ctx.local_registry(), doc, Mode::Secret)
}

/// `save_shared`: the SHARED half — committed, and carrying no value by construction, so a
/// committed file's mode: the one it has, or the umask's for a new one (`Mode::Kept`).
pub fn save_shared(ctx: &Ctx, doc: &Map<String, Value>) -> Result<(), VaultError> {
    write(&ctx.shared_registry(), doc, Mode::Kept)
}

fn half_vaults(half: &mut Map<String, Value>) -> &mut Map<String, Value> {
    let entry = half
        .entry("vaults")
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("just made an object")
}

/// `add_vault`: register `name`, refusing to replace an existing registration without
/// `force`. With `share`, the entry goes to the committed half and local-only keys stay local.
pub fn add_vault(
    ctx: &Ctx,
    name: &str,
    provider: &str,
    cfg: Map<String, Value>,
    persona: Option<&str>,
    force: bool,
    share: bool,
) -> Result<(), VaultError> {
    if !PROVIDERS.contains(&provider) {
        let mut all = PROVIDERS.to_vec();
        all.sort();
        return Err(VaultError::new(format!(
            "unknown provider '{provider}'. Available: {}",
            all.join(", ")
        )));
    }
    let doc = load_registry(ctx)?;
    if let Some(existing) = vaults(&doc).get(name)
        && !force
    {
        let old = existing
            .get("provider")
            .map(super::py_str)
            .unwrap_or_else(|| "?".into());
        let where_ = existing
            .get("config")
            .and_then(|c| c.get("file"))
            .filter(|f| !f.is_null() && f.as_str() != Some(""))
            .map(super::py_str);
        let detail = where_.map(|w| format!(" ({w})")).unwrap_or_default();
        return Err(VaultError::new(format!(
            "vault '{name}' is already registered with provider '{old}'{detail}. charter will \
             not replace it: the registration is the only pointer to that vault's secrets, so \
             replacing it strands them with nothing referring to them.\n  keep both:     \
             charter vault add <other-name> --provider {provider} …\n  inspect it:    charter \
             vault list\n  replace anyway: re-run with --force (this does NOT migrate secrets)"
        )));
    }
    let mut cfg = cfg;
    let mut local_cfg = Map::new();
    for k in LOCAL_ONLY_KEYS {
        if let Some(v) = cfg.remove(k) {
            local_cfg.insert(k.to_string(), v);
        }
    }
    let entry = |config: Map<String, Value>| {
        let mut e = Map::new();
        e.insert("provider".into(), Value::String(provider.to_string()));
        e.insert(
            "persona".into(),
            persona.map_or(Value::Null, |p| Value::String(p.to_string())),
        );
        e.insert("config".into(), Value::Object(config));
        Value::Object(e)
    };
    if share {
        let mut shared = load_shared(ctx)?;
        half_vaults(&mut shared).insert(name.to_string(), entry(cfg));
        save_shared(ctx, &shared)?;
        let mut local = load_local(ctx)?;
        let previous = half_vaults(&mut local).get(name).cloned();
        let mut keep = Map::new();
        if let Some(prev_cfg) = previous
            .as_ref()
            .and_then(|p| p.get("config"))
            .and_then(Value::as_object)
        {
            for (k, v) in prev_cfg {
                if LOCAL_ONLY_KEYS.contains(&k.as_str()) {
                    keep.insert(k.clone(), v.clone());
                }
            }
        }
        for (k, v) in local_cfg {
            keep.insert(k, v);
        }
        if !keep.is_empty() {
            let mut e = Map::new();
            e.insert("config".into(), Value::Object(keep));
            half_vaults(&mut local).insert(name.to_string(), Value::Object(e));
            save_local(ctx, &local)?;
        } else if previous.is_some() {
            half_vaults(&mut local).remove(name);
            save_local(ctx, &local)?;
        }
    } else {
        for (k, v) in local_cfg {
            cfg.insert(k, v);
        }
        let mut local = load_local(ctx)?;
        half_vaults(&mut local).insert(name.to_string(), entry(cfg));
        save_local(ctx, &local)?;
    }
    Ok(())
}

/// `remove_vault`: from wherever it is registered — both halves if both carry it.
pub fn remove_vault(ctx: &Ctx, name: &str) -> Result<(), VaultError> {
    let mut shared = load_shared(ctx)?;
    let mut local = load_local(ctx)?;
    let in_shared = half_vaults(&mut shared).contains_key(name);
    let in_local = half_vaults(&mut local).contains_key(name);
    if !(in_shared || in_local) {
        return Err(VaultError::not_configured(format!(
            "no vault named '{name}'"
        )));
    }
    if in_shared {
        half_vaults(&mut shared).shift_remove(name);
        save_shared(ctx, &shared)?;
    }
    if in_local {
        half_vaults(&mut local).shift_remove(name);
        save_local(ctx, &local)?;
    }
    Ok(())
}

/// `_identity_vars`: every environment-variable name a registered vault uses as an identity,
/// by vault — both halves of each `env` binding.
pub fn identity_vars(doc: &Map<String, Value>) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for (name, vc) in vaults(doc) {
        let Some(mapping) = vc
            .get("config")
            .and_then(|c| c.get("env"))
            .and_then(Value::as_object)
        else {
            out.push((name, Vec::new()));
            continue;
        };
        let mut names: Vec<String> = Vec::new();
        for (k, v) in mapping {
            names.push(k.clone());
            names.push(super::py_str(v));
        }
        names.retain(|n| !n.is_empty());
        out.push((name, names));
    }
    out
}
