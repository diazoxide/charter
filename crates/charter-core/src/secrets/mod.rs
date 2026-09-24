//! Vaults and the secrets in them — `charter/secrets/` and `charter/commands_secrets.py`.
//!
//! **The overriding rule: a secret value never reaches the model.** Everything here is built
//! around the three routes a value may leave this process by, and the ones it may not:
//!
//! - `secret list` prints key names, never values; `secret get` prints a size band and a keyed
//!   fingerprint ([`fingerprint`]), and `--reveal` refuses a stdout that is not a terminal;
//! - `secret exec` hands a value to a child's ENVIRONMENT, or to a temp file created 0600 and
//!   removed when the child is done, and redacts every value it resolved from what the child
//!   printed ([`redact`]) — a net against an accidental echo, not a boundary;
//! - no error message carries a value, and none carries a resolver's own output (`op`'s
//!   stderr can echo what it read, and on a read path its stdout IS the secret).
//!
//! Every route by which a value does leave — `exec`, `cp` and `get --reveal` — writes one
//! trace event naming the vault, the keys and the command, never a value ([`cmd`]).
//!
//! Four providers: `keyring` (the operating system's own credential store, one item per
//! secret, the default for a new vault — [`keyring`], ADR 0047), `plain-file` (a 0600 JSON
//! object on disk), `reference` (`op://` and `vault://` URIs resolved through the vendor's CLI at
//! read time) and `1password` (one item per vault, whose fields are the secrets, read and
//! written through `op`). The registry that names them is two files, merged per field
//! ([`registry`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub mod cmd;
pub mod dotenv;
pub mod exec;
pub mod fingerprint;
pub mod identity;
pub mod keyring;
pub mod onepassword;
pub mod plain_file;
pub mod reference;
pub mod registry;
pub mod run;
pub mod tty;
pub mod vaultcmd;

/// Which kind of failure a vault operation met — `base.VaultError` and its subclasses.
///
/// Kept because callers tell them apart: `1password`'s `set` treats a read-back that raised
/// [`Kind::NotFound`] differently from any other failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// No vault by that name is registered — `VaultNotConfigured`.
    NotConfigured,
    /// The vault has no secret under that key — `SecretNotFound`.
    NotFound,
    /// The provider is not implemented, or its backend is missing — `ProviderUnavailable`.
    Unavailable,
    /// A terminating signal arrived while a resolver ran, and it was stopped.
    Interrupted,
    /// Any other vault failure — `VaultError`.
    Other,
}

/// A vault failure: its kind and the sentence charter prints for it.
///
/// **The sentence never holds a value.** Every constructor in this module is handed names —
/// a vault, a key, a URI, an exit status — and nothing a resolver printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultError {
    pub kind: Kind,
    pub message: String,
}

impl VaultError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Other,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::NotFound,
            message: message.into(),
        }
    }

    pub fn not_configured(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::NotConfigured,
            message: message.into(),
        }
    }

    pub fn interrupted() -> Self {
        Self {
            kind: Kind::Interrupted,
            message: "stopped by a signal before the value was read".into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Unavailable,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for VaultError {}

/// This process's environment, as a value.
///
/// A snapshot rather than reads of `std::env` wherever one is needed, for two reasons: a child
/// is handed exactly the environment the decisions were made against, and a test can build one
/// without mutating its own process (which edition 2024 makes `unsafe`).
#[derive(Clone, Default)]
pub struct Env {
    vars: Vec<(OsString, OsString)>,
}

/// Names only: an environment holds tokens, and a `{:?}` in a panic or a log line is a
/// print.
impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(
                self.vars
                    .iter()
                    .map(|(k, _)| format!("{}=***", k.to_string_lossy())),
            )
            .finish()
    }
}

impl std::fmt::Debug for Ctx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx")
            .field("root", &self.root)
            .field("state", &self.state)
            .field("env", &self.env)
            .finish()
    }
}

impl Env {
    /// The environment this process was started with.
    pub fn from_process() -> Self {
        Self {
            vars: std::env::vars_os().collect(),
        }
    }

