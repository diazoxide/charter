//! The keyring provider — the operating system's own credential store (ADR 0047): the macOS
//! Keychain, the Secret Service on Linux, and the Windows Credential Manager, through the
//! standard `keyring` crate.
//!
//! **One item per secret**, and one index file beside the registry that names them:
//!
//! ```text
//! charter vault 'ops', key 'API_TOKEN'
//!     → keyring item   service  charter/ops/3f9a2c1b   (made once per vault, random)
//!                      account  API_TOKEN
//!     → keys index     .charter/vaults/ops.keys.json:
//!                      {"service": "charter/ops/3f9a2c1b",
//!                       "keys": {"API_TOKEN": {"size": "16–31 bytes",
//!                                              "updated": "2026-09-24T11:32:17Z"}}}
//! ```
//!
//! **The index is how a keyring vault is listed**, because a keyring cannot be asked what it
//! holds — the `keyring` crate reads, writes and deletes one named entry and nothing else. It
//! holds each key's name, its size BAND and when it was last written, and never a value. It is
//! written after every write that succeeded, 0600, under the state directory — which is
//! gitignored, so nothing here lands in a committed path.
//!
//! **The service is random per vault** (`charter/<vault>/<8 hex>`), made at the vault's first
//! write and kept in its index. A vault name is only unique within one plane, and the keyring is
//! the machine's: two planes each with a vault called `ops` would otherwise read, overwrite and
//! delete each other's items while each index claimed its own. A service the index names is
//! refused unless it is one of charter's (`charter/…`): the index is a file on disk, and one
//! pointing at another program's item would make charter a deputy for reading it.
//!
//! **Which store a build talks to** is decided once, here ([`store`]): a fenced build — every
//! test build, and the app's `e2e` build (`crate::fence`) — keeps values in
//! `<state>/keyring-stub.json` (the state directory: `.charter/`, or `$CHARTER_HOME`) and never
//! reaches the operating system's store. So no test in this repository can read or write the
//! operator's login keychain, however it is written.
//!
//! **On macOS every item is held to charter's app** (ruling V90a, [`super::keyhold`]): written
//! by the app's own binary, so the Keychain's default rule lets that binary alone read it
//! without asking, and any other program, the `charter` command and every program a chat runs
//! included, is refused or makes the Keychain ask the person. The index marks each key whose
//! item was written that way (`held`). An item written before is written again the same way
//! the next time charter reads it (one the `charter` command made, the next time the command
//! does), and the vault's next read through the command says so once (ruling V90d).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Map, Value};

use super::registry::Vault;
use super::{Ctx, VaultError, fingerprint};

/// Every service charter writes starts with this, and every service it reads must.
pub const SERVICE_PREFIX: &str = "charter/";

/// The file a fenced build keeps its keyring in, under the state directory. Plaintext, and only
/// ever written by a build that a test or a scenario run made.
pub const STUB_FILE: &str = "keyring-stub.json";

/// How the command line names the store, the same words on every platform: what charter prints
/// is recorded, and the recording is replayed on macOS and Linux alike.
pub const STORE_NAME: &str = "the system keyring";

/// A value read out of a store. Its `Debug` never prints it.
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// What a write left an item under (ruling V90a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// Held to charter's app: any other program is refused or makes the store ask the person.
    ToTheApp,
    /// This store keeps no rule charter sets: the test stub, the Secret Service and the
    /// Credential Manager.
    NoRule,
    /// This store has a rule and the write could not set it, so the item has the access the
    /// writing program gave it. The next read tries again.
    NotYet,
}

/// A credential store: one entry per `(service, account)`, read, written and deleted by name.
///
/// **An error never carries a value**, and never an entry the store returned: an
/// implementation says which operation failed and the store's own reason, which names a
/// failure and not a credential.
pub trait Store: Send + Sync {
    /// The entry, or `None` when the store PROVABLY has none.
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError>;
    /// Create or replace the entry, held to charter's app where the store can hold it.
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError>;
    /// Delete the entry. `false` when there was none.
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError>;
    /// Whether this store holds an item to charter's app, so an item written before it did is
    /// written again when read (ruling V90d).
    fn holds(&self) -> bool {
        false
    }
    /// The entry's value, unchanged, written again held to charter's app (ruling V90d). Where it
    /// cannot be held, a store may leave the item as it is rather than write it in place.
    fn rehold(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.set(service, account, value)
    }
}

/// The store this build talks to. See the module header: a fenced build never reaches the
/// operating system's.
pub fn store(ctx: &Ctx) -> Box<dyn Store> {
    if crate::fence::FENCED {
        Box::new(FileStore::at(ctx.state.join(STUB_FILE)))
    } else {
        Box::new(OsStore)
    }
}

// ---------------------------------------------------------------------------------------
// The operating system's store.

/// The platform's own store, through `keyring`'s one-entry API: the login keychain on macOS,
/// the Secret Service on Linux, the Credential Manager on Windows.
///
/// **On macOS an item is held to charter's app** (ruling V90a): it is written by the app's own
/// binary ([`super::keyhold::set`]), and the Keychain's default rule lets the program that
/// created an item read it without asking. Any other program — `security
/// find-generic-password -w`, the `charter` command, a script a chat runs — is refused or
/// makes the Keychain ask the person first. ADR 0047 records what was measured and what that
/// does and does not protect.
pub struct OsStore;

impl std::fmt::Debug for OsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OsStore")
    }
}

impl OsStore {
    fn entry(service: &str, account: &str) -> Result<::keyring::Entry, VaultError> {
        ::keyring::Entry::new(service, account).map_err(|e| failure("reach", service, &e))
    }
}

