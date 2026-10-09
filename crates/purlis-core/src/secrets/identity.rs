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
//! - a **random item id per (vault, plane)** — the keyring item is `purlis/@identity/<id>`, or
//!   `charter/@identity/<id>` for a record made before the rename (the record's `base` says
//!   which, [`READ_BASES`]),
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
//!
//! **A token is stored per vault, from that vault's own tab** (#1526, D-1526-7). A store marks
//! the one vault the person is in and no other, however many vaults are read through the same
//! variable: a vault the committed half names is never given a token the person did not put in
//! from its tab. [`unset_alike`] only NAMES the other vaults that still have none, for the tab
//! to point at.
//!
//! **A vault may declare a token with no variable at all** (#1527, D-1527-1): `"token":
//! "keyring"` says the vault is read with a service-account token purlis keeps in the keyring
//! and names nothing else. It reads as one binding whose source is [`KEPT_SOURCE`], a word that
//! is no variable's name and is never looked up in an environment, so everything above holds
//! for it unchanged: the declaration may be committed, and the record, the item and the pinned
//! program are this machine's alone. Such a vault is made, tested and given its record in one
//! step by [`super::setup`].
//!
//! **A replaced token does not stay behind.** Once a new record is saved, the items the
//! previous record named are deleted. A store that fails part way deletes what it had stored
//! and leaves the record as it was.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::registry::{self, Vault};
use super::{Ctx, VaultError, keyring};
use crate::names::KEYCHAIN_IDENTITY_PREFIX;

/// The keyring service every moved identity item is written under, before its random id. The
/// `@` cannot begin a vault name ([`registry::name_ok`]), so no vault's own service
/// (`purlis/<vault>/…`) can ever collide with one of these, and [`keyring::service_ok_for`]
/// never accepts one as a vault's secret item (#271 review, U4).
pub const SERVICE_BASE: &str = KEYCHAIN_IDENTITY_PREFIX.write;

/// The service bases an identity item can live under, each with the word the record's
/// [`BASE`] field names it by: [`SERVICE_BASE`] (`purlis`), and the old `charter/@identity`
/// while the rename window lasts (#1261).
///
/// **The record says which, and charter reads that one alone** (D-RN4-6). A record without the
/// field was written before the rename and reads only `charter/@identity/<id>`; a move made now
/// writes `"base": "purlis"`. The switch is explicit, made by the keychain copy after it has
/// verified the copy (RN-6, V93h), never by an item merely existing under the new name — so an
/// item planted at `purlis/@identity/<id>` cannot shadow the one the record was made for.
pub const READ_BASES: [(&str, &str); 2] = [
    ("purlis", KEYCHAIN_IDENTITY_PREFIX.write),
    ("charter", KEYCHAIN_IDENTITY_PREFIX.reads[0]),
];

/// The record's field naming the base its items live under.
pub const BASE: &str = "base";

/// What a move made now writes into [`BASE`].
const BASE_NOW: &str = READ_BASES[0].0;

/// The base a record's items are read under: its [`BASE`] field's, or the old base when it has
/// none. A value charter does not know is an error, never a guess.
fn base_of(rec: &Map<String, Value>, vault: &Vault) -> Result<&'static str, VaultError> {
    let word = match rec.get(BASE) {
        None => READ_BASES[1].0,
        Some(Value::String(w)) => w.as_str(),
        Some(_) => "",
    };
    READ_BASES
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, base)| *base)
        .ok_or_else(|| {
            VaultError::new(format!(
                "vault '{}' has a moved identity whose record names a keyring base purlis does \
                 not know. Put the token in again from the vault's tab.",
                vault.name
            ))
        })
}

/// The segment after the product's keyring prefix that marks an identity item.
pub const OWNER: &str = "@identity";

/// The config field, in the local registry half, that records a moved identity.
pub const MARK: &str = "identity";

/// [`MARK`]'s `held` value.
pub const IN_KEYRING: &str = "keyring";

