//! The memory commands where this binary answers differently from the Python charter, on
//! purpose — each a place the differential harness cannot compare, because the two are
//! meant to disagree.
//!
//! Everything else these commands do is compared byte for byte against Python by
//! `tests/differential/run.py`; what is here is what that harness would report as a
//! difference: a name that is a path, and a link read by name.
//!
//! **"the rungs of which workspace, which persona that this binary does not climb" used to
//! be the third item, and M2.9 deleted it** — the ladders are in `charter_core::active` now,
//! and the tests that pinned the refusals pin the resolutions instead. What is left here of
//! that is the one answer the two implementations still share and neither explains: a
//! persona command on a plane with no front door exits 1 and says nothing.

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
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(args)
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        .env("CHARTER_SESSION_ID", "fixture-session-1");
    // Since M2.9 these decide which workspace and which persona a command acts on, so a
    // suite run inside a real chat would otherwise inherit that chat's. `CHARTER_SESSION_ID`
    // is set positively above, for the fixture's own pointer.
    for name in [
        "CLAUDE_CODE_SESSION_ID",
        "CHARTER_WORKSPACE",
        "CHARTER_PERSONA",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
    ] {
        command.env_remove(name);
    }
    command.output().expect("the binary runs")
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
fn recall_resolves_the_owners_it_was_not_given() {
    // **M2.9 turned this test round, and its old name said why it existed**: `recall` used to
    // refuse with exit 2 — "this charter does not resolve the active workspace" — and a
    // harness calls it with no flags at all, at session start. Both owners now come off the
    // ladders in `charter_core::active`. The `daily` fixture's own session pointer says
    // `alpha`; its `[persona] default` says `steward`.
    let tmp = daily();
    let root = root(&tmp);

    let no_workspace = charter(&root, &["recall", "--persona", "devops"]);
    assert_eq!(
        no_workspace.status.code(),
        Some(0),
        "{}",
        stderr(&no_workspace)
    );
    assert!(
        stdout(&no_workspace).contains("workspace:alpha"),
        "the session's pointer did not decide: {}",
        stdout(&no_workspace)
    );

    // The persona rung is climbed too, and proved by a persona that HAS memory: the plane's
    // declared front door is `steward`, which has none, so a passing assertion over it would
    // hold whether or not anything was searched.
    std::fs::write(root.join(".charter/active-persona"), "devops\n").unwrap();
    let no_persona = charter(&root, &["recall", "-w", "alpha"]);
    assert_eq!(no_persona.status.code(), Some(0), "{}", stderr(&no_persona));
    assert!(
        stdout(&no_persona).contains("persona:devops"),
        "the active-persona file did not decide: {}",
        stdout(&no_persona)
    );

    // And with nothing named at all, which is the call this milestone exists for.
    let nothing = charter(&root, &["recall"]);
    assert_eq!(nothing.status.code(), Some(0), "{}", stderr(&nothing));
    assert!(stdout(&nothing).contains("workspace:alpha"));
    assert!(stdout(&nothing).contains("persona:devops"));

    // Scopes that need neither still need neither.
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
fn recall_on_a_plane_with_no_front_door_searches_the_rest_rather_than_refusing() {
    // A persona is allowed to resolve to nothing where a workspace is not: `recall.sources`
    // skips "a scope with no owner". Refusing instead would make a plane that has declared
    // no persona unable to recall anything at all.
    let tmp = daily();
    let root = root(&tmp);
    let toml = std::fs::read_to_string(root.join("charter.toml")).unwrap();
    std::fs::write(
        root.join("charter.toml"),
        toml.replace("[persona]\ndefault = \"steward\"\n", ""),
    )
    .unwrap();

    let out = charter(&root, &["recall"]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("workspace:alpha"), "{}", stdout(&out));
    assert!(!stdout(&out).contains("persona:"), "{}", stdout(&out));
}

#[test]
fn persona_remember_with_one_word_takes_it_as_the_fact_and_resolves_the_persona() {
    // `[NAME] TEXT`, told apart by the SHAPE of the call — charter's own `name nargs="?"`.
    // This used to refuse with "name the persona", so a chat could not record what it had
    // learned without repeating an identity it was already running as.
    let tmp = daily();
    let root = root(&tmp);

    let out = charter(&root, &["persona", "remember", "only text", "--no-sync"]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        root.join("personas/steward/memory/only-text.md").is_file(),
        "nothing was written to the persona charter.toml declares: {}",
        stderr(&out)
    );
}

#[test]
fn a_persona_command_on_a_plane_with_no_front_door_exits_one_and_says_nothing() {
    // **charter's own answer, ported rather than improved.** `cmd_persona_remember` and
    // `cmd_persona_recall` both open with `if not name or not _require(name): return 1`, and
    // the first half of that prints nothing at all. Measured against the oracle on a plane
    // with no `[persona] default`; it reads like a defect, and it is charter's.
    let tmp = daily();
    let root = root(&tmp);
    let toml = std::fs::read_to_string(root.join("charter.toml")).unwrap();
    std::fs::write(
        root.join("charter.toml"),
        toml.replace("[persona]\ndefault = \"steward\"\n", ""),
    )
    .unwrap();

    let out = charter(&root, &["persona", "recall"]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert_eq!(stderr(&out), "");
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
