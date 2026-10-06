//! The keychain copy in `rename-local` (RN-6, V93h) on a temporary home and the keyring stub: each
//! `charter/…` item of a project's keyring vault and identity record is copied to `purlis/…`,
//! read back, and only then read from there; the old items are kept, and `purlis migrate --undo`
//! reads them again.
//!
//! Driven through the core's public surface — [`renamelocal::run`] and [`renamelocal::undo`],
//! and the readers every other caller uses (`keyring::get`, `identity::from_keyring`) — and
//! asserted on the stub keyring, the keys index and the registry afterwards. Never the login
//! keychain: a test build's store is the stub (`crate::fence`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use purlis_core::machine;
use purlis_core::names;
use purlis_core::renamelocal::keychain::{Asking, Reach};
use purlis_core::renamelocal::{self, Local, Seams};
use purlis_core::secrets::keyring::{self, FileStore, Held, Secret, Store};
use purlis_core::secrets::registry::{self, Vault};
use purlis_core::secrets::{Ctx, Env, VaultError, identity};
use serde_json::{Map, Value, json};

const API: &str = "kr-api-6f1d0b9e2a";
const DB: &str = "kr-db-93c4e7a1f8";
const TOKEN: &str = "ops_team_token_4b2e9d";
const SOURCE: &str = "OP_TEAM_TOKEN";

/// A machine that remembers one project, in git, from before the rename: a keyring vault `ops`
/// with two secrets under `charter/ops/<id>`, and a 1Password vault `team` whose identity token
/// is under `charter/@identity/<id>`, its record with no `base`.
struct Machine {
    _dir: tempfile::TempDir,
    plane: PathBuf,
    local: Local,
}

fn machine() -> Machine {
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    let plane = home.join("work/plane");
    std::fs::create_dir_all(plane.join(".charter")).unwrap();
    let init = purlis_core::forklock::output(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&plane)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null"),
    )
    .unwrap();
    assert!(init.status.success(), "{init:?}");
    std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
    let config_root = home.join(".config");
    std::fs::create_dir_all(&config_root).unwrap();
    machine::update(&config_root, |store| store.remember(&plane, 100)).unwrap();

    let ctx = ctx(&plane);
    registry::add_vault(&ctx, "ops", "keyring", Map::new(), None, false, false).unwrap();
    let ops = vault(&plane, "ops");
    let stub = stub(&plane);
    keyring::set_with(&stub, &ctx, &ops, "API_TOKEN", API, "2026-10-01T00:00:00Z").unwrap();
    keyring::set_with(&stub, &ctx, &ops, "DB_PASSWORD", DB, "2026-10-01T00:00:00Z").unwrap();
    let mut config = Map::new();
    config.insert("env".into(), json!({ "OP_SERVICE_ACCOUNT_TOKEN": SOURCE }));
    registry::add_vault(&ctx, "team", "1password", config, None, false, false).unwrap();
    identity::put_in_keyring(&ctx, &vault(&plane, "team"), TOKEN).unwrap();

    // As a build from before the rename left them: the same files, the old prefix, no `base`.
    let index = keyring::index_path(&ctx, &ops);
    replace_in(&index, "\"purlis/ops/", "\"charter/ops/");
    replace_in(
        &ctx.state.join(keyring::STUB_FILE),
        "\"purlis/",
        "\"charter/",
    );
    let mut half = registry::load_local(&ctx).unwrap();
    record_of(&mut half).shift_remove(identity::BASE);
    registry::save_local(&ctx, &half).unwrap();

    let m = Machine {
        _dir: dir,
        plane,
        local: Local {
            config_root,
            data_base: None,
            logs: None,
            planes: Vec::new(),
            own_app: None,
            plugin: None,
        },
    };
    assert!(service(&m).starts_with("charter/ops/"), "{}", service(&m));
    reads_everything(&m);
    m
}

fn replace_in(file: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains(from), "{}: {text}", file.display());
    std::fs::write(file, text.replace(from, to)).unwrap();
}

fn record_of(half: &mut Map<String, Value>) -> &mut Map<String, Value> {
    half["vaults"]["team"]["config"][identity::MARK]
        .as_object_mut()
        .unwrap()
}

/// The project as every reader sees it now: its state folder wherever it is.
fn ctx(plane: &Path) -> Ctx {
    Ctx::new(plane, Env::of(&[]))
}

fn vault(plane: &Path, name: &str) -> Vault {
    registry::vault(&ctx(plane), name).unwrap()
}

fn stub(plane: &Path) -> FileStore {
    FileStore::at(ctx(plane).state.join(keyring::STUB_FILE))
}

fn item(m: &Machine, service: &str, account: &str) -> Option<String> {
    stub(&m.plane)
        .get(service, account)
        .unwrap()
        .map(Secret::into_inner)
}

fn service(m: &Machine) -> String {
    keyring::load_index(&ctx(&m.plane), &vault(&m.plane, "ops"))
        .unwrap()
        .service
        .unwrap()
}

/// The identity record's item id and its `base`.
fn record(m: &Machine) -> (String, Option<String>) {
    let mut half = registry::load_local(&ctx(&m.plane)).unwrap();
    let rec = record_of(&mut half);
    (
        rec["ids"][SOURCE].as_str().unwrap().to_owned(),
        rec.get(identity::BASE)
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
}

/// Every secret and the token read the way charter reads them.
fn reads_everything(m: &Machine) {
    let ctx = ctx(&m.plane);
    let ops = vault(&m.plane, "ops");
    assert_eq!(keyring::get(&ctx, &ops, "API_TOKEN").unwrap(), API);
    assert_eq!(keyring::get(&ctx, &ops, "DB_PASSWORD").unwrap(), DB);
    assert_eq!(
        identity::from_keyring(&ctx, &vault(&m.plane, "team"), SOURCE)
            .unwrap()
            .as_deref(),
        Some(TOKEN)
    );
}

fn nobody_running() -> Seams<'static> {
    Seams {
        busy: &|_, _| None,
        ..Seams::real()
    }
}