/// The variable the 1Password CLI reads a service-account token from.
pub const TOKEN_TARGET: &str = "OP_SERVICE_ACCOUNT_TOKEN";

/// The config field that declares a token kept in the keyring and read through no variable
/// (#1527). It names no secret and no keyring item, so it may be committed.
pub const TOKEN: &str = "token";

/// [`TOKEN`]'s one value: the keyring.
pub const TOKEN_IN_KEYRING: &str = "keyring";

/// The source a [`TOKEN`] declaration reads as: what the record's `bindings` and `ids` and the
/// keyring item's account say where a bound vault's say its variable. The hyphens keep it from
/// being a name `--env` accepts, and it is never looked up in an environment ([`set_here`]).
pub const KEPT_SOURCE: &str = "service-account-token";

/// Whether `source` is [`KEPT_SOURCE`]: a token that lives in the keyring and nowhere else.
pub fn kept(source: &str) -> bool {
    source == KEPT_SOURCE
}

/// Whether `vault` declares a token kept in the keyring ([`TOKEN`]). A 1Password vault's only:
/// the field means nothing to another provider.
pub fn declares_kept(vault: &Vault) -> bool {
    vault.provider == "1password"
        && super::config_str(&vault.config, TOKEN) == Some(TOKEN_IN_KEYRING)
}

/// The refusal for a [`TOKEN`] this purlis does not know, or `None`. A value a newer purlis
/// wrote is refused, never read as "no token": that would read the vault as somebody else.
pub fn token_refusal(vault: &Vault) -> Option<VaultError> {
    let declared = vault.config.get(TOKEN).filter(|v| !v.is_null())?;
    if vault.provider != "1password" || declared.as_str() == Some(TOKEN_IN_KEYRING) {
        return None;
    }
    Some(VaultError::new(format!(
        "vault '{}' declares a way of keeping its token that this purlis does not know. purlis \
         will not read it as a vault with no token. Update purlis, or set it up again from the \
         vault's tab.",
        vault.name
    )))
}

/// `source` as this process's environment has it, when it is set to something. Never for
/// [`KEPT_SOURCE`], which is no variable.
pub fn set_here(ctx: &Ctx, source: &str) -> Option<String> {
    if kept(source) {
        return None;
    }
    ctx.env.get(source).filter(|v| !v.is_empty())
}

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
///
/// A vault that declares a kept token ([`declares_kept`]) reads its token through
/// [`KEPT_SOURCE`] and through nothing else: an `env` binding of [`TOKEN_TARGET`] beside the
/// declaration is not honoured, so no half of the registry can name a variable to read the
/// token from instead.
pub fn bindings(vault: &Vault) -> Vec<(String, String)> {
    let kept = declares_kept(vault);
    let mut out: Vec<(String, String)> = vault
        .config
        .get("env")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(target, _)| !(kept && target.as_str() == TOKEN_TARGET))
        .map(|(target, source)| (target.clone(), super::py_str(source)))
        .collect();
    if kept {
        out.push((TOKEN_TARGET.to_owned(), KEPT_SOURCE.to_owned()));
    }
    out
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

/// The item the vault's secrets are kept in: the one it names, or its default.
fn op_item_of(vault: &Vault) -> String {
    cfg_str(vault, "op-item", "op_item").unwrap_or_else(|| format!("charter-{}", vault.name))
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
    // The item is pinned by every record made since #1527; an older record does not name one
    // and is not held to it until its next read pins it ([`pin_the_item`], #1542).
    let item = !rec.contains_key("op_item") || recorded("op_item") == Some(op_item_of(vault));
    recorded_env == env
        && recorded("op_vault") == op_vault
        && recorded("account") == account
        && item
}

