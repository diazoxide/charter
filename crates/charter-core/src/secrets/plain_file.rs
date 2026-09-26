//! The plain-file provider — `charter/secrets/plain_file.py`: a JSON object of key → secret,
//! at mode 0600, with a 0600 sidecar recording when each key was last set.
//!
//! **Plaintext on disk, and it says so.** What it protects is the conversation, not the disk:
//! values leave only through `secret exec`, `secret cp` and an interactive `--reveal`.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::registry::Vault;
use super::{Ctx, VaultError};

/// The vault's file, resolved — `VaultProvider.file_path`.
pub fn path(ctx: &Ctx, vault: &Vault) -> Result<PathBuf, VaultError> {
    file_path(ctx, vault)
}

/// `VaultProvider.file_path`: the configured `file`, relative to the plane root when relative.
/// Raises when the provider needs one and none is configured.
pub fn file_path(ctx: &Ctx, vault: &Vault) -> Result<PathBuf, VaultError> {
    match super::config_str(&vault.config, "file").filter(|f| !f.is_empty()) {
        Some(f) => Ok(ctx.vault_file_path(f)),
        None => Err(VaultError::new(format!(
            "vault '{}' has no 'file' configured",
            vault.name
        ))),
    }
}

/// `_load`: the vault's JSON object. **Never writes.** A missing file is an empty vault.
pub fn load(ctx: &Ctx, vault: &Vault, what: &str) -> Result<Map<String, Value>, VaultError> {
    let p = file_path(ctx, vault)?;
    if !p.exists() {
        return Ok(Map::new());
    }
    let text = std::fs::read_to_string(&p)
        .map_err(|e| VaultError::new(format!("vault file {} cannot be read: {e}", p.display())))?;
    let text = if text.is_empty() { "{}" } else { text.as_str() };
    let data: Value = serde_json::from_str(text).map_err(|e| {
        VaultError::new(format!(
            "vault file {} is not valid JSON: {}",
            p.display(),
            super::registry::py_json_error(&e)
        ))
    })?;
    match data {
        Value::Object(map) => Ok(map),
        _ => Err(VaultError::new(format!(
            "vault file {} must be a JSON object of key -> {what}",
            p.display()
        ))),
    }
}

/// `_tighten`: force the file to 0600 if any group/other bit is set. Called from the VALUE
/// paths only, never from a health check (#331). Best-effort; never fails.
pub fn tighten(p: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(p)
            && meta.permissions().mode() & super::OTHERS != 0
        {
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
        }
    }
    #[cfg(not(unix))]
    let _ = p;
}

/// `get`: tighten the file BEFORE reading it — the plaintext must not sit in a group-readable
/// file while charter hands it out — then the value, as Python's `str()` of it.
pub fn get(ctx: &Ctx, vault: &Vault, key: &str) -> Result<String, VaultError> {
    if let Ok(p) = file_path(ctx, vault) {
        tighten(&p);
    }
    let data = load(ctx, vault, "secret")?;
    match data.get(key) {
        Some(v) => Ok(super::py_str(v)),
        None => Err(VaultError::not_found(format!(
            "secret '{key}' not found in vault '{}'",
            vault.name
        ))),
    }
}

/// `keys`: the secret names, sorted.
pub fn keys(ctx: &Ctx, vault: &Vault) -> Result<Vec<String>, VaultError> {
    let mut keys: Vec<String> = load(ctx, vault, "secret")?.keys().cloned().collect();
    keys.sort();
    Ok(keys)
}

/// `_write_private`: `payload` into `p` with `p` provably 0600 before a byte lands — or
/// nothing written at all (#437). Opened without `O_TRUNC`, the mode settled on the
/// DESCRIPTOR and read back, and only then truncated and written.
pub fn write_private(p: &Path, payload: &Value) -> Result<(), VaultError> {
    write_private_text(p, &crate::pyjson::dumps_indent2_unicode(payload))
}

