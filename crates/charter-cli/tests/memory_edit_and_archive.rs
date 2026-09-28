//! `charter workspace edit|archive|unarchive` and `charter persona edit-memory|archive-memory|
//! unarchive-memory` (SI-9a, ADR 0065) through the binary, on a copy of the committed `daily`
//! fixture plane. The Python charter has none of them, so this is where their surface is held.

use std::io::Write as _;
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
        command.env_remove(name);
    }
    command
}

fn charter(tmp: &tempfile::TempDir, args: &[&str]) -> Output {
    command(tmp, args)
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs")
}

fn charter_with_stdin(tmp: &tempfile::TempDir, args: &[&str], input: &str) -> Output {
    let mut child = command(tmp, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
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

const API: &str = "workspaces/alpha/memory/20260302-091200-the-api-returns-418-on-mondays.md";
const ALPHA_INDEX: &str = "workspaces/alpha/memory/MEMORY.md";
const PROD: &str = "personas/devops/memory/cluster-prod-1-lives-in-eu-west-1.md";
const SHARED: &str = "personas/_shared/memory/the-plane-is-the-unit-of-work.md";

// --- workspace ------------------------------------------------------------------------------

#[test]
fn workspace_edit_rewrites_a_memory_in_place_and_retitles_its_index_line() {
    let tmp = daily();

    let out = charter(
        &tmp,
        &[
            "workspace",
            "edit",
            "the-api-returns-418-on-mondays",
            "--title",
            "The API returns 418 on Tuesdays",
            "It moved a day.",
            "-w",
            "alpha",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        read(&tmp, API),
        "# The API returns 418 on Tuesdays\n\n_2026-03-02 09:12 · persistent_\n\nIt moved a day.\n"
    );
    assert!(
        read(&tmp, ALPHA_INDEX).contains(
            "- [The API returns 418 on Tuesdays](20260302-091200-the-api-returns-418-on-mondays.md)\n"
        ),
        "{}",
        read(&tmp, ALPHA_INDEX)
    );
    assert!(said(&out).contains("Edited"), "{}", said(&out));
}

#[test]
fn workspace_edit_with_only_a_title_keeps_the_body() {
    let tmp = daily();

    let out = charter(
        &tmp,
        &[
            "workspace",
            "edit",
            "the-api-returns-418-on-mondays",
            "--title",
            "Mondays: 418",
            "-w",
            "alpha",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        read(&tmp, API),
        "# Mondays: 418\n\n_2026-03-02 09:12 · persistent_\n\nThe API returns 418 on Mondays\n"
    );
}

#[test]
fn workspace_edit_reads_the_body_from_standard_input_when_it_is_a_dash() {
    let tmp = daily();

    let out = charter_with_stdin(
        &tmp,
        &[
            "workspace",
            "edit",
            "the-api-returns-418-on-mondays",
            "-",
            "-w",
            "alpha",
        ],
        "Piped body\n\nwith two paragraphs\n",
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        read(&tmp, API),
        "# The API returns 418 on Mondays\n\n_2026-03-02 09:12 · persistent_\n\nPiped body\n\nwith two paragraphs\n"
    );
}

#[test]
fn workspace_edit_with_neither_a_title_nor_a_body_is_refused_and_changes_nothing() {
    let tmp = daily();
    let before = read(&tmp, API);

    let out = charter(
        &tmp,
        &[
            "workspace",
            "edit",
            "the-api-returns-418-on-mondays",
            "-w",
            "alpha",
        ],
    );

    // A usage error, which this binary exits 1 on as it exits 1 on every other.
    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(said(&out).contains("required"), "{}", said(&out));
    assert_eq!(read(&tmp, API), before);
}

#[test]
fn workspace_edit_of_a_memory_that_is_not_there_says_so() {
    let tmp = daily();

    let out = charter(
        &tmp,
        &[
            "workspace",
            "edit",
            "no-such-memory",
            "--title",
            "T",
            "-w",
            "alpha",
        ],
    );

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(
        said(&out).contains("no memory 'no-such-memory'"),
        "{}",
        said(&out)
    );
}

#[test]
fn workspace_archive_hides_a_memory_and_unarchive_brings_it_back() {
    let tmp = daily();
    let file_before = read(&tmp, API);
    let index_before = read(&tmp, ALPHA_INDEX);

    let out = charter(
        &tmp,
        &[
            "workspace",
            "archive",
            "the-api-returns-418-on-mondays",
            "-w",
            "alpha",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!root(&tmp).join(API).exists());
    assert!(
        root(&tmp)
            .join(
                "workspaces/alpha/memory/archive/20260302-091200-the-api-returns-418-on-mondays.md"
            )
            .is_file()
    );
    assert!(!read(&tmp, ALPHA_INDEX).contains("the-api-returns-418-on-mondays"));
    let listed = charter(&tmp, &["workspace", "recall", "-w", "alpha"]);
    assert!(
        !said(&listed).contains("418"),
        "an archived memory is out of the listing: {}",
        said(&listed)
    );

    let out = charter(
        &tmp,
        &[
            "workspace",
            "unarchive",
            "the-api-returns-418-on-mondays",
            "-w",
            "alpha",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(read(&tmp, API), file_before);
    let lines = |t: &str| {
        let mut v: Vec<String> = t.lines().map(str::to_string).collect();
        v.sort();
        v
    };
    assert_eq!(
        lines(&read(&tmp, ALPHA_INDEX)),
        lines(&index_before),
        "the same lines; the restored one is appended at the end"
    );
}

#[test]
fn workspace_archive_of_a_memory_that_is_not_there_says_so() {
    let tmp = daily();

    for verb in ["archive", "unarchive"] {
        let out = charter(&tmp, &["workspace", verb, "no-such-memory", "-w", "alpha"]);
        assert_eq!(out.status.code(), Some(1), "{verb}: {}", said(&out));
        assert!(
            said(&out).contains("no memory 'no-such-memory'"),
            "{verb}: {}",
            said(&out)
        );
    }
}

// --- persona and _shared ------------------------------------------------------------------------

#[test]
fn persona_edit_memory_rewrites_the_personas_own_memory_in_place() {
    let tmp = daily();

    let out = charter(
        &tmp,
        &[
            "persona",
            "edit-memory",
            "devops",
            "cluster-prod-1-lives-in-eu-west-1",
            "--title",
            "Cluster prod-1 lives in eu-central-1",
            "It moved in September.",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        read(&tmp, PROD),
        "# Cluster prod-1 lives in eu-central-1\n\n_2026-03-02 09:27 · persistent_\n\nIt moved in September.\n"
    );
    assert!(read(&tmp, "personas/devops/memory/MEMORY.md").contains(
        "- [Cluster prod-1 lives in eu-central-1](cluster-prod-1-lives-in-eu-west-1.md)"
    ));
}

#[test]
fn persona_edit_memory_with_shared_edits_the_shared_store() {
    let tmp = daily();

    let out = charter(
        &tmp,
        &[
            "persona",
            "edit-memory",
            "devops",
            "the-plane-is-the-unit-of-work",
            "--shared",
            "--title",
            "The plane is the unit of work, always",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(read(&tmp, SHARED).starts_with("# The plane is the unit of work, always\n\n"));
    assert!(
        read(&tmp, "personas/_shared/memory/MEMORY.md").contains(
            "- [The plane is the unit of work, always](the-plane-is-the-unit-of-work.md)"
        )
    );
}

#[test]
fn persona_edit_memory_without_shared_does_not_reach_the_shared_store() {
    let tmp = daily();
    let before = read(&tmp, SHARED);

    let out = charter(
        &tmp,
        &[
            "persona",
            "edit-memory",
            "devops",
            "the-plane-is-the-unit-of-work",
            "--title",
            "Nope",
        ],
    );

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert_eq!(read(&tmp, SHARED), before);
}

#[test]
fn persona_archive_memory_and_unarchive_memory_round_trip_in_the_shared_store() {
    let tmp = daily();
    let before = read(&tmp, SHARED);

    let out = charter(
        &tmp,
        &[
            "persona",
            "archive-memory",
            "devops",
            "the-plane-is-the-unit-of-work",
            "--shared",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!root(&tmp).join(SHARED).exists());
    assert!(
        root(&tmp)
            .join("personas/_shared/memory/archive/the-plane-is-the-unit-of-work.md")
            .is_file()
    );

    let out = charter(
        &tmp,
        &[
            "persona",
            "unarchive-memory",
            "devops",
            "the-plane-is-the-unit-of-work",
            "--shared",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(read(&tmp, SHARED), before);
    assert!(
        read(&tmp, "personas/_shared/memory/MEMORY.md")
            .contains("- [The plane is the unit of work](the-plane-is-the-unit-of-work.md)")
    );
}

#[test]
fn persona_archive_memory_of_a_slug_that_is_a_path_is_refused() {
    let tmp = daily();

    let out = charter(&tmp, &["persona", "archive-memory", "devops", "../persona"]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(
        said(&out).contains("not the slug of one memory"),
        "{}",
        said(&out)
    );
    assert!(root(&tmp).join("personas/devops/persona.md").is_file());
}
