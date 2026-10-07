//! `charter doctor` as a process: the environment it is run in, its output and its exit.
//!
//! What each row says is `purlis-core`'s tests and the differential's; these are the parts
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
    Command::new(env!("CARGO_BIN_EXE_purlis"))
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
    assert!(text.starts_with("purlis preflight:\n\n"), "{text}");
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
    assert!(said.contains("fix plugin-install:\n"), "{said}");
    assert!(said.contains("  codex:\n    done "), "{said}");
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
    // And the identity row, whose fix takes a name and an email (FX-3): this home has none.
    assert_eq!(row(&rows, "git identity")["fix"], "git-identity");
    let with_fix: Vec<_> = rows.iter().filter(|r| r.get("fix").is_some()).collect();
    assert_eq!(with_fix.len(), 2, "{with_fix:?}");
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

/// git, for a test's own setup, with nothing of the developer's config.
fn git(root: &Path, args: &[&str]) {
    let ran = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", root.join("home"))
        .output()
        .expect("git runs");
    assert!(ran.status.success(), "git {args:?}: {ran:?}");
}

#[test]
fn fix_local_ignore_adds_the_one_ignore_line_and_reports_the_profiles_clean() {
    let (_d, root) = plane();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
    std::fs::write(
        root.join("charter.local.toml"),
        "[harness.claude-work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    )
    .unwrap();
    let home = root.join("home");
    let before = rows(&doctor(&root, &home, &["--json"]));
    assert_eq!(row(&before, "harness profiles")["fix"], "local-ignore");

    let out = doctor(&root, &home, &["--json", "--fix", "local-ignore"]);

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix local-ignore:"), "{said}");
    assert!(said.contains("/charter.local.toml"), "{said}");
    let ignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(ignore.starts_with("node_modules/\n"), "{ignore:?}");
    assert!(
        ignore.lines().any(|l| l == "/charter.local.toml"),
        "{ignore:?}"
    );
    let profiles = rows(&out);
    let profiles = row(&profiles, "harness profiles");
    assert_eq!(profiles["status"], "ok", "{profiles}");
    assert!(profiles.get("fix").is_none(), "{profiles}");
    // `--fix <id>` is that fix alone: bare `--fix`'s ask rule is not added.
    assert!(!root.join(".claude/settings.json").exists(), "{said}");
}

#[test]
fn fix_local_ignore_on_a_tracked_local_file_is_refused_and_untracks_nothing() {
    let (_d, root) = plane();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("charter.local.toml"), "[harness]\n").unwrap();
    git(&root, &["add", "charter.local.toml"]);

    let out = doctor(&root, &root.join("home"), &["--fix", "local-ignore"]);

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("✗ refused:"), "{said}");
    assert!(
        said.contains("git rm --cached charter.local.toml"),
        "{said}"
    );
    assert!(!root.join(".gitignore").exists(), "{said}");
}

#[test]
fn fix_memory_optimize_links_an_unindexed_memory_and_reports_the_indexes_clean() {
    let (_d, root) = plane();
    let mem = root.join("workspaces/alpha/memory");
    std::fs::create_dir_all(&mem).unwrap();
    std::fs::write(mem.join("MEMORY.md"), "").unwrap();
    std::fs::write(mem.join("note.md"), "# Note\nkept\n").unwrap();
    let home = root.join("home");
    let before = rows(&doctor(&root, &home, &["--json"]));
    assert_eq!(row(&before, "memory indexes")["fix"], "memory-optimize");

    let out = doctor(&root, &home, &["--json", "--fix", "memory-optimize"]);

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix memory-optimize:"), "{said}");
    assert!(said.contains("repaired index"), "{said}");
    assert!(
        std::fs::read_to_string(mem.join("MEMORY.md"))
            .unwrap()
            .contains("(note.md)")
    );
    assert_eq!(row(&rows(&out), "memory indexes")["status"], "ok");
}

