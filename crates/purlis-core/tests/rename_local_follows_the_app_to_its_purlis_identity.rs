//! The app's first launch as `dev.purlis.app` (RN-9, V93b): its keychain items are held again by
//! the app under its new identity, and its log folder is moved before it opens a log file there.
//!
//! Driven through the core's public surface — [`renamelocal::run`] as the app's launch runs it
//! (`own_app` set), [`renamelocal::logs_at_launch_with`] and [`renamelocal::undo`] — on a
//! temporary home, and asserted on the keyring stand-in, the keys index and the folders
//! afterwards. Never the login keychain: a test build's store is the stub (`crate::fence`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use purlis_core::machine;
use purlis_core::renamelocal::keychain::{Asking, HELD_TO, Reach};
use purlis_core::renamelocal::{self, Local, Logs, Seams, Waiting};
use purlis_core::secrets::keyring::{self, FileStore, Held, Secret, Store};
use purlis_core::secrets::registry::{self, Vault};
use purlis_core::secrets::{Ctx, Env, VaultError, identity};
use serde_json::{Map, json};

const API: &str = "kr-api-1c9e3b7d5a";
const DB: &str = "kr-db-7f02a4e6c1";
const TOKEN: &str = "ops_team_token_9a3c1e";
const SOURCE: &str = "OP_TEAM_TOKEN";
const NEW_APP: &str = "dev.purlis.app";

/// A machine that remembers one project whose keychain items are already under `purlis/…` and
/// held to the app as it was called before: a keyring vault `ops` with two secrets, and a
/// 1Password vault `team` whose identity token is under `purlis/@identity/<id>`.
struct Machine {
    _dir: tempfile::TempDir,
    home: PathBuf,
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
    // Written where items are held, so the index says each is held (to the old app).
    let held = Holding::answering(&ctx, Held::ToTheApp);
    keyring::set_with(&held, &ctx, &ops, "API_TOKEN", API, "2026-10-01T00:00:00Z").unwrap();
    keyring::set_with(&held, &ctx, &ops, "DB_PASSWORD", DB, "2026-10-01T00:00:00Z").unwrap();
    let mut config = Map::new();
    config.insert("env".into(), json!({ "OP_SERVICE_ACCOUNT_TOKEN": SOURCE }));
    registry::add_vault(&ctx, "team", "1password", config, None, false, false).unwrap();
    identity::put_in_keyring(&ctx, &vault(&plane, "team"), TOKEN).unwrap();

    let m = Machine {
        _dir: dir,
        local: Local {
            config_root,
            data_base: None,
            logs: None,
            planes: Vec::new(),
            own_app: Some(NEW_APP.to_owned()),
            plugin: None,
        },
        home,
        plane,
    };
    assert!(service(&m).starts_with("purlis/ops/"), "{}", service(&m));
    assert_eq!(held_flags(&m), [true, true]);
    m
}

fn ctx(plane: &Path) -> Ctx {
    Ctx::new(plane, Env::of(&[]))
}

fn vault(plane: &Path, name: &str) -> Vault {
    registry::vault(&ctx(plane), name).unwrap()
}

fn service(m: &Machine) -> String {
    keyring::load_index(&ctx(&m.plane), &vault(&m.plane, "ops"))
        .unwrap()
        .service
        .unwrap()
}

/// Whether the index says each key's item is held to the app, by key.
fn held_flags(m: &Machine) -> Vec<bool> {
    keyring::load_index(&ctx(&m.plane), &vault(&m.plane, "ops"))
        .unwrap()
        .keys
        .values()
        .map(|entry| entry.held)
        .collect()
}

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

fn held_to(m: &Machine) -> Option<String> {
    std::fs::read_to_string(machine::dir(&m.local.config_root).join(HELD_TO)).ok()
}

/// The stub as a store that holds items to the app, as macOS's does. Every write of an item
/// again (`rehold`) is answered by `answer` and counted. An item in `asks` is one the system
/// would ask the person about: with the dialogs off ([`Asking::Never`]) its read fails as
/// would-ask; on the person's press ([`Asking::Allowed`]) the ask is counted, and it is read
/// unless it is also in `denied`.
struct Holding {
    under: FileStore,
    answer: Held,
    asking: Asking,
    reholds: Arc<AtomicUsize>,
    asked: Arc<AtomicUsize>,
    asks: Vec<String>,
    denied: Vec<String>,
}

