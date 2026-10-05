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
//! for every one. So the copy runs where reading asks nothing — the app, a store with no such
//! rule (Linux, Windows, a test build's stub) — and a terminal on macOS leaves it to the app's
//! next launch ([`real`]). Its copies are written through the app's own writer, so they are
//! held to the app like the originals. An undo that has keys to copy back runs from the terminal
//! too, and the system asks twice for each such key (reading its copy, and reading back what was
//! written under the old name): it is rare and the person asked for it.

use std::collections::BTreeMap;
use std::path::Path;

use super::{Entry, Local, Moved, Seams, write_entry};
use crate::names::KEYCHAIN_PREFIX;
use crate::secrets::keyring::{self, Held, Store};
use crate::secrets::registry::{self, Vault};
use crate::secrets::{Ctx, Env, identity, keyhold};

/// Whether reading a project's items may ask the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asking {
    /// A run's copy: only where reading asks nothing.
    Never,
    /// An undo's copy back of keys written since the switch.
    Allowed,
}

/// The keychain for a project, or `None` where reading it could ask the person and that is not
/// [`Asking::Allowed`].
pub type Keyring = dyn Fn(&Ctx, Asking) -> Option<Box<dyn Store>>;

/// The store this build talks to for the project ([`keyring::store`]), where reading asks
/// nothing: a store that holds no item to the app, or this process is the app.
pub fn real(ctx: &Ctx, asking: Asking) -> Option<Box<dyn Store>> {
    let store = keyring::store(ctx);
    (asking == Asking::Allowed || !store.holds() || keyhold::is_the_app()).then_some(store)
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

/// The project's keyring vaults whose index still names a `charter/…` service: each with that
/// service and every key's `updated`. A vault whose index cannot be read is the doctor's, and
/// left here.
fn on_the_old_prefix(ctx: &Ctx) -> Vec<(Vault, String, BTreeMap<String, String>)> {
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
            let keys = index
                .keys
                .into_iter()
                .map(|(key, entry)| (key, entry.updated))
                .collect();
            Some((vault, from, keys))
        })
        .collect()
}

/// Copy the keychain items of the project at `plane` and switch each vault and record whose
/// items all read back the same (see the module).
pub(super) fn copy_plane(
    local: &Local,
    seams: &Seams,
    moved: &mut Moved,
    plane: &Path,
    written: &[(String, String)],
) {
    // A project this charter may not write is left as it is, as its folders are.
    if matches!(
        crate::compat::read(plane),
        crate::compat::Compat::ReadOnly(_)
    ) {
        return;
    }
    let ctx = ctx_of(plane);
    let vaults = on_the_old_prefix(&ctx);
    let records = identity::on_the_old_base(&ctx);
    if vaults.is_empty() && records.is_empty() {
        return;
    }
    let Some(store) = (seams.keyring)(&ctx, Asking::Never) else {
        moved.note(format!(
            "{}: its keychain items stay under '{}…' for now: reading them here would make the \
             system ask you for each, so the app copies them at its next launch",
            plane.display(),
            KEYCHAIN_PREFIX.reads[0]
        ));
        return;
    };
    let copy = Copy {
        local,
        store: &*store,
        written,
    };
    for (vault, from, keys) in vaults {
        copy.vault(&ctx, moved, plane, &vault, &from, keys);
    }
    for (vault, ids) in records {
        copy.record(&ctx, moved, plane, &vault, ids);
    }
}

struct Copy<'a> {
    local: &'a Local,
    store: &'a dyn Store,
    written: &'a [(String, String)],
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
            if let Err(why) = self.item(from, &to, key) {
                moved.failed(format!("{what}: {why}; {stays}"));
                return;
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
            if let Err(why) = self.item(&from, &to, source) {
                moved.failed(format!("{what}: {why}; {stays}"));
                return;
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
    fn item(&self, from: &str, to: &str, account: &str) -> Result<(), String> {
        let read = |service: &str| {
            self.store
                .get(service, account)
                .map(|found| found.map(keyring::Secret::into_inner))
                .map_err(|e| e.message)
        };
        let there = read(to)?;
        let Some(original) = read(from)? else {
            // Nothing to copy, and nothing lost: the key is not found under either prefix. An
            // item at `to` alone would be one nothing vouches for.
            return match there {
                None => Ok(()),
                Some(_) => Err(format!(
                    "'{account}' has no item under {from}, and one under {to} that rename-local \
                     did not copy there; it is never trusted"
                )),
            };
        };
        let holds = self.store.holds();
        match there {
            // Where nothing is held to the app, the same value is the copy; where it is, the
            // item is made again by the app, so it is never one another program made (D-RN6-8).
            Some(value) if value == original && !holds => return Ok(()),
            Some(value) if value != original && !self.wrote(to, account) => {
                return Err(format!(
                    "an item for '{account}' under {to} is there already and holds a different \
                     value; rename-local never trusts an item it did not copy there. If you do \
                     not know that item, remove it, then run `purlis migrate` again"
                ));
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
        .map_err(|e| format!("the journal could not be written ({e})"))?;
        let held = self
            .store
            .set(to, account, &original)
            .map_err(|e| e.message)?;
        held_by_the_app(self.store, held, to, account)?;
        match read(to) {
            Ok(Some(copy)) if copy == original => Ok(()),
            Ok(_) => Err(format!(
                "the copy of '{account}' under {to} did not read back the same"
            )),
            Err(why) => Err(format!(
                "the copy of '{account}' under {to} could not be read back ({why})"
            )),
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
        let Some(store) = (seams.keyring)(&ctx, Asking::Allowed) else {
            moved.failed(format!(
                "{what}: the keychain could not be reached to copy back the secrets written \
                 since it moved; it still reads them under {to}"
            ));
            return;
        };
        for key in since {
            if let Err(why) = copy_back(&*store, to, from, key) {
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
            "{what}: a secret was written while it was put back; it still reads its secrets              under {to}, and `{}` again finishes it",
            super::UNDO_COMMAND
        )),
        Err(e) => moved.failed(format!("{what}: {}", e.message)),
    }
}

/// A key written since the switch, copied back to its vault's old service and read back.
fn copy_back(store: &dyn Store, to: &str, from: &str, key: &str) -> Result<(), String> {
    let Some(value) = store
        .get(to, key)
        .map_err(|e| e.message)?
        .map(keyring::Secret::into_inner)
    else {
        // Not found under either: nothing to lose.
        return Ok(());
    };
    let held = store.set(from, key, &value).map_err(|e| e.message)?;
    held_by_the_app(store, held, from, key)?;
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
        "the item for '{account}' under {service} could not be made the app's own: another          program's item is there. If you do not know it, remove it, then run this again"
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
