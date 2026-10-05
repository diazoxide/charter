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
fn fix_adds_the_ask_rule_a_report_is_filed_behind_and_then_reports_it_present() {
    // #363 D11: a report files a public issue, so the plane asks before `--yes` files one.
    let (_d, root) = plane();
    let home = root.join("home");
    let vars = [("CHARTER_CONFIG_HOME", root.to_str().unwrap())];
    let before = rows(&doctor_with(&root, &home, &["--json"], &vars));
    let ask = row(&before, "ask rules");
    assert_eq!(ask["status"], "warn", "{ask}");
    assert_eq!(
        ask["detail"],
        "no ask rule for `charter report --yes` under claude-code, opencode"
    );

    let out = doctor_with(&root, &home, &["--json", "--fix"], &vars);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("claude-code: asking for Bash(charter report *--yes*)"),
        "{said}"
    );
    let after = rows(&out);
    let ask = row(&after, "ask rules");
    assert_eq!(ask["status"], "ok", "{ask}");
    let settings = std::fs::read_to_string(root.join(".claude/settings.json")).unwrap();
    assert!(
        settings.contains("\"Bash(charter report *--yes*)\""),
        "{settings}"
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

/// A project with a `charter.toml` and nothing else: the `schema` row's finding, which reinit
/// fixes (FX-1).
fn bare_project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(root.join("home")).unwrap();
    (dir, root)
}

#[test]
fn json_names_the_fix_on_a_fixable_row_and_on_no_other() {
    let (_d, root) = bare_project();
    let rows = rows(&doctor(&root, &root.join("home"), &["--json"]));
    assert_eq!(row(&rows, "schema")["fix"], "reinit");
    assert_eq!(
        row(&rows, "schema")
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["name", "status", "detail", "hint", "fix"],
        "the four keys first, as they always were"
    );
    let with_fix: Vec<_> = rows.iter().filter(|r| r.get("fix").is_some()).collect();
    assert_eq!(with_fix.len(), 1, "{with_fix:?}");
}

#[test]
fn fix_with_an_id_applies_that_fix_says_what_it_changed_and_reports_it_clean() {
    let (_d, root) = bare_project();
    let home = root.join("home");
    let vars = [("CHARTER_CONFIG_HOME", root.to_str().unwrap())];
    let out = doctor_with(&root, &home, &["--json", "--fix", "reinit"], &vars);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix reinit:"), "{said}");
    assert!(
        said.contains("personas"),
        "it names what it created: {said}"
    );
    // `--fix <id>` applies that fix alone: the plugin install bare `--fix` makes is not run.
    assert!(!said.contains("codex:"), "{said}");
    let after = rows(&out);
    let schema = row(&after, "schema");
    assert_eq!(schema["status"], "ok", "{schema}");
    assert!(schema.get("fix").is_none(), "{schema}");
    assert!(root.join("personas").is_dir());
}

#[test]
fn bare_fix_applies_every_fix_the_doctor_offers() {
    let (_d, root) = bare_project();
    let home = root.join("home");
    let vars = [("CHARTER_CONFIG_HOME", root.to_str().unwrap())];
    let out = doctor_with(&root, &home, &["--json", "--fix"], &vars);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix reinit:"), "{said}");
    assert_eq!(row(&rows(&out), "schema")["status"], "ok");
    for dir in ["personas", "inventory", "workspaces"] {
        assert!(root.join(dir).is_dir(), "{dir}/ missing: {said}");
    }
}

#[test]
fn fix_with_an_id_no_fix_has_is_refused_and_names_the_fixes() {
    let (_d, root) = bare_project();
    let out = doctor(&root, &root.join("home"), &["--fix", "index-lock"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(
        said.contains("reinit"),
        "it names the fixes there are: {said}"
    );
    assert!(!root.join("personas").exists(), "nothing was fixed");
}
