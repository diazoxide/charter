//! The reference provider — `charter/secrets/reference.py`: entries are URIs into someone
//! else's secret store, resolved through that store's CLI at read time.
//!
//! ```text
//! op://<vault>/<item>/<field>   → op read --no-newline <uri>
//! vault://<path>#<FIELD>        → vault kv get -field=<FIELD> <path>
//! ```
//!
//! Each resolver maps a URI to an ARGV, never a shell string, and every value that enters it
//! is validated. What the file stores is not a secret, and it is still written 0600: it names
//! every item and field this plane reaches.
//!
//! `browser://` is recognised as a scheme and refused at read time: the browser lane is not in
//! this charter.

use std::time::Duration;

use serde_json::{Map, Value};

use super::plain_file;
use super::registry::Vault;
use super::run::{self, RunError};
use super::{Ctx, VaultError};

/// How long one reference may take to resolve — `reference.RESOLVE_TIMEOUT`.
pub const RESOLVE_TIMEOUT: Duration = Duration::from_secs(60);

/// The schemes a reference may use, in `_RESOLVERS`' order.
pub const SCHEMES: [&str; 3] = ["op", "vault", "browser"];

/// `urllib.parse.urlsplit`, the parts a resolver reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Split {
    pub scheme: String,
    pub netloc: String,
    pub path: String,
    pub query: String,
    pub fragment: String,
}

/// `urlsplit(url)` with `allow_fragments=True`: CPython's algorithm, for the shapes a vault
/// reference takes.
pub fn urlsplit(url: &str) -> Split {
    // CPython strips leading C0 controls and spaces, and removes tab/CR/LF anywhere.
    let url: String = url
        .trim_start_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    let mut rest = url.as_str();
    let mut scheme = String::new();
    // `i > 0` is CPython's; `i >= 0` would answer the same, since an empty candidate fails the
    // first-character check below (`.cargo/mutants.toml` excludes that mutant).
    if let Some(i) = rest.find(':')
        && i > 0
    {
        let candidate = &rest[..i];
        let mut chars = candidate.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
        if first_ok
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            scheme = candidate.to_ascii_lowercase();
            rest = &rest[i + 1..];
        }
    }
    let mut netloc = String::new();
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        netloc = after[..end].to_string();
        rest = &after[end..];
    }
    let mut fragment = String::new();
    let mut body = rest.to_string();
    if let Some((b, f)) = rest.split_once('#') {
        body = b.to_string();
        fragment = f.to_string();
    }
    let mut query = String::new();
    if let Some((b, q)) = body.clone().split_once('?') {
        body = b.to_string();
        query = q.to_string();
    }
    Split {
        scheme,
        netloc,
        path: body,
        query,
        fragment,
    }
}

/// `scheme_of`: the reference scheme of `value`, or `None` when it is not a supported one.
/// TOTAL over anything JSON can hold — a committed `{"K": 123}` is not a reference, and must
/// not crash the health check that reports on it.
pub fn scheme_of(value: &Value) -> Option<&'static str> {
    let text = value.as_str()?;
    let scheme = urlsplit(text).scheme;
    SCHEMES.iter().copied().find(|s| *s == scheme)
}

/// A resolver's argv and the CLI it runs, or why the URI is malformed.
pub fn argv(uri: &str, scheme: &str) -> Result<(Vec<String>, &'static str), VaultError> {
    let parts = urlsplit(uri);
    match scheme {
        "op" => {
            let segments = parts.path.trim_matches('/').split('/').count();
            if parts.netloc.is_empty() || segments < 2 {
                return Err(VaultError::new(
                    "malformed 1Password reference — expected op://<vault>/<item>/<field>",
                ));
            }
            Ok((
                vec![
                    "op".into(),
                    "read".into(),
                    "--no-newline".into(),
                    uri.into(),
                ],
                "op",
            ))
        }
        "vault" => {
            let path = format!("{}{}", parts.netloc, parts.path);
            let path = path.trim_matches('/').to_string();
            // A path or field that reads as a flag would be one to `vault`: refused, never passed.
            if path.is_empty()
                || parts.fragment.is_empty()
                || path.starts_with('-')
                || parts.fragment.starts_with('-')
            {
                return Err(VaultError::new(
                    "malformed Vault reference — expected vault://<path>#<FIELD>",
                ));
            }
            Ok((
                vec![
                    "vault".into(),
                    "kv".into(),
                    "get".into(),
                    format!("-field={}", parts.fragment),
                    path,
                ],
                "vault",
            ))
        }
        _ => Err(VaultError::unavailable(
            "a browser:// reference cannot be read: reading a value out of a browser session is \
             not in this version of charter.",
        )),
    }
}

