//! The keychain copy (RN-6, ruling V93h): each `charter/…` keychain item of a project's keyring
//! vaults and identity records copied to `purlis/…`, read back, and only then read from there.
//!
//! **A vault, or an identity record, switches whole or not at all.** Every one of its items is
//! copied and read back first, byte for byte against the original; only then is the vault's
//! index pointed at the purlis service ([`keyring::switch_service`]) or the record given the
//! purlis base ([`identity::switch_base`]), journalled first. One item that fails leaves the
//! whole vault or record reading its old items, where every reader still finds them.
//!
//! **An item already under `purlis/…` is never trusted for being there** (RN-4 review). Its value
//! is compared with the original: different, the vault stays on the old prefix and the run says
//! so; the same, it counts as copied in a store that holds no item to the app. The one exception
//! is an item this machine's rename-local wrote itself (the journal's `copied`): a copy left
//! behind by an undo, or by a run that failed its read-back, is written again rather than refused
//! for good.
//!
//! **Where a store holds items to the app, every copy is a fresh item the app holds**
//! (D-RN6-8). An item already there, even with the very value, is replaced by a new one the app's
//! own writer makes, never updated in place, and a write that comes back anything but held to the
//! app ([`Held::ToTheApp`]) fails that item: the item there is another program's, and keeps the
//! access that program gave it. The same holds for the undo's copy back.
//!
//! **The old items are kept** and never deleted, so `purlis migrate --undo` only points each
//! vault and record back at them. A key written since the switch lives only under `purlis/…`,
//! so the undo copies that key back, read back the same way, before it points the vault back.
//! The copies stay too (D-RN6-2).
//!
//! **Where the copy runs.** On macOS each item is held to charter's app (ruling V90a): the app
//! reads its items without anyone being asked, and the `purlis` command makes the Keychain ask
//! for every one. So a run's copy never lets the Keychain ask (#1306): it runs with the
//! Keychain's dialogs off ([`keyring::DialogsOff`]), and an item that would ask fails to be
//! read instead. Its vault or record stays on the old prefix, where it keeps working, and is
//! said to wait ([`super::Waiting`]); the app offers to finish moving those, and only then, on
//! the person's press, lets the Keychain ask, once per item ([`super::finish`]).
//!
//! - **In the app**, every vault and record is copied, and the copies are written through the
//!   app's own writer, so they are held to the app like the originals.
//! - **In a terminal on macOS**, only a vault whose every key is not yet held to the app
//!   ([`Reach::ItsOwn`]): the command made those items, so it reads them without anyone being
//!   asked, and it makes their copies itself, so it reads those back the same way. They stay
//!   the command's, as the originals were, until the app holds them on a later read (ruling
//!   V90d). A vault held to the app, and every identity record, is left to the app's launch.
//!   That also moves a command-line install with no app beside the command.
//! - **Elsewhere** (Linux, Windows, a test build's stub) nothing is held, nothing asks, and
//!   everything is copied.
//!
//! An undo that has keys to copy back runs from the terminal too, with the dialogs on, and the
//! system asks twice for each such key held to the app (reading its copy, and reading back what
//! was written under the old name): it is rare and the person asked for it.

use std::collections::BTreeMap;
use std::path::Path;

use super::{Entry, Local, Moved, Seams, Waiting, write_entry};
use crate::names::KEYCHAIN_PREFIX;
use crate::secrets::keyring::{self, Held, Store};
use crate::secrets::registry::{self, Vault};
use crate::secrets::{Ctx, Env, Kind, VaultError, identity, keyhold};

/// Whether reading a project's items may ask the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asking {
    /// A run's copy: the Keychain's dialogs off, so an item that would ask fails to be read.
    Never,
    /// What the person asked for: finishing the vaults a run left waiting, or an undo's copy
    /// back of keys written since the switch.
    Allowed,
}

/// Which of a project's items this process can copy.
pub enum Reach {
    /// Every one: this is the app, or the store holds no item to it.
    Every(Box<dyn Store>),
    /// Only those this program made: a terminal where the store holds items to the app. A
    /// vault is copied only when its index says no key is held to the app yet, and each copy
    /// is made by this process ([`Store::make_own`]), so it reads it back without asking.
    ItsOwn(Box<dyn Store>),
}

