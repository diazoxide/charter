//! A vault's identity, kept in the keyring rather than in a shell's environment (#237, ADR 0047
//! as amended; hardened after the #271 review).
//!
//! A vault may be read through an identity variable — `"env": {"OP_SERVICE_ACCOUNT_TOKEN":
//! "OP_TEAM_TOKEN"}` — and until this module the token had to sit in `$OP_TEAM_TOKEN` in every
//! shell charter ran in, which is every shell an agent's commands run in. One `echo` printed it.
//!
//! **The token goes into the keyring, and the whole binding that decides how it is used is
//! pinned in this machine's registry half beside it.** A record is written only by an operator
//! action (the vault tab's password box, or a move from the app's own environment). It holds:
//!
//! - a **random item id per (vault, plane)** — the keyring item is `charter/@identity/<id>`,
//!   account the source variable's name. Random and local, so a chat cannot name another plane's
//!   item, and two planes that both bind `$OP_TEAM_TOKEN` never share one (#271 review, U2);
//! - the **binding the move was made against** — the `env` map, the op-vault, the account. The
//!   mark is honoured only while the vault's effective binding still equals this, so a committed
//!   `vaults.json` that changes any of them cannot redirect the token (#271 review, U5);
//! - the **absolute path of `op`** resolved from the operator's own PATH, and its code-signing
//!   Team identifier. A keyring-held read runs exactly that binary and refuses a mismatch, so a
//!   chat cannot hand the token to an `op` it dropped on its own PATH (#271 review, U1).
//!
//! Everything here reads the record from the **LOCAL half only**. The keyring is this machine's,
//! and so is every decision that hands a keyring item to a subprocess.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::registry::{self, Vault};
use super::{Ctx, VaultError, keyring};

/// The keyring service every moved identity item lives under, before its random id. The `@`
/// cannot begin a vault name ([`registry::name_ok`]), so no vault's own service
/// (`charter/<vault>/…`) can ever collide with one of these, and [`keyring::service_ok_for`]
/// never accepts one as a vault's secret item (#271 review, U4).
pub const SERVICE_BASE: &str = "charter/@identity";

/// The config field, in the local registry half, that records a moved identity.
pub const MARK: &str = "identity";

/// [`MARK`]'s `held` value.
pub const IN_KEYRING: &str = "keyring";

/// The code-signing Team identifier of AgileBits' `op`, the value a signed 1Password CLI is
/// expected to carry. It is not hard-checked — the record pins whatever `codesign` reported at
/// move time and a later read must match THAT — but it is named here so an operator can confirm
/// a fresh install against it. (Confirm against a real signed `op`; unverified in this repo.)
pub const ONEPASSWORD_TEAM_ID: &str = "2BUA8C4S2C";

/// Where one of a vault's identity variables is read from now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// Moved: the keyring item, under the pinned binding.
    Keyring,
    /// Set in this process's environment, and not moved.
    Environment,
    /// Neither moved nor set: a read of the vault is refused.
    Unset,
}

/// One identity binding of a vault, and where that variable is read from now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub target: String,
    pub source: String,
    pub held: Held,
}

/// The vault's identity bindings as `(target, source)` — the variable the CLI reads, and the
/// variable this machine carries it in. Empty for a vault that declares none.
pub fn bindings(vault: &Vault) -> Vec<(String, String)> {
    vault
        .config
        .get("env")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(target, source)| (target.clone(), super::py_str(source)))
        .collect()
}

/// The binding a move is pinned against: the `env` map, the op-vault and the account, read from
/// the merged vault. Two vaults with the same fingerprint are read the same way; a change to any
/// of it is a different identity and unpins the mark.
fn fingerprint(vault: &Vault) -> (BTreeMap<String, String>, Option<String>, Option<String>) {
    let env: BTreeMap<String, String> = bindings(vault).into_iter().collect();
    (env, op_vault_of(vault), account_of(vault))
}

fn cfg_str(vault: &Vault, key: &str, legacy: &str) -> Option<String> {
    super::config_str(&vault.config, key)
        .or_else(|| super::config_str(&vault.config, legacy))
        .map(|v| crate::memstore::py_strip(v).to_string())
        .filter(|v| !v.is_empty())
}

fn op_vault_of(vault: &Vault) -> Option<String> {
    cfg_str(vault, "op-vault", "op_vault")
}

fn account_of(vault: &Vault) -> Option<String> {
    cfg_str(vault, "account", "account")
}