/// The files the switch rewrites, as bytes: the keys index and the registry's local half.
fn index_and_record(m: &Machine) -> (Vec<u8>, Vec<u8>) {
    let ctx = ctx(&m.plane);
    (
        std::fs::read(keyring::index_path(&ctx, &vault(&m.plane, "ops"))).unwrap(),
        std::fs::read(ctx.local_registry()).unwrap(),
    )
}

fn failures(moved: &renamelocal::Moved) -> Vec<&String> {
    moved.said.iter().filter(|l| l.starts_with('✗')).collect()
}

#[test]
fn each_item_is_copied_read_back_and_switched_to_and_the_old_item_is_kept() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let (id, _) = record(&m);

    let moved = renamelocal::run(&m.local, &nobody_running());

    assert!(moved.complete && moved.changed, "{:#?}", moved.said);
    // The vault reads under the purlis service with the same random tail, the record under the
    // purlis base.
    let new = service(&m);
    assert_eq!(new, old.replacen("charter/", "purlis/", 1));
    assert_eq!(record(&m), (id.clone(), Some("purlis".to_owned())));
    reads_everything(&m);
    // Copied, and the old items kept.
    for (key, value) in [("API_TOKEN", API), ("DB_PASSWORD", DB)] {
        assert_eq!(item(&m, &new, key).as_deref(), Some(value));
        assert_eq!(item(&m, &old, key).as_deref(), Some(value));
    }
    let (old_id, new_id) = identity::item_services(&id);
    assert_eq!(item(&m, &new_id, SOURCE).as_deref(), Some(TOKEN));
    assert_eq!(item(&m, &old_id, SOURCE).as_deref(), Some(TOKEN));
    assert!(
        moved.said.iter().any(|l| l.contains("vault 'ops'")
            && l.contains("2 secret(s) copied")
            && l.contains("kept")),
        "{:#?}",
        moved.said
    );
    // Journalled, and nothing left to do the next time.
    let journal =
        std::fs::read_to_string(machine::dir(&m.local.config_root).join(renamelocal::JOURNAL))
            .unwrap();
    assert!(journal.contains("\"op\":\"switched\""), "{journal}");
    assert!(journal.contains("\"op\":\"rebased\""), "{journal}");
    let again = renamelocal::run(&m.local, &nobody_running());
    assert!(again.complete && !again.changed, "{:#?}", again.said);
}

#[test]
fn the_undo_reads_the_old_items_again_and_gives_back_the_index_and_record_byte_for_byte() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let before = index_and_record(&m);
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    let new = service(&m);

    let undone = renamelocal::undo(&m.local, &nobody_running());

    assert!(undone.complete && undone.changed, "{:#?}", undone.said);
    assert_eq!(names::state(&m.plane), m.plane.join(".charter"));
    assert_eq!(service(&m), old);
    assert_eq!(record(&m).1, None);
    assert!(
        index_and_record(&m) == before,
        "the index or the record changed"
    );
    reads_everything(&m);
    // Reads come from the old items: the copies are kept, and changing them changes nothing.
    stub(&m.plane).set(&new, "API_TOKEN", "not-read").unwrap();
    assert_eq!(
        keyring::get(&ctx(&m.plane), &vault(&m.plane, "ops"), "API_TOKEN").unwrap(),
        API
    );
    // Run again on purpose: the copy this machine made is written again, and switched to.
    let again = renamelocal::run(&m.local, &nobody_running());
    assert!(again.complete, "{:#?}", again.said);
    assert_eq!(service(&m), new);
    reads_everything(&m);
}

#[test]
fn a_secret_written_after_the_switch_is_copied_back_by_the_undo() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    let ctx = ctx(&m.plane);
    let ops = vault(&m.plane, "ops");
    keyring::set_with(
        &stub(&m.plane),
        &ctx,
        &ops,
        "API_TOKEN",
        "rotated-1",
        "2026-10-06T09:00:00Z",
    )
    .unwrap();
    keyring::set_with(
        &stub(&m.plane),
        &ctx,
        &ops,
        "NEW_KEY",
        "added-2",
        "2026-10-06T09:00:00Z",
    )
    .unwrap();

    let undone = renamelocal::undo(&m.local, &nobody_running());

    assert!(undone.complete, "{:#?}", undone.said);
    assert_eq!(service(&m), old);
    let ctx = self::ctx(&m.plane);
    assert_eq!(keyring::get(&ctx, &ops, "API_TOKEN").unwrap(), "rotated-1");
    assert_eq!(keyring::get(&ctx, &ops, "NEW_KEY").unwrap(), "added-2");
    assert_eq!(keyring::get(&ctx, &ops, "DB_PASSWORD").unwrap(), DB);
}

/// The stub, with every write garbled: what a copy that does not read back the same looks like.
struct Garbling(FileStore);

impl Store for Garbling {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        self.0.get(service, account)
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.0.set(service, account, &format!("{value}-garbled"))
    }
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        self.0.delete(service, account)
    }
}