impl Reach {
    fn store(&self) -> &dyn Store {
        match self {
            Self::Every(store) | Self::ItsOwn(store) => &**store,
        }
    }

    fn its_own(&self) -> bool {
        matches!(self, Self::ItsOwn(_))
    }
}

/// The keychain for a project and how far this process reaches in it, or `None` where it
/// cannot be reached without the person being asked and that is not [`Asking::Allowed`].
pub type Keyring = dyn Fn(&Ctx, Asking) -> Option<Reach>;

/// The store this build talks to for the project ([`keyring::store`]): with the Keychain's
/// dialogs off for as long as it is held when nothing may ask ([`Asking::Never`]), and only
/// the command's own items from a terminal where the store holds items to the app.
pub fn real(ctx: &Ctx, asking: Asking) -> Option<Reach> {
    let store = keyring::store(ctx);
    if !store.holds() {
        return Some(Reach::Every(store));
    }
    let store: Box<dyn Store> = match asking {
        // Dialogs that cannot be turned off leave every item unread, and every vault waiting for
        // the person to finish it, rather than let the system ask.
        Asking::Never => Box::new(Quiet {
            off: keyring::DialogsOff::now(),
            store,
        }),
        Asking::Allowed => store,
    };
    // An undo's copy back, from a terminal with the app beside it, still goes through the
    // app's writer: the copy back is held to the app, as what it copies was.
    let every = keyhold::is_the_app() || (asking == Asking::Allowed && keyhold::app_is_beside());
    Some(if every {
        Reach::Every(store)
    } else {
        Reach::ItsOwn(store)
    })
}

/// A store whose dialogs are off while it lives. When they could not be turned off it reads
/// and writes nothing: every call answers [`Kind::WouldAsk`].
struct Quiet {
    store: Box<dyn Store>,
    off: Result<keyring::DialogsOff, VaultError>,
}

impl Quiet {
    fn on(&self) -> Result<&dyn Store, VaultError> {
        match &self.off {
            Ok(_) => Ok(&*self.store),
            Err(e) => Err(VaultError::would_ask(format!(
                "{}; so that the system asks nothing, nothing was read",
                e.message
            ))),
        }
    }
}

impl Store for Quiet {
    fn get(&self, service: &str, account: &str) -> Result<Option<keyring::Secret>, VaultError> {
        self.on()?.get(service, account)
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.on()?.set(service, account, value)
    }
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        self.on()?.delete(service, account)
    }
    fn holds(&self) -> bool {
        self.store.holds()
    }
    fn rehold(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.on()?.rehold(service, account, value)
    }
    fn make_fresh(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.on()?.make_fresh(service, account, value)
    }
    fn make_own(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.on()?.make_own(service, account, value)
    }
}

/// Why an item was not copied.
enum Miss {
    /// Reading or writing it would have asked the person: its vault waits.
    WouldAsk,
    /// Anything else, said.
    Failed(String),
}

impl From<String> for Miss {
    fn from(why: String) -> Self {
        Self::Failed(why)
    }
}

impl From<VaultError> for Miss {
    fn from(e: VaultError) -> Self {
        if e.kind == Kind::WouldAsk {
            Self::WouldAsk
        } else {
            Self::Failed(e.message)
        }
    }
}

/// Every item the journal says rename-local wrote, as `(service, account)`.
pub(super) fn written(journal: &[Entry]) -> Vec<(String, String)> {
    journal
        .iter()
        .filter_map(|entry| match entry {
            Entry::Copied { service, account } => Some((service.clone(), account.clone())),
            _ => None,
        })
        .collect()
}

/// Whether a keychain entry of the journal is one this copy makes (a forged journal, S3 of the
/// RN-5 review): a copy under the purlis prefix in charter's own shape, a switch of a vault
/// between its own two services, a record of a vault a name could be.
pub(super) fn ours(entry: &Entry) -> bool {
    match entry {
        Entry::Copied { service, account } => {
            service.starts_with(KEYCHAIN_PREFIX.write)
                && keyring::own_service(service)
                && !account.is_empty()
        }
        Entry::Switched {
            plane,
            vault,
            from,
            to,
            ..
        } => {
            plane.is_absolute()
                && registry::name_ok(vault)
                && keyring::service_ok_for(from, vault)
                && keyring::renamed(from).as_deref() == Some(to.as_str())
        }
        Entry::Rebased { plane, vault, ids } => {
            plane.is_absolute()
                && registry::name_ok(vault)
                && ids
                    .values()
                    .all(|id| keyring::own_service(&identity::item_services(id).0))
        }
        _ => false,
    }
}