impl Store for OsStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        match Self::entry(service, account)?.get_password() {
            Ok(value) => Ok(Some(Secret(value))),
            Err(::keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(failure("read", service, &e)),
        }
    }

    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        if cfg!(target_os = "macos") {
            return super::keyhold::set(service, account, value, super::keyhold::Why::Write);
        }
        set_here(service, account, value)?;
        Ok(Held::NoRule)
    }

    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        match Self::entry(service, account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(::keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(failure("delete", service, &e)),
        }
    }

    fn holds(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn rehold(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        if cfg!(target_os = "macos") {
            return super::keyhold::set(service, account, value, super::keyhold::Why::Move);
        }
        Ok(Held::NoRule)
    }
}

/// Create or replace the entry from this process, with whatever access the store gives an
/// item this program writes (on macOS: this program alone reads a new one without asking, and
/// a replaced one keeps the access it had).
pub(super) fn set_here(service: &str, account: &str, value: &str) -> Result<(), VaultError> {
    OsStore::entry(service, account)?
        .set_password(value)
        .map_err(|e| failure("write", service, &e))
}

/// A keyring failure as charter says it: the operation, the service, and the store's reason.
///
/// **Never `e`'s `Display` wholesale.** `BadEncoding` carries the bytes the store returned —
/// the credential itself — and prints nothing of them only by the crate's grace; it is named
/// here and not formatted. The other kinds carry the platform's own error, which names a
/// failure ("User interaction is not allowed.") and not a value.
pub(super) fn failure(what: &str, service: &str, e: &::keyring::Error) -> VaultError {
    let why = match e {
        ::keyring::Error::BadEncoding(_) => "the stored entry is not UTF-8 text".to_string(),
        ::keyring::Error::NoStorageAccess(inner) | ::keyring::Error::PlatformFailure(inner) => {
            inner.to_string()
        }
        ::keyring::Error::NoDefaultStore => "this platform has no keyring charter can use".into(),
        ::keyring::Error::TooLong(name, _) => format!("the {name} is longer than the store allows"),
        ::keyring::Error::Invalid(name, reason) => format!("the {name} {reason}"),
        _ => "the store refused it".into(),
    };
    VaultError::new(format!(
        "charter could not {what} '{service}' in {STORE_NAME}: {why}"
    ))
}

// ---------------------------------------------------------------------------------------
// The stub a fenced build keeps.

/// A keyring in one 0600 JSON file, for fenced builds only: `{"<service>\n<account>": value}`.
pub struct FileStore {
    path: PathBuf,
}

impl std::fmt::Debug for FileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileStore")
            .field("path", &self.path)
            .finish()
    }
}

impl FileStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    fn slot(service: &str, account: &str) -> String {
        format!("{service}\n{account}")
    }

    fn load(&self) -> Result<Map<String, Value>, VaultError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(map)) => Ok(map),
                // Named by path and never quoted: the file holds values.
                _ => Err(VaultError::new(format!(
                    "the test keyring {} is not a JSON object",
                    self.path.display()
                ))),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
            Err(e) => Err(VaultError::new(format!(
                "the test keyring {} cannot be read: {e}",
                self.path.display()
            ))),
        }
    }

    fn save(&self, map: Map<String, Value>) -> Result<(), VaultError> {
        super::plain_file::write_private(&self.path, &Value::Object(map))
    }
}

impl Store for FileStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        Ok(self
            .load()?
            .get(&Self::slot(service, account))
            .and_then(Value::as_str)
            .map(|v| Secret(v.to_owned())))
    }

    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        let mut map = self.load()?;
        map.insert(
            Self::slot(service, account),
            Value::String(value.to_owned()),
        );
        self.save(map)?;
        Ok(Held::NoRule)
    }

    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        let mut map = self.load()?;
        if map.shift_remove(&Self::slot(service, account)).is_none() {
            return Ok(false);
        }
        self.save(map)?;
        Ok(true)
    }
}

// ---------------------------------------------------------------------------------------
// The keys index.

/// What the index says about one key. Never its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub key: String,
    /// The value's size band (`fingerprint::size_band`), never its length.
    pub size: String,
    /// When it was last written, as RFC 3339 UTC to the second.
    pub updated: String,
}

/// What the index records about one key: its size band, when it was last written, and whether
/// its item was written held to charter's app (ruling V90a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub size: String,
    pub updated: String,
    pub held: bool,
}

/// A vault's index: the service its items live under, what each key is, and where the vault's
/// one note that its items were moved under charter's access rule stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Index {
    pub service: Option<String>,
    pub keys: BTreeMap<String, Entry>,
    pub note: Note,
}

/// The one note a vault gets when charter moves its items under its access rule (ruling V90d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Note {
    /// No item was moved yet.
    #[default]
    None,
    /// An item was moved, and the note is still to be said.
    Due,
    /// It was said, and never is again.
    Said,
}

impl Note {
    fn word(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Due => Some("due"),
            Self::Said => Some("said"),
        }
    }

    fn of(word: Option<&str>) -> Self {
        match word {
            Some("due") => Self::Due,
            Some("said") => Self::Said,
            _ => Self::None,
        }
    }
}

/// The index's field saying where the vault's one note about its items' access rule stands.
const NOTE: &str = "held_note";

/// Where a keyring vault's index lives: beside the local registry, in the state directory.
pub fn index_path(ctx: &Ctx, vault: &Vault) -> PathBuf {
    ctx.vaults_dir().join(format!("{}.keys.json", vault.name))
}