#[test]
fn a_copy_that_does_not_read_back_the_same_leaves_everything_on_the_old_prefix() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let garbling = |ctx: &Ctx, _: Asking| {
        Some(Reach::Every(Box::new(Garbling(FileStore::at(
            ctx.state.join(keyring::STUB_FILE),
        )))))
    };
    let seams = Seams {
        keyring: &garbling,
        ..nobody_running()
    };

    let moved = renamelocal::run(&m.local, &seams);

    assert!(!moved.complete, "{:#?}", moved.said);
    let failed = failures(&moved);
    assert_eq!(failed.len(), 2, "{failed:#?}");
    assert!(
        failed
            .iter()
            .all(|l| l.contains("did not read back the same")),
        "{failed:#?}"
    );
    assert_eq!(service(&m), old);
    assert_eq!(record(&m).1, None);
    reads_everything(&m);

    // The next run writes its own garbled copies again, and switches once they read back.
    let again = renamelocal::run(&m.local, &nobody_running());
    assert!(again.complete, "{:#?}", again.said);
    assert!(service(&m).starts_with("purlis/ops/"));
    assert_eq!(record(&m).1.as_deref(), Some("purlis"));
    reads_everything(&m);
}

#[test]
fn an_item_planted_under_purlis_with_another_value_is_refused_and_never_read() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let new = old.replacen("charter/", "purlis/", 1);
    let (id, _) = record(&m);
    let (_, new_id) = identity::item_services(&id);
    stub(&m.plane).set(&new, "DB_PASSWORD", "planted").unwrap();
    stub(&m.plane)
        .set(&new_id, SOURCE, "planted-token")
        .unwrap();
    // One planted with the very value counts as copied.
    stub(&m.plane).set(&new, "API_TOKEN", API).unwrap();

    let moved = renamelocal::run(&m.local, &nobody_running());

    assert!(!moved.complete, "{:#?}", moved.said);
    let failed = failures(&moved);
    assert_eq!(failed.len(), 2, "{failed:#?}");
    assert!(
        failed
            .iter()
            .all(|l| l.contains("holds a different value") && l.contains("never trusts")),
        "{failed:#?}"
    );
    assert_eq!(service(&m), old);
    assert_eq!(record(&m).1, None);
    reads_everything(&m);
    // Left as it was found: not overwritten, not deleted.
    assert_eq!(item(&m, &new, "DB_PASSWORD").as_deref(), Some("planted"));
    assert_eq!(item(&m, &new_id, SOURCE).as_deref(), Some("planted-token"));
    // And refused again the next time, until someone removes it.
    assert!(!renamelocal::run(&m.local, &nobody_running()).complete);
    assert_eq!(service(&m), old);
}

#[test]
fn an_item_under_purlis_alone_is_never_switched_to() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let new = old.replacen("charter/", "purlis/", 1);
    stub(&m.plane).delete(&old, "DB_PASSWORD").unwrap();
    stub(&m.plane).set(&new, "DB_PASSWORD", "planted").unwrap();

    let moved = renamelocal::run(&m.local, &nobody_running());

    assert!(
        failures(&moved).iter().any(|l| l.contains("vault 'ops'")),
        "{:#?}",
        moved.said
    );
    assert_eq!(service(&m), old);
}

/// The stub, failing every write after the first `ok`: a run cut off part of the way.
struct CutOff {
    under: FileStore,
    ok: usize,
    writes: AtomicUsize,
}

impl Store for CutOff {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        self.under.get(service, account)
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        if self.writes.fetch_add(1, Ordering::SeqCst) >= self.ok {
            return Err(VaultError::new("the keychain went away"));
        }
        self.under.set(service, account, value)
    }
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        self.under.delete(service, account)
    }
}

fn cut_off_after_one(m: &Machine) -> renamelocal::Moved {
    // Asked once for the one project: one write in all, across the vault and the record.
    let cut = |ctx: &Ctx, _: Asking| {
        Some(Reach::Every(Box::new(CutOff {
            under: FileStore::at(ctx.state.join(keyring::STUB_FILE)),
            ok: 1,
            writes: AtomicUsize::new(0),
        }) as Box<dyn Store>))
    };
    renamelocal::run(
        &m.local,
        &Seams {
            keyring: &cut,
            ..nobody_running()
        },
    )
}

#[test]
fn a_run_cut_off_part_of_the_way_is_finished_by_the_next() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);

    let cut = cut_off_after_one(&m);

    assert!(!cut.complete, "{:#?}", cut.said);
    assert!(
        failures(&cut)
            .iter()
            .any(|l| l.contains("the keychain went away")),
        "{:#?}",
        cut.said
    );
    assert_eq!(service(&m), old);
    reads_everything(&m);

    let finished = renamelocal::run(&m.local, &nobody_running());

    assert!(finished.complete, "{:#?}", finished.said);
    assert!(service(&m).starts_with("purlis/ops/"));
    assert_eq!(record(&m).1.as_deref(), Some("purlis"));
    reads_everything(&m);
}

#[test]
fn a_run_cut_off_part_of_the_way_is_undone_cleanly() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let before = index_and_record(&m);
    assert!(!cut_off_after_one(&m).complete);

    let undone = renamelocal::undo(&m.local, &nobody_running());

    assert!(undone.complete, "{:#?}", undone.said);
    assert_eq!(service(&m), old);
    assert!(
        index_and_record(&m) == before,
        "the index or the record changed"
    );
    reads_everything(&m);
}

#[test]
fn a_switch_journalled_but_never_made_is_left_alone_by_the_undo_and_made_by_the_next_run() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let new = old.replacen("charter/", "purlis/", 1);
    let before = index_and_record(&m);
    let plane = std::fs::canonicalize(&m.plane).unwrap();
    let journal = machine::dir(&m.local.config_root).join(renamelocal::JOURNAL);
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    let line = json!({
        "op": "switched", "plane": plane, "vault": "ops", "from": old, "to": new,
        "keys": {"API_TOKEN": "2026-10-01T00:00:00Z", "DB_PASSWORD": "2026-10-01T00:00:00Z"},
    });
    std::fs::write(&journal, format!("{line}\n")).unwrap();

    let undone = renamelocal::undo(&m.local, &nobody_running());

    assert!(undone.complete, "{:#?}", undone.said);
    assert!(index_and_record(&m) == before);
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    assert_eq!(service(&m), new);
}