fn ctx_of(plane: &Path) -> Ctx {
    Ctx::new(plane, Env::of(&[]))
}

/// A keyring vault still on the old prefix.
struct OldVault {
    vault: Vault,
    /// The `charter/…` service its index names.
    from: String,
    /// Every key's `updated`.
    keys: BTreeMap<String, String>,
    /// Whether no key is held to the app yet (ruling V90a): every item is the command's own.
    unheld: bool,
}

/// The project's keyring vaults whose index still names a `charter/…` service. A vault whose
/// index cannot be read is the doctor's, and left here.
fn on_the_old_prefix(ctx: &Ctx) -> Vec<OldVault> {
    let Ok(doc) = registry::load_registry(ctx) else {
        return Vec::new();
    };
    registry::vaults(&doc)
        .keys()
        .filter_map(|name| registry::vault_in(&doc, name).ok())
        .filter(|vault| vault.provider == "keyring")
        .filter_map(|vault| {
            let index = keyring::load_index(ctx, &vault).ok()?;
            let from = index.service.filter(|s| keyring::renamed(s).is_some())?;
            let unheld = index.keys.values().all(|entry| !entry.held);
            let keys = index
                .keys
                .into_iter()
                .map(|(key, entry)| (key, entry.updated))
                .collect();
            Some(OldVault {
                vault,
                from,
                keys,
                unheld,
            })
        })
        .collect()
}

/// Copy the keychain items of the project at `plane` and switch each vault and record whose
/// items all read back the same (see the module): every one still on the old prefix, or only
/// those `only` names.
pub(super) fn copy_plane(
    local: &Local,
    seams: &Seams,
    moved: &mut Moved,
    plane: &Path,
    written: &[(String, String)],
    asking: Asking,
    only: Option<&[Waiting]>,
) {
    // A project this charter may not write is left as it is, as its folders are.
    if matches!(
        crate::compat::read(plane),
        crate::compat::Compat::ReadOnly(_)
    ) {
        return;
    }
    let ctx = ctx_of(plane);
    let named = |vault: &str, identity: bool| {
        only.is_none_or(|only| {
            only.iter()
                .any(|w| w.plane == plane && w.vault == vault && w.identity == identity)
        })
    };
    let vaults: Vec<OldVault> = on_the_old_prefix(&ctx)
        .into_iter()
        .filter(|old| named(&old.vault.name, false))
        .collect();
    let records: Vec<(String, BTreeMap<String, String>)> = identity::on_the_old_base(&ctx)
        .into_iter()
        .filter(|(vault, _)| named(vault, true))
        .collect();
    if vaults.is_empty() && records.is_empty() {
        return;
    }
    let Some(reach) = (seams.keyring)(&ctx, asking) else {
        moved.note(format!(
            "{}: its keychain items stay under '{}…' for now: the keychain could not be reached \
             here without the system asking you for each, so the app copies them at its next \
             launch",
            plane.display(),
            KEYCHAIN_PREFIX.reads[0]
        ));
        return;
    };
    let copy = Copy {
        local,
        store: reach.store(),
        written,
        its_own: reach.its_own(),
    };
    let to_the_app = |moved: &mut Moved, what: String| {
        moved.note(format!(
            "{}: {what} stays under '{}…' for now: reading its items here could make the \
             system ask you for each, so the app copies it at its next launch",
            plane.display(),
            KEYCHAIN_PREFIX.reads[0]
        ));
    };
    for old in vaults {
        if copy.its_own && !old.unheld {
            to_the_app(moved, format!("vault '{}'", old.vault.name));
            continue;
        }
        copy.vault(&ctx, moved, plane, &old.vault, &old.from, old.keys);
    }
    for (vault, ids) in records {
        // Nothing says who made an identity record's items, so only the app copies them.
        if copy.its_own {
            to_the_app(moved, format!("vault '{vault}''s identity"));
            continue;
        }
        copy.record(&ctx, moved, plane, &vault, ids);
    }
}