/// The vault's index. A missing file is a vault nothing has been written to.
pub fn load_index(ctx: &Ctx, vault: &Vault) -> Result<Index, VaultError> {
    let p = index_path(ctx, vault);
    // Never through a link, on the way from the plane or at the index (#440).
    let text = match crate::contain::read_text_no_link(ctx.trust(), &p) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index::default()),
        Err(e) => {
            return Err(VaultError::new(format!(
                "the keys index {} cannot be read: {e}",
                p.display()
            )));
        }
    };
    let corrupt = |why: &str| {
        VaultError::new(format!(
            "the keys index {} is corrupt: {why}",
            super::short_path(&ctx.root, &p)
        ))
    };
    let doc: Value =
        serde_json::from_str(&text).map_err(|e| corrupt(&super::registry::py_json_error(&e)))?;
    let service = match doc.get("service") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if service_ok_for(s, &vault.name) => Some(s.clone()),
        Some(_) => {
            return Err(corrupt(&format!(
                "its service is not one charter writes for vault '{}' (they are \
                 '{SERVICE_PREFIX}{}/<id>'), and charter will not read a keyring item this vault \
                 does not own",
                vault.name, vault.name
            )));
        }
    };
    let mut keys = BTreeMap::new();
    for (k, v) in doc
        .get("keys")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let field = |name: &str| {
            v.get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        keys.insert(
            k.clone(),
            Entry {
                size: field("size"),
                updated: field("updated"),
                held: v.get("held").and_then(Value::as_bool).unwrap_or(false),
            },
        );
    }
    let note = Note::of(doc.get(NOTE).and_then(Value::as_str));
    Ok(Index {
        service,
        keys,
        note,
    })
}

fn save_index(ctx: &Ctx, vault: &Vault, index: &Index) -> Result<(), VaultError> {
    let mut keys = Map::new();
    for (k, e) in &index.keys {
        let mut entry = Map::new();
        entry.insert("size".into(), Value::String(e.size.clone()));
        entry.insert("updated".into(), Value::String(e.updated.clone()));
        // Written only when true, so a vault in a store with no rule reads as it always has.
        if e.held {
            entry.insert("held".into(), Value::Bool(true));
        }
        keys.insert(k.clone(), Value::Object(entry));
    }
    let mut doc = Map::new();
    doc.insert(
        "service".into(),
        index.service.clone().map_or(Value::Null, Value::String),
    );
    doc.insert("keys".into(), Value::Object(keys));
    if let Some(word) = index.note.word() {
        doc.insert(NOTE.into(), Value::String(word.into()));
    }
    let p = index_path(ctx, vault);
    // The index lives in the state directory, so it is gated from the plane (#440): a
    // `.charter/` that is itself a link is refused, not written through.
    crate::contain::no_link_on_the_way(ctx.trust(), &p)
        .map_err(|e| VaultError::new(format!("cannot write {}: {e}", p.display())))?;
    super::plain_file::write_private(&p, &Value::Object(doc))
}

/// Whether `service` is one charter wrote for THIS vault: `charter/<vault>/<id>`, and nothing
/// else. Scoped to the vault on purpose — a prefix-only check let an index name
/// `charter/identity` or another vault's `charter/<other>/<hex>`, so a keyring vault could be
/// pointed at an item it must never read as a secret (#271 review, U4). The `<id>` is the
/// random tail [`new_service`] makes: one path segment, no control character.
pub fn service_ok_for(service: &str, vault: &str) -> bool {
    let want = format!("{SERVICE_PREFIX}{vault}/");
    match service.strip_prefix(&want) {
        Some(tail) => {
            !tail.is_empty() && !tail.contains('/') && !tail.chars().any(char::is_control)
        }
        None => false,
    }
}

/// Whether `key` can name a keyring item: not empty, and no control character.
fn key_ok(key: &str) -> bool {
    !key.is_empty() && !key.chars().any(char::is_control)
}

fn check_key(key: &str) -> Result<(), VaultError> {
    if key_ok(key) {
        Ok(())
    } else {
        Err(VaultError::new(
            "a keyring secret needs a key name that is not empty and holds no control character",
        ))
    }
}

/// A new service for `vault`: `charter/<vault>/<8 hex>`.
fn new_service(vault: &Vault) -> Result<String, VaultError> {
    let mut bytes = [0u8; 4];
    getrandom::fill(&mut bytes)
        .map_err(|e| VaultError::new(format!("charter could not make a random name: {e}")))?;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("{SERVICE_PREFIX}{}/{hex}", vault.name))
}

// ---------------------------------------------------------------------------------------
// The provider.

/// `keys`: the names the index holds, sorted.
pub fn keys(ctx: &Ctx, vault: &Vault) -> Result<Vec<String>, VaultError> {
    Ok(load_index(ctx, vault)?.keys.into_keys().collect())
}

/// Every key with its size band and when it was written — what a list of the vault shows.
/// Read from the index alone: nothing here touches the store, so nothing here can prompt.
pub fn listed(ctx: &Ctx, vault: &Vault) -> Result<Vec<Listed>, VaultError> {
    Ok(load_index(ctx, vault)?
        .keys
        .into_iter()
        .map(|(key, e)| Listed {
            key,
            size: e.size,
            updated: e.updated,
        })
        .collect())
}

fn not_found(vault: &Vault, key: &str) -> VaultError {
    VaultError::not_found(format!(
        "secret '{key}' not found in vault '{}'",
        vault.name
    ))
}

