//! `charter workspace move-memory` and `charter persona move-memory` (KN-3): a memory moved from
//! one scope to another through the binary, on a copy of the committed `daily` fixture plane.

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

fn command(tmp: &tempfile::TempDir, args: &[&str]) -> Command {
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
        "CHARTER_PLANE_ROOT_SESSION",
        "CHARTER_PERSONA",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
        "CLAUDE_CONFIG_DIR",
    ] {
        // Under either name (V93k): a suite run in a chat inherits both.
        for spelling in charter_core::envvar::spellings(name) {
            command.env_remove(spelling);
        }
    }
    command
}

fn charter(tmp: &tempfile::TempDir, args: &[&str]) -> Output {
    command(tmp, args)
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs")
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn read(tmp: &tempfile::TempDir, rel: &str) -> String {
    std::fs::read_to_string(root(tmp).join(rel)).unwrap()
}

fn exists(tmp: &tempfile::TempDir, rel: &str) -> bool {
    root(tmp).join(rel).exists()
}

const API: &str = "workspaces/alpha/memory/20260302-091200-the-api-returns-418-on-mondays.md";
const ALPHA_INDEX: &str = "workspaces/alpha/memory/MEMORY.md";
const PROD: &str = "personas/devops/memory/cluster-prod-1-lives-in-eu-west-1.md";
const SHARED: &str = "personas/_shared/memory/the-plane-is-the-unit-of-work.md";

#[test]
fn workspace_move_memory_takes_a_journal_memory_to_a_persona_whole() {
    let tmp = daily();
    let before = read(&tmp, API);

    let out = charter(
        &tmp,
        &[
            "workspace",
            "move-memory",
            "the-api-returns-418-on-mondays",
            "--to-persona",
            "devops",
            "-w",
            "alpha",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let moved = "personas/devops/memory/the-api-returns-418-on-mondays.md";
    assert_eq!(read(&tmp, moved), before, "title and stamp kept");
    assert!(!exists(&tmp, API), "no copy left behind");
    assert!(
        !read(&tmp, ALPHA_INDEX).contains("the-api-returns-418-on-mondays"),
        "{}",
        read(&tmp, ALPHA_INDEX)
    );
    assert!(
        read(&tmp, "personas/devops/memory/MEMORY.md")
            .contains("- [The API returns 418 on Mondays](the-api-returns-418-on-mondays.md)\n")
    );
    let words = said(&out);
    assert!(words.contains("Moved"), "{words}");
    assert!(words.contains(moved), "{words}");
    // The way back is printed — not called an Undo: a journal name comes back to the minute.
    assert!(
        words.contains(
            "To move it back: `charter persona move-memory devops the-api-returns-418-on-mondays \
             --to-workspace alpha`"
        ),
        "{words}"
    );
}

#[test]
fn persona_move_memory_takes_a_persona_memory_to_shared() {
    let tmp = daily();
    let before = read(&tmp, PROD);

    let out = charter(
        &tmp,
        &[
            "persona",
            "move-memory",
            "devops",
            "cluster-prod-1-lives-in-eu-west-1",
            "--to-shared",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        read(
            &tmp,
            "personas/_shared/memory/cluster-prod-1-lives-in-eu-west-1.md"
        ),
        before
    );
    assert!(!exists(&tmp, PROD));
}

#[test]
fn persona_move_memory_from_shared_into_a_journal_takes_the_memorys_stamp_as_its_prefix() {
    let tmp = daily();
    let before = read(&tmp, SHARED);

    let out = charter(
        &tmp,
        &[
            "persona",
            "move-memory",
            "steward",
            "the-plane-is-the-unit-of-work",
            "--shared",
            "--to-workspace",
            "beta",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let names: Vec<String> = std::fs::read_dir(root(&tmp).join("workspaces/beta/memory"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with("the-plane-is-the-unit-of-work.md"))
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert_eq!(
        read(&tmp, &format!("workspaces/beta/memory/{}", names[0])),
        before
    );
    assert!(!exists(&tmp, SHARED));
}

#[test]
fn a_move_onto_a_name_the_target_holds_is_refused_and_changes_nothing() {
    let tmp = daily();
    // The same memory, already in devops' store under the slug the move would take.
    std::fs::write(
        root(&tmp).join("personas/devops/memory/the-api-returns-418-on-mondays.md"),
        "# Taken\n",
    )
    .unwrap();
    let before = read(&tmp, API);

    let out = charter(
        &tmp,
        &[
            "workspace",
            "move-memory",
            "the-api-returns-418-on-mondays",
            "--to-persona",
            "devops",
            "-w",
            "alpha",
        ],
    );

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(said(&out).contains("already holds"), "{}", said(&out));
    assert_eq!(read(&tmp, API), before);
    assert_eq!(
        read(
            &tmp,
            "personas/devops/memory/the-api-returns-418-on-mondays.md"
        ),
        "# Taken\n"
    );
}

#[test]
fn a_move_to_a_persona_the_project_does_not_have_is_refused() {
    let tmp = daily();

    let out = charter(
        &tmp,
        &[
            "workspace",
            "move-memory",
            "the-api-returns-418-on-mondays",
            "--to-persona",
            "nobody",
            "-w",
            "alpha",
        ],
    );

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(said(&out).contains("no persona 'nobody'"), "{}", said(&out));
    assert!(exists(&tmp, API));
}

#[test]
fn a_move_names_exactly_one_target() {
    let tmp = daily();

    let none = charter(
        &tmp,
        &[
            "workspace",
            "move-memory",
            "the-api-returns-418-on-mondays",
            "-w",
            "alpha",
        ],
    );
    let two = charter(
        &tmp,
        &[
            "workspace",
            "move-memory",
            "the-api-returns-418-on-mondays",
            "--to-shared",
            "--to-persona",
            "devops",
            "-w",
            "alpha",
        ],
    );

    assert_ne!(none.status.code(), Some(0), "{}", said(&none));
    assert_ne!(two.status.code(), Some(0), "{}", said(&two));
    assert!(exists(&tmp, API));
}

#[test]
fn a_memory_the_store_does_not_hold_is_named_in_the_refusal() {
    let tmp = daily();

    let out = charter(
        &tmp,
        &[
            "persona",
            "move-memory",
            "devops",
            "nothing-here",
            "--to-shared",
        ],
    );

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(said(&out).contains("nothing-here"), "{}", said(&out));
}