#[test]
fn an_undo_of_a_switch_to_a_service_rename_local_never_makes_is_refused_whole() {
    purlis_core::unsteered!();
    let m = machine();
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    let new = service(&m);
    let plane = std::fs::canonicalize(&m.plane).unwrap();
    let journal = machine::dir(&m.local.config_root).join(renamelocal::JOURNAL);
    let forged = json!({
        "op": "switched", "plane": plane, "vault": "ops", "from": "charter/other/3f9a2c1b",
        "to": new, "keys": {},
    });
    let mut text = std::fs::read_to_string(&journal).unwrap();
    text.push_str(&format!("{forged}\n"));
    std::fs::write(&journal, text).unwrap();

    let undone = renamelocal::undo(&m.local, &nobody_running());

    assert!(undone.refused.is_some(), "{undone:#?}");
    assert_eq!(service(&m), new);
}

#[test]
fn a_store_that_would_ask_for_each_item_leaves_the_copy_to_the_app() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let asks = |_: &Ctx, _: Asking| None;

    let moved = renamelocal::run(
        &m.local,
        &Seams {
            keyring: &asks,
            ..nobody_running()
        },
    );

    assert!(moved.complete, "{:#?}", moved.said);
    assert!(
        moved
            .said
            .iter()
            .any(|l| l.contains("the app copies them at its next launch")),
        "{:#?}",
        moved.said
    );
    assert_eq!(service(&m), old);
    assert_eq!(record(&m).1, None);
    reads_everything(&m);
}

/// The stub as a store that holds items to the app, as macOS's does: each write is answered by
/// `answer`, told whether an item was there before it; `writes` counts them.
struct Holding {
    under: FileStore,
    answer: fn(bool) -> Held,
    writes: AtomicUsize,
    /// Items another program owns: a fresh write refuses each and writes nothing into it.
    foreign: Vec<(String, String)>,
}

impl Store for Holding {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        self.under.get(service, account)
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        let was_there = self.under.get(service, account)?.is_some();
        self.under.set(service, account, value)?;
        Ok((self.answer)(was_there))
    }
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        self.under.delete(service, account)
    }
    fn holds(&self) -> bool {
        true
    }
    fn make_fresh(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        if self
            .foreign
            .iter()
            .any(|(s, a)| s == service && a == account)
        {
            return Err(VaultError::new("another program's item is there"));
        }
        self.set(service, account, value)
    }
    /// A terminal's copy: made by this process, so never written into an item it did not make.
    fn make_own(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        if self
            .foreign
            .iter()
            .any(|(s, a)| s == service && a == account)
        {
            return Err(VaultError::new("another program's item is there"));
        }
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.under.set(service, account, value)?;
        Ok(Held::NotYet)
    }
}

fn holding(ctx: &Ctx, answer: fn(bool) -> Held) -> Box<dyn Store> {
    holding_with(ctx, answer, Vec::new())
}

fn holding_with(
    ctx: &Ctx,
    answer: fn(bool) -> Held,
    foreign: Vec<(String, String)>,
) -> Box<dyn Store> {
    Box::new(Holding {
        under: FileStore::at(ctx.state.join(keyring::STUB_FILE)),
        answer,
        writes: AtomicUsize::new(0),
        foreign,
    })
}

#[test]
fn a_copy_the_store_could_not_hold_to_the_app_is_never_switched_to() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let not_yet = |ctx: &Ctx, _: Asking| Some(Reach::Every(holding(ctx, |_| Held::NotYet)));

    let moved = renamelocal::run(
        &m.local,
        &Seams {
            keyring: &not_yet,
            ..nobody_running()
        },
    );

    assert!(!moved.complete, "{:#?}", moved.said);
    let failed = failures(&moved);
    assert_eq!(failed.len(), 2, "{failed:#?}");
    assert!(
        failed
            .iter()
            .all(|l| l.contains("could not be made the app's own")),
        "{failed:#?}"
    );
    assert_eq!(service(&m), old);
    assert_eq!(record(&m).1, None);
    reads_everything(&m);
}

#[test]
fn an_undo_whose_copy_back_the_store_could_not_hold_keeps_reading_the_copies() {
    purlis_core::unsteered!();
    let m = machine();
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    let new = service(&m);
    let (ctx, ops) = (ctx(&m.plane), vault(&m.plane, "ops"));
    keyring::set_with(
        &stub(&m.plane),
        &ctx,
        &ops,
        "NEW_KEY",
        "added-3",
        "2026-10-06T09:00:00Z",
    )
    .unwrap();
    let not_yet = |ctx: &Ctx, _: Asking| Some(Reach::Every(holding(ctx, |_| Held::NotYet)));

    let undone = renamelocal::undo(
        &m.local,
        &Seams {
            keyring: &not_yet,
            ..nobody_running()
        },
    );

    assert!(!undone.complete, "{:#?}", undone.said);
    assert!(
        failures(&undone)
            .iter()
            .any(|l| l.contains("could not be made the app's own")),
        "{:#?}",
        undone.said
    );
    assert_eq!(service(&m), new);
    let ctx = self::ctx(&m.plane);
    assert_eq!(keyring::get(&ctx, &ops, "NEW_KEY").unwrap(), "added-3");
    // A second undo, through a store that holds it, finishes it.
    assert!(renamelocal::undo(&m.local, &nobody_running()).complete);
    assert!(service(&m).starts_with("charter/ops/"));
    assert_eq!(
        keyring::get(&self::ctx(&m.plane), &ops, "NEW_KEY").unwrap(),
        "added-3"
    );
}