/// Whether `waiting` still reads its items under the old prefix: its vault's index still names
/// a `charter/…` service, or its record still has no purlis base.
pub(super) fn still_waits(waiting: &Waiting) -> bool {
    let ctx = ctx_of(&waiting.plane);
    if waiting.identity {
        identity::on_the_old_base(&ctx)
            .iter()
            .any(|(vault, _)| *vault == waiting.vault)
    } else {
        on_the_old_prefix(&ctx)
            .iter()
            .any(|old| old.vault.name == waiting.vault)
    }
}

struct Copy<'a> {
    local: &'a Local,
    store: &'a dyn Store,
    written: &'a [(String, String)],
    /// A terminal's copy of the command's own items ([`Reach::ItsOwn`]).
    its_own: bool,
}

/// What a vault or record that waits is said with.
fn waits(moved: &mut Moved, waiting: Waiting, what: &str, stays: &str, its_own: bool) {
    let then = if its_own {
        "the app offers to finish moving it"
    } else {
        "it waits for you to finish moving it from the window"
    };
    moved.waits(
        waiting,
        format!(
            "{what}: waits to move: reading its items here could make the system ask you for \
             each, so {then}; {stays}"
        ),
    );
}

impl Copy<'_> {
    fn vault(
        &self,
        ctx: &Ctx,
        moved: &mut Moved,
        plane: &Path,
        vault: &Vault,
        from: &str,
        keys: BTreeMap<String, String>,
    ) {
        let what = format!("{}: vault '{}'", plane.display(), vault.name);
        let stays = format!("it still reads its secrets under {from}");
        let Some(to) = keyring::renamed(from) else {
            return;
        };
        for key in keys.keys() {
            match self.item(from, &to, key) {
                Ok(()) => {}
                Err(Miss::WouldAsk) => {
                    let waiting = Waiting {
                        plane: plane.to_path_buf(),
                        vault: vault.name.clone(),
                        identity: false,
                        items: keys.len(),
                    };
                    return waits(moved, waiting, &what, &stays, self.its_own);
                }
                Err(Miss::Failed(why)) => {
                    moved.failed(format!("{what}: {why}; {stays}"));
                    return;
                }
            }
        }
        let count = keys.len();
        let entry = Entry::Switched {
            plane: plane.to_path_buf(),
            vault: vault.name.clone(),
            from: from.to_owned(),
            to: to.clone(),
            keys,
        };
        if let Err(e) = write_entry(self.local, &entry) {
            moved.failed(format!(
                "{what}: the journal could not be written ({e}); {stays}"
            ));
            return;
        }
        let Entry::Switched { keys, .. } = &entry else {
            return;
        };
        match keyring::switch_service(ctx, vault, from, &to, keys) {
            Ok(true) => moved.done(format!(
                "{what}: {count} secret(s) copied to {to}, each read back the same, and read \
                 from there now; the items under {from} are kept"
            )),
            Ok(false) => moved.failed(format!(
                "{what}: its keys index changed while it was copied; {stays}, and the next run \
                 copies it again"
            )),
            Err(e) => moved.failed(format!("{what}: {}; {stays}", e.message)),
        }
    }

    fn record(
        &self,
        ctx: &Ctx,
        moved: &mut Moved,
        plane: &Path,
        vault: &str,
        ids: BTreeMap<String, String>,
    ) {
        let what = format!("{}: vault '{vault}''s identity", plane.display());
        let stays = format!("it is still read under {}", identity::READ_BASES[1].1);
        for (source, id) in &ids {
            let (from, to) = identity::item_services(id);
            match self.item(&from, &to, source) {
                Ok(()) => {}
                Err(Miss::WouldAsk) => {
                    let waiting = Waiting {
                        plane: plane.to_path_buf(),
                        vault: vault.to_owned(),
                        identity: true,
                        items: ids.len(),
                    };
                    return waits(moved, waiting, &what, &stays, self.its_own);
                }
                Err(Miss::Failed(why)) => {
                    moved.failed(format!("{what}: {why}; {stays}"));
                    return;
                }
            }
        }
        let entry = Entry::Rebased {
            plane: plane.to_path_buf(),
            vault: vault.to_owned(),
            ids: ids.clone(),
        };
        if let Err(e) = write_entry(self.local, &entry) {
            moved.failed(format!(
                "{what}: the journal could not be written ({e}); {stays}"
            ));
            return;
        }
        match identity::switch_base(ctx, vault, &ids, true) {
            Ok(true) => moved.done(format!(
                "{what}: copied to {}, read back the same, and read from there now; the old \
                 items are kept",
                identity::READ_BASES[0].1
            )),
            Ok(false) => moved.failed(format!(
                "{what}: its record changed while it was copied; {stays}, and the next run \
                 copies it again"
            )),
            Err(e) => moved.failed(format!("{what}: {}; {stays}", e.message)),
        }
    }

    /// The item `from`/`account` copied to `to`/`account` and read back the same, or why not.
    /// Never trusts an item already at `to` for being there.
    fn item(&self, from: &str, to: &str, account: &str) -> Result<(), Miss> {
        let read = |service: &str| {
            self.store
                .get(service, account)
                .map(|found| found.map(keyring::Secret::into_inner))
                .map_err(Miss::from)
        };
        let there = read(to)?;
        let Some(original) = read(from)? else {
            // Nothing to copy, and nothing lost: the key is not found under either prefix. An
            // item at `to` alone would be one nothing vouches for.
            return match there {
                None => Ok(()),
                Some(_) => Err(Miss::Failed(format!(
                    "'{account}' has no item under {from}, and one under {to} that rename-local \
                     did not copy there; it is never trusted"
                ))),
            };
        };
        let holds = self.store.holds();
        match there {
            // Where nothing is held to the app, the same value is the copy; where it is, the
            // item is made again by the app, so it is never one another program made (D-RN6-8).
            Some(value) if value == original && !holds => return Ok(()),
            Some(value) if value != original && !self.wrote(to, account) => {
                return Err(Miss::Failed(format!(
                    "an item for '{account}' under {to} is there already and holds a different \
                     value; rename-local never trusts an item it did not copy there. If you do \
                     not know that item, remove it, then run `purlis migrate` again"
                )));
            }
            _ => {}
        }
        write_entry(
            self.local,
            &Entry::Copied {
                service: to.to_owned(),
                account: account.to_owned(),
            },
        )
        .map_err(|e| Miss::Failed(format!("the journal could not be written ({e})")))?;
        let held = if self.its_own {
            self.store.make_own(to, account, &original)?
        } else {
            self.store.make_fresh(to, account, &original)?
        };
        if !self.its_own {
            held_by_the_app(self.store, held, to, account)?;
        }
        match read(to) {
            Ok(Some(copy)) if copy == original => Ok(()),
            Ok(_) => Err(Miss::Failed(format!(
                "the copy of '{account}' under {to} did not read back the same"
            ))),
            Err(Miss::WouldAsk) => Err(Miss::WouldAsk),
            Err(Miss::Failed(why)) => Err(Miss::Failed(format!(
                "the copy of '{account}' under {to} could not be read back ({why})"
            ))),
        }
    }

    fn wrote(&self, service: &str, account: &str) -> bool {
        self.written
            .iter()
            .any(|(s, a)| s == service && a == account)
    }
}