/// `get`: the value of `key`, read from the store under the vault's service.
pub fn get(ctx: &Ctx, vault: &Vault, key: &str) -> Result<String, VaultError> {
    get_with(&*store(ctx), ctx, vault, key)
}

/// [`get`] against a store the caller chose.
pub fn get_with(
    store: &dyn Store,
    ctx: &Ctx,
    vault: &Vault,
    key: &str,
) -> Result<String, VaultError> {
    check_key(key)?;
    let index = load_index(ctx, vault)?;
    let Some(service) = index.service.clone() else {
        return Err(not_found(vault, key));
    };
    let value = store
        .get(&service, key)?
        .map(Secret::into_inner)
        .ok_or_else(|| not_found(vault, key))?;
    if store.holds()
        && let Some(read) = index.keys.get(key).filter(|entry| !entry.held)
    {
        rehold(store, ctx, vault, &service, key, &value, &read.updated);
    }
    Ok(value)
}

/// Ruling V90d: an item written before the store held it is written again, held, on its first
/// read. Best-effort: the read already succeeded, and a failure tries again next time.
///
/// **Never over a newer write.** A `secret set` of the key between the read and now would be
/// undone by writing the value read, so the index is read again first and the move is made only
/// while the key's `updated` is still `read_at`; afterwards only `held` and the note change, and
/// only while it still is. A write during the move itself is not locked out (#1180).
fn rehold(
    store: &dyn Store,
    ctx: &Ctx,
    vault: &Vault,
    service: &str,
    key: &str,
    value: &str,
    read_at: &str,
) {
    let unchanged = |index: &Index| {
        index.service.as_deref() == Some(service)
            && index
                .keys
                .get(key)
                .is_some_and(|entry| !entry.held && entry.updated == read_at)
    };
    if !load_index(ctx, vault).is_ok_and(|index| unchanged(&index)) {
        return;
    }
    if store.rehold(service, key, value) != Ok(Held::ToTheApp) {
        return;
    }
    // A newer write that landed during the move is its own: it is marked as it was written,
    // never as held by this move.
    let Ok(mut index) = load_index(ctx, vault) else {
        return;
    };
    match index.keys.get_mut(key) {
        Some(entry) if entry.updated == read_at => entry.held = true,
        _ => return,
    }
    if index.note == Note::None {
        index.note = Note::Due;
    }
    let _ = save_index(ctx, vault, &index);
}

/// Whether `key`'s item was just written where its store holds items to charter's app and
/// could not be held this time ([`Held::NotYet`]), as its index says.
pub fn left_unheld(ctx: &Ctx, vault: &Vault, key: &str) -> bool {
    left_unheld_in(&*store(ctx), ctx, vault, key)
}

/// [`left_unheld`] against a store the caller chose.
pub fn left_unheld_in(store: &dyn Store, ctx: &Ctx, vault: &Vault, key: &str) -> bool {
    store.holds()
        && load_index(ctx, vault)
            .ok()
            .and_then(|index| index.keys.get(key).map(|entry| !entry.held))
            .unwrap_or(false)
}

/// What `secret set` says when the item it wrote could not be held to charter's app.
pub fn unheld_sentence(key: &str) -> String {
    format!(
        "charter could not hold '{key}' to its app, so the program that wrote it reads it \
         without asking you. charter tries again the next time the charter command reads it."
    )
}

/// The one note that this vault's items were moved under charter's access rule (ruling V90d),
/// once: `None` after it was taken, and until an item was moved.
pub fn take_note(ctx: &Ctx, vault: &Vault) -> Option<String> {
    let mut index = load_index(ctx, vault).ok()?;
    if index.note != Note::Due {
        return None;
    }
    index.note = Note::Said;
    save_index(ctx, vault, &index).ok()?;
    Some(format!(
        "charter moved the secrets of vault '{}' under its access rule in {STORE_NAME}: from \
         now on only charter's app reads them without asking you. Any other program, this \
         command included, makes the system ask you first.",
        vault.name
    ))
}

/// `set`: the value into the store, then its size band and time into the index.
pub fn set(ctx: &Ctx, vault: &Vault, key: &str, value: &str) -> Result<(), VaultError> {
    set_with(&*store(ctx), ctx, vault, key, value, &now())
}

/// [`set`] against a store the caller chose, at a time the caller chose.
pub fn set_with(
    store: &dyn Store,
    ctx: &Ctx,
    vault: &Vault,
    key: &str,
    value: &str,
    at: &str,
) -> Result<(), VaultError> {
    check_key(key)?;
    let mut index = load_index(ctx, vault)?;
    let service = match &index.service {
        Some(s) => s.clone(),
        None => {
            // Kept BEFORE the first item is written: an item under a service no index records
            // is one nothing will ever find again.
            let made = new_service(vault)?;
            index.service = Some(made.clone());
            save_index(ctx, vault, &index)?;
            made
        }
    };
    let held = store.set(&service, key, value)?;
    index.keys.insert(
        key.to_owned(),
        Entry {
            size: fingerprint::size_band(value),
            updated: at.to_owned(),
            held: held == Held::ToTheApp,
        },
    );
    save_index(ctx, vault, &index)
}

/// `delete`: the item, then its line in the index. A key the index does not hold is not
/// found, whatever the store holds under that name.
pub fn delete(ctx: &Ctx, vault: &Vault, key: &str) -> Result<(), VaultError> {
    delete_with(&*store(ctx), ctx, vault, key)
}