#[test]
fn where_items_are_held_an_item_there_with_the_same_value_is_made_again_by_the_app() {
    purlis_core::unsteered!();
    let m = machine();
    let new = service(&m).replacen("charter/", "purlis/", 1);
    stub(&m.plane).set(&new, "API_TOKEN", API).unwrap();
    // What was there before is another program's: the app cannot make it its own.
    let planted = vec![(new.clone(), "API_TOKEN".to_owned())];
    let foreign = move |ctx: &Ctx, _: Asking| {
        Some(Reach::Every(holding_with(
            ctx,
            |_| Held::ToTheApp,
            planted.clone(),
        )))
    };

    let moved = renamelocal::run(
        &m.local,
        &Seams {
            keyring: &foreign,
            ..nobody_running()
        },
    );

    let failed = failures(&moved);
    assert_eq!(failed.len(), 1, "{:#?}", moved.said);
    assert!(
        failed[0].contains("vault 'ops'") && failed[0].contains("another program's item"),
        "{failed:#?}"
    );
    assert!(service(&m).starts_with("charter/ops/"));
    // The identity, with nothing there before, moved.
    assert_eq!(record(&m).1.as_deref(), Some("purlis"));

    // An app that can make it its own writes a fresh item over the same value, then switches.
    let writes = std::sync::Arc::new(AtomicUsize::new(0));
    let counted = {
        let writes = writes.clone();
        move |ctx: &Ctx, _: Asking| {
            Some(Reach::Every(Box::new(Counted {
                under: FileStore::at(ctx.state.join(keyring::STUB_FILE)),
                writes: writes.clone(),
            }) as Box<dyn Store>))
        }
    };
    let moved = renamelocal::run(
        &m.local,
        &Seams {
            keyring: &counted,
            ..nobody_running()
        },
    );
    assert!(moved.complete, "{:#?}", moved.said);
    assert_eq!(service(&m), new);
    assert_eq!(
        writes.load(Ordering::SeqCst),
        2,
        "each of the vault's items was made again"
    );
    reads_everything(&m);
}

/// A holding store that always holds, counting writes across every store it is asked for.
struct Counted {
    under: FileStore,
    writes: std::sync::Arc<AtomicUsize>,
}

impl Store for Counted {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        self.under.get(service, account)
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.under.set(service, account, value)?;
        Ok(Held::ToTheApp)
    }
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        self.under.delete(service, account)
    }
    fn holds(&self) -> bool {
        true
    }
}

/// The stub, over which a `secret set` of `DB_PASSWORD` lands right after its copy is written:
/// while the index still names the old service, so it writes there.
struct SetDuringTheCopy {
    under: FileStore,
    plane: PathBuf,
}

impl Store for SetDuringTheCopy {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        self.under.get(service, account)
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        let held = self.under.set(service, account, value)?;
        if account == "DB_PASSWORD" && service.starts_with("purlis/ops/") {
            keyring::set_with(
                &self.under,
                &ctx(&self.plane),
                &vault(&self.plane, "ops"),
                "DB_PASSWORD",
                "rotated-mid-copy",
                "2026-10-06T10:00:00Z",
            )?;
        }
        Ok(held)
    }
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        self.under.delete(service, account)
    }
}

#[test]
fn a_secret_written_between_its_copy_and_the_switch_is_never_left_behind() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let racing = |ctx: &Ctx, _: Asking| {
        Some(Reach::Every(Box::new(SetDuringTheCopy {
            under: FileStore::at(ctx.state.join(keyring::STUB_FILE)),
            plane: ctx.root.clone(),
        }) as Box<dyn Store>))
    };

    let moved = renamelocal::run(
        &m.local,
        &Seams {
            keyring: &racing,
            ..nobody_running()
        },
    );

    assert!(
        failures(&moved)
            .iter()
            .any(|l| l.contains("vault 'ops'") && l.contains("changed while it was copied")),
        "{:#?}",
        moved.said
    );
    assert_eq!(service(&m), old);
    let (ctx, ops) = (ctx(&m.plane), vault(&m.plane, "ops"));
    assert_eq!(
        keyring::get(&ctx, &ops, "DB_PASSWORD").unwrap(),
        "rotated-mid-copy"
    );

    // The next run copies the newer value and switches.
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    assert!(service(&m).starts_with("purlis/ops/"));
    assert_eq!(
        keyring::get(&self::ctx(&m.plane), &ops, "DB_PASSWORD").unwrap(),
        "rotated-mid-copy"
    );
}

fn journal(m: &Machine, line: &Value) {
    let journal = machine::dir(&m.local.config_root).join(renamelocal::JOURNAL);
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    let mut text = std::fs::read_to_string(&journal).unwrap_or_default();
    text.push_str(&format!("{line}\n"));
    std::fs::write(&journal, text).unwrap();
}

