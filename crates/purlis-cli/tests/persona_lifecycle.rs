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
    let mut command = Command::new(env!("CARGO_BIN_EXE_purlis"));
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
        "CHARTER_PLANE_ROOT_SESSION",
        "CHARTER_PERSONA",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
        "CLAUDE_CONFIG_DIR",
    ] {
        // Under either name (V93k): a suite run in a chat inherits both.
        for spelling in purlis_core::envvar::spellings(name) {
            command.env_remove(spelling);
        }
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
        "✗ no persona 'qa' (create it: purlis persona create qa)\n"
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
fn create_of_the_reserved_name_charter_is_refused_and_writes_nothing() {
    let tmp = daily();
    let made = charter(
        &tmp,
        &[
            "persona",
            "create",
            "charter",
            "--delegate-when",
            "anything",
        ],
    );
    assert_eq!(made.status.code(), Some(1));
    assert!(
        err(&made).contains("the persona name 'charter' is reserved"),
        "{}",
        err(&made)
    );
    assert!(!root(&tmp).join("personas/charter").exists());
}

#[test]
fn create_of_the_reserved_name_purlis_is_refused_and_writes_nothing() {
    // #1266: the harness plugin is `purlis`, its skills `purlis:<skill>`.
    let tmp = daily();
    let made = charter(
        &tmp,
        &["persona", "create", "purlis", "--delegate-when", "anything"],
    );
    assert_eq!(made.status.code(), Some(1));
    assert!(
        err(&made).contains("the persona name 'purlis' is reserved — `purlis:<skill>`"),
        "{}",
        err(&made)
    );
    assert!(!root(&tmp).join("personas/purlis").exists());
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
    // Nothing inherited, as `doctor.rs` runs it: a CI runner's environment is not the plane's.
    let json = |tmp: &tempfile::TempDir| -> serde_json::Value {
        let root = std::fs::canonicalize(root(tmp)).unwrap();
        let doctor = Command::new(env!("CARGO_BIN_EXE_purlis"))
            .args(["doctor", "--json"])
            .current_dir(&root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", tmp.path().join("home"))
            .env("CHARTER_ROOT", &root)
            .env("CHARTER_SESSION_ID", "fixture-session-1")
            .output()
            .expect("the binary runs");
        serde_json::from_slice(&doctor.stdout).unwrap_or_else(|e| {
            panic!(
                "doctor --json is not JSON ({e}), exit {:?}:\n{}",
                doctor.status.code(),
                err(&doctor)
            )
        })
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
            "uses: 'ghost' — no such persona (dangling)  → purlis persona lint steward"
        ),
        "{grant}"
    );
    assert_eq!(
        row(&doc, "personas")["detail"],
        "1 with error(s): steward · 1 draft: devops"
    );
}

#[test]
fn sync_agents_is_retired_and_answers_with_what_replaced_it_whatever_it_is_given() {
    // #1451: a persona runs as its own chat, and purlis generates no sub-agent for it. The
    // word is still taken, with the flags a script may still pass, and answers one sentence.
    let tmp = daily();
    let retired = "✗ `purlis persona sync-agents` is retired: a persona runs as its own chat \
                   and purlis generates no helper for it. Give a persona work with `purlis \
                   dispatch --to <persona>`, and remove the helper files purlis wrote with \
                   `purlis doctor --fix persona-agents`.\n";
    for args in [
        vec!["persona", "sync-agents"],
        vec!["persona", "sync-agents", "--persona", "devops"],
        vec![
            "persona",
            "sync-agents",
            "--approve-mcp",
            "--yes",
            "--dry-run",
        ],
    ] {
        let said = charter(&tmp, &args);
        assert_eq!(said.status.code(), Some(1), "{args:?}");
        assert_eq!(err(&said), retired, "{args:?}");
        assert_eq!(out(&said), "", "{args:?}");
    }
    assert!(
        !root(&tmp).join(".claude/agents").exists(),
        "nothing is generated"
    );
    // It is not offered: the help of `persona` does not list it.
    let help = out(&charter(&tmp, &["persona", "--help"]));
    assert!(!help.contains("sync-agents"), "{help}");
}

#[test]
fn create_writes_no_sub_agent_file_and_says_a_draft_is_dispatched_to_by_nobody() {
    let tmp = daily();
    let made = charter(
        &tmp,
        &["persona", "create", "qa", "--delegate-when", "test plans"],
    );
    assert_eq!(made.status.code(), Some(0), "{}", err(&made));
    assert!(
        err(&made).contains("marked `draft: true` — no chat is dispatched to 'qa' yet."),
        "{}",
        err(&made)
    );
    assert!(!err(&made).contains("sub-agent"), "{}", err(&made));
    assert!(!root(&tmp).join(".claude/agents").exists());
}

#[test]
fn approve_mcp_is_refused_inside_a_chat_and_shows_its_lines_on_a_dry_run() {
    // #1451, D-1451-17: the approval that lets a persona chat start a server with a vault
    // credential is a person's. A chat cannot give it, with `--yes` or without.
    let tmp = daily();
    std::fs::write(
        root(&tmp).join("personas/devops/mcp.json"),
        r#"{"mcpServers": {"ga4": {"command": "ga4-mcp", "secrets": {"GA_TOKEN": "ga-token"}}}}"#,
    )
    .unwrap();
    let dry = charter(&tmp, &["persona", "approve-mcp", "--dry-run"]);
    assert_eq!(dry.status.code(), Some(0), "{}", err(&dry));
    assert!(
        err(&dry).contains("devops/ga4 → run ga4-mcp"),
        "{}",
        err(&dry)
    );
    assert!(
        err(&dry).contains("--dry-run: nothing approved"),
        "{}",
        err(&dry)
    );

    let mut in_chat = Command::new(env!("CARGO_BIN_EXE_purlis"));
    in_chat
        .args(["persona", "approve-mcp", "--yes"])
        .current_dir(root(&tmp))
        .env("CHARTER_ROOT", root(&tmp))
        .env("HOME", tmp.path().join("home"))
        .env(purlis_core::hookwire::CHAT_ENV, "7")
        .env("NO_COLOR", "1");
    let refused = in_chat.output().expect("the binary runs");
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        err(&refused).contains("is not run from inside a chat"),
        "{}",
        err(&refused)
    );
    assert!(!root(&tmp).join(".charter/mcp-approved.json").exists());
}
