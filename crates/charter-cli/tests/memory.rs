//! The memory commands where this binary answers differently from the Python charter, on
//! purpose — each a place the differential harness cannot compare, because the two are
//! meant to disagree.
//!
//! Everything else these commands do is compared byte for byte against Python by
//! `tests/differential/run.py`; what is here is what that harness would report as a
//! difference: a name that is a path, a link read by name, and the rungs of "which
//! workspace, which persona" that this binary does not climb.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A copy of the committed `daily` fixture plane, which the Python charter wrote.
fn daily() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().unwrap();
    copy(&fixture, &dir.path().join("plane"));
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

fn charter(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(args)
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        .env("CHARTER_SESSION_ID", "fixture-session-1")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .expect("the binary runs")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn forget_refuses_a_slug_that_is_a_path_and_deletes_nothing() {
    // `forget` deleting through a slug is where M1.1's third review found arbitrary file
    // deletion. A slug names one file in the store; each of these names something else —
    // a neighbour's charter, a persona's definition, and a file outside the plane.
    let tmp = daily();
    let root = root(&tmp);
    let outside = tmp.path().join("victim.md");
    std::fs::write(&outside, "PRECIOUS\n").unwrap();
    let neighbour = root.join("workspaces/beta/workspace.md");
    let persona = root.join("personas/devops/persona.md");

    for slug in [
        "../../beta/workspace",
        "../../../personas/devops/persona",
        "../../../../victim",
        outside.to_str().unwrap(),
        "..",
    ] {
        let out = charter(&root, &["workspace", "forget", slug, "-w", "alpha"]);

        assert_eq!(out.status.code(), Some(1), "{slug}: {}", stderr(&out));
        assert!(
            stderr(&out).contains("is not the slug of one memory"),
            "refused as a slug, not by some later check: {}",
            stderr(&out)
        );
    }

    assert!(
        outside.exists(),
        "the file outside the plane is still there"
    );
    assert!(neighbour.exists(), "beta's charter is still there");
    assert!(persona.exists(), "devops's definition is still there");
}

#[test]
fn forget_still_takes_a_real_slug() {
    // The guard above against a gate so strict it stops `forget` working at all.
    let tmp = daily();
    let root = root(&tmp);
    let entry =
        root.join("workspaces/alpha/memory/20260302-091200-the-api-returns-418-on-mondays.md");

    let out = charter(
        &root,
        &[
            "workspace",
            "forget",
            "the-api-returns-418-on-mondays",
            "-w",
            "alpha",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(!entry.exists());
}

#[test]
fn persona_recall_does_not_print_an_index_linked_out_of_the_plane() {
    // charter reads the index by name and prints it; a committed `MEMORY.md -> /elsewhere`
    // printed whatever it pointed at. The store's own gate is asked here.
    let tmp = daily();
    let root = root(&tmp);
    let secret = tmp.path().join("passwd");
    std::fs::write(&secret, "root:x:0:0:SECRET\n").unwrap();
    let index = root.join("personas/devops/memory/MEMORY.md");
    std::fs::remove_file(&index).unwrap();
    std::os::unix::fs::symlink(&secret, &index).unwrap();

    let out = charter(&root, &["persona", "recall", "devops"]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(!stdout(&out).contains("SECRET"), "{}", stdout(&out));
    assert!(
        stdout(&out)
            .contains("── persistent memory · devops (1) [personas/devops/memory/]\n(no index)\n"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn recall_does_not_read_a_workspace_named_by_a_path() {
    // charter joins `-w` onto `workspaces/` and reads whatever that lands on inside the
    // plane's data: `-w ../personas/devops` searched a persona's memory and labelled it a
    // workspace. A name is checked before it becomes a path.
    let tmp = daily();
    let root = root(&tmp);

    let out = charter(
        &root,
        &["recall", "-w", "../personas/devops", "--scope", "workspace"],
    );

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).contains("no workspace '../personas/devops'"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn recall_names_the_owners_it_needs_rather_than_guessing_them() {
    // charter resolves the active workspace and persona through a ladder of pointers this
    // binary has not ported. Guessing would search the wrong bases and answer as if it had
    // searched the right ones.
    let tmp = daily();
    let root = root(&tmp);

    let no_workspace = charter(&root, &["recall", "--persona", "devops"]);
    assert_eq!(no_workspace.status.code(), Some(2));
    assert!(stderr(&no_workspace).contains("-w <workspace> or --all-workspaces"));

    let no_persona = charter(&root, &["recall", "-w", "alpha"]);
    assert_eq!(no_persona.status.code(), Some(2));
    assert!(stderr(&no_persona).contains("--persona <name>"));

    // Scopes that need neither need neither.
    let shared_only = charter(&root, &["recall", "--scope", "shared"]);
    assert_eq!(
        shared_only.status.code(),
        Some(0),
        "{}",
        stderr(&shared_only)
    );
    assert!(stdout(&shared_only).contains("The plane is the unit of work"));
}

#[test]
fn persona_remember_names_the_persona_rather_than_guessing_it() {
    let tmp = daily();
    let root = root(&tmp);

    let out = charter(&root, &["persona", "remember", "only text", "--no-sync"]);

    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("name the persona"),
        "{}",
        stderr(&out)
    );
    assert!(
        !root.join("personas/steward/memory/only-text.md").exists(),
        "nothing was written to the persona charter.toml would have chosen"
    );
}

#[test]
fn a_plane_that_commits_memory_is_told_this_charter_did_not() {
    // `[memory] share = "commit"` means charter commits each memory as it is written. This
    // binary does not; saying nothing would leave the operator to find it uncommitted.
    let tmp = daily();
    let root = root(&tmp);
    let toml = std::fs::read_to_string(root.join("charter.toml")).unwrap();
    std::fs::write(
        root.join("charter.toml"),
        toml.replace("share = \"local\"", "share = \"commit\""),
    )
    .unwrap();

    let out = charter(&root, &["persona", "remember", "devops", "A fact"]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("does not commit memory"),
        "{}",
        stderr(&out)
    );
}