impl Holding {
    fn answering(ctx: &Ctx, answer: Held) -> Self {
        Self {
            under: FileStore::at(ctx.state.join(keyring::STUB_FILE)),
            answer,
            asking: Asking::Never,
            reholds: Arc::new(AtomicUsize::new(0)),
            asked: Arc::new(AtomicUsize::new(0)),
            asks: Vec::new(),
            denied: Vec::new(),
        }
    }
}

impl Store for Holding {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, VaultError> {
        if self.asks.iter().any(|a| a == account) {
            if self.asking == Asking::Never {
                return Err(VaultError::would_ask("the system would ask"));
            }
            self.asked.fetch_add(1, Ordering::SeqCst);
            if self.denied.iter().any(|a| a == account) {
                return Err(VaultError::new("the person did not allow it"));
            }
        }
        self.under.get(service, account)
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.under.set(service, account, value)?;
        Ok(self.answer)
    }
    fn delete(&self, service: &str, account: &str) -> Result<bool, VaultError> {
        self.under.delete(service, account)
    }
    fn holds(&self) -> bool {
        true
    }
    fn rehold(&self, service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
        self.reholds.fetch_add(1, Ordering::SeqCst);
        self.set(service, account, value)
    }
}

fn nobody_running() -> Seams<'static> {
    Seams {
        busy: &|_, _| None,
        ..Seams::real()
    }
}

/// The keychain as one launch or one press sees it, counting what it wrote again and asked.
#[derive(Clone, Default)]
struct Keychain {
    answer: Option<Held>,
    asks: Vec<String>,
    denied: Vec<String>,
    reholds: Arc<AtomicUsize>,
    asked: Arc<AtomicUsize>,
}

impl Keychain {
    fn holding() -> Self {
        Self {
            answer: Some(Held::ToTheApp),
            ..Self::default()
        }
    }

    fn asking_for(keys: &[&str]) -> Self {
        Self {
            asks: keys.iter().map(|k| (*k).to_owned()).collect(),
            ..Self::holding()
        }
    }

    fn store(&self, ctx: &Ctx, asking: Asking) -> Box<dyn Store> {
        Box::new(Holding {
            asking,
            reholds: self.reholds.clone(),
            asked: self.asked.clone(),
            asks: self.asks.clone(),
            denied: self.denied.clone(),
            ..Holding::answering(ctx, self.answer.unwrap_or(Held::ToTheApp))
        })
    }

    fn reholds(&self) -> usize {
        self.reholds.load(Ordering::SeqCst)
    }

    fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

/// The launch's rename-local, through `keychain`.
fn launch(m: &Machine, keychain: &Keychain) -> renamelocal::Moved {
    let me = keychain.clone();
    let keyring = move |ctx: &Ctx, asking: Asking| Some(Reach::Every(me.store(ctx, asking)));
    renamelocal::run(
        &m.local,
        &Seams {
            keyring: &keyring,
            ..nobody_running()
        },
    )
}

/// The person's press on "Finish moving", through `keychain`.
fn finish(m: &Machine, keychain: &Keychain, waiting: &[Waiting]) -> renamelocal::Moved {
    let me = keychain.clone();
    let keyring = move |ctx: &Ctx, asking: Asking| Some(Reach::Every(me.store(ctx, asking)));
    renamelocal::finish(
        &m.local,
        &Seams {
            keyring: &keyring,
            ..nobody_running()
        },
        waiting,
    )
}

#[test]
fn the_first_launch_under_the_new_identity_holds_every_item_again_once() {
    purlis_core::unsteered!();
    let m = machine();
    assert_eq!(held_to(&m), None);
    let keychain = Keychain::holding();

    let moved = launch(&m, &keychain);

    assert!(moved.complete && moved.changed, "{:#?}", moved.said);
    assert!(moved.waiting.is_empty(), "{:#?}", moved.waiting);
    // Both secrets and the identity token, written again by the app that reads them now.
    assert_eq!(keychain.reholds(), 3, "{:#?}", moved.said);
    assert_eq!(held_flags(&m), [true, true]);
    assert!(
        moved
            .said
            .iter()
            .any(|l| l.contains("vault 'ops'") && l.contains("2 secret(s) held again by")),
        "{:#?}",
        moved.said
    );
    assert!(
        moved
            .said
            .iter()
            .any(|l| l.contains("vault 'team''s identity") && l.contains("held again by")),
        "{:#?}",
        moved.said
    );
    assert_eq!(held_to(&m).as_deref(), Some("dev.purlis.app\n"));
    reads_everything(&m);

    // The next launch, as the same app, holds nothing again.
    let again = Keychain::holding();
    let moved = launch(&m, &again);
    assert_eq!(again.reholds(), 0, "{:#?}", moved.said);
    assert!(moved.complete && !moved.changed, "{:#?}", moved.said);
}

#[test]
fn items_that_would_ask_never_ask_at_launch_and_their_vault_waits_and_keeps_working() {
    purlis_core::unsteered!();
    let m = machine();
    let keychain = Keychain::asking_for(&["DB_PASSWORD"]);

    let moved = launch(&m, &keychain);

    assert_eq!(keychain.asked(), 0, "nothing asked at launch");
    // Not a failure: the vault keeps working and is said to wait for the person.
    assert!(moved.complete, "{:#?}", moved.said);
    assert_eq!(
        moved.waiting,
        vec![Waiting {
            plane: std::fs::canonicalize(&m.plane).unwrap(),
            vault: "ops".to_owned(),
            identity: false,
            items: 1,
            hold: true,
        }]
    );
    // The key that would ask is left exactly as it was; the other was held again.
    assert_eq!(held_flags(&m), [true, true]);
    assert_eq!(held_to(&m), None, "the record waits for every item");
    reads_everything(&m);

    // The next launch asks nothing either, and makes nothing already done again.
    let next = Keychain::asking_for(&["DB_PASSWORD"]);
    let moved = launch(&m, &next);
    assert_eq!(next.asked(), 0);
    assert_eq!(moved.waiting.len(), 1, "{:#?}", moved.waiting);
    assert_eq!(
        next.reholds(),
        1,
        "only the vault that waits: {:#?}",
        moved.said
    );

    // The person's press asks once, for that item alone, and holds it.
    let press = Keychain::asking_for(&["DB_PASSWORD"]);
    let finished = finish(&m, &press, &moved.waiting);
    assert_eq!(press.asked(), 1, "{:#?}", finished.said);
    assert!(finished.waiting.is_empty(), "{:#?}", finished.said);
    // And the launch after it records that every item is held, asking nothing.
    let after = Keychain::asking_for(&["DB_PASSWORD"]);
    let moved = launch(&m, &after);
    assert_eq!(after.asked(), 0);
    assert_eq!(after.reholds(), 0, "{:#?}", moved.said);
    assert!(moved.waiting.is_empty());
    assert_eq!(held_to(&m).as_deref(), Some("dev.purlis.app\n"));
}

#[test]
fn an_item_the_person_does_not_allow_stays_offered_and_never_brings_the_asks_back_at_launch() {
    purlis_core::unsteered!();
    let m = machine();
    let waiting = launch(&m, &Keychain::asking_for(&["API_TOKEN", "DB_PASSWORD"])).waiting;
    assert_eq!(waiting.len(), 1, "{waiting:#?}");

    let press = Keychain {
        denied: vec!["DB_PASSWORD".to_owned()],
        ..Keychain::asking_for(&["API_TOKEN", "DB_PASSWORD"])
    };
    let finished = finish(&m, &press, &waiting);

    assert_eq!(press.asked(), 2, "{:#?}", finished.said);
    assert_eq!(finished.waiting, waiting, "still offered");
    // The key the person allowed is held again; the denied one is as it was.
    assert_eq!(held_flags(&m), [true, true]);
    reads_everything(&m);
    for _ in 0..3 {
        let next = Keychain::asking_for(&["API_TOKEN", "DB_PASSWORD"]);
        let moved = launch(&m, &next);
        assert_eq!(next.asked(), 0, "a launch never asks: {:#?}", moved.said);
        assert_eq!(moved.waiting.len(), 1);
    }
    assert_eq!(held_to(&m), None);
}

#[test]
fn an_item_the_app_could_not_hold_is_left_to_the_next_read_through_the_command() {
    purlis_core::unsteered!();
    let m = machine();
    let keychain = Keychain {
        answer: Some(Held::NotYet),
        ..Keychain::default()
    };

    let moved = launch(&m, &keychain);

    // Read, and not held: marked one key at a time so the command's next read moves it (ruling
    // V90d), and not tried again at every launch.
    assert_eq!(held_flags(&m), [false, false]);
    assert!(
        moved
            .said
            .iter()
            .any(|l| l.contains("0 of 2 secret(s) held again")),
        "{:#?}",
        moved.said
    );
    assert_eq!(held_to(&m).as_deref(), Some("dev.purlis.app\n"));
    reads_everything(&m);
}

#[test]
fn a_key_that_fails_leaves_the_others_marked_as_they_were() {
    purlis_core::unsteered!();
    let m = machine();
    // API_TOKEN reads but its write would ask; DB_PASSWORD is held again.
    struct WriteAsks(Holding);
    impl Store for WriteAsks {
        fn get(&self, s: &str, a: &str) -> Result<Option<Secret>, VaultError> {
            self.0.get(s, a)
        }
        fn set(&self, s: &str, a: &str, v: &str) -> Result<Held, VaultError> {
            self.0.set(s, a, v)
        }
        fn delete(&self, s: &str, a: &str) -> Result<bool, VaultError> {
            self.0.delete(s, a)
        }
        fn holds(&self) -> bool {
            true
        }
        fn rehold(&self, s: &str, a: &str, v: &str) -> Result<Held, VaultError> {
            if a == "API_TOKEN" {
                return Err(VaultError::would_ask("the system would ask"));
            }
            self.0.rehold(s, a, v)
        }
    }
    let ctx = ctx(&m.plane);
    let store = WriteAsks(Holding::answering(&ctx, Held::ToTheApp));

    let again = keyring::hold_again_with(&store, &ctx, &vault(&m.plane, "ops")).unwrap();

    assert_eq!(
        (again.keys, again.held, again.would_ask),
        (2, 1, 1),
        "{again:?}"
    );
    assert_eq!(
        held_flags(&m),
        [true, true],
        "nothing marked by the failure"
    );
}

#[test]
fn a_terminal_holds_nothing_again_and_leaves_it_to_the_app() {
    purlis_core::unsteered!();
    let mut m = machine();
    m.local.own_app = None;
    let keychain = Keychain::holding();

    let moved = launch(&m, &keychain);

    assert_eq!(keychain.reholds(), 0, "{:#?}", moved.said);
    assert_eq!(held_to(&m), None);
}

/// The log folders under the old and the purlis identifier, the old one with a panic log in it.
fn with_logs(m: &mut Machine) -> Logs {
    let logs = Logs {
        old: m.home.join("Logs/dev.charter.app"),
        new: m.home.join("Logs/dev.purlis.app"),
    };
    std::fs::create_dir_all(&logs.old).unwrap();
    std::fs::write(logs.old.join("panics.log"), "panic\n").unwrap();
    m.local.logs = Some(logs.clone());
    m.local.own_app = None;
    logs
}

#[test]
fn the_launch_moves_the_log_folder_before_its_log_is_opened_and_the_undo_puts_it_back() {
    purlis_core::unsteered!();
    let mut m = machine();
    let logs = with_logs(&mut m);

    let moved = renamelocal::logs_at_launch_with(&m.local, &nobody_running()).expect("said");

    assert!(moved.complete && moved.changed, "{:#?}", moved.said);
    assert!(logs.new.join("panics.log").is_file());
    assert!(!logs.old.exists());
    // Done once: the launch after it, and `purlis migrate`, find nothing left to move.
    assert_eq!(
        renamelocal::logs_at_launch_with(&m.local, &nobody_running()),
        None
    );
    let migrate = renamelocal::run(&m.local, &nobody_running());
    assert!(
        !migrate.said.iter().any(|l| l.contains("the log folder")),
        "{:#?}",
        migrate.said
    );

    let undone = renamelocal::undo(&m.local, &nobody_running());
    assert!(undone.complete, "{:#?}", undone.said);
    assert!(logs.old.join("panics.log").is_file());
    assert!(!logs.new.exists());
    // After an undo the launch moves nothing until the operator asks again.
    assert_eq!(
        renamelocal::logs_at_launch_with(&m.local, &nobody_running()),
        None
    );
}

#[test]
fn the_log_folder_stays_while_another_charter_runs() {
    purlis_core::unsteered!();
    let mut m = machine();
    let logs = with_logs(&mut m);
    let running = |_: Option<&str>, _: &[PathBuf]| Some("charter-app (pid 7)".to_owned());

    let moved = renamelocal::logs_at_launch_with(
        &m.local,
        &Seams {
            busy: &running,
            ..Seams::real()
        },
    )
    .expect("said");

    assert!(moved.refused.is_some(), "{moved:#?}");
    assert!(logs.old.join("panics.log").is_file());
    assert!(!logs.new.exists());
}

#[test]
fn the_launch_says_nothing_about_logs_with_nothing_to_move_or_both_folders_there() {
    purlis_core::unsteered!();
    let mut m = machine();
    let logs = with_logs(&mut m);
    std::fs::create_dir_all(&logs.new).unwrap();
    assert_eq!(
        renamelocal::logs_at_launch_with(&m.local, &nobody_running()),
        None
    );
    std::fs::remove_dir_all(&logs.old).unwrap();
    assert_eq!(
        renamelocal::logs_at_launch_with(&m.local, &nobody_running()),
        None
    );
}

#[test]
fn the_purlis_app_sees_the_old_app_running_beside_it_and_a_scenario_build_does_not() {
    purlis_core::unsteered!();
    use renamelocal::busy::{Places, older_app};
    let dir = tempfile::tempdir().unwrap();
    let places = Places {
        linux: true,
        runtime: Some(dir.path().to_path_buf()),
        uid: 501,
        ..Default::default()
    };
    let lock = |id: &str| {
        let file = std::fs::File::create(dir.path().join(format!("{id}.lock"))).unwrap();
        file.lock().unwrap();
        file
    };

    assert_eq!(older_app(&places, NEW_APP), None, "nothing else running");
    // Its own lock is its own.
    let own = lock(NEW_APP);
    assert_eq!(older_app(&places, NEW_APP), None);
    drop(own);

    let old = lock("dev.charter.app");
    let why = older_app(&places, NEW_APP).expect("the old app is running");
    assert!(why.contains("dev.charter.app.lock"), "{why}");
    // A scenario build never stops for the operator's app.
    assert_eq!(older_app(&places, "dev.purlis.app.e2e"), None);
    drop(old);
    assert!(renamelocal::busy::OLDER_APP_RUNNING.contains("delete charter.app"));
}

#[test]
fn the_update_restart_from_the_old_app_is_not_stopped_by_the_old_app() {
    purlis_core::unsteered!();
    use renamelocal::busy::{Places, older_app_at_launch, started_by_the_app};
    let dir = tempfile::tempdir().unwrap();
    let places = Places {
        linux: true,
        runtime: Some(dir.path().to_path_buf()),
        uid: 501,
        ..Default::default()
    };
    let old = std::fs::File::create(dir.path().join("dev.charter.app.lock")).unwrap();
    old.lock().unwrap();

    // Started by the old app's own program: the update's restart, let through.
    for parent in [
        "/Applications/charter.app/Contents/MacOS/charter-app",
        "charter-app",
    ] {
        assert!(started_by_the_app(Some(parent)), "{parent}");
        assert_eq!(older_app_at_launch(&places, NEW_APP, Some(parent)), None);
    }
    // Started any other way, the old app running beside it stops this launch.
    for parent in [Some("/bin/zsh"), Some("launchd"), None] {
        assert!(!started_by_the_app(parent), "{parent:?}");
        assert!(older_app_at_launch(&places, NEW_APP, parent).is_some());
    }
    drop(old);
}
