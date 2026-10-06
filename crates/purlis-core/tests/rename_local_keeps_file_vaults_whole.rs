//! A plain-file or reference vault records its file relative to the project, under the state
//! folder (`.charter/vaults/<name>.json`). `rename-local` (RN-5) moves that folder to `.purlis/`
//! and leaves the record as it was; the vault must still be read, and written, where it moved —
//! never read as empty, and never split by a fresh file at the old spelling (D-VP-1).
//!
//! Driven through the core's public surface on a temporary home: [`renamelocal::run`] and
//! [`renamelocal::undo`], and the vault readers every command uses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use purlis_core::machine;
use purlis_core::renamelocal::{self, Local, Seams};
use purlis_core::secrets::{Ctx, Env, plain_file, reference, registry};

struct Machine {
    _dir: tempfile::TempDir,
    home: PathBuf,
    plane: PathBuf,
    local: Local,
}

fn private(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A project nothing has migrated, with a plain-file vault holding one secret (and its
/// rotation record) and a reference vault holding one reference, both at the default file
/// under `.charter/vaults/`, registered in this machine's half of the registry.
fn machine() -> Machine {
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    let plane = home.join("work/plane");
    let vaults = plane.join(".charter/vaults");
    std::fs::create_dir_all(&vaults).unwrap();
    private(&plane.join(".charter"), 0o700);
    private(&vaults, 0o700);
    std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::write(
        plane.join(".charter/vaults.json"),
        r#"{"vaults": {
            "app": {"provider": "plain-file", "config": {"file": ".charter/vaults/app.json"}},
            "refs": {"provider": "reference", "config": {"file": ".charter/vaults/refs.json"}}
        }}"#,
    )
    .unwrap();
    for (file, text) in [
        ("app.json", r#"{"TOKEN": "s3cret"}"#),
        ("app.meta.json", r#"{"TOKEN": {"set_at": "2026-10-01"}}"#),
        ("refs.json", r#"{"DB": "op://vault/item/field"}"#),
    ] {
        std::fs::write(vaults.join(file), text).unwrap();
        private(&vaults.join(file), 0o600);
    }

    let config_root = home.join(".config");
    std::fs::create_dir_all(&config_root).unwrap();
    machine::update(&config_root, |store| store.remember(&plane, 100)).unwrap();

    Machine {
        _dir: dir,
        local: Local {
            config_root,
            data_base: None,
            logs: None,
            planes: Vec::new(),
            own_app: None,
            plugin: None,
        },
        home,
        plane,
    }
}

fn nobody_running() -> Seams<'static> {
    Seams {
        busy: &|_, _| None,
        ..Seams::real()
    }
}

fn ctx(m: &Machine) -> Ctx {
    Ctx::new(&m.plane, Env::of(&[("HOME", m.home.to_str().unwrap())]))
}

fn day() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 10, 6).unwrap()
}

/// Every file under the project, with its bytes.
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                out.insert(path.strip_prefix(root).unwrap().to_path_buf(), bytes);
            }
        }
    }
    out
}

#[test]
fn a_plain_file_vault_is_read_and_written_where_rename_local_moved_it() {
    purlis_core::unsteered!();
    let m = machine();
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    assert!(!m.plane.join(".charter").exists());

    let ctx = ctx(&m);
    let app = registry::vault(&ctx, "app").unwrap();
    assert_eq!(plain_file::get(&ctx, &app, "TOKEN").unwrap(), "s3cret");
    assert_eq!(plain_file::keys(&ctx, &app).unwrap(), ["TOKEN"]);

    plain_file::set(&ctx, &app, "NEW", "value", day()).unwrap();
    let moved = std::fs::read_to_string(m.plane.join(".purlis/vaults/app.json")).unwrap();
    assert!(moved.contains("NEW") && moved.contains("s3cret"), "{moved}");
    assert!(
        !m.plane.join(".charter").exists(),
        "a write never starts a second vault at the old spelling"
    );
}

#[test]
fn a_reference_vault_is_read_where_rename_local_moved_it() {
    purlis_core::unsteered!();
    let m = machine();
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);

    let ctx = ctx(&m);
    let refs = registry::vault(&ctx, "refs").unwrap();
    assert_eq!(reference::keys(&ctx, &refs).unwrap(), ["DB"]);
}

#[test]
fn the_undo_puts_the_vaults_back_byte_for_byte_and_they_still_read() {
    purlis_core::unsteered!();
    let m = machine();
    let before = files(&m.plane);

    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    assert!(renamelocal::undo(&m.local, &nobody_running()).complete);

    assert_eq!(files(&m.plane), before);
    let ctx = ctx(&m);
    let app = registry::vault(&ctx, "app").unwrap();
    assert_eq!(plain_file::get(&ctx, &app, "TOKEN").unwrap(), "s3cret");
}
