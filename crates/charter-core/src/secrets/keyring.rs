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

/// A credential store: one entry per `(service, account)`, read, written and deleted by name.
///
/// **An error never carries a value**, and never an entry the store returned: an
/// implementation says which operation failed and the store's own reason, which names a
/// failure and not a credential.
pub trait Store: Send + Sync {
    /// The entry, or `None` when the store PROVABLY has none.
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError>;
    /// Create or replace the entry.
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), VaultError>;
    /// Delete the entry. `false` when there was none.
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError>;
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
/// **On macOS an item is created with the default access**: the program that created it may
/// read it back without asking, and any other program — `security find-generic-password -w`,
/// another charter binary, a script — makes the Keychain ask the operator first. ADR 0047
/// records what was measured and what that does and does not protect.
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

    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), VaultError> {
        Self::entry(service, account)?
            .set_password(value)
            .map_err(|e| failure("write", service, &e))
    }

    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        match Self::entry(service, account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(::keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(failure("delete", service, &e)),
        }
    }
}

/// A keyring failure as charter says it: the operation, the service, and the store's reason.
///
/// **Never `e`'s `Display` wholesale.** `BadEncoding` carries the bytes the store returned —
/// the credential itself — and prints nothing of them only by the crate's grace; it is named
/// here and not formatted. The other kinds carry the platform's own error, which names a
/// failure ("User interaction is not allowed.") and not a value.
fn failure(what: &str, service: &str, e: &::keyring::Error) -> VaultError {
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

    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), VaultError> {
        let mut map = self.load()?;
        map.insert(
            Self::slot(service, account),
            Value::String(value.to_owned()),
        );
        self.save(map)
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

/// What the index records about one key: its size band and when it was last written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub size: String,
    pub updated: String,
}

/// A vault's index: the service its items live under, and what each key is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Index {
    pub service: Option<String>,
    pub keys: BTreeMap<String, Entry>,
}

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
            },
        );
    }
    Ok(Index { service, keys })
}

fn save_index(ctx: &Ctx, vault: &Vault, index: &Index) -> Result<(), VaultError> {
    let mut keys = Map::new();
    for (k, e) in &index.keys {
        let mut entry = Map::new();
        entry.insert("size".into(), Value::String(e.size.clone()));
        entry.insert("updated".into(), Value::String(e.updated.clone()));
        keys.insert(k.clone(), Value::Object(entry));
    }
    let mut doc = Map::new();
    doc.insert(
        "service".into(),
        index.service.clone().map_or(Value::Null, Value::String),
    );
    doc.insert("keys".into(), Value::Object(keys));
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
    let Some(service) = load_index(ctx, vault)?.service else {
        return Err(not_found(vault, key));
    };
    store
        .get(&service, key)?
        .map(Secret::into_inner)
        .ok_or_else(|| not_found(vault, key))
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
    store.set(&service, key, value)?;
    index.keys.insert(
        key.to_owned(),
        Entry {
            size: fingerprint::size_band(value),
            updated: at.to_owned(),
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
        fn set(&self, _: &str, _: &str, _: &str) -> Result<(), VaultError> {
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
}