#[test]
fn fix_plugin_install_installs_the_plugin_alone() {
    let (_d, root) = plane();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let vars = [("CHARTER_CONFIG_HOME", root.to_str().unwrap())];
    let before = rows(&doctor_with(&root, &home, &["--json"], &vars));
    assert_eq!(row(&before, "plugin install")["fix"], "plugin-install");

    let out = doctor_with(&root, &home, &["--json", "--fix", "plugin-install"], &vars);

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix plugin-install:\n"), "{said}");
    assert!(said.contains("  codex:\n    done "), "{said}");
    assert_eq!(row(&rows(&out), "plugin install")["status"], "ok");
    assert!(!root.join(".claude/settings.json").exists(), "{said}");
}

#[test]
fn bare_fix_installs_the_plugin_once_however_many_rows_offer_it() {
    let (_d, root) = plane();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let vars = [("CHARTER_CONFIG_HOME", root.to_str().unwrap())];

    let out = doctor_with(&root, &home, &["--json", "--fix"], &vars);

    let said = String::from_utf8_lossy(&out.stderr);
    assert_eq!(said.matches("fix plugin-install:").count(), 1, "{said}");
    assert_eq!(said.matches("  codex:\n    done ").count(), 1, "{said}");
}

#[test]
fn fix_discover_against_a_forge_that_is_not_logged_in_is_refused_and_writes_nothing() {
    let (_d, root) = plane();
    std::fs::write(
        root.join("charter.toml"),
        "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n",
    )
    .unwrap();
    // A stand-in `gh` first on `PATH` that is not logged in: no test reaches a network.
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    stand_in::program(
        &bin,
        "gh",
        "#!/bin/sh\necho 'You are not logged into any GitHub hosts.' >&2\nexit 1\n",
    );
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let home = root.join("home");
    let before = rows(&doctor_with(&root, &home, &["--json"], &[("PATH", &path)]));
    assert_eq!(row(&before, "inventory")["fix"], "discover");

    let out = doctor_with(
        &root,
        &home,
        &["--json", "--fix", "discover"],
        &[("PATH", &path)],
    );

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("fix discover:\n  ✗ refused:"), "{said}");
    assert!(said.contains("not authenticated"), "{said}");
    assert!(!root.join("inventory/repos.json").exists(), "{said}");
}

#[test]
fn fix_plugin_install_where_charter_cannot_tell_the_harnesses_is_refused() {
    let (_d, root) = plane();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let vars = [
        ("CHARTER_CONFIG_HOME", root.to_str().unwrap()),
        ("CLAUDE_CONFIG_DIR", ""),
    ];

    let out = doctor_with(&root, &home, &["--fix", "plugin-install"], &vars);

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("fix plugin-install:\n  ✗ refused:"), "{said}");
    assert!(said.contains("CLAUDE_CONFIG_DIR"), "{said}");
    assert!(!home.join(".codex/config.toml").exists(), "{said}");
}

/// A plane that declares a forge and has an empty inventory, a home with a git identity, and a
/// stand-in `gh` first on `PATH` that writes down every call and answers one org of one repo.
/// Returns the `PATH` to run with and the log the stand-in writes.
fn forge_plane(root: &Path) -> (String, PathBuf) {
    std::fs::write(
        root.join("charter.toml"),
        "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("home/.gitconfig"),
        "[user]\n\tname = Fixture User\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = root.join("gh-calls.log");
    let repos = r#"[{"id": 11, "name": "widget", "full_name": "acme/widget", "default_branch": "main", "description": "", "html_url": "https://github.com/acme/widget", "ssh_url": "git@github.com:acme/widget.git", "topics": []}]"#;
    stand_in::program(
        &bin,
        "gh",
        &format!(
            "#!/bin/sh\n\
             printf '%s\\n' \"$*\" >> '{log}'\n\
             case \"$*\" in\n\
             \"auth status --hostname github.com\") exit 0;;\n\
             \"api --hostname github.com orgs/acme/repos?per_page=100&page=1\") echo '{repos}'; exit 0;;\n\
             esac\n\
             exit 1\n",
            log = log.display(),
        ),
    );
    (format!("{}:/usr/bin:/bin", bin.display()), log)
}

#[test]
fn bare_fix_never_runs_discover_and_exits_zero() {
    // Discover goes over the network and writes the inventory and the docs: it runs only when
    // it is named (D-FX2-9).
    let (_d, root) = plane();
    let (path, log) = forge_plane(&root);
    let home = root.join("home");
    let vars = [
        ("PATH", path.as_str()),
        ("CHARTER_CONFIG_HOME", root.to_str().unwrap()),
    ];
    let before = rows(&doctor_with(&root, &home, &["--json"], &vars));
    assert_eq!(row(&before, "inventory")["fix"], "discover");

    let out = doctor_with(&root, &home, &["--json", "--fix"], &vars);

    let said = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{said}");
    assert!(!said.contains("fix discover"), "{said}");
    let asked = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!asked.contains("repos"), "the forge was asked: {asked}");
    assert!(!root.join("inventory/repos.json").exists(), "{said}");
}

