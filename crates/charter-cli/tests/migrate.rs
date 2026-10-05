//! `purlis migrate [--undo]` and `doctor --fix rename-local` as processes (RN-5): what they move
//! on a temporary home, what they print, and that running either again is safe.
//!
//! What each move does is `charter-core`'s test (`rename_local_moves_local_state_and_puts_it_back`);
//! these are the parts only a real process has: the environment it reads its homes from, its
//! output and its exit.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Machine {
    _dir: tempfile::TempDir,
    home: PathBuf,
    plane: PathBuf,
}

fn machine() -> Machine {
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    let plane = home.join("plane");
    std::fs::create_dir_all(plane.join(".charter")).unwrap();
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&plane)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(init.status.success(), "{init:?}");
    std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::write(plane.join("charter.local.toml"), "\n").unwrap();
    std::fs::write(plane.join(".charter/kept"), "state\n").unwrap();
    std::fs::create_dir_all(home.join(".config/charter")).unwrap();
    std::fs::write(home.join(".config/charter/kept"), "config\n").unwrap();
    std::fs::create_dir_all(home.join("data/charter")).unwrap();
    Machine {
        _dir: dir,
        home,
        plane,
    }
}

/// `charter <args>` in the project, with the temporary home and nothing inherited.
fn charter(m: &Machine, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(args)
        .current_dir(&m.plane)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &m.home)
        .env("XDG_DATA_HOME", m.home.join("data"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("CHARTER_ROOT", &m.plane)
        .output()
        .expect("charter runs")
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn migrate_moves_the_local_names_and_undo_puts_them_back() {
    let m = machine();

    let out = charter(&m, &["migrate"]);

    assert!(out.status.success(), "{}", said(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("✓ the config home"), "{stdout}");
    assert!(stdout.contains("purlis migrate --undo"), "{stdout}");
    assert_eq!(read(&m.home.join(".config/purlis/kept")), "config\n");
    assert!(m.home.join("data/purlis").is_dir());
    assert_eq!(read(&m.plane.join(".purlis/kept")), "state\n");
    assert!(m.plane.join("purlis.local.toml").is_file());

    let out = charter(&m, &["migrate", "--undo"]);

    assert!(out.status.success(), "{}", said(&out));
    assert_eq!(read(&m.home.join(".config/charter/kept")), "config\n");
    assert!(!m.home.join(".config/purlis").exists());
    assert!(m.home.join("data/charter").is_dir());
    assert_eq!(read(&m.plane.join(".charter/kept")), "state\n");
    assert!(m.plane.join("charter.local.toml").is_file());
    assert!(!m.plane.join(".purlis").exists());
}

#[test]
fn the_rename_local_fix_runs_and_runs_again_safely() {
    let m = machine();

    let out = charter(&m, &["doctor", "--fix", "rename-local"]);

    let first = String::from_utf8_lossy(&out.stderr);
    assert!(first.contains("fix rename-local:"), "{first}");
    assert!(first.contains("✓ the config home"), "{first}");
    assert_eq!(read(&m.plane.join(".purlis/kept")), "state\n");

    let again = charter(&m, &["doctor", "--fix", "rename-local"]);

    let second = String::from_utf8_lossy(&again.stderr);
    assert!(second.contains("nothing to move"), "{second}");
    assert!(!second.contains('✗'), "{second}");
    assert_eq!(read(&m.plane.join(".purlis/kept")), "state\n");
    assert_eq!(read(&m.home.join(".config/purlis/kept")), "config\n");
}

#[test]
fn a_bare_fix_never_moves_this_machines_folders() {
    let m = machine();

    let _ = charter(&m, &["doctor", "--fix"]);

    assert!(m.home.join(".config/charter").is_dir());
    assert!(m.plane.join(".charter").is_dir());
    assert!(!m.plane.join(".purlis").exists());
}