/// This machine's recorded identity for `vault`, from the LOCAL half — never the merged view, so
/// a committed entry can neither add a record nor change one.
fn record(ctx: &Ctx, vault: &Vault) -> Option<Map<String, Value>> {
    let local = registry::load_local(ctx).ok()?;
    let rec = registry::usable_vaults(&local)
        .get(&vault.name)?
        .get("config")?
        .get(MARK)?
        .as_object()?
        .clone();
    (rec.get("held").and_then(Value::as_str) == Some(IN_KEYRING)).then_some(rec)
}

/// Whether the recorded binding still equals the vault's effective one. A `git pull` that edits
/// the committed `env`, `op-vault` or `account` fails this, so the mark stops being honoured
/// rather than steering the token somewhere new (#271 review, U5).
fn record_matches(rec: &Map<String, Value>, vault: &Vault) -> bool {
    let (env, op_vault, account) = fingerprint(vault);
    let recorded_env: BTreeMap<String, String> = rec
        .get("bindings")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), super::py_str(v)))
                .collect()
        })
        .unwrap_or_default();
    let recorded = |k: &str| rec.get(k).and_then(Value::as_str).map(str::to_owned);
    recorded_env == env && recorded("op_vault") == op_vault && recorded("account") == account
}

/// Whether this machine's registry marks `vault`'s identity as moved AND the pinned binding still
/// matches. Reads no keyring.
pub fn in_keyring(ctx: &Ctx, vault: &Vault) -> bool {
    record(ctx, vault).is_some_and(|rec| record_matches(&rec, vault))
}

/// Each identity variable the vault is read through, and where it is read from now. Never reads
/// the keyring: a moved identity is [`Held::Keyring`] by its pinned record.
pub fn held(ctx: &Ctx, vault: &Vault) -> Vec<Binding> {
    let moved = in_keyring(ctx, vault);
    bindings(vault)
        .into_iter()
        .map(|(target, source)| {
            let held = if moved {
                Held::Keyring
            } else if ctx.env.get(&source).is_some_and(|v| !v.is_empty()) {
                Held::Environment
            } else {
                Held::Unset
            };
            Binding {
                target,
                source,
                held,
            }
        })
        .collect()
}

/// The token kept for `source`, where the vault's identity was moved and the pinned binding still
/// matches; `None` otherwise, or when the keyring holds none. A keyring that could not be read is
/// an error, never a value.
pub fn from_keyring(ctx: &Ctx, vault: &Vault, source: &str) -> Result<Option<String>, VaultError> {
    let Some(rec) = record(ctx, vault).filter(|rec| record_matches(rec, vault)) else {
        return Ok(None);
    };
    let Some(id) = rec
        .get("ids")
        .and_then(Value::as_object)
        .and_then(|m| m.get(source))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    Ok(keyring::store(ctx)
        .get(&item_service(id), source)?
        .map(keyring::Secret::into_inner)
        .filter(|v| !v.is_empty()))
}

/// The absolute `op` a keyring-held read must run, verified against the pinned path and Team id;
/// `None` for a vault whose identity is not keyring-held, whose `op` is then resolved from PATH
/// as before (no keyring item is at stake). An error when the pin is missing or fails to verify —
/// charter never falls back to the caller's PATH for a keyring-held identity (#271 review, U1).
pub fn pinned_op(ctx: &Ctx, vault: &Vault) -> Result<Option<PathBuf>, VaultError> {
    let Some(rec) = record(ctx, vault).filter(|rec| record_matches(rec, vault)) else {
        return Ok(None);
    };
    let refuse = |why: &str| {
        VaultError::new(format!(
            "vault '{}' reads a token charter moved into {}, so it will only run the `op` that \
             was pinned when the token was stored — and {why}. Put the token in again from the \
             vault's tab, in a charter started from a shell where the real `op` is on PATH.",
            vault.name,
            keyring::STORE_NAME
        ))
    };
    let cmd = rec
        .get("op_cmd")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| refuse("no `op` was pinned"))?;
    let path = PathBuf::from(cmd);
    if !super::run::which(cmd, None).is_some_and(|p| p == path) {
        return Err(refuse(
            "the pinned `op` is gone or is no longer an executable at that path",
        ));
    }
    let want = rec.get("op_team").and_then(Value::as_str).unwrap_or("");
    let have = op_team_id(&path);
    if have.as_deref().unwrap_or("") != want {
        return Err(refuse(
            "the `op` at that path is signed by a different team than the one pinned",
        ));
    }
    Ok(Some(path))
}

/// The keyring service for item `id`.
fn item_service(id: &str) -> String {
    format!("{SERVICE_BASE}/{id}")
}