#[test]
fn another_programs_item_where_a_copy_was_journalled_is_never_written_into() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let new = old.replacen("charter/", "purlis/", 1);
    // A copy journalled whose write never landed, and another program's item there since.
    journal(
        &m,
        &json!({"op": "copied", "service": new, "account": "API_TOKEN"}),
    );
    stub(&m.plane).set(&new, "API_TOKEN", "planted").unwrap();
    let planted = vec![(new.clone(), "API_TOKEN".to_owned())];
    let foreign = move |ctx: &Ctx, _: Asking| {
        Some(Reach::Every(holding_with(
            ctx,
            |_| Held::ToTheApp,
            planted.clone(),
        )))
    };
    let seams = Seams {
        keyring: &foreign,
        ..nobody_running()
    };

    let moved = renamelocal::run(&m.local, &seams);

    assert!(
        failures(&moved)
            .iter()
            .any(|l| l.contains("vault 'ops'") && l.contains("another program's item")),
        "{:#?}",
        moved.said
    );
    assert_eq!(service(&m), old);
    assert_eq!(item(&m, &new, "API_TOKEN").as_deref(), Some("planted"));
    reads_everything(&m);

    assert!(renamelocal::undo(&m.local, &seams).complete);
    assert_eq!(item(&m, &new, "API_TOKEN").as_deref(), Some("planted"));
    assert_eq!(service(&m), old);
}

#[test]
fn another_programs_item_where_the_undo_copies_back_a_new_key_is_never_written_into() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    let new = service(&m);
    let (ctx, ops) = (ctx(&m.plane), vault(&m.plane, "ops"));
    keyring::set_with(
        &stub(&m.plane),
        &ctx,
        &ops,
        "NEW_KEY",
        "added-4",
        "2026-10-06T09:00:00Z",
    )
    .unwrap();
    // The key has no old item: another program makes one there, under its own access.
    stub(&m.plane).set(&old, "NEW_KEY", "planted").unwrap();
    let planted = vec![(old.clone(), "NEW_KEY".to_owned())];
    let foreign = move |ctx: &Ctx, _: Asking| {
        Some(Reach::Every(holding_with(
            ctx,
            |_| Held::ToTheApp,
            planted.clone(),
        )))
    };
    let seams = Seams {
        keyring: &foreign,
        ..nobody_running()
    };

    let undone = renamelocal::undo(&m.local, &seams);

    assert!(!undone.complete, "{:#?}", undone.said);
    assert!(
        failures(&undone)
            .iter()
            .any(|l| l.contains("another program's item")),
        "{:#?}",
        undone.said
    );
    assert_eq!(item(&m, &old, "NEW_KEY").as_deref(), Some("planted"));
    assert_eq!(service(&m), new);
    assert_eq!(
        keyring::get(&self::ctx(&m.plane), &ops, "NEW_KEY").unwrap(),
        "added-4"
    );

    // A run after it leaves it as it is too.
    renamelocal::run(&m.local, &seams);
    assert_eq!(item(&m, &old, "NEW_KEY").as_deref(), Some("planted"));
    assert_eq!(service(&m), new);
}

// ---------------------------------------------------------------------------------------
// No Keychain prompts at launch (#1306).

/// The stub as macOS's store: it holds items to the app, and an item in `asks` makes the
/// Keychain ask whoever reads it. With the dialogs off (`quiet`, a run's copy) that read fails
/// the way macOS fails it; with them on the person is asked, said yes, and it is counted.
struct Asks {
    under: FileStore,
    asks: Vec<(String, String)>,
    quiet: bool,
    /// Whether the person, asked, says no.
    denies: bool,
    asked: std::sync::Arc<AtomicUsize>,
    /// Items this process made itself (`make_own`): a terminal's copies.
    own: std::sync::Arc<AtomicUsize>,
}

impl Store for Asks {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        if self.asks.iter().any(|(s, a)| s == service && a == account) {
            if self.quiet {
                return Err(VaultError::would_ask(format!(
                    "charter could not read '{service}': User interaction is not allowed."
                )));
            }
            self.asked.fetch_add(1, Ordering::SeqCst);
            if self.denies {
                return Err(VaultError::new(format!(
                    "charter could not read '{service}': The user name or passphrase you \
                     entered is not correct."
                )));
            }
        }
        self.under.get(service, account)
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.under.set(service, account, value)?;
        Ok(Held::ToTheApp)
    }
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        self.under.delete(service, account)
    }
    fn holds(&self) -> bool {
        true
    }
    fn make_own(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.own.fetch_add(1, Ordering::SeqCst);
        self.under.set(service, account, value)?;
        Ok(Held::NotYet)
    }
}

/// How a test's keychain answers, and what it counted.
#[derive(Clone, Default)]
struct Keychain {
    asks: Vec<(String, String)>,
    denies: bool,
    asked: std::sync::Arc<AtomicUsize>,
    own: std::sync::Arc<AtomicUsize>,
}

impl Keychain {
    fn asking_for(asks: Vec<(String, String)>) -> Self {
        Self {
            asks,
            ..Self::default()
        }
    }

    fn store(&self, ctx: &Ctx, asking: Asking) -> Box<dyn Store> {
        Box::new(Asks {
            under: FileStore::at(ctx.state.join(keyring::STUB_FILE)),
            asks: self.asks.clone(),
            quiet: asking == Asking::Never,
            denies: self.denies,
            asked: self.asked.clone(),
            own: self.own.clone(),
        })
    }

    /// As the app reaches it.
    fn in_the_app(&self) -> impl Fn(&Ctx, Asking) -> Option<Reach> + use<> {
        let me = self.clone();
        move |ctx: &Ctx, asking: Asking| Some(Reach::Every(me.store(ctx, asking)))
    }

    /// As the command in a terminal reaches it.
    fn in_a_terminal(&self) -> impl Fn(&Ctx, Asking) -> Option<Reach> + use<> {
        let me = self.clone();
        move |ctx: &Ctx, asking: Asking| Some(Reach::ItsOwn(me.store(ctx, asking)))
    }

    fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

#[test]
fn at_launch_a_vault_whose_items_would_ask_waits_on_the_old_prefix_and_keeps_working() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    // Held to an earlier build: reading it would make the system ask.
    let keychain = Keychain::asking_for(vec![(old.clone(), "API_TOKEN".to_owned())]);
    let app = keychain.in_the_app();

    let moved = renamelocal::run(
        &m.local,
        &Seams {
            keyring: &app,
            ..nobody_running()
        },
    );

    assert_eq!(keychain.asked(), 0, "nothing asked at launch");
    // Not a failure: the vault is whole, on the old prefix, and said to wait.
    assert!(moved.complete, "{:#?}", moved.said);
    assert_eq!(
        moved.waiting,
        vec![renamelocal::Waiting {
            plane: m.plane.clone(),
            vault: "ops".to_owned(),
            identity: false,
            items: 2,
            hold: false,
        }]
    );
    assert!(
        moved
            .said
            .iter()
            .any(|l| l.starts_with('–') && l.contains("vault 'ops'") && l.contains("waits")),
        "{:#?}",
        moved.said
    );
    assert_eq!(service(&m), old, "not half-switched");
    // The record, which asks nothing, moved.
    assert_eq!(record(&m).1.as_deref(), Some("purlis"));
    reads_everything(&m);
}

#[test]
fn finishing_moves_the_waiting_vaults_with_the_keychain_asking_and_the_undo_puts_them_back() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let keychain = Keychain::asking_for(vec![(old.clone(), "API_TOKEN".to_owned())]);
    let app = keychain.in_the_app();
    let seams = Seams {
        keyring: &app,
        ..nobody_running()
    };
    let launched = renamelocal::run(&m.local, &seams);
    assert_eq!(launched.waiting.len(), 1, "{:#?}", launched.said);

    let finished = renamelocal::finish(&m.local, &seams, &launched.waiting);

    assert!(
        finished.complete && finished.changed,
        "{:#?}",
        finished.said
    );
    assert!(finished.waiting.is_empty(), "{:#?}", finished.said);
    assert_eq!(service(&m), old.replacen("charter/", "purlis/", 1));
    assert_eq!(
        keychain.asked(),
        1,
        "asked once, for the one item that asks"
    );
    reads_everything(&m);

    let undone = renamelocal::undo(&m.local, &seams);
    assert!(undone.complete, "{:#?}", undone.said);
    assert_eq!(service(&m), old);
    assert_eq!(record(&m).1, None);
    reads_everything(&m);
}

#[test]
fn finishing_moves_only_what_waits() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let app = Keychain::default().in_the_app();
    let seams = Seams {
        keyring: &app,
        ..nobody_running()
    };

    let finished = renamelocal::finish(&m.local, &seams, &[]);

    assert!(
        finished.complete && !finished.changed,
        "{:#?}",
        finished.said
    );
    assert_eq!(service(&m), old);
    assert_eq!(record(&m).1, None);
}

/// Mark every key of `vault`'s index held to the app, as the app's writes leave it.
fn held_to_the_app(m: &Machine, vault: &str) {
    let path = keyring::index_path(&ctx(&m.plane), &self::vault(&m.plane, vault));
    let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for entry in doc["keys"].as_object_mut().unwrap().values_mut() {
        entry["held"] = Value::Bool(true);
    }
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
}

#[test]
fn a_terminal_copies_a_vault_whose_every_key_is_the_commands_own_and_the_undo_puts_it_back() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    // A second vault the app wrote: held to it, so a terminal leaves it to the app.
    let ctx = ctx(&m.plane);
    registry::add_vault(&ctx, "app", "keyring", Map::new(), None, false, false).unwrap();
    let held = vault(&m.plane, "app");
    keyring::set_with(
        &stub(&m.plane),
        &ctx,
        &held,
        "K",
        "held-value",
        "2026-10-01T00:00:00Z",
    )
    .unwrap();
    let held_index = keyring::index_path(&ctx, &held);
    replace_in(&held_index, "\"purlis/app/", "\"charter/app/");
    let held_old = keyring::load_index(&ctx, &held).unwrap().service.unwrap();
    replace_in(
        &ctx.state.join(keyring::STUB_FILE),
        &format!("\"{}", held_old.replacen("charter/", "purlis/", 1)),
        &format!("\"{held_old}"),
    );
    held_to_the_app(&m, "app");
    let keychain = Keychain::default();
    let terminal = keychain.in_a_terminal();
    let seams = Seams {
        keyring: &terminal,
        ..nobody_running()
    };

    let moved = renamelocal::run(&m.local, &seams);

    assert!(moved.complete && moved.changed, "{:#?}", moved.said);
    // The state folder moved with the run.
    let ctx = self::ctx(&m.plane);
    let held = vault(&m.plane, "app");
    let new = old.replacen("charter/", "purlis/", 1);
    assert_eq!(service(&m), new);
    assert_eq!(
        keychain.own.load(Ordering::SeqCst),
        2,
        "each copy made by the command itself"
    );
    for (key, value) in [("API_TOKEN", API), ("DB_PASSWORD", DB)] {
        assert_eq!(item(&m, &new, key).as_deref(), Some(value));
        assert_eq!(item(&m, &old, key).as_deref(), Some(value));
    }
    // The vault held to the app, and the identity record, are left to the app's launch.
    assert_eq!(
        keyring::load_index(&ctx, &held).unwrap().service.unwrap(),
        held_old
    );
    assert_eq!(record(&m).1, None);
    for what in ["vault 'app'", "identity"] {
        assert!(
            moved
                .said
                .iter()
                .any(|l| l.starts_with('–') && l.contains(what) && l.contains("next launch")),
            "{what}: {:#?}",
            moved.said
        );
    }
    assert!(moved.waiting.is_empty(), "{:#?}", moved.waiting);
    reads_everything(&m);

    // A key written since, then the undo from the terminal: copied back by the command too.
    let ops = vault(&m.plane, "ops");
    keyring::set_with(
        &stub(&m.plane),
        &ctx,
        &ops,
        "NEW_KEY",
        "added-1306",
        "2026-10-06T09:00:00Z",
    )
    .unwrap();
    let undone = renamelocal::undo(&m.local, &seams);
    assert!(undone.complete, "{:#?}", undone.said);
    assert_eq!(service(&m), old);
    assert_eq!(item(&m, &old, "NEW_KEY").as_deref(), Some("added-1306"));
    assert_eq!(keychain.own.load(Ordering::SeqCst), 3);
    reads_everything(&m);
}