/// The CLI a scheme resolves through, for `health`.
fn cli_of(scheme: &str) -> &'static str {
    match scheme {
        "op" => "op",
        "vault" => "vault",
        _ => "npx",
    }
}

/// `keys`: the reference names, sorted.
pub fn keys(ctx: &Ctx, vault: &Vault) -> Result<Vec<String>, VaultError> {
    let mut keys: Vec<String> = plain_file::load(ctx, vault, "reference")?
        .keys()
        .cloned()
        .collect();
    keys.sort();
    Ok(keys)
}

/// `reference_for`: the stored URI — safe to print; it is not the secret.
pub fn reference_for(ctx: &Ctx, vault: &Vault, key: &str) -> Result<Value, VaultError> {
    let data = plain_file::load(ctx, vault, "reference")?;
    data.get(key).cloned().ok_or_else(|| {
        VaultError::not_found(format!("no secret '{key}' in vault '{}'", vault.name))
    })
}

/// `get`: resolve the reference through its CLI. **Never** a message holding what the
/// resolver printed: its stderr can echo what it fetched.
pub fn get(ctx: &Ctx, vault: &Vault, key: &str) -> Result<String, VaultError> {
    let uri_value = reference_for(ctx, vault, key)?;
    // **The stored entry is never put into a message**, on any path below. It is a URI only
    // when the file is what it claims to be; a raw value hand-put here, or a reference vault
    // pointed at another vault's file, would otherwise be printed by the error meant to
    // report it.
    let Some(scheme) = scheme_of(&uri_value) else {
        return Err(VaultError::new(format!(
            "'{key}' in vault '{}' is not a supported reference (an entry must start with op:// \
             or vault://; the entry itself is withheld, since it may be a value)",
            vault.name
        )));
    };
    let uri = super::py_str(&uri_value);
    let (argv, cli) = argv(&uri, scheme)?;
    if ctx.which(cli).is_none() {
        return Err(VaultError::new(format!(
            "'{key}' needs the '{cli}' CLI to resolve it — it is not on PATH. Install it and \
             authenticate, then retry."
        )));
    }
    let overlay = super::env_overlay(ctx, vault)?;
    let identity = super::identity_note(vault);
    let ran = match run::run(&ctx.env, &argv, None, &overlay, Some(RESOLVE_TIMEOUT)) {
        Ok(ran) => ran,
        Err(RunError::Timeout) => {
            return Err(VaultError::new(format!(
                "resolving '{key}' via {cli} did not finish within {}s and was stopped{identity}.\n  \
                 Almost always this is {cli} waiting on an authentication prompt that an \
                 unattended run has nowhere to display — re-authenticate in a terminal ({cli} \
                 sign-in), then retry.\n  (Resolver output withheld — it can contain the secret.)",
                RESOLVE_TIMEOUT.as_secs()
            )));
        }
        Err(RunError::Interrupted(_)) => return Err(VaultError::interrupted()),
        Err(RunError::Spawn(e)) => {
            return Err(VaultError::new(format!(
                "resolving '{key}' via {cli} could not start it: {e}"
            )));
        }
    };
    if ran.code != 0 {
        return Err(VaultError::new(format!(
            "resolving '{key}' via {cli} failed (exit {}){identity}.\n  Causes, roughly \
             in order of how often they are the real one:\n    - the item or field behind the \
             reference was renamed, moved or deleted (the vault stays healthy — `charter vault \
             verify` tests this)\n    - you are not authenticated to {cli}\n    - the identity \
             variable for this vault is unset, so {cli} read the vault as somebody else\n    - \
             charter version drift: an older build may not map this vault's identity into {cli}'s \
             environment (`charter version` shows the pin; `charter version sync` conforms this \
             machine)\n  (Resolver output withheld — it can contain the secret.)",
            ran.code
        )));
    }
    let value = ran.stdout;
    Ok(value.strip_suffix('\n').map(str::to_owned).unwrap_or(value))
}

