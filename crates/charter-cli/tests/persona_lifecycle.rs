//! `charter persona create | show | clear | remove | lint` through the binary (#365), on a copy
//! of the committed `daily` fixture plane. The Python oracle recorded none of these, so this
//! is where the command surface is held: the refusals the Python charter gave, the round trip
//! a persona makes, and the doctor rows that run the lint.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn daily() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().unwrap();
    copy(&fixture, &dir.path().join("plane"));
    std::fs::create_dir_all(dir.path().join("home")).unwrap();
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

fn root(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("plane")
}

fn charter(tmp: &tempfile::TempDir, args: &[&str]) -> Output {
    let root = root(tmp);
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(args)
        .current_dir(&root)
        .env("CHARTER_ROOT", &root)
        .env("HOME", tmp.path().join("home"))
        .env("CHARTER_SESSION_ID", "fixture-session-1")
        .env("NO_COLOR", "1");
    for name in [
        "CLAUDE_CODE_SESSION_ID",
        "CHARTER_WORKSPACE",
        "CHARTER_PERSONA",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
        "CLAUDE_CONFIG_DIR",
    ] {
        command.env_remove(name);
    }
    command.output().expect("the binary runs")
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn out(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn create_then_show_then_remove_round_trips() {
    let tmp = daily();
    let made = charter(
        &tmp,
        &[
            "persona",
            "create",
            "qa",
            "--role",
            "QA Engineer",
            "--delegate-when",
            "test plans",
        ],
    );
    assert!(made.status.success(), "{}", err(&made));
    assert!(root(&tmp).join("personas/qa/persona.md").exists());

    let shown = charter(&tmp, &["persona", "show", "qa"]);
    assert!(shown.status.success(), "{}", err(&shown));
    assert!(
        out(&shown).starts_with("qa — QA Engineer\n"),
        "{}",
        out(&shown)
    );
    assert!(out(&shown).contains("\n## When to delegate here\ntest plans\n"));

    let gone = charter(&tmp, &["persona", "remove", "qa"]);
    assert!(gone.status.success(), "{}", err(&gone));
    assert!(!root(&tmp).join("personas/qa").exists());
    let shown = charter(&tmp, &["persona", "show", "qa"]);
    assert_eq!(shown.status.code(), Some(1));
    assert_eq!(
        err(&shown),
        "✗ no persona 'qa' (create it: charter persona create qa)\n"
    );
}

#[test]
fn create_without_delegate_when_is_refused_and_writes_nothing() {
    let tmp = daily();
    let made = charter(&tmp, &["persona", "create", "qa"]);
    assert_eq!(made.status.code(), Some(1));
    assert!(
        err(&made).starts_with("✗ --delegate-when is required"),
        "{}",
        err(&made)
    );
    assert!(!root(&tmp).join("personas/qa").exists());
}

#[test]
fn remove_of_a_persona_another_extends_is_refused_without_force() {
    let tmp = daily();
    let made = charter(&tmp, &["persona", "create", "kid", "--extends", "devops"]);
    assert!(made.status.success(), "{}", err(&made));
    let refused = charter(&tmp, &["persona", "remove", "devops"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        err(&refused).contains("✗   kid (extends)\n"),
        "{}",
        err(&refused)
    );
    assert!(root(&tmp).join("personas/devops/persona.md").exists());
    let forced = charter(&tmp, &["persona", "remove", "devops", "--force"]);
    assert!(forced.status.success(), "{}", err(&forced));
    assert!(!root(&tmp).join("personas/devops").exists());
}

#[test]
fn clear_drops_this_sessions_selection() {
    let tmp = daily();
    let used = charter(&tmp, &["persona", "use", "devops"]);
    assert!(used.status.success(), "{}", err(&used));
    let cleared = charter(&tmp, &["persona", "clear"]);
    assert!(cleared.status.success(), "{}", err(&cleared));
    assert_eq!(
        err(&cleared),
        "✓ Active persona cleared.\n• This shell now resolves to 'steward' (via charter.toml).\n"
    );
    assert_eq!(out(&charter(&tmp, &["persona", "current"])), "steward\n");
}

#[test]
fn lint_warns_about_the_draft_and_fails_on_a_dangling_reference() {
    let tmp = daily();
    let linted = charter(&tmp, &["persona", "lint", "devops"]);
    assert!(linted.status.success(), "{}", err(&linted));
    assert!(
        err(&linted).starts_with("! devops: draft: true → charter unfinished"),
        "{}",
        err(&linted)
    );
    std::fs::write(
        root(&tmp).join("personas/steward/persona.md"),
        "---\nrole: Steward\nvault: none\ndelegate-when: x\nuses: ghost\n---\n",
    )
    .unwrap();
    let linted = charter(&tmp, &["persona", "lint", "steward"]);
    assert_eq!(linted.status.code(), Some(1));
    assert!(
        err(&linted).contains("✗ steward: uses: 'ghost' — no such persona (dangling)\n"),
        "{}",
        err(&linted)
    );
}

#[test]
fn the_doctor_runs_the_lint_for_the_personas_and_persona_grant_rows() {
    let tmp = daily();
    let json = |tmp: &tempfile::TempDir| -> serde_json::Value {
        let doctor = charter(tmp, &["doctor", "--json"]);
        serde_json::from_slice(&doctor.stdout).expect("doctor --json is JSON")
    };
    let row = |doc: &serde_json::Value, name: &str| -> serde_json::Value {
        doc.as_array()
            .or_else(|| doc["checks"].as_array())
            .expect("a list of rows")
            .iter()
            .find(|r| r["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("no row {name}"))
    };
    let doc = json(&tmp);
    assert_eq!(row(&doc, "personas")["detail"], "1 draft: devops");
    assert_eq!(
        row(&doc, "persona grant")["detail"],
        "'steward' is well-formed"
    );

    // The active persona broken and still granting a tool: said, with the lint to run.
    std::fs::write(
        root(&tmp).join("personas/steward/persona.md"),
        "---\nrole: Steward\nvault: none\ndelegate-when: x\ntools: kubectl\nuses: ghost\n---\n",
    )
    .unwrap();
    let doc = json(&tmp);
    let grant = row(&doc, "persona grant");
    assert_eq!(grant["status"], "warn");
    assert_eq!(
        grant["detail"],
        "'steward' is broken and still auto-approves kubectl"
    );
    assert!(
        grant["hint"].as_str().unwrap().starts_with(
            "uses: 'ghost' — no such persona (dangling)  → charter persona lint steward"
        ),
        "{grant}"
    );
    assert_eq!(
        row(&doc, "personas")["detail"],
        "1 with error(s): steward · 1 draft: devops"
    );
}