    /// An environment of exactly these variables.
    pub fn of(vars: &[(&str, &str)]) -> Self {
        Self {
            vars: vars
                .iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v)))
                .collect(),
        }
    }

    /// `os.environ.get(name)`, as text. A value that is not UTF-8 reads as unset: charter
    /// only ever asks for names it declared, and those carry tokens and paths.
    pub fn get(&self, name: &str) -> Option<String> {
        self.vars
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.to_str().map(str::to_owned))
    }

    /// Every variable, in the order the process received them.
    pub fn vars(&self) -> &[(OsString, OsString)] {
        &self.vars
    }
}

/// Where a plane keeps its vaults: the root (for the SHARED registry half and for resolving a
/// relative vault `file`) and the state directory (the LOCAL half, the default vault files and
/// the fingerprint key).
#[derive(Clone)]
pub struct Ctx {
    pub root: PathBuf,
    pub state: PathBuf,
    pub env: Env,
}

impl Ctx {
    /// The plane at `root`, with its state directory where `$CHARTER_HOME` puts it.
    pub fn new(root: &Path, env: Env) -> Self {
        let state = match env.get("CHARTER_HOME") {
            Some(home) if !home.is_empty() => PathBuf::from(home),
            _ => root.join(".charter"),
        };
        Self {
            root: root.to_path_buf(),
            state,
            env,
        }
    }

    /// `config.SHARED_VAULTS` — the committed half, beside `personas/`.
    pub fn shared_registry(&self) -> PathBuf {
        self.root.join("vaults.json")
    }

    /// `config.VAULTS_REGISTRY` — this machine's half, under the state directory.
    pub fn local_registry(&self) -> PathBuf {
        self.state.join("vaults.json")
    }

    /// `config.VAULTS_DIR` — where a plain-file or reference vault lives by default.
    pub fn vaults_dir(&self) -> PathBuf {
        self.state.join("vaults")
    }

    /// `base.vault_file_path`: absolute as given, relative to the plane root otherwise, with
    /// a leading `~` expanded.
    pub fn vault_file_path(&self, configured: &str) -> PathBuf {
        let p = expanduser(configured, &self.env);
        if p.is_absolute() {
            p
        } else {
            self.root.join(p)
        }
    }

    /// `shutil.which(name)` against this environment's `PATH`.
    pub fn which(&self, name: &str) -> Option<PathBuf> {
        run::which(name, self.env.get("PATH").as_deref())
    }
}

/// `Path(p).expanduser()`: a leading `~` or `~/` becomes `$HOME`. `~user` is left alone —
/// charter never writes one, and resolving another account's home is not this module's job.
pub fn expanduser(p: &str, env: &Env) -> PathBuf {
    if (p == "~" || p.starts_with("~/"))
        && let Some(home) = env.get("HOME").filter(|h| !h.is_empty())
    {
        return PathBuf::from(format!("{home}{}", &p[1..]));
    }
    PathBuf::from(p)
}

/// `base.redact`: every value this call resolved, where it appears literally in `text`, as
/// `***` — longest first, so a value that contains a shorter one is masked whole.
///
/// **A net, not a boundary**, and the two words are the point: it sees only the values it
/// was handed and only their exact bytes. A child that TRANSFORMS a value — `base64`, `rev`,
/// a JSON re-encode — hands back something this cannot recognise, and `--exec`/`--stream`
/// capture nothing, so nothing reaches here to mask (#444).
///
/// Over bytes rather than text: Python decoded the child's output first and died on one that
/// was not UTF-8. Bytes cannot fail to decode, and a value is masked wherever its UTF-8 bytes
/// appear.
pub fn redact(text: &[u8], secrets: &[String]) -> Vec<u8> {
    let mut values: Vec<&str> = secrets
        .iter()
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .collect();
    // `sorted(..., key=len, reverse=True)`: a stable sort by length, longest first. Python's
    // `len` counts characters, and so does this — two values of one byte length and different
    // character lengths are ordered as Python orders them.
    values.sort_by_key(|s| std::cmp::Reverse(s.chars().count()));
    let mut out = text.to_vec();
    for value in values {
        out = replace_all(&out, value.as_bytes(), b"***");
    }
    out
}

/// [`redact`] for a string charter is about to record, not print.
pub fn redact_str(text: &str, secrets: &[String]) -> String {
    String::from_utf8_lossy(&redact(text.as_bytes(), secrets)).into_owned()
}