/// Undo a vault's switch: when its index still names `to`, every key written since is copied
/// back to `from` and read back, then the index names `from` again.
pub(super) fn switch_back(
    seams: &Seams,
    moved: &mut Moved,
    plane: &Path,
    vault: &str,
    from: &str,
    to: &str,
    keys: &BTreeMap<String, String>,
) {
    let ctx = ctx_of(plane);
    let what = format!("{}: vault '{vault}'", plane.display());
    // A vault no longer registered, or no longer reading `to`, has nothing to point back.
    let Ok(vault) = registry::vault(&ctx, vault) else {
        return;
    };
    let index = match keyring::load_index(&ctx, &vault) {
        Ok(index) => index,
        Err(e) => {
            moved.failed(format!("{what}: {}", e.message));
            return;
        }
    };
    if index.service.as_deref() != Some(to) {
        return;
    }
    let since: Vec<&String> = index
        .keys
        .iter()
        .filter(|(key, entry)| keys.get(*key) != Some(&entry.updated))
        .map(|(key, _)| key)
        .collect();
    if !since.is_empty() {
        let Some(reach) = (seams.keyring)(&ctx, Asking::Allowed) else {
            moved.failed(format!(
                "{what}: the keychain could not be reached to copy back the secrets written \
                 since it moved; it still reads them under {to}"
            ));
            return;
        };
        for key in since {
            if let Err(why) = copy_back(&reach, to, from, key) {
                moved.failed(format!(
                    "{what}: {why}; it still reads its secrets under {to}"
                ));
                return;
            }
        }
    }
    let now: BTreeMap<String, String> = index
        .keys
        .iter()
        .map(|(key, entry)| (key.clone(), entry.updated.clone()))
        .collect();
    match keyring::switch_service(&ctx, &vault, to, from, &now) {
        Ok(true) => moved.done(format!(
            "{what}: reads its secrets under {from} again; the copies under {to} are kept"
        )),
        Ok(false) => moved.failed(format!(
            "{what}: a secret was written while it was put back; it still reads its secrets \
             under {to}, and `{}` again finishes it",
            super::UNDO_COMMAND
        )),
        Err(e) => moved.failed(format!("{what}: {}", e.message)),
    }
}