/// `set`: store a URI, refusing a bare value — a reference vault that silently accepted one
/// would become a plaintext vault without saying so.
pub fn set(ctx: &Ctx, vault: &Vault, key: &str, value: &str) -> Result<(), VaultError> {
    let value = crate::memstore::py_strip(value).to_string();
    let as_json = Value::String(value.clone());
    let Some(scheme) = scheme_of(&as_json) else {
        let mut schemes: Vec<String> = SCHEMES.iter().map(|s| format!("{s}://")).collect();
        schemes.sort();
        return Err(VaultError::new(format!(
            "'{key}': a reference vault stores a URI, not a value — expected one of {}.\n  A \
             reference vault POINTS AT items somebody else owns, which is why it declines to \
             create one. To have charter own the item — creating it and storing the value, with \
             the value on stdin and never in argv:\n      charter vault add <name> --provider \
             1password --op-vault <VAULT>\n      charter secret set <name> {key} --from-file \
             <path>\n  To keep the value on this machine instead: --provider plain-file.\n  To \
             register an item you created elsewhere: pass its URI here.",
            schemes.join(", ")
        )));
    };
    if scheme != "browser" {
        argv(&value, scheme)?;
    }
    let mut data = plain_file::load(ctx, vault, "reference")?;
    data.insert(key.to_string(), as_json);
    save(ctx, vault, data)
}

/// `delete`: drop the reference.
pub fn delete(ctx: &Ctx, vault: &Vault, key: &str) -> Result<(), VaultError> {
    let mut data = plain_file::load(ctx, vault, "reference")?;
    if data.shift_remove(key).is_none() {
        return Err(VaultError::not_found(format!(
            "no secret '{key}' in vault '{}'",
            vault.name
        )));
    }
    save(ctx, vault, data)
}

/// `_save`: sorted keys, indented, ASCII-escaped, 0600.
fn save(ctx: &Ctx, vault: &Vault, data: Map<String, Value>) -> Result<(), VaultError> {
    let p = plain_file::file_path(ctx, vault)?;
    let mut keys: Vec<&String> = data.keys().collect();
    keys.sort();
    let sorted: Map<String, Value> = keys
        .into_iter()
        .map(|k| (k.clone(), data[k].clone()))
        .collect();
    if let Some(parent) = p.parent() {
        super::make_private_dir(parent)
            .map_err(|e| VaultError::new(format!("cannot create {}: {e}", parent.display())))?;
    }
    let text = crate::pyjson::dumps_indent2(&Value::Object(sorted));
    std::fs::write(&p, text)
        .map_err(|e| VaultError::new(format!("cannot write {}: {e}", p.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// `health`: reference count and resolver availability — never resolving anything.
pub fn health(ctx: &Ctx, vault: &Vault) -> (bool, String) {
    if super::config_str(&vault.config, "file").is_none_or(str::is_empty) {
        return (false, "no 'file' configured".into());
    }
    let Ok(p) = plain_file::file_path(ctx, vault) else {
        return (false, "no 'file' configured".into());
    };
    let note = super::loose_dir_note(&ctx.root, &plain_file::loose_dirs(ctx, vault));
    let line = |text: String| {
        let mut parts = vec![text];
        for s in [super::mode_note(&p), note.clone()] {
            if !s.is_empty() {
                parts.push(s);
            }
        }
        parts.join(", ")
    };
    if !p.exists() {
        return (
            true,
            line(format!(
                "not created yet ({})",
                super::short_path(&ctx.root, &p)
            )),
        );
    }
    let data = match plain_file::load(ctx, vault, "reference") {
        Ok(d) => d,
        Err(e) => return (false, line(e.message)),
    };
    if data.is_empty() {
        return (true, line("no references yet".into()));
    }
    let mut needed: Vec<&'static str> = data.values().filter_map(scheme_of).collect();
    needed.sort();
    needed.dedup();
    let missing: Vec<&'static str> = needed
        .iter()
        .copied()
        .filter(|s| ctx.which(cli_of(s)).is_none())
        .collect();
    let n = data.len();
    let unsupported = data.values().filter(|v| scheme_of(v).is_none()).count();
    if unsupported > 0 || !missing.is_empty() {
        let mut problems = Vec::new();
        if unsupported > 0 {
            problems.push(format!(
                "{unsupported} not a supported URI (charter vault verify names them)"
            ));
        }
        if !missing.is_empty() {
            let clis: Vec<&str> = missing.iter().map(|s| cli_of(s)).collect();
            problems.push(format!("not on PATH: {}", clis.join(", ")));
        }
        return (
            false,
            line(format!("{n} reference(s), but {}", problems.join("; "))),
        );
    }
    (
        true,
        line(format!("{n} reference(s) via {}", needed.join(", "))),
    )
}