fn replace_all(data: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    // A fast path, not a guard: a needle longer than the haystack never matches below either,
    // and an empty one never arrives — `redact` drops empty values first. So `&&` for `||`
    // answers the same, and `.cargo/mutants.toml` says so.
    if from.is_empty() || data.len() < from.len() {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

/// The text of a JSON value as Python's `str()` gives it: a string is itself, anything else
/// its `repr` — `str(data[key])` in the plain-file provider.
pub fn py_str(value: &serde_json::Value) -> String {
    crate::pyrepr::str_json(value)
}

/// A string config field, as the provider reads it: `config.get(name)`, and only when it is
/// text. A committed registry is untrusted input, and a number where a path belongs is no
/// path.
pub fn config_str<'a>(
    config: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<&'a str> {
    config.get(name).and_then(serde_json::Value::as_str)
}

/// Every bit that lets an account other than the owner reach a directory.
pub const OTHERS: u32 = 0o077;

/// `base.short_path`: a path as it should be SHOWN — relative to the plane when inside it.
pub fn short_path(root: &Path, p: &Path) -> String {
    match (resolve(p), resolve(root)) {
        (Some(rp), Some(rr)) => match rp.strip_prefix(&rr) {
            Ok(rel) => rel.to_string_lossy().into_owned(),
            Err(_) => p.to_string_lossy().into_owned(),
        },
        _ => p.to_string_lossy().into_owned(),
    }
}

/// `Path.resolve()` (non-strict): the real path of the longest prefix that exists, with the
/// rest appended.
pub fn resolve(p: &Path) -> Option<PathBuf> {
    Some(PathBuf::from(crate::pypath::realpath(&p.to_string_lossy())))
}

/// `base.mode_note`: `perms 644 (want 600)` for a vault file that is not 0600, or `""`.
/// Reports, never repairs. Never fails: a mode charter could not read is `""`.
pub fn mode_note(p: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(p) {
            Ok(meta) => {
                let mode = meta.permissions().mode() & 0o7777;
                if mode == 0o600 {
                    String::new()
                } else {
                    format!("perms {} (want 600)", octal3(mode))
                }
            }
            Err(_) => String::new(),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = p;
        String::new()
    }
}

/// `oct(mode)[-3:]`.
fn octal3(mode: u32) -> String {
    let text = format!("{mode:o}");
    text[text.len().saturating_sub(3)..].to_string()
}

/// `base.loose_dirs`: the directories from `leaf` up to and including `stop` that another
/// account can reach, outermost last. A leaf not under `stop` yields at most itself.
pub fn loose_dirs(leaf: &Path, stop: &Path) -> Vec<(PathBuf, u32)> {
    let stop_rp = resolve(stop);
    let mut chain: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut cur = leaf.to_path_buf();
    loop {
        chain.push(cur.clone());
        let Some(rp) = resolve(&cur) else { break };
        let parent = cur.parent().map(Path::to_path_buf);
        // Python's root test is `p.parent == p`; here it is `parent.is_none()`, because
        // `Path::parent` drops a component and so never answers the path itself.
        if Some(&rp) == stop_rp.as_ref() || seen.contains(&rp) || parent.is_none() {
            break;
        }
        seen.push(rp);
        cur = parent.unwrap_or_default();
        if cur.as_os_str().is_empty() {
            break;
        }
    }
    let under = stop_rp
        .as_ref()
        .is_some_and(|s| chain.iter().any(|c| resolve(c).as_ref() == Some(s)));
    if !under {
        chain.truncate(1);
    }
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut out = Vec::new();
    #[cfg(unix)]
    for d in chain {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&d) {
            let mode = meta.permissions().mode() & 0o7777;
            if meta.is_dir() && mode & OTHERS != 0 {
                out.push((d, mode));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = chain;
    out
}

/// `base.loose_dir_note`: `listed by other accounts: .charter 755 (want 700 — chmod 700)`,
/// or `""`.
pub fn loose_dir_note(root: &Path, loose: &[(PathBuf, u32)]) -> String {
    if loose.is_empty() {
        return String::new();
    }
    let named: Vec<String> = loose
        .iter()
        .map(|(d, m)| format!("{} {}", short_path(root, d), octal3(*m)))
        .collect();
    format!(
        "listed by other accounts: {} (want 700 — chmod 700)",
        named.join(", ")
    )
}

/// `VaultProvider.env_overlay`: the environment this vault's CLI is invoked with, as
/// `(TARGET, value)` pairs — empty when it declares none.
///
/// A vault may bind the identity it is read through, target variable to source variable:
/// `"env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_ACME_DEVOPS_TOKEN"}`. Only NAMES are stored.
///
/// **A moved identity is read from the keyring first, then the environment** ([`identity`],
/// #237): a chat no longer carries `$OP_*`, and a plain terminal that still exports a stale one
/// does not outvote the token the operator moved. A keyring that holds none, or cannot be read,
/// falls to the environment.
///
/// A declared source found in neither is an ERROR: falling back to an ambient token would read
/// the vault under an identity the plane did not declare, and the failure would look like a
/// missing secret rather than a wrong credential.
pub fn env_overlay(
    ctx: &Ctx,
    vault: &registry::Vault,
) -> Result<Vec<(String, String)>, VaultError> {
    let mut out = Vec::new();
    for (target, source) in identity::bindings(vault) {
        let kept = identity::from_keyring(ctx, vault, &source);
        let found = match &kept {
            Ok(Some(token)) => Some(token.clone()),
            _ => ctx.env.get(&source).filter(|v| !v.is_empty()),
        };
        match found {
            Some(val) => out.push((target, val)),
            None => return Err(identity_unset(ctx, vault, &target, &source, kept.err())),
        }
    }
    Ok(out)
}

/// Whether every identity variable the vault is read through can be found — moved into the
/// keyring, or set — WITHOUT reading the keyring: the refusal [`env_overlay`] would give for the
/// first one that cannot, or `None`. What `vault list` and the app's panel draw from, so neither
/// makes the Keychain ask anything.
pub fn identity_missing(ctx: &Ctx, vault: &registry::Vault) -> Option<VaultError> {
    identity::held(ctx, vault)
        .into_iter()
        .find(|b| b.held == identity::Held::Unset)
        .map(|b| identity_unset(ctx, vault, &b.target, &b.source, None))
}

/// The refusal for an identity variable found nowhere. Names only: a keyring failure is the
/// store's sentence, which names the service and never a value.
fn identity_unset(
    ctx: &Ctx,
    vault: &registry::Vault,
    target: &str,
    source: &str,
    keyring: Option<VaultError>,
) -> VaultError {
    if identity::in_keyring(ctx, vault) {
        let why = keyring.map_or_else(
            || format!("{} holds no token for it", keyring::STORE_NAME),
            |e| e.message,
        );
        return VaultError::new(format!(
            "vault '{}' is read through ${source}, which was moved into {}, but {why}, and it \
             is not set here either. charter will not fall back to an ambient ${target}.\n  Set \
             ${source} where charter runs and move it again from the vault's tab.",
            vault.name,
            keyring::STORE_NAME
        ));
    }
    VaultError::new(format!(
        "vault '{}' is read through ${source}, which is unset. charter will not fall \
         back to an ambient ${target}: that would read this vault under an identity \
         it does not declare, and the failure would look like a missing secret rather \
         than a wrong credential.\n  export {source}=… , or drop the binding: charter \
         vault add {} --provider {} --force",
        vault.name, vault.name, vault.provider
    ))
}

/// `VaultProvider.identity_note`: ` (identity from $SOURCE)`, or `""` — appended to a read
/// failure so a permission error points at the identity in play.
pub fn identity_note(vault: &registry::Vault) -> String {
    let Some(mapping) = vault
        .config
        .get("env")
        .and_then(serde_json::Value::as_object)
    else {
        return String::new();
    };
    let srcs: Vec<String> = mapping
        .values()
        .map(|s| format!("${}", py_str(s)))
        .collect();
    if srcs.is_empty() {
        String::new()
    } else {
        format!(" (identity from {})", srcs.join(", "))
    }
}

/// Create `dir` and every missing level above it at 0700 — `base.make_private_dir`. A
/// directory that already exists is left exactly as it is.
pub fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    crate::trace::private_mkdir(dir)
}

#[cfg(test)]
mod tests;

// Unix only: its stand-in CLIs are shell scripts, and its vault files are judged by mode. Two
// attributes rather than `all(test, unix)`, which cargo-mutants does not read as test code.
#[cfg(test)]
#[cfg(unix)]
mod tests_store;
