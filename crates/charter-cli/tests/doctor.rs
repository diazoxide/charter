//! `charter doctor` as a process: the environment it is run in, its output and its exit.
//!
//! What each row says is `charter-core`'s tests and the differential's; these are the parts
//! only a real process has — the variables it is started with, and the status it ends with.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    for d in ["personas", "inventory", "workspaces", "home"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    (dir, root)
}

/// `charter doctor <args>` in `root`, with a home of the test's choosing and nothing inherited.
fn doctor(root: &Path, home: &Path, args: &[&str]) -> Output {
    doctor_with(root, home, args, &[])
}

/// [`doctor`], with `vars` set as well.
fn doctor_with(root: &Path, home: &Path, args: &[&str], vars: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_charter"))
        .arg("doctor")
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .env("CHARTER_ROOT", root)
        .envs(vars.iter().copied())
        .output()
        .expect("charter runs")
}

fn rows(out: &Output) -> Vec<serde_json::Value> {
    serde_json::from_slice(&out.stdout).expect("--json prints JSON")
}

fn row<'a>(rows: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    rows.iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no row {name}"))
}

#[test]
fn with_no_git_identity_the_doctor_names_a_blocker_and_exits_non_zero() {
    let (_d, root) = plane();
    let out = doctor(&root, &root.join("home"), &["--json"]);
    let rows = rows(&out);
    let identity = row(&rows, "git identity");
    assert_eq!(identity["status"], "fail", "{identity}");
    assert_eq!(identity["detail"], "not set: user.name, user.email");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn with_an_identity_and_nothing_wrong_the_exit_is_zero_and_the_warnings_are_counted() {
    let (_d, root) = plane();
    let home = root.join("home");
    std::fs::write(
        home.join(".gitconfig"),
        "[user]\n\tname = Fixture User\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let out = doctor(&root, &home, &["--json"]);
    let rows = rows(&out);
    assert_eq!(
        row(&rows, "git identity")["detail"],
        "Fixture User <fixture@example.invalid>"
    );
    assert!(
        rows.iter().all(|r| r["status"] != "fail"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(out.status.code(), Some(0));

    let table = doctor(&root, &home, &[]);
    let text = String::from_utf8(table.stdout).unwrap();
    assert!(text.starts_with("charter preflight:\n\n"), "{text}");
    assert!(
        text.ends_with("optional item(s) pending \u{2014} see hints above.\n"),
        "{text}"
    );
    // Never coloured into a pipe: another program is reading this.
    assert!(!text.contains('\x1b'), "{text:?}");
    assert_eq!(table.status.code(), Some(0));
}

#[test]
fn fix_installs_charters_plugin_says_what_it_did_and_then_reports() {
    // #373: the one repair the Python `--fix` made was installing charter's plugin, and that
    // is what it does again — through `charter plugin install`, whose steps it prints.
    let (_d, root) = plane();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let vars = [("CHARTER_CONFIG_HOME", root.to_str().unwrap())];
    let before = rows(&doctor_with(&root, &home, &["--json"], &vars));
    assert_eq!(row(&before, "plugin install")["status"], "warn");
    assert!(!home.join(".codex/config.toml").exists());

    let out = doctor_with(&root, &home, &["--json", "--fix"], &vars);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("codex:\n  done "), "{said}");
    let after = rows(&out);
    let install = row(&after, "plugin install");
    assert_eq!(install["status"], "ok", "{install}");
    assert_eq!(install["detail"], "installed for codex");
    assert!(
        std::fs::read_to_string(home.join(".codex/config.toml"))
            .unwrap()
            .contains("hook pretooluse")
    );
}

#[test]
fn a_preflight_probes_no_profile_even_one_that_is_declared() {
    let (_d, root) = plane();
    std::fs::write(
        root.join("charter.local.toml"),
        "[harness.claude-work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    )
    .unwrap();
    let preflight = rows(&doctor(
        &root,
        &root.join("home"),
        &["--json", "--preflight"],
    ));
    assert!(
        !preflight
            .iter()
            .any(|r| r["name"].as_str().unwrap().starts_with("profile ")),
        "{preflight:?}"
    );
    let typed = rows(&doctor(&root, &root.join("home"), &["--json"]));
    assert_eq!(row(&typed, "profile claude-work")["status"], "warn");
}

#[test]
fn a_harness_name_from_the_environment_cannot_forge_a_row() {
    let (_d, root) = plane();
    let out = doctor_with(
        &root,
        &root.join("home"),
        &[],
        &[("CHARTER_HARNESS", "x\n  \u{2713}  forged")],
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("\n  \u{2713}  forged"), "{text}");
    assert!(
        text.contains("how x\\x0a  \u{2713}  forged finds"),
        "{text}"
    );
}