/// Whether the item `vault` reads is one the committed half chose: the item the merged vault
/// names differs from the one this machine's half alone gives it (its own `op-item`, or the
/// default). Compared through [`op_item_of`] both ways, never by which keys are present: the
/// modern spelling outranks the legacy one across the merged map, so a committed `op-item`
/// decides even where this half holds a legacy `op_item`.
fn item_chosen_by_commit(ctx: &Ctx, vault: &Vault) -> bool {
    let Ok(local) = registry::load_local(ctx) else {
        return true;
    };
    let config = registry::usable_vaults(&local)
        .get(&vault.name)
        .and_then(|entry| entry.get("config"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let here = Vault {
        config,
        ..vault.clone()
    };
    op_item_of(vault) != op_item_of(&here)
}

/// The refusal for a record made before #1527 (it names no item) whose vault now reads an item
/// the committed half chose, or `None` (#1542 review, M1). Pinning that item would be a
/// committed entry changing a record, which ADR 0047 rules out, and honouring the record
/// unpinned would let the commit redirect the token. So the token is not used until it is given
/// again from the vault's tab, where the settings it will pin are shown.
pub fn unpinned_item_refusal(ctx: &Ctx, vault: &Vault) -> Option<VaultError> {
    let rec = record(ctx, vault)?;
    if rec.contains_key("op_item") || !record_matches(&rec, vault) {
        return None;
    }
    item_chosen_by_commit(ctx, vault).then(|| {
        VaultError::new(format!(
            "the token kept for vault '{}' was stored before purlis pinned the 1Password item it \
             reads, and the item it would read now ('{}') is the one the committed vaults.json \
             names, so purlis will not use it for that item. Give the token again from the \
             vault's tab in the app, which shows the settings it will pin.",
            vault.name,
            crate::personas::one_line(&op_item_of(vault))
        ))
    })
}

/// This machine's record of `vault` where it is honoured: it matches the vault's effective
/// binding, and either pins the item or reads an item this machine chose
/// ([`unpinned_item_refusal`]).
fn honoured(ctx: &Ctx, vault: &Vault) -> Option<Map<String, Value>> {
    record(ctx, vault).filter(|rec| {
        record_matches(rec, vault)
            && (rec.contains_key("op_item") || !item_chosen_by_commit(ctx, vault))
    })
}

/// Whether this machine's registry marks `vault`'s identity as moved AND the pinned binding still
/// matches, under a base charter knows (a record naming another fails every read, so it is not
/// shown as held either). Reads no keyring.
pub fn in_keyring(ctx: &Ctx, vault: &Vault) -> bool {
    honoured(ctx, vault).is_some_and(|rec| base_of(&rec, vault).is_ok())
}

/// The item `source`'s token lives under for record item `id`, under the old base and under the
/// purlis one: `(charter/@identity/<id>, purlis/@identity/<id>)`.
pub fn item_services(id: &str) -> (String, String) {
    (
        format!("{}/{id}", READ_BASES[1].1),
        format!("{}/{id}", READ_BASES[0].1),
    )
}

/// Every record in this machine's half still read under the old base (it has no [`BASE`]), by
/// vault name, with its items' ids by source: what the keychain copy (RN-6) copies. Reads no
/// keyring, and a half that cannot be read has none.
pub fn on_the_old_base(ctx: &Ctx) -> Vec<(String, BTreeMap<String, String>)> {
    let Ok(local) = registry::load_local(ctx) else {
        return Vec::new();
    };
    registry::usable_vaults(&local)
        .iter()
        .filter_map(|(name, entry)| {
            let rec = entry.get("config")?.get(MARK)?.as_object()?;
            if rec.get("held").and_then(Value::as_str) != Some(IN_KEYRING) || rec.contains_key(BASE)
            {
                return None;
            }
            Some((name.clone(), ids_of(rec)?))
        })
        .collect()
}

/// Every record in this machine's half read under the purlis base, by vault name, with its
/// items' ids by source: what the app holds again at its first launch under a new identity
/// (RN-9). Reads no keyring.
pub fn on_the_purlis_base(ctx: &Ctx) -> Vec<(String, BTreeMap<String, String>)> {
    let Ok(local) = registry::load_local(ctx) else {
        return Vec::new();
    };
    registry::usable_vaults(&local)
        .iter()
        .filter_map(|(name, entry)| {
            let rec = entry.get("config")?.get(MARK)?.as_object()?;
            if rec.get("held").and_then(Value::as_str) != Some(IN_KEYRING)
                || rec.get(BASE).and_then(Value::as_str) != Some(BASE_NOW)
            {
                return None;
            }
            Some((name.clone(), ids_of(rec)?))
        })
        .collect()
}

/// A record's `ids`, when every one is a string that can end a service.
fn ids_of(rec: &Map<String, Value>) -> Option<BTreeMap<String, String>> {
    rec.get("ids")?
        .as_object()?
        .iter()
        .map(|(source, id)| {
            let id = id.as_str()?;
            (!id.is_empty() && !id.contains('/') && !id.chars().any(char::is_control))
                .then(|| (source.clone(), id.to_owned()))
        })
        .collect()
}

/// Switch the record of `vault` in this machine's half to the purlis base (`to_purlis`), or
/// back to the old one, and change nothing else in it: `false`, with the half untouched, unless
/// the record is still the one with exactly `ids` on the other base. The keychain copy's switch,
/// made only after it verified every item (RN-6), and its undo's.
pub fn switch_base(
    ctx: &Ctx,
    vault: &str,
    ids: &BTreeMap<String, String>,
    to_purlis: bool,
) -> Result<bool, VaultError> {
    let mut local = registry::load_local(ctx)?;
    let Some(rec) = local
        .get_mut("vaults")
        .and_then(Value::as_object_mut)
        .and_then(|vaults| vaults.get_mut(vault))
        .and_then(|entry| entry.get_mut("config"))
        .and_then(|config| config.get_mut(MARK))
        .and_then(Value::as_object_mut)
    else {
        return Ok(false);
    };
    let on_purlis = rec.get(BASE).and_then(Value::as_str) == Some(BASE_NOW);
    let on_old = !rec.contains_key(BASE);
    let same = rec.get("held").and_then(Value::as_str) == Some(IN_KEYRING)
        && ids_of(rec).as_ref() == Some(ids);
    if !same || (to_purlis && !on_old) || (!to_purlis && !on_purlis) {
        return Ok(false);
    }
    if to_purlis {
        rec.insert(BASE.into(), Value::String(BASE_NOW.into()));
    } else {
        rec.shift_remove(BASE);
    }
    registry::save_local(ctx, &local)?;
    Ok(true)
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
            } else if set_here(ctx, &source).is_some() {
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
    if let Some(refused) = unpinned_item_refusal(ctx, vault) {
        return Err(refused);
    }
    let Some(rec) = honoured(ctx, vault) else {
        return Ok(None);
    };
    if !rec.contains_key("op_item") {
        pin_the_item(ctx, vault);
    }
    let Some(id) = rec
        .get("ids")
        .and_then(Value::as_object)
        .and_then(|m| m.get(source))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let base = base_of(&rec, vault)?;
    Ok(keyring::store(ctx)
        .get(&format!("{base}/{id}"), source)?
        .map(keyring::Secret::into_inner)
        .filter(|v| !v.is_empty()))
}

/// **Upgrade a record made before #1527, which names no item, to pin the one it is read with
/// now** (#1542), so a later commit that changes `op-item` unpins it as it does a record made
/// since. **Only for an item this machine chose** (its half's own `op-item`, or the default): a
/// record whose vault reads an item the committed half chose is refused instead
/// ([`unpinned_item_refusal`]), since a committed entry never changes a record. Asked at a read
/// through the record, which it already honours for that item: pinning changes nothing about
/// this read. Best effort and quiet: a half that cannot be written now (a sandboxed chat's
/// read, a read-only file) is upgraded at a later read.
///
/// The half is read again just before it is written, and the record is changed only while it
/// is still the one honoured here, with no item: one a store replaced in between is left alone.
fn pin_the_item(ctx: &Ctx, vault: &Vault) {
    let Ok(mut local) = registry::load_local(ctx) else {
        return;
    };
    let Some(rec) = local
        .get_mut("vaults")
        .and_then(Value::as_object_mut)
        .and_then(|vaults| vaults.get_mut(&vault.name))
        .and_then(|entry| entry.get_mut("config"))
        .and_then(|config| config.get_mut(MARK))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let still = rec.get("held").and_then(Value::as_str) == Some(IN_KEYRING)
        && !rec.contains_key("op_item")
        && record_matches(rec, vault)
        && !item_chosen_by_commit(ctx, vault);
    if !still {
        return;
    }
    rec.insert("op_item".into(), Value::String(op_item_of(vault)));
    if let Err(why) = registry::save_local(ctx, &local) {
        tracing::debug!(
            "purlis: the record of vault '{}' was not upgraded to pin its item ({})",
            crate::personas::one_line(&vault.name),
            why.message
        );
    }
}

/// The absolute `op` a keyring-held read must run, verified against the pinned path and Team id;
/// `None` for a vault whose identity is not keyring-held, whose `op` is then resolved from PATH
/// as before (no keyring item is at stake). An error when the pin is missing or fails to verify —
/// charter never falls back to the caller's PATH for a keyring-held identity (#271 review, U1).
pub fn pinned_op(ctx: &Ctx, vault: &Vault) -> Result<Option<PathBuf>, VaultError> {
    let Some(rec) = honoured(ctx, vault) else {
        return Ok(None);
    };
    let refuse = |why: &str| {
        VaultError::new(format!(
            "vault '{}' reads a token purlis moved into {}, so it will only run the `op` that \
             was pinned when the token was stored — and {why}. Put the token in again from the \
             vault's tab, in a purlis started from a shell where the real `op` is on PATH.",
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

/// The keyring service a new item `id` is written under.
fn item_service(id: &str) -> String {
    format!("{SERVICE_BASE}/{id}")
}

/// A random id for a new identity item.
fn new_id() -> Result<String, VaultError> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes)
        .map_err(|e| VaultError::new(format!("purlis could not make a random name: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// `op` as the one lookup finds it now ([`Ctx::program`]), absolute, with its Team id — the
/// trusted resolution recorded at move/put time. `("", "")` when the lookup runs none here,
/// because there is none or because the only one is where a chat may write: the record is
/// still written (the token is stored), and [`pinned_op`] then refuses every read until the
/// token is put again where a real `op` is found, rather than resolving it from the caller's
/// PATH. Best effort here, strict there.
pub(super) fn resolve_op_now(ctx: &Ctx) -> (String, String) {
    match ctx.program("op") {
        // Where it was found, not the file behind an installer's link: the link keeps its
        // name across an upgrade. The lookup has already refused one a chat may write.
        Ok(found) => {
            let team = op_team_id(&found.found).unwrap_or_default();
            (found.found.display().to_string(), team)
        }
        Err(_) => (String::new(), String::new()),
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

/// Tokens stored as new items: the ids by source, for a record, and the items as `(service,
/// account)`, to delete again if the record is never written.
pub(super) struct Stashed {
    pub ids: BTreeMap<String, String>,
    pub items: Vec<(String, String)>,
}

/// Store each of `tokens` (`(source, token)`) as a new item. A keyring that fails part way has
/// the items stored so far deleted again.
pub(super) fn stash(ctx: &Ctx, tokens: &[(String, String)]) -> Result<Stashed, VaultError> {
    let mut ids = BTreeMap::new();
    let mut items: Vec<(String, String)> = Vec::new();
    for (source, token) in tokens {
        match store_token(ctx, source, token) {
            Ok(id) => {
                items.push((item_service(&id), source.clone()));
                ids.insert(source.clone(), id);
            }
            Err(why) => {
                forget(ctx, &items);
                return Err(why);
            }
        }
    }
    Ok(Stashed { ids, items })
}

/// The pinned record of `vault` for the items `ids`: the binding as the vault has it now, the
/// item it keeps its secrets in, and the program found now.
pub(super) fn record_of(
    vault: &Vault,
    ids: &BTreeMap<String, String>,
    op_cmd: &str,
    op_team: &str,
) -> Map<String, Value> {
    let (env, op_vault, account) = fingerprint(vault);
    let mut rec = Map::new();
    rec.insert("held".into(), Value::String(IN_KEYRING.into()));
    rec.insert(BASE.into(), Value::String(BASE_NOW.into()));
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
    rec.insert("op_item".into(), Value::String(op_item_of(vault)));
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
    rec
}

/// Record in the event log that a token of `vault` was stored. Named, never valued.
pub(super) fn traced(ctx: &Ctx, vault: &Vault) -> Vec<String> {
    let sources: Vec<String> = fingerprint(vault).0.into_values().collect();
    let listed: Vec<Value> = sources.iter().map(|s| Value::String(s.clone())).collect();
    super::cmd::trace_secret_use(
        ctx,
        "identity-move",
        &[],
        &[
            ("vault", Value::String(vault.name.clone())),
            ("variables", Value::Array(listed)),
        ],
    );
    sources
}

/// Write the pinned record into this machine's half, beside whatever else it says of the vault.
fn write_record(
    ctx: &Ctx,
    vault: &Vault,
    ids: BTreeMap<String, String>,
    op_cmd: &str,
    op_team: &str,
) -> Result<Vec<String>, VaultError> {
    let rec = record_of(vault, &ids, op_cmd, op_team);
    let mut local = registry::load_local(ctx)?;
    let vaults = object_at(&mut local, "vaults");
    let entry = object_at(vaults, &vault.name);
    object_at(entry, "config").insert(MARK.into(), Value::Object(rec));
    registry::save_local(ctx, &local)?;
    Ok(traced(ctx, vault))
}

/// The object under `key` in `map`, made — or put in place of whatever else a hand-edited file
/// held there.
pub(super) fn object_at<'a>(
    map: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
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
pub(super) fn sole_source(vault: &Vault) -> Result<String, VaultError> {
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
            "vault '{}' is read through more than one identity variable ({}), and the box stores \
             one token. Start purlis from a shell that exports them, then move them from this \
             vault's tab.",
            vault.name,
            many.join(", ")
        ))),
    }
}

/// The other vaults of the project read through one of `vault`'s identity variables whose token
/// is found nowhere, by name, sorted: what the tab points at after a store (#1526). **Names
/// only, and nothing is written for them**: each takes its token from its own tab. Empty for a
/// vault that declares no identity, and for a registry that cannot be read. Reads no keyring.
pub fn unset_alike(ctx: &Ctx, vault: &Vault) -> Vec<String> {
    let mine: Vec<String> = bindings(vault).into_iter().map(|(_, s)| s).collect();
    if mine.is_empty() {
        return Vec::new();
    }
    let Ok(doc) = registry::load_registry(ctx) else {
        return Vec::new();
    };
    let mut names: Vec<String> = registry::vaults(&doc).keys().cloned().collect();
    names.sort();
    names
        .into_iter()
        .filter(|name| *name != vault.name)
        .filter(|name| {
            registry::vault_in(&doc, name).is_ok_and(|other| {
                held(ctx, &other)
                    .iter()
                    .any(|b| b.held == Held::Unset && mine.contains(&b.source))
            })
        })
        .collect()
}

/// The keyring items this machine's record of `vault` names now, as `(service, account)`:
/// what a new record replaces. Read from the local half whether or not the record still
/// matches the vault's binding, since an item nothing honours is still a token in the keyring.
pub(super) fn items_recorded(ctx: &Ctx, vault: &Vault) -> Vec<(String, String)> {
    let Some(rec) = record(ctx, vault) else {
        return Vec::new();
    };
    let (Ok(base), Some(ids)) = (base_of(&rec, vault), ids_of(&rec)) else {
        return Vec::new();
    };
    ids.into_iter()
        .map(|(source, id)| (format!("{base}/{id}"), source))
        .collect()
}

/// Delete `items` from the keyring, best effort, and answer how many could not be deleted. A
/// failure never refuses the store that asked: the new token is already in place.
pub(super) fn forget(ctx: &Ctx, items: &[(String, String)]) -> usize {
    let store = keyring::store(ctx);
    items
        .iter()
        .filter(|(service, account)| store.delete(service, account).is_err())
        .count()
}

/// Keep `tokens` (`(source, token)`) for `vault`, and for no other vault: an item for each,
/// then the pinned record, then the items of the record it replaces are deleted (#1526).
///
/// **All or nothing.** A keyring that fails part way, or a record that cannot be saved, has the
/// items stored so far deleted again and the error answered: the record is the one that was
/// there, and no token is left that nothing refers to.
pub(super) fn keep(
    ctx: &Ctx,
    vault: &Vault,
    tokens: &[(String, String)],
) -> Result<Vec<String>, VaultError> {
    let (op_cmd, op_team) = resolve_op_now(ctx);
    let before = items_recorded(ctx, vault);
    let mut ids = BTreeMap::new();
    let mut stored: Vec<(String, String)> = Vec::new();
    let undo = |stored: &[(String, String)], why: VaultError| {
        if forget(ctx, stored) > 0 {
            return VaultError::new(format!(
                "{} A token purlis had just stored could not be removed again from {}.",
                why.message,
                keyring::STORE_NAME
            ));
        }
        why
    };
    for (source, token) in tokens {
        match store_token(ctx, source, token) {
            Ok(id) => {
                stored.push((item_service(&id), source.clone()));
                ids.insert(source.clone(), id);
            }
            Err(why) => return Err(undo(&stored, why)),
        }
    }
    let sources =
        write_record(ctx, vault, ids, &op_cmd, &op_team).map_err(|why| undo(&stored, why))?;
    // The new token is in place whatever happens to the old one, so this never refuses.
    let left = forget(ctx, &before);
    if left > 0 {
        tracing::warn!(
            "purlis: {left} replaced identity item(s) of vault '{}' could not be deleted from {}",
            crate::personas::one_line(&vault.name),
            keyring::STORE_NAME
        );
    }
    Ok(sources)
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
    keep(ctx, vault, &[(source, token.to_owned())])
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
        if kept(source) {
            return Err(VaultError::new(format!(
                "vault '{}' keeps its token in {} and reads it through no variable, so there is \
                 none to move. Paste the token into the vault's tab.",
                vault.name,
                keyring::STORE_NAME
            )));
        }
        match set_here(ctx, source) {
            Some(token) => tokens.push((source.clone(), token)),
            None => {
                return Err(VaultError::new(format!(
                    "${source} is not set where purlis is running, so there is no token of \
                     vault '{}' to move. Start purlis from a shell that exports it, then move \
                     it — or paste it into the vault's tab instead.",
                    vault.name
                )));
            }
        }
    }
    keep(ctx, vault, &tokens)
}

/// Whether charter's OWN environment still carries any identity variable of `vault` — the reason
/// the move path warns the operator to relaunch. On macOS and Linux a same-user process can read
/// another's environment block, so a token left in the app's environment is reachable from a chat
/// however the in-process value is scrubbed (#271 review, U3).
pub fn app_env_holds_a_token(ctx: &Ctx, vault: &Vault) -> Vec<String> {
    bindings(vault)
        .into_iter()
        .map(|(_, source)| source)
        .filter(|source| set_here(ctx, source).is_some())
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

/// macOS only: off macOS there is nothing here to test.
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// macOS only: off macOS `op_team_id` is `None` by construction, and there is no `codesign`.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_binary_signed_by_no_team_or_not_signed_at_all_has_no_team() {
        // Apple's own binaries are signed with `TeamIdentifier=not set`.
        assert_eq!(op_team_id(Path::new("/usr/bin/true")), None);
        let bin = tempfile::tempdir().unwrap();
        let op = stand_in::program(bin.path(), "op", "#!/bin/sh\n");
        assert_eq!(op_team_id(&op), None);
    }
}