/// [`delete`] against a store the caller chose.
pub fn delete_with(
    store: &dyn Store,
    ctx: &Ctx,
    vault: &Vault,
    key: &str,
) -> Result<(), VaultError> {
    check_key(key)?;
    let mut index = load_index(ctx, vault)?;
    let (Some(service), true) = (index.service.clone(), index.keys.contains_key(key)) else {
        return Err(not_found(vault, key));
    };
    store.delete(&service, key)?;
    index.keys.remove(key);
    save_index(ctx, vault, &index)
}

/// `ages`: key → days since it was last written, `None` where the index has no readable time.
pub fn ages(
    ctx: &Ctx,
    vault: &Vault,
    today: chrono::NaiveDate,
) -> Result<Vec<(String, Option<i64>)>, VaultError> {
    Ok(listed(ctx, vault)?
        .into_iter()
        .map(|l| {
            let age = chrono::DateTime::parse_from_rfc3339(&l.updated)
                .ok()
                .map(|t| (today - t.date_naive()).num_days());
            (l.key, age)
        })
        .collect())
}

/// `health`: the count and where the items are, from the index alone — never the store, so
/// `vault list` never makes the Keychain ask the operator anything.
pub fn health(ctx: &Ctx, vault: &Vault) -> (bool, String) {
    match load_index(ctx, vault) {
        Err(e) => (false, e.message),
        Ok(Index { service: None, .. }) => (true, format!("no secrets yet in {STORE_NAME}")),
        Ok(Index {
            service: Some(service),
            keys,
            ..
        }) => (
            true,
            format!(
                "{} secret(s) in {STORE_NAME}, service '{service}'",
                keys.len()
            ),
        ),
    }
}

