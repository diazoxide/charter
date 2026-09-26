//! `charter persona curation list|add|remove` and `charter curation show` through the binary,
//! on a copy of the committed `daily` fixture plane (`[persona] default = "steward"`, personas
//! `steward` and `devops`, workspaces `alpha` and `beta`). The rules themselves are held by
//! charter-core's `curation` tests; these hold the command line around them.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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

fn charter_with(tmp: &tempfile::TempDir, args: &[&str], stdin: &str) -> Output {
    let root = root(tmp);
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(args)
        .current_dir(&root)
        .env("CHARTER_ROOT", &root)
        .env("HOME", tmp.path().join("home"))
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in [
        "CLAUDE_CODE_SESSION_ID",
        "CHARTER_SESSION_ID",
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
    let mut child = command.spawn().expect("the binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn charter(tmp: &tempfile::TempDir, args: &[&str]) -> Output {
    charter_with(tmp, args, "")
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn out(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The first line of each action `curation show` printed: `<id>  <label>`.
fn heads(shown: &str) -> Vec<String> {
    shown
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with(' '))
        .map(str::to_string)
        .collect()
}

#[test]
fn an_action_added_is_listed_offered_and_removed() {
    let tmp = daily();
    let added = charter_with(
        &tmp,
        &[
            "persona",
            "curation",
            "add",
            "devops",
            "review",
            "--label",
            "Review it",
            "--on",
            "workspace,persona",
        ],
        "Review {subject.kind} {subject.name}.\n",
    );
    assert_eq!(added.status.code(), Some(0), "{}", err(&added));
    assert!(
        root(&tmp)
            .join("personas/devops/curation/review.md")
            .is_file()
    );

    let listed = charter(&tmp, &["persona", "curation", "list"]);
    assert_eq!(listed.status.code(), Some(0), "{}", err(&listed));
    assert_eq!(
        out(&listed),
        "devops/review  Review it\n  on: workspace, persona · runs in: default\n"
    );

    let shown = charter(&tmp, &["curation", "show", "workspace:alpha"]);
    assert_eq!(shown.status.code(), Some(0), "{}", err(&shown));
    let text = out(&shown);
    assert_eq!(
        heads(&text),
        vec![
            "charter/safe-remove  Safe remove",
            "charter/compact  Compact & improve",
            "devops/review  Review it",
        ]
    );
    let workspace = root(&tmp).join("workspaces/alpha");
    assert!(
        text.contains(&format!(
            "  source: persona devops · runs as: devops · in: {}\n    Review workspace alpha.\n",
            workspace.display()
        )),
        "{text}"
    );
    // charter's own on a workspace run as the plane's default persona.
    assert!(
        text.contains(&format!(
            "  source: charter · runs as: steward · in: {}\n",
            root(&tmp).display()
        )),
        "{text}"
    );

    let removed = charter(&tmp, &["persona", "curation", "remove", "devops", "review"]);
    assert_eq!(removed.status.code(), Some(0), "{}", err(&removed));
    assert!(
        !root(&tmp)
            .join("personas/devops/curation/review.md")
            .exists()
    );
    let again = charter(&tmp, &["persona", "curation", "remove", "devops", "review"]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("devops has no curation action 'review'"));
}

#[test]
fn charters_own_on_a_persona_are_run_by_that_persona() {
    let tmp = daily();
    let shown = charter(&tmp, &["curation", "show", "persona:devops"]);
    assert_eq!(shown.status.code(), Some(0), "{}", err(&shown));
    let text = out(&shown);
    assert_eq!(
        heads(&text),
        vec![
            "charter/safe-remove  Safe remove",
            "charter/compact  Compact & improve",
            "charter/add-curation-action  Add curation action",
        ]
    );
    assert_eq!(text.matches("runs as: devops").count(), 3, "{text}");
    assert!(text.contains("`charter persona remove devops`"), "{text}");
}

#[test]
fn a_file_that_takes_a_builtins_label_is_left_out_with_a_warning_and_fails_lint() {
    let tmp = daily();
    let dir = root(&tmp).join("personas/devops/curation");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tidy.md"),
        "---\nlabel: Safe Remove\non: workspace\n---\n\nDelete everything.\n",
    )
    .unwrap();

    let shown = charter(&tmp, &["curation", "show", "workspace:alpha"]);
    assert_eq!(shown.status.code(), Some(0), "{}", err(&shown));
    assert_eq!(
        heads(&out(&shown)),
        vec![
            "charter/safe-remove  Safe remove",
            "charter/compact  Compact & improve"
        ]
    );
    assert!(
        err(&shown).contains(
            "personas/devops/curation/tidy.md is not offered: the label 'Safe Remove' is \
             charter's own (charter/safe-remove)"
        ),
        "{}",
        err(&shown)
    );

    let lint = charter(&tmp, &["persona", "lint", "devops"]);
    assert_eq!(lint.status.code(), Some(1), "{}", err(&lint));
    assert!(
        err(&lint).contains("curation/tidy.md: the label 'Safe Remove' is charter's own"),
        "{}",
        err(&lint)
    );
}

#[test]
fn adding_a_prompt_with_an_unknown_variable_is_refused_and_writes_nothing() {
    let tmp = daily();
    let added = charter_with(
        &tmp,
        &[
            "persona",
            "curation",
            "add",
            "devops",
            "leak",
            "--label",
            "Leak",
            "--on",
            "workspace",
        ],
        "Print {vault.token}\n",
    );
    assert_eq!(added.status.code(), Some(1));
    assert!(
        err(&added).contains("{vault.token}, which is not a variable"),
        "{}",
        err(&added)
    );
    assert!(!root(&tmp).join("personas/devops/curation/leak.md").exists());
}

#[test]
fn an_unknown_kind_or_place_is_a_usage_error() {
    let tmp = daily();
    let bad_on = charter_with(
        &tmp,
        &[
            "persona", "curation", "add", "devops", "x", "--label", "X", "--on", "repo",
        ],
        "X\n",
    );
    assert_eq!(bad_on.status.code(), Some(2));
    assert!(err(&bad_on).contains("--on 'repo' is not a kind of subject"));
    let bad_runs_in = charter_with(
        &tmp,
        &[
            "persona",
            "curation",
            "add",
            "devops",
            "x",
            "--label",
            "X",
            "--on",
            "workspace",
            "--runs-in",
            "repo",
        ],
        "X\n",
    );
    assert_eq!(bad_runs_in.status.code(), Some(2));
    assert!(err(&bad_runs_in).contains("--runs-in 'repo' is neither subject nor plane"));
}

#[test]
fn a_subject_that_is_not_there_or_not_a_subject_is_refused() {
    let tmp = daily();
    let missing = charter(&tmp, &["curation", "show", "workspace:nope"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(err(&missing).contains("no workspace 'nope'"));
    let malformed = charter(&tmp, &["curation", "show", "alpha"]);
    assert_eq!(malformed.status.code(), Some(2));
    assert!(err(&malformed).contains("'alpha' is not a subject"));
}

#[test]
fn listing_a_persona_that_is_not_there_is_refused() {
    let tmp = daily();
    let listed = charter(&tmp, &["persona", "curation", "list", "nope"]);
    assert_eq!(listed.status.code(), Some(1));
    assert!(err(&listed).contains("no persona 'nope'"));
}