#[test]
fn a_terminal_leaves_a_vault_of_its_own_whose_items_would_ask_waiting() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    // The command's own, but an earlier build's: reading it would ask.
    let keychain = Keychain::asking_for(vec![(old.clone(), "DB_PASSWORD".to_owned())]);
    let terminal = keychain.in_a_terminal();

    let moved = renamelocal::run(
        &m.local,
        &Seams {
            keyring: &terminal,
            ..nobody_running()
        },
    );

    assert!(moved.complete, "{:#?}", moved.said);
    assert_eq!(keychain.asked(), 0);
    assert_eq!(service(&m), old);
    assert_eq!(moved.waiting.len(), 1, "{:#?}", moved.said);
    assert!(
        moved
            .said
            .iter()
            .any(|l| l.contains("waits") && l.contains("the app offers to finish moving it")),
        "{:#?}",
        moved.said
    );
    reads_everything(&m);
}

#[test]
fn a_vault_the_person_does_not_allow_while_finishing_still_waits_and_keeps_working() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let keychain = Keychain {
        denies: true,
        ..Keychain::asking_for(vec![(old.clone(), "API_TOKEN".to_owned())])
    };
    let app = keychain.in_the_app();
    let seams = Seams {
        keyring: &app,
        ..nobody_running()
    };
    let launched = renamelocal::run(&m.local, &seams);

    let finished = renamelocal::finish(&m.local, &seams, &launched.waiting);

    assert!(!finished.complete, "{:#?}", finished.said);
    assert_eq!(finished.waiting, launched.waiting, "offered again");
    assert_eq!(keychain.asked(), 1);
    assert_eq!(service(&m), old);
    reads_everything(&m);
}

#[test]
fn from_a_terminal_another_programs_item_where_a_copy_was_journalled_is_never_written_into() {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let new = old.replacen("charter/", "purlis/", 1);
    journal(
        &m,
        &json!({"op": "copied", "service": new, "account": "API_TOKEN"}),
    );
    stub(&m.plane).set(&new, "API_TOKEN", "planted").unwrap();
    let planted = vec![(new.clone(), "API_TOKEN".to_owned())];
    let terminal = move |ctx: &Ctx, _: Asking| {
        Some(Reach::ItsOwn(holding_with(
            ctx,
            |_| Held::NotYet,
            planted.clone(),
        )))
    };
    let seams = Seams {
        keyring: &terminal,
        ..nobody_running()
    };

    let moved = renamelocal::run(&m.local, &seams);

    assert!(
        failures(&moved)
            .iter()
            .any(|l| l.contains("vault 'ops'") && l.contains("another program's item")),
        "{:#?}",
        moved.said
    );
    assert_eq!(service(&m), old);
    assert_eq!(item(&m, &new, "API_TOKEN").as_deref(), Some("planted"));
    reads_everything(&m);

    assert!(renamelocal::undo(&m.local, &seams).complete);
    assert_eq!(item(&m, &new, "API_TOKEN").as_deref(), Some("planted"));
    assert_eq!(service(&m), old);
}

#[test]
fn from_a_terminal_another_programs_item_where_the_undo_copies_back_a_new_key_is_never_written_into()
 {
    purlis_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let plain = Keychain::default().in_a_terminal();
    let moved = renamelocal::run(
        &m.local,
        &Seams {
            keyring: &plain,
            ..nobody_running()
        },
    );
    assert!(moved.complete, "{:#?}", moved.said);
    let new = service(&m);
    assert_ne!(new, old);
    let (ctx, ops) = (ctx(&m.plane), vault(&m.plane, "ops"));
    keyring::set_with(
        &stub(&m.plane),
        &ctx,
        &ops,
        "NEW_KEY",
        "added-5",
        "2026-10-06T09:00:00Z",
    )
    .unwrap();
    stub(&m.plane).set(&old, "NEW_KEY", "planted").unwrap();
    let planted = vec![(old.clone(), "NEW_KEY".to_owned())];
    let terminal = move |ctx: &Ctx, _: Asking| {
        Some(Reach::ItsOwn(holding_with(
            ctx,
            |_| Held::NotYet,
            planted.clone(),
        )))
    };
    let seams = Seams {
        keyring: &terminal,
        ..nobody_running()
    };

    let undone = renamelocal::undo(&m.local, &seams);

    assert!(!undone.complete, "{:#?}", undone.said);
    assert!(
        failures(&undone)
            .iter()
            .any(|l| l.contains("another program's item")),
        "{:#?}",
        undone.said
    );
    assert_eq!(item(&m, &old, "NEW_KEY").as_deref(), Some("planted"));
    assert_eq!(service(&m), new);
    assert_eq!(
        keyring::get(&self::ctx(&m.plane), &ops, "NEW_KEY").unwrap(),
        "added-5"
    );
}
