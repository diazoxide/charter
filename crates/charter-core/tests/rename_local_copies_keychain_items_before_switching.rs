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

use charter_core::machine;
use charter_core::names;
use charter_core::renamelocal::keychain::Asking;
use charter_core::renamelocal::{self, Local, Seams};
use charter_core::secrets::keyring::{self, FileStore, Held, Secret, Store};
use charter_core::secrets::registry::{self, Vault};
use charter_core::secrets::{Ctx, Env, VaultError, identity};
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
    let init = charter_core::forklock::output(
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
    let m = machine();
    let old = service(&m);
    let garbling = |ctx: &Ctx, _: Asking| {
        Some(
            Box::new(Garbling(FileStore::at(ctx.state.join(keyring::STUB_FILE)))) as Box<dyn Store>,
        )
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
        Some(Box::new(CutOff {
            under: FileStore::at(ctx.state.join(keyring::STUB_FILE)),
            ok: 1,
            writes: AtomicUsize::new(0),
        }) as Box<dyn Store>)
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