/// [`write_private`] for text the caller has already encoded — the reference provider's,
/// which sorts its keys and escapes to ASCII where this provider does neither (#356). Same
/// guarantee: `p` is `0600` before a byte of `text` reaches it, or nothing is written.
pub fn write_private_text(p: &Path, text: &str) -> Result<(), VaultError> {
    if let Some(parent) = p.parent() {
        super::make_private_dir(parent)
            .map_err(|e| VaultError::new(format!("cannot create {}: {e}", parent.display())))?;
    }
    let fail = |e: std::io::Error| VaultError::new(format!("cannot write {}: {e}", p.display()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(p).map_err(fail)?;
    #[cfg(test)]
    watch::opened(&file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
        let mode = file.metadata().map_err(fail)?.permissions().mode() & 0o7777;
        if mode & super::OTHERS != 0 {
            return Err(VaultError::new(format!(
                "refusing to write {}: it is mode {:03o} and charter could not make it 0600, so \
                 the plaintext would be readable by other accounts on this machine. Nothing was \
                 written.\n  Filesystems with fixed permissions (exFAT, many network mounts) \
                 cannot hold a plain-file vault — point the vault at a path on a filesystem that \
                 keeps modes, or use a provider that does not store plaintext.",
                p.display(),
                mode & 0o777
            )));
        }
    }
    file.set_len(0).map_err(fail)?;
    std::io::Write::write_all(&mut file, text.as_bytes()).map_err(fail)
}

fn meta_path(p: &Path) -> PathBuf {
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    p.with_file_name(format!("{stem}.meta.json"))
}

fn load_meta(p: &Path) -> Map<String, Value> {
    let mp = meta_path(p);
    std::fs::read_to_string(&mp)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(if t.is_empty() { "{}" } else { &t }).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// `set`: the value, then the date it was set, for the rotation audit.
pub fn set(
    ctx: &Ctx,
    vault: &Vault,
    key: &str,
    value: &str,
    today: chrono::NaiveDate,
) -> Result<(), VaultError> {
    let p = file_path(ctx, vault)?;
    let mut data = load(ctx, vault, "secret")?;
    data.insert(key.to_string(), Value::String(value.to_string()));
    write_private(&p, &Value::Object(data))?;
    let mut meta = load_meta(&p);
    let mut stamp = Map::new();
    stamp.insert(
        "set_at".into(),
        Value::String(today.format("%Y-%m-%d").to_string()),
    );
    meta.insert(key.to_string(), Value::Object(stamp));
    write_private(&meta_path(&p), &Value::Object(meta))
}

/// `delete`: the key, and its date if it had one.
pub fn delete(ctx: &Ctx, vault: &Vault, key: &str) -> Result<(), VaultError> {
    let p = file_path(ctx, vault)?;
    let mut data = load(ctx, vault, "secret")?;
    if data.shift_remove(key).is_none() {
        return Err(VaultError::not_found(format!(
            "secret '{key}' not found in vault '{}'",
            vault.name
        )));
    }
    write_private(&p, &Value::Object(data))?;
    let mut meta = load_meta(&p);
    if meta.shift_remove(key).is_some_and(|v| !v.is_null()) {
        write_private(&meta_path(&p), &Value::Object(meta))?;
    }
    Ok(())
}

/// `ages`: key → days since it was last set, `None` when it predates tracking.
pub fn ages(
    ctx: &Ctx,
    vault: &Vault,
    today: chrono::NaiveDate,
) -> Result<Vec<(String, Option<i64>)>, VaultError> {
    let p = file_path(ctx, vault)?;
    let meta = load_meta(&p);
    let mut out = Vec::new();
    for k in keys(ctx, vault)? {
        let set_at = meta
            .get(&k)
            .and_then(|m| m.get("set_at"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let age = set_at
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| (today - d).num_days());
        out.push((k, age));
    }
    Ok(out)
}

/// `health`: `(ok, detail)` — the count, the file's mode and the loose directories. Never a
/// value, and never a write.
pub fn health(ctx: &Ctx, vault: &Vault) -> (bool, String) {
    if super::config_str(&vault.config, "file").is_none_or(str::is_empty) {
        return (false, "no 'file' configured".into());
    }
    let Ok(pp) = file_path(ctx, vault) else {
        return (false, "no 'file' configured".into());
    };
    let note = super::loose_dir_note(&ctx.root, &loose_dirs(ctx, vault));
    if !pp.exists() {
        let mut line = format!("not created yet ({})", super::short_path(&ctx.root, &pp));
        if !note.is_empty() {
            line.push_str(&format!(", {note}"));
        }
        return (true, line);
    }
    let count = match load(ctx, vault, "secret") {
        Ok(d) => d.len(),
        Err(e) => return (false, e.message),
    };
    let mut parts = vec![format!("{count} secret(s)")];
    for s in [super::mode_note(&pp), note] {
        if !s.is_empty() {
            parts.push(s);
        }
    }
    (true, parts.join(", "))
}

/// `VaultProvider.loose_dirs`, keyed on the vault's file: its directory's chain up to the
/// state directory. Empty for a vault with no file.
pub fn loose_dirs(ctx: &Ctx, vault: &Vault) -> Vec<(PathBuf, u32)> {
    match file_path(ctx, vault) {
        Ok(p) => match p.parent() {
            Some(parent) => super::loose_dirs(parent, &ctx.state),
            None => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

/// A test's view of the descriptor [`write_private_text`] holds, the instant it is opened:
/// before its mode is touched and before any content is written, so a new file's mode here is
/// the one it was created with. Thread-local, so it sees only its own test.
#[cfg(test)]
pub(crate) mod watch {
    use std::cell::RefCell;

    type Watch = Box<dyn FnMut(&std::fs::File)>;

    thread_local! {
        static WATCH: RefCell<Option<Watch>> = const { RefCell::new(None) };
    }

    pub(crate) fn set(f: impl FnMut(&std::fs::File) + 'static) -> Unset {
        WATCH.with(|w| *w.borrow_mut() = Some(Box::new(f)));
        Unset
    }

    pub(crate) struct Unset;

    impl Drop for Unset {
        fn drop(&mut self) {
            WATCH.with(|w| *w.borrow_mut() = None);
        }
    }

    pub(super) fn opened(file: &std::fs::File) {
        WATCH.with(|w| {
            if let Some(f) = w.borrow_mut().as_mut() {
                f(file);
            }
        });
    }
}