/// Now, as the index writes it.
fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::Env;

    const VALUE: &str = "kr-unit-4b9d27e1f0a3";

    fn vault(name: &str) -> Vault {
        Vault {
            name: name.into(),
            provider: "keyring".into(),
            persona: None,
            config: Map::new(),
        }
    }

    #[test]
    fn two_planes_with_a_vault_of_one_name_never_read_each_others_items() {
        // The keyring is the machine's and a vault name is only the plane's: one store, two
        // planes, one name.
        let shared = tempfile::tempdir().expect("a directory");
        let store = FileStore::at(shared.path().join("machine-keyring.json"));
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (ctx_a, ctx_b) = (
            Ctx::new(a.path(), Env::of(&[])),
            Ctx::new(b.path(), Env::of(&[])),
        );
        let ops = vault("ops");

        set_with(
            &store,
            &ctx_a,
            &ops,
            "API_TOKEN",
            VALUE,
            "2026-09-24T00:00:00Z",
        )
        .unwrap();
        set_with(
            &store,
            &ctx_b,
            &ops,
            "OTHER",
            "b-value",
            "2026-09-24T00:00:00Z",
        )
        .unwrap();

        assert_eq!(get_with(&store, &ctx_a, &ops, "API_TOKEN").unwrap(), VALUE);
        let err = get_with(&store, &ctx_b, &ops, "API_TOKEN").unwrap_err();
        assert_eq!(err.kind, crate::secrets::Kind::NotFound);
        assert_ne!(
            load_index(&ctx_a, &ops).unwrap().service,
            load_index(&ctx_b, &ops).unwrap().service
        );
    }

    #[test]
    fn a_key_that_could_not_name_an_item_is_refused_before_the_store_is_asked() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx::new(dir.path(), Env::of(&[]));
        let store = FileStore::at(dir.path().join("k.json"));
        for key in ["", "A\nB", "tab\there"] {
            assert!(set_with(&store, &ctx, &vault("ops"), key, VALUE, "t").is_err());
        }
        assert!(!dir.path().join("k.json").exists(), "the store was written");
    }

    #[test]
    fn nothing_that_holds_a_value_prints_it_in_debug() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::at(dir.path().join("k.json"));
        store.set("charter/ops/x", "K", VALUE).unwrap();
        for shown in [
            format!("{:?}", Secret::new(VALUE.into())),
            format!("{store:?}"),
            format!("{:?}", store.get("charter/ops/x", "K").unwrap()),
            format!("{OsStore:?}"),
        ] {
            assert!(!shown.contains(VALUE), "{shown}");
        }
    }

    #[test]
    fn each_store_and_a_value_debug_print_as_what_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.json");
        assert_eq!(format!("{:?}", Secret::new(VALUE.into())), "Secret(***)");
        assert_eq!(format!("{OsStore:?}"), "OsStore");
        assert_eq!(
            format!("{:?}", FileStore::at(path.clone())),
            format!("FileStore {{ path: {path:?} }}")
        );
    }

    #[test]
    fn each_keyring_failure_says_the_operation_the_service_and_the_stores_reason() {
        let said = |e: ::keyring::Error| failure("write", "charter/ops/x", &e).message;
        let head = "charter could not write 'charter/ops/x' in the system keyring: ";
        let platform = || -> Box<dyn std::error::Error + Send + Sync> {
            "User interaction is not allowed.".into()
        };
        for (e, why) in [
            (
                ::keyring::Error::BadEncoding(VALUE.as_bytes().to_vec()),
                "the stored entry is not UTF-8 text",
            ),
            (
                ::keyring::Error::NoStorageAccess(platform()),
                "User interaction is not allowed.",
            ),
            (
                ::keyring::Error::PlatformFailure(platform()),
                "User interaction is not allowed.",
            ),
            (
                ::keyring::Error::NoDefaultStore,
                "this platform has no keyring charter can use",
            ),
            (
                ::keyring::Error::TooLong("service".into(), 255),
                "the service is longer than the store allows",
            ),
            (
                ::keyring::Error::Invalid("account".into(), "is empty".into()),
                "the account is empty",
            ),
            (::keyring::Error::NoEntry, "the store refused it"),
        ] {
            assert_eq!(said(e), format!("{head}{why}"));
        }
    }

    #[test]
    fn a_stub_keyring_that_cannot_be_read_is_an_error_and_never_an_empty_one() {
        // A directory where the file belongs: read fails, and not with "not found".
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::at(dir.path().to_path_buf());
        let err = store.get("charter/ops/x", "K").unwrap_err();
        assert!(err.message.contains("cannot be read"), "{}", err.message);
        // A missing file is an empty keyring.
        let missing = FileStore::at(dir.path().join("none.json"));
        assert!(missing.get("charter/ops/x", "K").unwrap().is_none());
    }

    #[test]
    fn deleting_from_the_stub_keyring_says_whether_there_was_an_entry_and_drops_only_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::at(dir.path().join("k.json"));
        store.set("charter/ops/x", "K", VALUE).unwrap();
        store.set("charter/ops/x", "L", "other").unwrap();
        assert!(store.delete("charter/ops/x", "K").unwrap());
        assert!(!store.delete("charter/ops/x", "K").unwrap());
        assert!(store.get("charter/ops/x", "K").unwrap().is_none());
        assert_eq!(
            store
                .get("charter/ops/x", "L")
                .unwrap()
                .unwrap()
                .into_inner(),
            "other"
        );
    }

    #[test]
    fn a_vault_service_is_charters_for_that_vault_with_one_plain_segment_after_it() {
        assert!(service_ok_for("charter/ops/3f9a2c1b", "ops"));
        for bad in [
            "charter/ops/",
            "charter/ops/a/b",
            "charter/ops/a\nb",
            "charter/other/3f9a2c1b",
            "charter/identity",
            "someone-else/ops/3f9a2c1b",
        ] {
            assert!(!service_ok_for(bad, "ops"), "{bad:?}");
        }
    }

    /// A keyring vault `ops` in a fresh plane.
    fn plane() -> (tempfile::TempDir, Ctx, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx::new(dir.path(), Env::of(&[]));
        (dir, ctx, vault("ops"))
    }

    fn write_index(ctx: &Ctx, v: &Vault, doc: &str) {
        let p = index_path(ctx, v);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, doc).unwrap();
    }

    #[test]
    fn an_index_naming_another_vaults_service_is_refused_as_corrupt() {
        let (_dir, ctx, ops) = plane();
        write_index(
            &ctx,
            &ops,
            r#"{"service": "charter/other/3f9a2c1b", "keys": {}}"#,
        );
        let err = load_index(&ctx, &ops).unwrap_err();
        assert!(err.message.contains("is corrupt"), "{}", err.message);
        let (ok, detail) = health(&ctx, &ops);
        assert!(!ok);
        assert_eq!(detail, err.message);
    }

    #[test]
    fn an_index_that_cannot_be_read_is_an_error_and_never_an_empty_vault() {
        let (_dir, ctx, ops) = plane();
        // A directory where the index belongs.
        std::fs::create_dir_all(index_path(&ctx, &ops)).unwrap();
        let err = load_index(&ctx, &ops).unwrap_err();
        assert!(err.message.contains("cannot be read"), "{}", err.message);
    }

    #[test]
    fn a_keyring_vault_lists_each_key_with_its_size_band_and_when_it_was_written() {
        let (dir, ctx, ops) = plane();
        let store = FileStore::at(dir.path().join("k.json"));
        set_with(&store, &ctx, &ops, "B", VALUE, "2026-09-20T10:00:00Z").unwrap();
        set_with(&store, &ctx, &ops, "A", "x", "2026-09-24T00:00:00Z").unwrap();
        assert_eq!(
            listed(&ctx, &ops).unwrap(),
            [
                Listed {
                    key: "A".into(),
                    size: "1–15 bytes".into(),
                    updated: "2026-09-24T00:00:00Z".into(),
                },
                Listed {
                    key: "B".into(),
                    size: "16–31 bytes".into(),
                    updated: "2026-09-20T10:00:00Z".into(),
                },
            ]
        );
    }

    #[test]
    fn a_keys_age_is_whole_days_since_it_was_written_and_none_when_the_time_is_unreadable() {
        let (dir, ctx, ops) = plane();
        let store = FileStore::at(dir.path().join("k.json"));
        set_with(&store, &ctx, &ops, "OLD", VALUE, "2026-09-20T23:59:59Z").unwrap();
        set_with(&store, &ctx, &ops, "TODAY", VALUE, "2026-09-24T00:00:00Z").unwrap();
        set_with(&store, &ctx, &ops, "UNDATED", VALUE, "not a time").unwrap();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        assert_eq!(
            ages(&ctx, &ops, today).unwrap(),
            [
                ("OLD".to_string(), Some(4)),
                ("TODAY".to_string(), Some(0)),
                ("UNDATED".to_string(), None),
            ]
        );
    }

    #[test]
    fn a_keyring_vaults_health_counts_its_keys_and_names_its_service_from_the_index() {
        let (dir, ctx, ops) = plane();
        assert_eq!(
            health(&ctx, &ops),
            (true, "no secrets yet in the system keyring".to_string())
        );
        let store = FileStore::at(dir.path().join("k.json"));
        set_with(&store, &ctx, &ops, "A", VALUE, "t").unwrap();
        set_with(&store, &ctx, &ops, "B", VALUE, "t").unwrap();
        let service = load_index(&ctx, &ops).unwrap().service.unwrap();
        assert_eq!(
            health(&ctx, &ops),
            (
                true,
                format!("2 secret(s) in the system keyring, service '{service}'")
            )
        );
    }

    #[test]
    fn a_write_through_the_builds_own_store_is_stamped_with_the_time_it_was_made() {
        let (_dir, ctx, ops) = plane();
        let before = chrono::Utc::now().timestamp();
        set(&ctx, &ops, "A", VALUE).unwrap();
        let after = chrono::Utc::now().timestamp();
        let updated = &listed(&ctx, &ops).unwrap()[0].updated;
        assert_eq!(updated.len(), "2026-09-24T11:32:17Z".len(), "{updated}");
        assert!(updated.ends_with('Z'), "{updated}");
        let at = chrono::DateTime::parse_from_rfc3339(updated)
            .unwrap()
            .timestamp();
        assert!((before..=after).contains(&at), "{updated}");
    }

    #[test]
    fn a_keyring_failure_never_quotes_the_bytes_the_store_returned() {
        let e = ::keyring::Error::BadEncoding(VALUE.as_bytes().to_vec());
        let said = failure("read", "charter/ops/x", &e).message;
        assert!(!said.contains(VALUE), "{said}");
        assert!(said.contains("charter/ops/x"), "{said}");
    }

    /// A store that refuses every write.
    struct Refusing;

    impl Store for Refusing {
        fn get(&self, _: &str, _: &str) -> Result<Option<Secret>, VaultError> {
            Ok(None)
        }
        fn set(&self, _: &str, _: &str, _: &str) -> Result<Held, VaultError> {
            Err(VaultError::new("refused"))
        }
        fn delete(&self, _: &str, _: &str) -> Result<bool, VaultError> {
            Err(VaultError::new("refused"))
        }
    }

    #[test]
    fn a_vaults_service_is_kept_before_its_first_item_is_written() {
        // Were it kept only after, an item written by a store that then failed the index
        // write would sit in the keyring under a service nothing records.
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx::new(dir.path(), Env::of(&[]));
        let ops = vault("ops");

        assert!(set_with(&Refusing, &ctx, &ops, "API_TOKEN", VALUE, "t").is_err());

        let index = load_index(&ctx, &ops).unwrap();
        assert!(index.service.is_some_and(|s| s.starts_with("charter/ops/")));
        assert!(index.keys.is_empty(), "a key the store refused was listed");
    }

    /// A store that holds items to charter's app, as macOS's does, over the stub: it counts the
    /// writes it was asked for, and answers each with `answer`.
    struct Holding {
        under: FileStore,
        writes: std::sync::atomic::AtomicUsize,
        answer: Held,
    }

    impl Holding {
        fn at(dir: &std::path::Path, answer: Held) -> Self {
            Self {
                under: FileStore::at(dir.join("k.json")),
                writes: std::sync::atomic::AtomicUsize::new(0),
                answer,
            }
        }

        fn writes(&self) -> usize {
            self.writes.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl Store for Holding {
        fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
            self.under.get(service, account)
        }
        fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
            self.writes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.under.set(service, account, value)?;
            Ok(self.answer)
        }
        fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
            self.under.delete(service, account)
        }
        fn holds(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_write_to_a_store_that_holds_it_marks_the_key_held() {
        let (dir, ctx, ops) = plane();
        let store = Holding::at(dir.path(), Held::ToTheApp);
        set_with(&store, &ctx, &ops, "A", VALUE, "t").unwrap();
        assert!(load_index(&ctx, &ops).unwrap().keys["A"].held);
        // Nothing was moved, so there is nothing to say.
        assert_eq!(take_note(&ctx, &ops), None);
    }

    #[test]
    fn an_item_written_before_the_store_held_it_is_held_on_its_first_read_and_still_resolves() {
        let (dir, ctx, ops) = plane();
        // Written before: the same items, by a store that set no rule.
        set_with(
            &FileStore::at(dir.path().join("k.json")),
            &ctx,
            &ops,
            "A",
            VALUE,
            "t",
        )
        .unwrap();
        assert!(!load_index(&ctx, &ops).unwrap().keys["A"].held);

        let store = Holding::at(dir.path(), Held::ToTheApp);
        assert_eq!(get_with(&store, &ctx, &ops, "A").unwrap(), VALUE);
        assert_eq!(store.writes(), 1, "the item was not written again");
        assert!(load_index(&ctx, &ops).unwrap().keys["A"].held);

        assert_eq!(get_with(&store, &ctx, &ops, "A").unwrap(), VALUE);
        assert_eq!(store.writes(), 1, "a held item was written again");
    }

    #[test]
    fn a_vault_whose_items_were_moved_says_so_once() {
        let (dir, ctx, ops) = plane();
        let before = FileStore::at(dir.path().join("k.json"));
        set_with(&before, &ctx, &ops, "A", VALUE, "t").unwrap();
        set_with(&before, &ctx, &ops, "B", VALUE, "t").unwrap();
        let store = Holding::at(dir.path(), Held::ToTheApp);

        get_with(&store, &ctx, &ops, "A").unwrap();
        assert_eq!(
            take_note(&ctx, &ops).as_deref(),
            Some(
                "charter moved the secrets of vault 'ops' under its access rule in the system \
                 keyring: from now on only charter's app reads them without asking you. Any \
                 other program, this command included, makes the system ask you first."
            )
        );
        assert_eq!(take_note(&ctx, &ops), None);
        // A later key moved is not said again.
        get_with(&store, &ctx, &ops, "B").unwrap();
        assert_eq!(take_note(&ctx, &ops), None);
    }

    #[test]
    fn an_item_the_store_could_not_hold_yet_is_tried_again_at_the_next_read() {
        let (dir, ctx, ops) = plane();
        set_with(
            &FileStore::at(dir.path().join("k.json")),
            &ctx,
            &ops,
            "A",
            VALUE,
            "t",
        )
        .unwrap();
        let store = Holding::at(dir.path(), Held::NotYet);

        assert_eq!(get_with(&store, &ctx, &ops, "A").unwrap(), VALUE);
        assert_eq!(get_with(&store, &ctx, &ops, "A").unwrap(), VALUE);
        assert_eq!(store.writes(), 2);
        assert!(!load_index(&ctx, &ops).unwrap().keys["A"].held);
        assert_eq!(take_note(&ctx, &ops), None);
    }

    #[test]
    fn a_store_that_keeps_no_rule_never_writes_on_a_read() {
        let (dir, ctx, ops) = plane();
        let store = FileStore::at(dir.path().join("k.json"));
        set_with(&store, &ctx, &ops, "A", VALUE, "t").unwrap();
        let before = std::fs::read(index_path(&ctx, &ops)).unwrap();
        assert_eq!(get_with(&store, &ctx, &ops, "A").unwrap(), VALUE);
        assert_eq!(std::fs::read(index_path(&ctx, &ops)).unwrap(), before);
    }

    /// A holding store over which another charter's `secret set` of the same key lands right
    /// after each read.
    struct Racing {
        holding: Holding,
        ctx: Ctx,
        vault: Vault,
    }

    impl Store for Racing {
        fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
            let read = self.holding.get(service, account)?;
            set_with(
                &self.holding.under,
                &self.ctx,
                &self.vault,
                account,
                "newer-value-9d2",
                "2026-10-04T12:00:00Z",
            )?;
            Ok(read)
        }
        fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
            self.holding.set(service, account, value)
        }
        fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
            self.holding.delete(service, account)
        }
        fn holds(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_move_never_writes_the_value_it_read_over_a_newer_one() {
        let (dir, ctx, ops) = plane();
        set_with(
            &FileStore::at(dir.path().join("k.json")),
            &ctx,
            &ops,
            "A",
            VALUE,
            "t",
        )
        .unwrap();
        let racing = Racing {
            holding: Holding::at(dir.path(), Held::ToTheApp),
            ctx: Ctx::new(dir.path(), Env::of(&[])),
            vault: vault("ops"),
        };

        assert_eq!(get_with(&racing, &ctx, &ops, "A").unwrap(), VALUE);

        let after = FileStore::at(dir.path().join("k.json"));
        assert_eq!(
            get_with(&after, &ctx, &ops, "A").unwrap(),
            "newer-value-9d2"
        );
        let entry = &load_index(&ctx, &ops).unwrap().keys["A"];
        assert_eq!(entry.updated, "2026-10-04T12:00:00Z");
        assert_eq!(
            racing.holding.writes(),
            0,
            "the move wrote over the newer value"
        );
    }

    #[test]
    fn a_write_the_store_could_not_hold_is_told_apart_from_one_it_held() {
        let (dir, ctx, ops) = plane();
        let held = Holding::at(dir.path(), Held::ToTheApp);
        set_with(&held, &ctx, &ops, "A", VALUE, "t").unwrap();
        assert!(!left_unheld_in(&held, &ctx, &ops, "A"));

        let not_yet = Holding::at(dir.path(), Held::NotYet);
        set_with(&not_yet, &ctx, &ops, "B", VALUE, "t").unwrap();
        assert!(left_unheld_in(&not_yet, &ctx, &ops, "B"));

        // A store with no rule never says it.
        let stub = FileStore::at(dir.path().join("k.json"));
        set_with(&stub, &ctx, &ops, "C", VALUE, "t").unwrap();
        assert!(!left_unheld_in(&stub, &ctx, &ops, "C"));
        assert_eq!(
            unheld_sentence("C"),
            "charter could not hold 'C' to its app, so the program that wrote it reads it \
             without asking you. charter tries again the next time the charter command reads it."
        );
    }

    /// A holding store where another charter's `secret set` of the same key, which could not be
    /// held, lands while the move is being made.
    struct SetDuringTheMove {
        under: FileStore,
        ctx: Ctx,
        vault: Vault,
    }

    impl Store for SetDuringTheMove {
        fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
            self.under.get(service, account)
        }
        fn set(&self, _: &str, account: &str, _: &str) -> Result<Held, VaultError> {
            set_with(
                &self.under,
                &self.ctx,
                &self.vault,
                account,
                "newer-value-4c8",
                "2026-10-04T13:00:00Z",
            )?;
            Ok(Held::ToTheApp)
        }
        fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
            self.under.delete(service, account)
        }
        fn holds(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_newer_write_that_lands_during_a_move_is_never_marked_held_by_it() {
        let (dir, ctx, ops) = plane();
        set_with(
            &FileStore::at(dir.path().join("k.json")),
            &ctx,
            &ops,
            "A",
            VALUE,
            "t",
        )
        .unwrap();
        let store = SetDuringTheMove {
            under: FileStore::at(dir.path().join("k.json")),
            ctx: Ctx::new(dir.path(), Env::of(&[])),
            vault: vault("ops"),
        };

        assert_eq!(get_with(&store, &ctx, &ops, "A").unwrap(), VALUE);

        let entry = &load_index(&ctx, &ops).unwrap().keys["A"];
        assert_eq!(entry.updated, "2026-10-04T13:00:00Z");
        assert!(!entry.held, "the newer value was marked held by the move");
        assert_eq!(take_note(&ctx, &ops), None);
    }
}