/// A random id for a new identity item.
fn new_id() -> Result<String, VaultError> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes)
        .map_err(|e| VaultError::new(format!("charter could not make a random name: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// `op` as the operator's own PATH resolves it now, absolute, with its Team id — the trusted
/// resolution recorded at move/put time. `("", "")` when `op` is not on PATH here: the record is
/// still written (the token is stored), and [`pinned_op`] then refuses every read until the token
/// is put again from a shell where the real `op` is found, rather than resolving it from the
/// caller's PATH. Best effort here, strict there.
fn resolve_op_now(ctx: &Ctx) -> (String, String) {
    match ctx.which("op") {
        Some(path) => {
            let team = op_team_id(&path).unwrap_or_default();
            (path.display().to_string(), team)
        }
        None => (String::new(), String::new()),
    }
}

/// The code-signing Team identifier of the binary at `path`, or `None` where it is unsigned,
/// where `codesign` is not available, or off macOS. Runs `codesign` as a child and reads only its
/// diagnostic output, which names a team and not a credential.
fn op_team_id(path: &Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = crate::forklock::output(
            std::process::Command::new("/usr/bin/codesign")
                .args(["-dv", "--verbose=4"])
                .arg(path),
        )
        .ok()?;
        // codesign writes the fields to stderr.
        let text = String::from_utf8_lossy(&out.stderr);
        text.lines()
            .find_map(|l| l.strip_prefix("TeamIdentifier="))
            .map(str::to_owned)
            .filter(|t| t != "not set")
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

/// Store one token for `source` in the keyring and return the item id it was stored under.
fn store_token(ctx: &Ctx, source: &str, token: &str) -> Result<String, VaultError> {
    let id = new_id()?;
    keyring::store(ctx).set(&item_service(id.as_str()), source, token)?;
    Ok(id)
}

/// Write the pinned record into this machine's half, beside whatever else it says of the vault.
fn write_record(
    ctx: &Ctx,
    vault: &Vault,
    ids: BTreeMap<String, String>,
    op_cmd: &str,
    op_team: &str,
) -> Result<Vec<String>, VaultError> {
    let (env, op_vault, account) = fingerprint(vault);
    let mut rec = Map::new();
    rec.insert("held".into(), Value::String(IN_KEYRING.into()));
    rec.insert(
        "bindings".into(),
        Value::Object(
            env.iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect(),
        ),
    );
    rec.insert(
        "op_vault".into(),
        op_vault.map_or(Value::Null, Value::String),
    );
    rec.insert("account".into(), account.map_or(Value::Null, Value::String));
    rec.insert("op_cmd".into(), Value::String(op_cmd.to_owned()));
    rec.insert("op_team".into(), Value::String(op_team.to_owned()));
    rec.insert(
        "ids".into(),
        Value::Object(
            ids.iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect(),
        ),
    );

    let mut local = registry::load_local(ctx)?;
    let vaults = object_at(&mut local, "vaults");
    let entry = object_at(vaults, &vault.name);
    object_at(entry, "config").insert(MARK.into(), Value::Object(rec));
    registry::save_local(ctx, &local)?;

    let sources: Vec<String> = env.into_values().collect();
    let listed: Vec<Value> = sources.iter().map(|s| Value::String(s.clone())).collect();
    // Named, never valued: the fields are scrubbed against the tokens too.
    super::cmd::trace_secret_use(
        ctx,
        "identity-move",
        &[],
        &[
            ("vault", Value::String(vault.name.clone())),
            ("variables", Value::Array(listed)),
        ],
    );
    Ok(sources)
}

/// The object under `key` in `map`, made — or put in place of whatever else a hand-edited file
/// held there.
fn object_at<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let slot = map
        .entry(key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    match slot {
        Value::Object(inner) => inner,
        _ => unreachable!("just made an object"),
    }
}

/// The vault's single identity source, for the password-box path — which stores ONE token the
/// operator pasted and so needs exactly one. An error naming the variables when a vault declares
/// none or several.
fn sole_source(vault: &Vault) -> Result<String, VaultError> {
    let mut sources: Vec<String> = bindings(vault).into_iter().map(|(_, s)| s).collect();
    sources.sort();
    sources.dedup();
    match sources.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(VaultError::new(format!(
            "vault '{}' is not read through an identity variable, so there is no token to store.",
            vault.name
        ))),
        many => Err(VaultError::new(format!(
            "vault '{}' is read through more than one identity variable ({}); paste each with \
             the terminal's `charter vault add`, or move them from a shell that exports them.",
            vault.name,
            many.join(", ")
        ))),
    }
}

/// **The password-box path (#237, #271 review U3).** Store the token the operator pasted for the
/// vault's one identity variable straight into the keyring, and pin the binding. The token is
/// taken here and nowhere else: it never sits in the app's environment, so no chat can read it
/// from there. `op` is pinned from the environment charter runs in — the operator's, resolved on
/// the trusted thread that handles the command.
pub fn put_in_keyring(ctx: &Ctx, vault: &Vault, token: &str) -> Result<Vec<String>, VaultError> {
    if token.is_empty() {
        return Err(VaultError::new(
            "refusing to store an empty token — the vault would look set up and every read would \
             fail as if it were not.",
        ));
    }
    let source = sole_source(vault)?;
    let (op_cmd, op_team) = resolve_op_now(ctx);
    let id = store_token(ctx, &source, token)?;
    let ids = BTreeMap::from([(source, id)]);
    write_record(ctx, vault, ids, &op_cmd, &op_team)
}

/// **The move-from-environment path.** Read each identity variable the vault declares from this
/// process's environment, store it, and pin the binding. Refused, with nothing stored, when the
/// vault declares no identity or any variable is unset — a half-moved identity would read one
/// token from the keyring and look for the other in an environment about to lose it.
///
/// The token is read from charter's OWN environment, which a chat cannot reach; still, the caller
/// is told to relaunch charter without the export, because until it does the app's process keeps
/// the token in its environment block ([`app_env_holds_a_token`]).
pub fn move_to_keyring(ctx: &Ctx, vault: &Vault) -> Result<Vec<String>, VaultError> {
    let sources: Vec<String> = {
        let mut s: Vec<String> = bindings(vault).into_iter().map(|(_, s)| s).collect();
        s.sort();
        s.dedup();
        s
    };
    if sources.is_empty() {
        return Err(VaultError::new(format!(
            "vault '{}' is not read through an identity variable, so there is no token to move.",
            vault.name
        )));
    }
    let mut tokens: Vec<(String, String)> = Vec::new();
    for source in &sources {
        match ctx.env.get(source).filter(|v| !v.is_empty()) {
            Some(token) => tokens.push((source.clone(), token)),
            None => {
                return Err(VaultError::new(format!(
                    "${source} is not set where charter is running, so there is no token of \
                     vault '{}' to move. Start charter from a shell that exports it, then move \
                     it — or paste it into the vault's tab instead.",
                    vault.name
                )));
            }
        }
    }
    let (op_cmd, op_team) = resolve_op_now(ctx);
    let mut ids = BTreeMap::new();
    for (source, token) in &tokens {
        ids.insert(source.clone(), store_token(ctx, source, token)?);
    }
    write_record(ctx, vault, ids, &op_cmd, &op_team)
}

/// Whether charter's OWN environment still carries any identity variable of `vault` — the reason
/// the move path warns the operator to relaunch. On macOS and Linux a same-user process can read
/// another's environment block, so a token left in the app's environment is reachable from a chat
/// however the in-process value is scrubbed (#271 review, U3).
pub fn app_env_holds_a_token(ctx: &Ctx, vault: &Vault) -> Vec<String> {
    bindings(vault)
        .into_iter()
        .map(|(_, source)| source)
        .filter(|source| ctx.env.get(source).is_some_and(|v| !v.is_empty()))
        .collect()
}

/// The prefix of every variable no chat the app starts is given: 1Password's, whose
/// service-account tokens are the identities this module moves (#232, decision 4). Every one,
/// not only those a vault declares — a token nobody registered is still a token an agent could
/// `echo`, and the 1Password CLI reads `$OP_SERVICE_ACCOUNT_TOKEN` itself.
pub const KEPT_FROM_CHATS: &str = "OP_";

/// Whether `name` is a variable no chat is given: an `OP_`-prefixed one, matched **case
/// insensitively** so `op_*` and `Op_*` are caught too (#271 review, U6). A vault that binds a
/// source of another spelling (`--token-env PROD_1P_TOKEN`) is stripped by NAME, not by this;
/// see [`crate::secrets::registry::identity_vars`], gathered by the caller.
pub fn kept_from_chats(name: &std::ffi::OsStr) -> bool {
    let bytes = name.as_encoded_bytes();
    bytes.len() >= KEPT_FROM_CHATS.len()
        && bytes[..KEPT_FROM_CHATS.len()].eq_ignore_ascii_case(KEPT_FROM_CHATS.as_bytes())
}
