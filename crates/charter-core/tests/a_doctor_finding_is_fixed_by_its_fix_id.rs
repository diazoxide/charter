//! The fix registry (FX-1, V91p): a doctor finding charter can fix carries a **fix id**, and one
//! core entry point applies a fix by that id — what `charter doctor --fix` and the Doctor
//! dialog's Fix button both call. Driven here the way both callers drive it: run the doctor,
//! read the id off the row, apply it, run the doctor again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use charter_core::doctor::fix::{self, FixId, Fixed};
use charter_core::doctor::{Doctor, Row, Status};

fn project(manifest: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("charter.toml"), manifest).unwrap();
    (dir, root)
}

fn row(root: &Path, name: &str) -> Row {
    Doctor::at(root, root, false, true)
        .run()
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row {name}"))
}

/// Every file and directory under `root` with its bytes: what "untouched" is measured against.
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                out.insert(rel, None);
                walk(&path, root, out);
            } else {
                out.insert(rel, Some(std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn a_project_missing_its_baseline_folders_is_fixed_by_reinit_and_then_checks_clean() {
    charter_core::unsteered!();
    let (_d, root) = project("schema = 1\n");

    let found = row(&root, "schema");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, Some(FixId::Reinit), "{found:?}");

    let applied = fix::apply(&root, FixId::Reinit);
    let Fixed::Ran { said, complete } = &applied else {
        panic!("reinit was refused: {applied:?}");
    };
    assert!(complete, "{said:?}");
    assert!(
        said.iter().any(|line| line.contains("personas")),
        "it says what it created: {said:?}"
    );
    for dir in ["personas", "inventory", "workspaces"] {
        assert!(root.join(dir).is_dir(), "{dir}/ was not created");
    }

    let after = row(&root, "schema");
    assert_eq!(after.status, Status::Ok, "{after:?}");
    assert_eq!(after.fix, None, "a clean row offers no fix: {after:?}");
}

#[test]
fn a_fix_on_a_project_this_charter_cannot_write_is_refused_and_writes_nothing() {
    charter_core::unsteered!();
    let (_d, root) =
        project("schema = 2\nrequires = [{ feature = \"memory-proposals\", since = \"0.9.0\" }]\n");
    let before = tree(&root);

    let applied = fix::apply(&root, FixId::Reinit);

    let Fixed::Refused(why) = &applied else {
        panic!("a read-only project was written: {applied:?}");
    };
    assert!(why.contains("memory-proposals"), "it says why: {why}");
    assert_eq!(tree(&root), before, "the refused fix wrote to the project");
}

#[test]
fn a_fix_where_there_is_no_project_is_refused_and_writes_nothing() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();

    let applied = fix::apply(&root, FixId::Reinit);

    assert!(matches!(applied, Fixed::Refused(_)), "{applied:?}");
    assert!(tree(&root).is_empty(), "{:?}", tree(&root));
}

#[test]
fn a_stale_index_lock_is_reported_and_never_offered_as_a_fix() {
    charter_core::unsteered!();
    let (_d, root) = project("schema = 1\n");
    for dir in ["personas", "inventory", "workspaces"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let git = charter_core::forklock::status(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root),
    )
    .unwrap();
    assert!(git.success());
    let lock = std::fs::File::create(root.join(".git/index.lock")).unwrap();
    lock.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(24 * 3600))
        .unwrap();

    let found = row(&root, "index lock");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, None, "charter never removes a lock: {found:?}");
    assert!(
        !Doctor::at(&root, &root, false, true)
            .fixes()
            .iter()
            .any(|id| id.id().contains("lock")),
    );
}

#[test]
fn the_doctor_offers_exactly_the_fixes_its_rows_carry() {
    charter_core::unsteered!();
    let (_d, root) = project("schema = 1\n");
    let doctor = Doctor::at(&root, &root, false, true);
    assert_eq!(doctor.fixes(), vec![FixId::Reinit]);

    for dir in ["personas", "inventory", "workspaces"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    assert_eq!(Doctor::at(&root, &root, false, true).fixes(), vec![]);
}

#[test]
fn a_fix_id_is_spelled_one_way_and_read_back_the_same() {
    charter_core::unsteered!();
    assert_eq!(FixId::Reinit.id(), "reinit");
    assert_eq!(FixId::parse("reinit"), Some(FixId::Reinit));
    assert_eq!(FixId::parse("index-lock"), None);
    assert_eq!(FixId::ALL.map(FixId::id), ["reinit"]);
}
