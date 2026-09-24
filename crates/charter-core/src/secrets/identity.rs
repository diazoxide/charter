//! A vault's identity, kept in the keyring rather than in a shell's environment (#237, ADR 0047
//! as amended).
//!
//! A vault may be read through an identity variable — `"env": {"OP_SERVICE_ACCOUNT_TOKEN":
//! "OP_TEAM_TOKEN"}` — and until this module the token had to sit in `$OP_TEAM_TOKEN` in every
//! shell charter ran in, which is every shell an agent's commands run in. One `echo` printed it,
//! and one service-account token outweighs every secret it unlocks.
//!
//! **Moving** reads the token from this process's environment, stores it in the keyring
//! ([`super::keyring::store`], so a test build writes the stub), and marks the vault's identity
//! as held there. **From then on** [`super::env_overlay`] reads each of the vault's identity
//! variables from the keyring first and the environment second.
//!
//! ```text
//! keyring item   service  charter/identity
//!                account  OP_TEAM_TOKEN        (the variable's NAME)
//! local registry .charter/vaults.json → vaults.team.config.identity = "keyring"
//! ```
//!
//! **The item is keyed by the variable's name, machine-wide**, because the variable is: two
//! planes whose vaults both read `$OP_TEAM_TOKEN` were handed the same token by the same shell.
//!
//! **The mark is read from the LOCAL half only.** The keyring is this machine's, and so is the
//! decision to read it. A committed `vaults.json` arrives by `git pull`, and one that could say
//! "read this identity from the keyring" would choose which keyring item charter hands to the
//! `op` it runs next.
//!
//! **Nothing here reads the keyring to say where an identity is** ([`held`]): the mark answers,
//! so drawing a vault's tab or `vault list` never makes the Keychain ask anything.

use serde_json::{Map, Value};

use super::registry::{self, Vault};
use super::{Ctx, VaultError, keyring};

/// The keyring service every moved identity is kept under; the account is the variable's name.
pub const SERVICE: &str = "charter/identity";

/// The config field, in the local registry half, that marks a vault's identity as moved.
pub const MARK: &str = "identity";

/// [`MARK`]'s one value.
pub const IN_KEYRING: &str = "keyring";

/// Where one of a vault's identity variables is read from now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// Moved: the keyring first, then the environment.
    Keyring,
    /// Set in this process's environment, and not moved.
    Environment,
    /// Neither moved nor set: a read of the vault is refused.
    Unset,
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

/// Whether this machine's registry half marks `vault`'s identity as moved into the keyring.
pub fn in_keyring(ctx: &Ctx, vault: &Vault) -> bool {
    registry::load_local(ctx)
        .ok()
        .map(|local| registry::usable_vaults(&local))
        .and_then(|vaults| {
            vaults
                .get(&vault.name)
                .and_then(|e| e.get("config"))
                .and_then(|c| c.get(MARK))
                .map(|m| m.as_str() == Some(IN_KEYRING))
        })
        .unwrap_or(false)
}

/// Each identity variable the vault is read through, and where it is read from now. Never reads
/// the keyring: a moved identity is [`Held::Keyring`] by its mark.
pub fn held(ctx: &Ctx, vault: &Vault) -> Vec<(String, Held)> {
    let moved = in_keyring(ctx, vault);
    bindings(vault)
        .into_iter()
        .map(|(_, source)| {
            let at = if moved {
                Held::Keyring
            } else if ctx.env.get(&source).is_some_and(|v| !v.is_empty()) {
                Held::Environment
            } else {
                Held::Unset
            };
            (source, at)
        })
        .collect()
}

/// The token kept for `source`, where the vault's identity was moved; `None` where it was not,
/// or the keyring holds none. A keyring that could not be read is an error, never a value.
pub fn from_keyring(ctx: &Ctx, vault: &Vault, source: &str) -> Result<Option<String>, VaultError> {
    if !in_keyring(ctx, vault) {
        return Ok(None);
    }
    Ok(keyring::store(ctx)
        .get(SERVICE, source)?
        .map(keyring::Secret::into_inner)
        .filter(|v| !v.is_empty()))
}

/// Move the vault's identity into the keyring: every identity variable it is read through, read
/// from this process's environment and stored, then the vault marked. Answers the variables'
/// NAMES. Refused, with nothing stored or marked, when the vault declares no identity or any of
/// its variables is unset — a half-moved identity would read one token from the keyring and
/// look for the other in an environment that will soon not carry it.
pub fn move_to_keyring(ctx: &Ctx, vault: &Vault) -> Result<Vec<String>, VaultError> {
    let sources: Vec<String> = bindings(vault).into_iter().map(|(_, s)| s).collect();
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
                     it.",
                    vault.name
                )));
            }
        }
    }
    let store = keyring::store(ctx);
    for (source, token) in &tokens {
        store.set(SERVICE, source, token)?;
    }
    mark(ctx, vault)?;
    let (names, values): (Vec<String>, Vec<String>) = tokens.into_iter().unzip();
    let listed: Vec<Value> = names.iter().map(|n| Value::String(n.clone())).collect();
    // Named, never valued: the event says which identity moved, and the fields are scrubbed
    // against the tokens anyway.
    super::cmd::trace_secret_use(
        ctx,
        "identity-move",
        &values,
        &[
            ("vault", Value::String(vault.name.clone())),
            ("variables", Value::Array(listed)),
        ],
    );
    Ok(names)
}

/// Write the mark into this machine's half, beside whatever that half already says of the vault.
fn mark(ctx: &Ctx, vault: &Vault) -> Result<(), VaultError> {
    let mut local = registry::load_local(ctx)?;
    let vaults = local
        .entry("vaults")
        .or_insert_with(|| Value::Object(Map::new()));
    if !vaults.is_object() {
        *vaults = Value::Object(Map::new());
    }
    let entry = vaults
        .as_object_mut()
        .expect("just made an object")
        .entry(vault.name.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    let config = entry
        .as_object_mut()
        .expect("just made an object")
        .entry("config")
        .or_insert_with(|| Value::Object(Map::new()));
    if !config.is_object() {
        *config = Value::Object(Map::new());
    }
    config
        .as_object_mut()
        .expect("just made an object")
        .insert(MARK.into(), Value::String(IN_KEYRING.into()));
    registry::save_local(ctx, &local)
}

/// The prefix of every variable no chat the app starts is given: 1Password's, whose
/// service-account tokens are the identities this module moves (#232, decision 4). Every one,
/// not only those a vault declares — a token nobody registered is still a token an agent could
/// `echo`, and the 1Password CLI reads `$OP_SERVICE_ACCOUNT_TOKEN` itself.
pub const KEPT_FROM_CHATS: &str = "OP_";

/// Whether `name` is a variable no chat is given ([`KEPT_FROM_CHATS`]).
pub fn kept_from_chats(name: &std::ffi::OsStr) -> bool {
    name.as_encoded_bytes()
        .starts_with(KEPT_FROM_CHATS.as_bytes())
}