/// A key written since the switch, copied back to its vault's old service and read back. From
/// a terminal with no app beside it ([`Reach::ItsOwn`]) the copy back is the command's own, as
/// every item such an install makes is.
fn copy_back(reach: &Reach, to: &str, from: &str, key: &str) -> Result<(), String> {
    let store = reach.store();
    let Some(value) = store
        .get(to, key)
        .map_err(|e| e.message)?
        .map(keyring::Secret::into_inner)
    else {
        // Not found under either: nothing to lose.
        return Ok(());
    };
    if reach.its_own() {
        store.make_own(from, key, &value).map_err(|e| e.message)?;
    } else {
        let held = store.make_fresh(from, key, &value).map_err(|e| e.message)?;
        held_by_the_app(store, held, from, key)?;
    }
    match store
        .get(from, key)
        .map(|back| back.map(keyring::Secret::into_inner))
    {
        Ok(Some(back)) if back == value => Ok(()),
        _ => Err(format!(
            "'{key}', written since it moved, did not read back the same under {from}"
        )),
    }
}

/// A write to a store that holds items to the app, which did not leave the item held to it:
/// the item there is another program's, written in place and still under its access, so it is
/// never switched to.
fn held_by_the_app(
    store: &dyn Store,
    held: Held,
    service: &str,
    account: &str,
) -> Result<(), String> {
    if !store.holds() || held == Held::ToTheApp {
        return Ok(());
    }
    Err(format!(
        "the item for '{account}' under {service} could not be made the app's own: another \
         program's item is there. If you do not know it, remove it, then run this again"
    ))
}

/// Undo a record's switch: the record, while it is still the one with `ids`, read under the old
/// base again. One made again since is left as it is: its items are only under the purlis base.
pub(super) fn rebase_back(
    moved: &mut Moved,
    plane: &Path,
    vault: &str,
    ids: &BTreeMap<String, String>,
) {
    let what = format!("{}: vault '{vault}''s identity", plane.display());
    match identity::switch_base(&ctx_of(plane), vault, ids, false) {
        Ok(true) => moved.done(format!(
            "{what}: read under {} again; the copies under {} are kept",
            identity::READ_BASES[1].1,
            identity::READ_BASES[0].1
        )),
        Ok(false) => {}
        Err(e) => moved.failed(format!("{what}: {}", e.message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_whose_dialogs_could_not_be_turned_off_reads_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let under = keyring::FileStore::at(dir.path().join("stub.json"));
        under.set("charter/ops/1", "K", "v").unwrap();
        let quiet = Quiet {
            store: Box::new(keyring::FileStore::at(dir.path().join("stub.json"))),
            off: Err(VaultError::new("the dialogs could not be turned off")),
        };

        let read = quiet.get("charter/ops/1", "K").unwrap_err();
        let wrote = quiet.make_fresh("purlis/ops/1", "K", "v").unwrap_err();

        assert_eq!(read.kind, Kind::WouldAsk);
        assert!(
            read.message.contains("could not be turned off"),
            "{}",
            read.message
        );
        assert_eq!(wrote.kind, Kind::WouldAsk);
        assert!(under.get("purlis/ops/1", "K").unwrap().is_none());
    }
}