#[test]
fn fix_discover_by_name_asks_the_forge_and_builds_the_inventory() {
    let (_d, root) = plane();
    let (path, log) = forge_plane(&root);
    let home = root.join("home");
    let vars = [
        ("PATH", path.as_str()),
        ("CHARTER_CONFIG_HOME", root.to_str().unwrap()),
    ];

    let out = doctor_with(&root, &home, &["--json", "--fix", "discover"], &vars);

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix discover:"), "{said}");
    assert!(said.contains("Wrote 1 repos"), "{said}");
    let asked = std::fs::read_to_string(&log).unwrap();
    assert!(asked.contains("orgs/acme/repos"), "{asked}");
    let inventory = std::fs::read_to_string(root.join("inventory/repos.json")).unwrap();
    assert!(inventory.contains("\"widget\""), "{inventory}");
    assert_eq!(row(&rows(&out), "inventory")["status"], "ok");
}

// ---- git-identity (FX-3): a fix that takes a name and an email ----------------------------

#[test]
fn fix_git_identity_with_a_name_and_an_email_sets_the_global_identity_and_reports_it_clean() {
    let (_d, root) = plane();
    let home = root.join("home");
    let out = doctor(
        &root,
        &home,
        &[
            "--json",
            "--fix",
            "git-identity",
            "--name",
            "Ann Example",
            "--email",
            "ann@example.invalid",
        ],
    );
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix git-identity:"), "{said}");
    assert!(said.contains("user.email = ann@example.invalid"), "{said}");
    let identity = row(&rows(&out), "git identity").clone();
    assert_eq!(identity["status"], "ok", "{identity}");
    assert_eq!(identity["detail"], "Ann Example <ann@example.invalid>");
    assert!(identity.get("fix").is_none(), "{identity}");
    let written = std::fs::read_to_string(home.join(".gitconfig")).expect("the global config");
    assert!(written.contains("Ann Example"), "{written}");
    assert_eq!(out.status.code(), Some(0), "{said}");
}

#[test]
fn fix_git_identity_with_a_bad_email_is_refused_by_field_and_writes_nothing() {
    let (_d, root) = plane();
    let home = root.join("home");
    let out = doctor(
        &root,
        &home,
        &["--fix", "git-identity", "--name", "Ann", "--email", "nope"],
    );
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("✗ refused: --email:"), "{said}");
    assert!(!said.contains("--name:"), "the name was fine: {said}");
    assert!(!out.status.success());
    assert!(
        !home.join(".gitconfig").exists(),
        "a refusal wrote the config"
    );
}

#[test]
fn fix_git_identity_without_a_name_and_an_email_says_what_to_give() {
    let (_d, root) = plane();
    let home = root.join("home");
    let out = doctor(&root, &home, &["--fix", "git-identity"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("--name") && said.contains("--email"),
        "{said}"
    );
    assert!(!out.status.success());
    assert!(!home.join(".gitconfig").exists());
}

#[test]
fn a_name_and_an_email_for_another_fix_are_refused_rather_than_dropped() {
    let (_d, root) = bare_project();
    let home = root.join("home");
    let out = doctor(&root, &home, &["--fix", "reinit", "--name", "Ann"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{said}");
    assert!(said.contains("--fix git-identity"), "{said}");
    assert!(!root.join("personas").exists(), "nothing was fixed");
}

#[test]
fn a_name_and_an_email_with_bare_fix_and_a_complete_identity_say_they_wrote_nothing() {
    let (_d, root) = plane();
    let home = root.join("home");
    let before = "[user]\n\tname = Bea Terminal\n\temail = bea@example.invalid\n";
    std::fs::write(home.join(".gitconfig"), before).unwrap();
    let vars = [("CHARTER_CONFIG_HOME", root.to_str().unwrap())];
    for args in [
        &["--fix", "--name", "Ann", "--email", "ann@example.invalid"][..],
        &[
            "--fix",
            "git-identity",
            "--name",
            "Ann",
            "--email",
            "ann@example.invalid",
        ][..],
    ] {
        let out = doctor_with(&root, &home, args, &vars);
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains("git identity is already set (Bea Terminal <bea@example.invalid>)"),
            "{args:?}: {said}"
        );
        assert!(!out.status.success(), "{args:?}: {said}");
        assert_eq!(
            std::fs::read_to_string(home.join(".gitconfig")).unwrap(),
            before
        );
    }
}

/// A plane under git, committed, with an identity and no signer in its home.
fn committed_plane() -> (tempfile::TempDir, PathBuf) {
    let (d, root) = plane();
    std::fs::write(
        root.join("home").join(".gitconfig"),
        "[user]\n\tname = Fixture User\n\temail = fixture@example.invalid\n\
         [commit]\n\tgpgsign = false\n",
    )
    .unwrap();
    std::fs::write(root.join(".gitignore"), "/home/\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "before"]);
    (d, root)
}

/// `rename-plane` (RN-7) is applied by name, and makes its one commit.
#[test]
fn fix_rename_plane_by_name_renames_the_project_in_one_commit() {
    let (_d, root) = committed_plane();
    let out = doctor(&root, &root.join("home"), &["--fix", "rename-plane"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix rename-plane:"), "{said}");
    assert!(
        said.contains("✓ renamed charter.toml to purlis.toml"),
        "{said}"
    );
    assert!(said.contains("✓ committed"), "{said}");
    assert!(root.join("purlis.toml").is_file(), "{said}");
    assert!(!root.join("charter.toml").exists(), "{said}");
}

/// From inside a chat it is refused (D-RN7-12): the product's chat variable is set.
#[test]
fn fix_rename_plane_is_refused_inside_a_chat() {
    let (_d, root) = committed_plane();
    let out = doctor_with(
        &root,
        &root.join("home"),
        &["--fix", "rename-plane"],
        &[("PURLIS_SESSION_ID", "chat-1")],
    );
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("not run from inside a chat"), "{said}");
    assert!(root.join("charter.toml").is_file(), "{said}");
    assert!(!root.join("purlis.toml").exists(), "{said}");
}

/// Bare `--fix` never runs it: a commit every teammate pulls waits to be asked for (V93g).
#[test]
fn bare_fix_never_renames_the_project() {
    let (_d, root) = committed_plane();
    let out = doctor(&root, &root.join("home"), &["--fix"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!said.contains("rename-plane"), "{said}");
    assert!(root.join("charter.toml").is_file(), "{said}");
    assert!(!root.join("purlis.toml").exists(), "{said}");
}

// ---- persona-agents (#1451) -----------------------------------------------------------------

/// A sub-agent file as `persona sync-agents` wrote it before it was retired.
fn generated_agent(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: \"The {name} persona.\"\n---\n<!-- {} from \
         personas/{name}/persona.md — edit the persona, not this file. -->\n\nThis sub-agent \
         acts as the **{name}** persona — Role — in an\nisolated context. Adopt the charter \
         below as your role.\n\n# {name}\n",
        purlis_core::names::SYNC_AGENTS_MARKER.reads[0]
    )
}

/// A project that used `sync-agents`, committed: one generated sub-agent, one hand-written, a
/// generated one nobody committed, and a persona that carries a key which only fed the
/// generated file.
fn plane_with_generated_agents() -> (tempfile::TempDir, PathBuf) {
    let (d, root) = committed_plane();
    std::fs::create_dir_all(root.join("personas/ops")).unwrap();
    std::fs::write(
        root.join("personas/ops/persona.md"),
        "---\nname: ops\nrole: Ops\nvault: none\ndelegate-when: deploys\ncolor: cyan\n\
         agent-tools: Read, Bash\n---\n\n# Ops\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join(".claude/agents")).unwrap();
    std::fs::write(root.join(".claude/agents/ops.md"), generated_agent("ops")).unwrap();
    std::fs::write(
        root.join(".claude/agents/runner.md"),
        "---\nname: runner\n---\nMine.\n",
    )
    .unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "with agents"]);
    // Generated after the last commit: git could not give this one back.
    std::fs::write(root.join(".claude/agents/late.md"), generated_agent("late")).unwrap();
    (d, root)
}

/// `persona-agents` is applied by name, prints every file and key it changed and every file it
/// left alone, and a second run changes nothing.
#[test]
fn fix_persona_agents_by_name_removes_what_purlis_generated_and_says_every_line() {
    let (_d, root) = plane_with_generated_agents();

    let out = doctor(&root, &root.join("home"), &["--fix", "persona-agents"]);

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("fix persona-agents:"), "{said}");
    for line in [
        "✓ removed .claude/agents/ops.md (purlis generated it).",
        "• left alone .claude/agents/runner.md: hand-written (it does not carry purlis's marker).",
        "✓ personas/ops/persona.md: `color: cyan` is now `color: teal`.",
        "• personas/ops/persona.md: `agent-tools:` is no longer read: ",
        "• left alone .claude/agents/late.md: it is a file purlis generated, but git does not \
         track it",
        "✓ removed 1 generated sub-agent file(s) and rewrote 1 persona key(s).",
        "`git restore -- .claude/agents personas`",
    ] {
        assert!(said.contains(line), "{line}\n--\n{said}");
    }
    assert!(!root.join(".claude/agents/ops.md").exists(), "{said}");
    assert!(root.join(".claude/agents/late.md").is_file(), "{said}");
    assert_eq!(
        std::fs::read_to_string(root.join(".claude/agents/runner.md")).unwrap(),
        "---\nname: runner\n---\nMine.\n"
    );
    let ops = std::fs::read_to_string(root.join("personas/ops/persona.md")).unwrap();
    assert!(
        ops.contains("\ncolor: teal\nagent-tools: Read, Bash\n"),
        "{ops}"
    );

    let again = doctor(&root, &root.join("home"), &["--fix", "persona-agents"]);
    let said = String::from_utf8_lossy(&again.stderr);
    assert!(
        said.contains(
            "✓ no generated persona sub-agent file to remove and no persona key to carry over"
        ),
        "{said}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("personas/ops/persona.md")).unwrap(),
        ops
    );
}

/// Bare `--fix` never runs it: it changes committed files, so it waits to be asked for. The
/// `personas` row says the files are there and carries the fix's id.
#[test]
fn bare_fix_never_removes_a_generated_sub_agent_and_the_row_offers_the_fix() {
    let (_d, root) = plane_with_generated_agents();

    let out = doctor(&root, &root.join("home"), &["--fix"]);

    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!said.contains("fix persona-agents:"), "{said}");
    assert!(root.join(".claude/agents/ops.md").is_file(), "{said}");
    let json = doctor(&root, &root.join("home"), &["--json"]);
    let personas = rows(&json)
        .into_iter()
        .find(|row| row["name"] == "personas")
        .expect("a personas row");
    assert_eq!(personas["status"], "warn", "{personas}");
    assert_eq!(personas["fix"], "persona-agents", "{personas}");
    assert!(
        personas["detail"]
            .as_str()
            .unwrap()
            .contains("2 generated sub-agent file(s) remain in .claude/agents/"),
        "{personas}"
    );
}
